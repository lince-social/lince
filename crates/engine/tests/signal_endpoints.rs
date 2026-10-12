use engine::{
    Engine,
    actions::Action,
    signal_endpoints::{Reading, Receipt, Request, hex},
};
use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use serde_json::Value;
use store::signal_endpoints::Endpoint;

mod support;

fn signing_key(byte: u8) -> SigningKey {
    SigningKey::from_slice(&[byte; 32]).unwrap()
}

fn public_key(key: &SigningKey) -> String {
    hex(key.verifying_key().to_encoded_point(true).as_bytes())
}

async fn inspect(engine: &Engine) -> Value {
    engine
        .act(
            Action::SignalEndpoints {
                request: Request::Inspect,
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap()
}

async fn enroll(engine: &Engine, key: &SigningKey, slug: &str) -> (String, Endpoint) {
    let data = engine
        .act(
            Action::SignalEndpoints {
                request: Request::Save {
                    endpoint: None,
                    expected_revision: None,
                    name: slug.into(),
                    public_key: public_key(key),
                    signal_slug: slug.into(),
                    enabled: false,
                    cooldown_seconds: 60,
                    minimum: "-40".into(),
                    maximum: "100".into(),
                    expires_at: (nucleus::execution::now() + chrono::Duration::days(1))
                        .to_rfc3339(),
                },
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    let endpoint: Endpoint = serde_json::from_value(
        data["endpoints"]
            .as_array()
            .unwrap()
            .iter()
            .find(|endpoint| endpoint["uid"] == data["saved"])
            .unwrap()
            .clone(),
    )
    .unwrap();
    (data["audience"].as_str().unwrap().into(), endpoint)
}

async fn enable(engine: &Engine, endpoint: &Endpoint, enabled: bool) -> Endpoint {
    let data = engine
        .act(
            Action::SignalEndpoints {
                request: Request::SetEnabled {
                    endpoint: endpoint.uid.clone(),
                    expected_revision: endpoint.revision,
                    enabled,
                },
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    serde_json::from_value(
        data["endpoints"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["uid"] == endpoint.uid)
            .unwrap()
            .clone(),
    )
    .unwrap()
}

fn sign(mut reading: Reading, key: &SigningKey) -> Reading {
    let signature: Signature = key.sign(&reading.signing_payload().unwrap());
    reading.signature = hex(&signature.to_bytes());
    reading
}

fn reading(
    audience: &str,
    endpoint: &Endpoint,
    key: &SigningKey,
    sequence: i64,
    value: &str,
) -> Reading {
    sign(
        Reading {
            audience: audience.into(),
            endpoint: endpoint.uid.clone(),
            credential_epoch: endpoint.credential_epoch,
            signal: endpoint.signal_uid.clone(),
            sequence,
            issued_at: nucleus::execution::now().timestamp(),
            value: value.into(),
            signature: String::new(),
        },
        key,
    )
}

async fn quantity(engine: &Engine, endpoint: &Endpoint) -> String {
    store::records::quantity(&engine.store.pool, &endpoint.signal_uid)
        .await
        .unwrap()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn enrollment_is_disabled_and_signed_readings_are_exact_retry_safe_signals() {
    let engine = support::engine().await;
    let key = signing_key(1);
    let (audience, disabled) = enroll(&engine, &key, "kitchen").await;
    assert!(
        engine
            .accept_signal_endpoint_reading(reading(&audience, &disabled, &key, 1, "23.7"))
            .await
            .is_err()
    );
    assert_eq!(quantity(&engine, &disabled).await, "0");
    let endpoint = enable(&engine, &disabled, true).await;
    let pending = reading(&audience, &endpoint, &key, 1, "23.7");
    let first = engine
        .accept_signal_endpoint_reading(pending.clone())
        .await
        .unwrap();
    assert_eq!(
        engine
            .accept_signal_endpoint_reading(pending)
            .await
            .unwrap(),
        first
    );
    assert_eq!(quantity(&engine, &endpoint).await, "23.7");
    let count: i64 = store::sqlx::query_scalar(
        "SELECT count(*) FROM fact WHERE record_uid = ? AND cause_kind = 'signal'",
    )
    .bind(&endpoint.signal_uid)
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    let result = engine
        .act(
            Action::PreviewKarmaReading {
                source: "signal(@kitchen)".into(),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(result.data.unwrap()["value"], "23.7");
    assert_eq!(
        engine
            .accept_signal_endpoint_reading(reading(&audience, &endpoint, &key, 1, "24"))
            .await
            .unwrap_err()
            .code(),
        Some("signal_endpoint_replay")
    );
    assert_eq!(
        engine
            .accept_signal_endpoint_reading(reading(&audience, &endpoint, &key, 2, "24"))
            .await
            .unwrap_err()
            .code(),
        Some("signal_endpoint_cooldown")
    );
}

#[tokio::test]
async fn forged_scopes_tampering_and_wrong_keys_cannot_change_records() {
    let engine = support::engine().await;
    let key = signing_key(1);
    let (audience, disabled) = enroll(&engine, &key, "sensor").await;
    let endpoint = enable(&engine, &disabled, true).await;
    let other = support::plain(&engine, "other", 7.0).await;
    let original = reading(&audience, &endpoint, &key, 1, "12");
    let mut variants = Vec::new();
    let mut changed = original.clone();
    changed.value = "13".into();
    variants.push(changed);
    let mut changed = original.clone();
    changed.signal = other.clone();
    variants.push(sign(changed, &key));
    let mut changed = original.clone();
    changed.audience = "00".repeat(32);
    variants.push(sign(changed, &key));
    let mut changed = original.clone();
    changed.credential_epoch += 1;
    variants.push(sign(changed, &key));
    variants.push(reading(&audience, &endpoint, &signing_key(2), 1, "12"));
    for variant in variants {
        assert!(
            engine
                .accept_signal_endpoint_reading(variant)
                .await
                .is_err()
        );
    }
    assert_eq!(quantity(&engine, &endpoint).await, "0");
    assert_eq!(
        store::records::quantity(&engine.store.pool, &other)
            .await
            .unwrap()
            .unwrap()
            .to_string(),
        "7"
    );
    engine
        .accept_signal_endpoint_reading(original)
        .await
        .unwrap();
}

#[tokio::test]
async fn range_freshness_expiry_and_disable_are_enforced_before_retries() {
    let engine = support::engine().await;
    let key = signing_key(1);
    let (audience, disabled) = enroll(&engine, &key, "sensor").await;
    let endpoint = enable(&engine, &disabled, true).await;
    for value in ["-41", "101"] {
        assert_eq!(
            engine
                .accept_signal_endpoint_reading(reading(&audience, &endpoint, &key, 1, value))
                .await
                .unwrap_err()
                .code(),
            Some("signal_endpoint_invalid")
        );
    }
    for offset in [-121, 31] {
        let mut stale = reading(&audience, &endpoint, &key, 1, "12");
        stale.issued_at += offset;
        assert!(
            engine
                .accept_signal_endpoint_reading(sign(stale, &key))
                .await
                .is_err()
        );
    }
    let pending = reading(&audience, &endpoint, &key, 1, "12");
    engine
        .accept_signal_endpoint_reading(pending.clone())
        .await
        .unwrap();
    let disabled = enable(&engine, &endpoint, false).await;
    assert!(
        engine
            .accept_signal_endpoint_reading(pending.clone())
            .await
            .is_err()
    );
    let enabled = enable(&engine, &disabled, true).await;
    store::sqlx::query("UPDATE signal_endpoint SET expires_at = ? WHERE uid = ?")
        .bind(nucleus::execution::now().timestamp())
        .bind(&enabled.uid)
        .execute(&engine.store.pool)
        .await
        .unwrap();
    assert!(
        engine
            .accept_signal_endpoint_reading(pending)
            .await
            .is_err()
    );
    assert_eq!(quantity(&engine, &endpoint).await, "12");
}

#[tokio::test]
async fn cooldown_boundary_and_old_sequences_remain_enforced() {
    let clock = nucleus::execution::Execution::new([4; 32], 1_900_000_000_000).unwrap();
    clock
        .scope(async {
            let engine = support::engine().await;
            let key = signing_key(1);
            let (audience, disabled) = enroll(&engine, &key, "sensor").await;
            let endpoint = enable(&engine, &disabled, true).await;
            let first = reading(&audience, &endpoint, &key, 1, "10");
            engine
                .accept_signal_endpoint_reading(first.clone())
                .await
                .unwrap();
            clock.set_time(1_900_000_059_000).unwrap();
            assert_eq!(
                engine
                    .accept_signal_endpoint_reading(reading(&audience, &endpoint, &key, 2, "20"))
                    .await
                    .unwrap_err()
                    .code(),
                Some("signal_endpoint_cooldown")
            );
            clock.set_time(1_900_000_060_000).unwrap();
            engine
                .accept_signal_endpoint_reading(reading(&audience, &endpoint, &key, 2, "20"))
                .await
                .unwrap();
            assert_eq!(
                engine
                    .accept_signal_endpoint_reading(first)
                    .await
                    .unwrap_err()
                    .code(),
                Some("signal_endpoint_replay")
            );
            assert_eq!(quantity(&engine, &endpoint).await, "20");
        })
        .await;
}

#[tokio::test]
async fn simultaneous_retries_commit_one_fact() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", directory.path().join("lince.db").display());
    let engine = Engine::open(&url).await.unwrap();
    store::organs::ensure_local(&engine.store.pool, "http://test")
        .await
        .unwrap();
    let key = signing_key(1);
    let (audience, disabled) = enroll(&engine, &key, "sensor").await;
    let endpoint = enable(&engine, &disabled, true).await;
    let pending = reading(&audience, &endpoint, &key, 1, "12");
    let another_host = Engine::open(&url).await.unwrap();
    let (first, second) = tokio::join!(
        engine.accept_signal_endpoint_reading(pending.clone()),
        another_host.accept_signal_endpoint_reading(pending)
    );
    assert_eq!(first.unwrap(), second.unwrap());
    let count: i64 = store::sqlx::query_scalar(
        "SELECT count(*) FROM fact WHERE record_uid = ? AND cause_kind = 'signal'",
    )
    .bind(&endpoint.signal_uid)
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn signed_signal_updates_trigger_rules_and_duplicates_do_not_repeat_consequences() {
    let engine = support::karma::engine().await;
    let key = signing_key(1);
    let (audience, disabled) = enroll(&engine, &key, "sensor").await;
    let endpoint = enable(&engine, &disabled, true).await;
    let target = support::plain(&engine, "target", 0.0).await;
    support::declare_rule(
        &engine,
        &target,
        nucleus::karma::Cadence::every_days(1),
        &nucleus::execution::now().to_rfc3339(),
        Some("signal(@sensor)"),
        Some("!=0"),
        Some("value"),
        vec![nucleus::karma::Consequence::AddQuantity { delta: None }],
    )
    .await;
    let pending = reading(&audience, &endpoint, &key, 1, "12");
    engine
        .accept_signal_endpoint_reading(pending.clone())
        .await
        .unwrap();
    assert_eq!(
        store::records::quantity(&engine.store.pool, &target)
            .await
            .unwrap()
            .unwrap()
            .to_string(),
        "12"
    );
    engine
        .accept_signal_endpoint_reading(pending)
        .await
        .unwrap();
    assert_eq!(
        store::records::quantity(&engine.store.pool, &target)
            .await
            .unwrap()
            .unwrap()
            .to_string(),
        "12"
    );
}

#[tokio::test]
async fn replacing_identity_requires_disable_and_invalidates_old_key() {
    let engine = support::engine().await;
    let key = signing_key(1);
    let replacement = signing_key(2);
    let (audience, disabled) = enroll(&engine, &key, "sensor").await;
    let endpoint = enable(&engine, &disabled, true).await;
    let old = reading(&audience, &endpoint, &key, 1, "12");
    let save = |endpoint: &Endpoint| Action::SignalEndpoints {
        request: Request::Save {
            endpoint: Some(endpoint.uid.clone()),
            expected_revision: Some(endpoint.revision),
            name: endpoint.name.clone(),
            public_key: public_key(&replacement),
            signal_slug: String::new(),
            enabled: false,
            cooldown_seconds: 60,
            minimum: "-40".into(),
            maximum: "100".into(),
            expires_at: (nucleus::execution::now() + chrono::Duration::days(1)).to_rfc3339(),
        },
    };
    assert!(engine.act(save(&endpoint), None).await.is_err());
    let disabled = enable(&engine, &endpoint, false).await;
    let data = engine
        .act(save(&disabled), None)
        .await
        .unwrap()
        .data
        .unwrap();
    let replacement_endpoint: Endpoint =
        serde_json::from_value(data["endpoints"][0].clone()).unwrap();
    assert_eq!(replacement_endpoint.credential_epoch, 2);
    let enabled = enable(&engine, &replacement_endpoint, true).await;
    assert!(engine.accept_signal_endpoint_reading(old).await.is_err());
    engine
        .accept_signal_endpoint_reading(reading(&audience, &enabled, &replacement, 1, "13"))
        .await
        .unwrap();
    assert_eq!(quantity(&engine, &enabled).await, "13");
}

#[tokio::test]
async fn clients_can_share_a_signal_and_the_reader_uses_the_latest_sample() {
    let engine = support::karma::engine().await;
    let key = signing_key(1);
    let (audience, first) = enroll(&engine, &key, "shared").await;
    let (_, second) = enroll(&engine, &key, "shared").await;
    assert_ne!(first.uid, second.uid);
    assert_eq!(first.signal_uid, second.signal_uid);
    let first = enable(&engine, &first, true).await;
    let second = enable(&engine, &second, true).await;
    engine
        .accept_signal_endpoint_reading(reading(&audience, &first, &key, 1, "12"))
        .await
        .unwrap();
    engine
        .accept_signal_endpoint_reading(reading(&audience, &second, &key, 1, "23"))
        .await
        .unwrap();
    let preview = engine
        .act(
            Action::PreviewKarmaReading {
                source: "signal(@shared)".into(),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(preview.data.unwrap()["value"], "23");
    assert_eq!(quantity(&engine, &first).await, "23");
    for name in ["job-a", "job-b"] {
        engine
            .act(
                Action::SignalEndpoints {
                    request: Request::Save {
                        endpoint: None,
                        expected_revision: None,
                        name: name.into(),
                        public_key: String::new(),
                        signal_slug: first.signal_uid.clone(),
                        enabled: false,
                        cooldown_seconds: 1,
                        minimum: "0".into(),
                        maximum: "100".into(),
                        expires_at: (nucleus::execution::now() + chrono::Duration::days(1))
                            .to_rfc3339(),
                    },
                },
                None,
            )
            .await
            .unwrap();
    }
    assert_eq!(
        inspect(&engine).await["endpoints"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}

#[tokio::test]
async fn restart_keeps_audience_receipt_and_cooldown() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", directory.path().join("lince.db").display());
    let engine = Engine::open(&url).await.unwrap();
    store::organs::ensure_local(&engine.store.pool, "http://test")
        .await
        .unwrap();
    let key = signing_key(1);
    let (audience, disabled) = enroll(&engine, &key, "sensor").await;
    let endpoint = enable(&engine, &disabled, true).await;
    let pending = reading(&audience, &endpoint, &key, 1, "12");
    let receipt: Receipt = engine
        .accept_signal_endpoint_reading(pending.clone())
        .await
        .unwrap();
    engine.store.pool.close().await;
    drop(engine);
    let engine = Engine::open(&url).await.unwrap();
    assert_eq!(inspect(&engine).await["audience"], audience);
    assert_eq!(
        engine
            .accept_signal_endpoint_reading(pending)
            .await
            .unwrap(),
        receipt
    );
    assert_eq!(
        engine
            .accept_signal_endpoint_reading(reading(&audience, &endpoint, &key, 2, "13"))
            .await
            .unwrap_err()
            .code(),
        Some("signal_endpoint_cooldown")
    );
}

#[tokio::test]
async fn ordinary_users_cannot_inspect_or_change_endpoint_grants_and_stale_edits_fail() {
    let engine = support::engine().await;
    let permissions: Vec<_> = utils::auth::ALL_PERMISSIONS
        .iter()
        .map(|p| (p.subject, p.action))
        .collect();
    store::seed::seed(&engine.store.pool, &permissions)
        .await
        .unwrap();
    let role = store::auth::ensure_role(&engine.store.pool, "reader")
        .await
        .unwrap();
    let hash = utils::auth::hash_password("password").unwrap();
    store::auth::create_person_login(&engine.store.pool, "Reader", "reader", &hash, role)
        .await
        .unwrap();
    let user = store::auth::user_by_username(&engine.store.pool, "reader")
        .await
        .unwrap()
        .unwrap();
    assert!(
        engine
            .act(
                Action::SignalEndpoints {
                    request: Request::Inspect
                },
                Some(user.uid.clone())
            )
            .await
            .is_err()
    );
    let (_, disabled) = enroll(&engine, &signing_key(1), "sensor").await;
    assert!(
        engine
            .act(
                Action::SignalEndpoints {
                    request: Request::IssueToken {
                        endpoint: disabled.uid.clone(),
                        expected_revision: disabled.revision,
                    }
                },
                Some(user.uid.clone())
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .act(
                Action::SignalEndpoints {
                    request: Request::SetEnabled {
                        endpoint: disabled.uid.clone(),
                        expected_revision: disabled.revision,
                        enabled: true
                    }
                },
                Some(user.uid.clone())
            )
            .await
            .is_err()
    );
    enable(&engine, &disabled, true).await;
    assert!(
        engine
            .act(
                Action::SignalEndpoints {
                    request: Request::SetEnabled {
                        endpoint: disabled.uid,
                        expected_revision: disabled.revision,
                        enabled: false
                    }
                },
                None
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn bearer_tokens_bind_existing_signals_rotate_and_keep_retry_limits() {
    use engine::signal_endpoints::Submission;
    let clock = nucleus::execution::Execution::new([9; 32], 1_900_000_000_000).unwrap();
    clock
        .scope(async {
            let engine = support::engine().await;
            let signal = engine
                .act(
                    Action::CreateRecord {
                        slug: Some("external-count".into()),
                        kind: nucleus::RecordKind::Signal,
                        head: "External count".into(),
                        body: String::new(),
                        quantity: 0.0,
                    },
                    None,
                )
                .await
                .unwrap()
                .created
                .unwrap();
            let data = engine
                .act(
                    Action::SignalEndpoints {
                        request: Request::Save {
                            endpoint: None,
                            expected_revision: None,
                            name: "Automation".into(),
                            public_key: String::new(),
                            signal_slug: signal.clone(),
                            enabled: false,
                            cooldown_seconds: 60,
                            minimum: "0".into(),
                            maximum: "100".into(),
                            expires_at: (clock.now() + chrono::Duration::days(1)).to_rfc3339(),
                        },
                    },
                    None,
                )
                .await
                .unwrap()
                .data
                .unwrap();
            let endpoint: Endpoint = serde_json::from_value(data["endpoints"][0].clone()).unwrap();
            assert_eq!(endpoint.signal_uid, signal);
            let endpoint = enable(&engine, &endpoint, true).await;
            let issued = engine
                .act(
                    Action::SignalEndpoints {
                        request: Request::IssueToken {
                            endpoint: endpoint.uid.clone(),
                            expected_revision: endpoint.revision,
                        },
                    },
                    None,
                )
                .await
                .unwrap()
                .data
                .unwrap();
            let token = issued["issued_token"]["token"].as_str().unwrap();
            assert_eq!(token.len(), 64);
            let endpoint: Endpoint =
                serde_json::from_value(issued["endpoints"][0].clone()).unwrap();
            assert!(inspect(&engine).await["issued_token"].is_null());
            let stored: String = store::sqlx::query_scalar(
                "SELECT digest FROM signal_endpoint_token WHERE endpoint_uid = ?",
            )
            .bind(&endpoint.uid)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
            assert_ne!(stored, token);
            let audit: Vec<String> =
                store::sqlx::query_scalar("SELECT configuration FROM signal_endpoint_audit")
                    .fetch_all(&engine.store.pool)
                    .await
                    .unwrap();
            assert!(audit.iter().all(|row| !row.contains(token)));
            let submission = |sequence, value: &str| Submission {
                sequence,
                issued_at: clock.now().timestamp(),
                value: value.into(),
            };
            assert!(
                engine
                    .accept_signal_token(&nucleus::new_uid("r"), token, submission(1, "12"))
                    .await
                    .is_err()
            );
            assert!(
                engine
                    .accept_signal_token(&signal, &"00".repeat(32), submission(1, "12"))
                    .await
                    .is_err()
            );
            assert!(
                engine
                    .accept_signal_token(&signal, token, submission(1, "101"))
                    .await
                    .is_err()
            );
            let pending = submission(1, "12");
            let receipt = engine
                .accept_signal_token(&signal, token, pending.clone())
                .await
                .unwrap();
            assert_eq!(
                engine
                    .accept_signal_token(&signal, token, pending.clone())
                    .await
                    .unwrap(),
                receipt
            );
            assert_eq!(
                engine
                    .accept_signal_token(&signal, token, submission(1, "13"))
                    .await
                    .unwrap_err()
                    .code(),
                Some("signal_endpoint_replay")
            );
            assert_eq!(
                engine
                    .accept_signal_token(&signal, token, submission(2, "13"))
                    .await
                    .unwrap_err()
                    .code(),
                Some("signal_endpoint_cooldown")
            );
            let facts: Vec<Option<String>> =
                store::sqlx::query_scalar("SELECT payload FROM fact WHERE record_uid = ?")
                    .bind(&signal)
                    .fetch_all(&engine.store.pool)
                    .await
                    .unwrap();
            assert!(
                facts
                    .iter()
                    .flatten()
                    .all(|payload| !payload.contains(token))
            );
            let replacement = engine
                .act(
                    Action::SignalEndpoints {
                        request: Request::IssueToken {
                            endpoint: endpoint.uid.clone(),
                            expected_revision: endpoint.revision,
                        },
                    },
                    None,
                )
                .await
                .unwrap()
                .data
                .unwrap();
            let new_token = replacement["issued_token"]["token"].as_str().unwrap();
            assert_ne!(new_token, token);
            assert!(
                engine
                    .accept_signal_token(&signal, token, pending)
                    .await
                    .is_err()
            );
            clock.set_time(1_900_000_060_000).unwrap();
            engine
                .accept_signal_token(&signal, new_token, submission(1, "13"))
                .await
                .unwrap();
            assert_eq!(quantity(&engine, &endpoint).await, "13");
            let endpoint: Endpoint =
                serde_json::from_value(replacement["endpoints"][0].clone()).unwrap();
            let endpoint = enable(&engine, &endpoint, false).await;
            assert!(
                engine
                    .accept_signal_token(&signal, new_token, submission(1, "13"))
                    .await
                    .is_err()
            );
            enable(&engine, &endpoint, true).await;
            clock.set_time(1_900_086_400_000).unwrap();
            assert!(
                engine
                    .accept_signal_token(&signal, new_token, submission(1, "13"))
                    .await
                    .is_err()
            );
        })
        .await;
}

#[tokio::test]
async fn administrative_permissions_do_not_bypass_signal_read_filters_or_creation_policy() {
    let engine = support::engine().await;
    let permissions: Vec<_> = utils::auth::ALL_PERMISSIONS
        .iter()
        .map(|p| (p.subject, p.action))
        .collect();
    store::seed::seed(&engine.store.pool, &permissions)
        .await
        .unwrap();
    let role = store::auth::ensure_role(&engine.store.pool, "endpoint-manager")
        .await
        .unwrap();
    for (subject, action) in [
        ("permission", "assign"),
        ("configuration", "update"),
        ("organ", "update"),
        ("record", "create"),
        ("record", "update"),
        ("record", "read"),
    ] {
        let permission = store::auth::ensure_permission(&engine.store.pool, subject, action)
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, permission)
            .await
            .unwrap();
    }
    let hash = utils::auth::hash_password("password").unwrap();
    store::auth::create_person_login(&engine.store.pool, "Manager", "manager", &hash, role)
        .await
        .unwrap();
    let user = store::auth::user_by_username(&engine.store.pool, "manager")
        .await
        .unwrap()
        .unwrap();
    let (_, endpoint) = enroll(&engine, &signing_key(1), "sensor").await;
    let visible = support::plain(&engine, "visible", 0.0).await;
    let filter = serde_json::to_string(&protein::Predicate::UidEq(visible)).unwrap();
    store::read_filter::set(&engine.store.pool, &user.uid, Some(&filter))
        .await
        .unwrap();
    let data = engine
        .act(
            Action::SignalEndpoints {
                request: Request::Inspect,
            },
            Some(user.uid.clone()),
        )
        .await
        .unwrap()
        .data
        .unwrap();
    assert!(data["endpoints"].as_array().unwrap().is_empty());
    assert!(
        engine
            .act(
                Action::SignalEndpoints {
                    request: Request::SetEnabled {
                        endpoint: endpoint.uid,
                        expected_revision: endpoint.revision,
                        enabled: true,
                    }
                },
                Some(user.uid.clone())
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .act(
                Action::SignalEndpoints {
                    request: Request::Save {
                        endpoint: None,
                        expected_revision: None,
                        name: "Unseen".into(),
                        public_key: public_key(&signing_key(2)),
                        signal_slug: "unseen".into(),
                        enabled: false,
                        cooldown_seconds: 60,
                        minimum: "0".into(),
                        maximum: "100".into(),
                        expires_at: (nucleus::execution::now() + chrono::Duration::days(1))
                            .to_rfc3339(),
                    }
                },
                Some(user.uid)
            )
            .await
            .is_err()
    );
    assert!(
        store::records::resolve(&engine.store.pool, "unseen")
            .await
            .unwrap()
            .is_none()
    );
}

#[test]
fn signing_format_is_unambiguous_and_rejects_noncanonical_numbers() {
    let key = signing_key(1);
    let mut reading = Reading {
        audience: "00".repeat(32),
        endpoint: "r_00000000000000000000000001".into(),
        signal: "r_00000000000000000000000002".into(),
        credential_epoch: 1,
        sequence: 1,
        issued_at: 1900000000,
        value: "12.5".into(),
        signature: String::new(),
    };
    let bytes = reading.signing_payload().unwrap();
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        format!(
            "lince-signal-endpoint-v1\n{}\nr_00000000000000000000000001\n1\nr_00000000000000000000000002\n1\n1900000000\n12.5\n",
            "00".repeat(32)
        )
    );
    assert_eq!(sign(reading.clone(), &key).signature.len(), 128);
    for value in ["NaN", "1e2", "+1", "01", "1\n2", "-0"] {
        reading.value = value.into();
        assert!(reading.signing_payload().is_err(), "{value}");
    }
}
