pub mod support;

use engine::{Engine, actions::Action, trust::Signer};
use nucleus::identity::{Command, PersonKey};
use serde_json::Value;

async fn preview(engine: &Engine, key: &PersonKey, actor: Option<String>) -> Value {
    engine
        .act(
            Action::Identity {
                request: Command::PreviewPersonKey { key: key.clone() },
            },
            actor,
        )
        .await
        .unwrap()
        .data
        .unwrap()["identity"]
        .clone()
}

fn accept(value: &Value) -> Action {
    Action::Identity {
        request: Command::AcceptPersonKey {
            key: serde_json::from_value(value["key"].clone()).unwrap(),
            expected_profile_revision: value["profile_revision"].as_i64().unwrap(),
            expected_review: serde_json::from_value(value["review_hash"].clone()).unwrap(),
        },
    }
}

#[test]
fn key_recognition_is_reviewed_immutable_and_does_not_change_profiles_or_quantities() {
    support::run_async_test("identity-key-review", || async {
        let engine = support::engine().await;
        let person = support::person(&engine, "identity.subject").await;
        let signer = Signer::generate(&person.uid, "external-person-v1");
        let key = PersonKey {
            subject: person.uid.clone(),
            key_id: signer.key_id.clone(),
            public_key: signer.public_key_b64(),
        };
        let baseline:(i64,i64,String) = store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),organ_uid FROM record WHERE uid=?").bind(&person.uid).fetch_one(&engine.store.pool).await.unwrap();
        let watched = engine.watch_query_changes();
        let value = preview(&engine, &key, None).await;
        assert!(!watched.has_changed().unwrap());
        assert!(
            engine::trust::keys_of(&engine.store, &person.uid)
                .await
                .unwrap()
                .is_empty()
        );
        let mut wrong = value.clone();
        wrong["key"]["key_id"] = serde_json::json!("changed-after-review");
        assert!(engine.act(accept(&wrong), None).await.is_err());
        let recognized = engine.act(accept(&value), None).await.unwrap();
        assert!(recognized.facts.is_empty());
        engine.act(accept(&value), None).await.unwrap();
        assert_eq!(
            engine::trust::keys_of(&engine.store, &person.uid)
                .await
                .unwrap()
                .len(),
            1
        );
        let after:(i64,i64,String) = store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),organ_uid FROM record WHERE uid=?").bind(&person.uid).fetch_one(&engine.store.pool).await.unwrap();
        assert_eq!(after, baseline);
        let mut replacement = key.clone();
        replacement.public_key = Signer::generate(&person.uid, "other").public_key_b64();
        assert!(
            engine
                .act(
                    Action::Identity {
                        request: Command::PreviewPersonKey { key: replacement }
                    },
                    None
                )
                .await
                .is_err()
        );
        let keys = engine
            .act(
                Action::Identity {
                    request: Command::PersonKeys {
                        subject: person.uid.clone(),
                    },
                },
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap()["identity"]
            .clone();
        assert_eq!(keys["keys"][0]["fingerprint"], value["fingerprint"]);
        assert_eq!(keys["keys"][0]["revoked"], false);
        let (revoked, signature) = engine.revocation_certificate(&signer);
        assert!(
            engine
                .adopt_revocation(&person.uid, &revoked, &signature)
                .await
                .unwrap()
        );
        assert!(engine.act(accept(&value), None).await.is_err());
        let keys = engine
            .act(
                Action::Identity {
                    request: Command::PersonKeys {
                        subject: person.uid,
                    },
                },
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap()["identity"]
            .clone();
        assert_eq!(keys["keys"][0]["revoked"], true);
    });
}

#[test]
fn profile_changes_permissions_and_visibility_invalidate_key_acceptance() {
    support::run_async_test("identity-key-access", || async {
        let engine = support::engine().await;
        let subject = support::person(&engine, "identity.profile").await;
        let key = PersonKey {
            subject: subject.uid.clone(),
            key_id: "reviewed-v1".into(),
            public_key: subject.signer.public_key_b64(),
        };
        let value = preview(&engine, &key, None).await;
        engine
            .act(
                Action::SetSlug {
                    target: subject.uid.clone(),
                    slug: Some("identity.renamed".into()),
                },
                None,
            )
            .await
            .unwrap();
        assert!(engine.act(accept(&value), None).await.is_err());
        let fresh = preview(&engine, &key, None).await;
        assert_eq!(fresh["fingerprint"], value["fingerprint"]);
        assert_ne!(fresh["review_hash"], value["review_hash"]);
        let viewer = support::person(&engine, "identity.viewer").await;
        let role = store::auth::ensure_role(&engine.store.pool, "Identity comparison reader")
            .await
            .unwrap();
        let read = store::auth::ensure_permission(&engine.store.pool, "record", "read")
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, read)
            .await
            .unwrap();
        store::auth::create_credential(&engine.store.pool, &viewer.uid, &viewer.uid, "hash", role)
            .await
            .unwrap();
        store::visibility::grant(&engine.store.pool, "actor", Some(&viewer.uid), &subject.uid)
            .await
            .unwrap();
        let current = store::data_visibility::policy(
            &engine.store.pool,
            &subject.uid,
            nucleus::visibility::Data::Record,
        )
        .await
        .unwrap();
        let controller = current.as_ref().and_then(|policy| {
            (!policy.controller_uid.is_empty()).then(|| policy.controller_uid.clone())
        });
        let expected = current.as_ref().map_or(0, |policy| policy.revision);
        let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
        let visibility_revision = store::data_visibility::save_on(
            &mut tx,
            &subject.uid,
            nucleus::visibility::Data::Record,
            controller.as_deref(),
            &nucleus::visibility::Policy {
                include: vec![nucleus::visibility::Condition {
                    persons: vec![viewer.uid.clone()],
                    ..Default::default()
                }],
                exclude: vec![],
            },
            expected,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let value = preview(&engine, &key, Some(viewer.uid.clone())).await;
        assert_eq!(
            engine
                .act(accept(&value), Some(viewer.uid.clone()))
                .await
                .unwrap_err()
                .code(),
            Some("forbidden")
        );
        let admin = store::auth::ensure_permission(&engine.store.pool, "organ", "update")
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, admin)
            .await
            .unwrap();
        engine
            .act(accept(&value), Some(viewer.uid.clone()))
            .await
            .unwrap();
        let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
        store::data_visibility::save_on(
            &mut tx,
            &subject.uid,
            nucleus::visibility::Data::Record,
            controller.as_deref(),
            &nucleus::visibility::Policy {
                include: vec![],
                exclude: vec![],
            },
            visibility_revision,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            engine
                .act(accept(&value), Some(viewer.uid))
                .await
                .unwrap_err()
                .code(),
            Some("forbidden")
        );
        let local = store::organs::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap();
        assert!(
            engine
                .act(
                    Action::Identity {
                        request: Command::PersonKeys { subject: local.uid }
                    },
                    None
                )
                .await
                .is_err()
        );
        let mut malformed = key.clone();
        malformed.public_key = "not-an-ed25519-key".into();
        assert!(
            engine
                .act(
                    Action::Identity {
                        request: Command::PreviewPersonKey { key: malformed }
                    },
                    None
                )
                .await
                .is_err()
        );
        let mut weak = [0u8; 32];
        weak[0] = 1;
        use base64::Engine as _;
        let weak = PersonKey {
            key_id: "weak-candidate-v1".into(),
            public_key: base64::engine::general_purpose::STANDARD.encode(weak),
            ..key
        };
        assert!(
            engine
                .act(
                    Action::Identity {
                        request: Command::PreviewPersonKey { key: weak }
                    },
                    None
                )
                .await
                .is_err()
        );
    });
}
