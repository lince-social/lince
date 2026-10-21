use super::*;
use engine::{EngineError, social::Network};
use nucleus::{
    social::{Command as Social, ask::ContactConsent, gossip},
    trust_analysis::{
        custody::forwarding as model,
        discovery::{Question, Request},
    },
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct Wire {
    source: String,
    target_endpoint: String,
    target: Arc<Engine>,
    offline: AtomicBool,
    calls: AtomicUsize,
    frames: Mutex<Vec<Value>>,
}

#[async_trait::async_trait]
impl Network for Wire {
    async fn request(&self, endpoint: &str, request: PublicRequest) -> Result<Value, EngineError> {
        assert_eq!(endpoint, self.target_endpoint);
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.frames
            .lock()
            .unwrap()
            .push(serde_json::to_value(&request).unwrap());
        if self.offline.load(Ordering::SeqCst) {
            return Err(EngineError::Consequence(
                "scripted disconnected peer".into(),
            ));
        }
        self.target
            .social_public_frame(
                &self.source,
                endpoint,
                &serde_json::to_vec(&request).unwrap(),
                nucleus::execution::now().timestamp(),
            )
            .await
    }
}

async fn setup_target(f: &Fixture) -> (Arc<Engine>, support::Person, String, String, Arc<Wire>) {
    let target = Arc::new(support::engine().await);
    let operator = support::person(&target, "forwarded-custody-operator").await;
    authorize(&target, &operator).await;
    let role = store::auth::ensure_role(&target.store.pool, "Contact evidence owner")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&target.store.pool, "organ", "update")
        .await
        .unwrap();
    store::auth::grant(&target.store.pool, role, permission)
        .await
        .unwrap();
    let (organ, endpoint, _) =
        super::super::serving::peer(&f.receiver, &target, &f.owner, 124).await;
    store::organs::add_contact(
        &target.store.pool,
        &f.receiver_organ,
        None,
        "Known forwarding source",
        "",
        1,
    )
    .await
    .unwrap();
    store::organs::set_trust(&target.store.pool, &f.receiver_organ, "known")
        .await
        .unwrap();
    store::organs::set_node_id(
        &target.store.pool,
        &f.receiver_organ,
        Some(&f.receiver_endpoint),
    )
    .await
    .unwrap();
    store::replica::make_own_root(&target.store.pool, &f.receiver_organ)
        .await
        .unwrap();
    store::visibility::grant(
        &target.store.pool,
        "actor",
        Some(&operator.uid),
        &f.receiver_organ,
    )
    .await
    .unwrap();
    target.publish_root_key(&f.receiver_root).await.unwrap();
    target
        .adopt_roster(
            &f.receiver
                .roster_of(&f.receiver_organ)
                .await
                .unwrap()
                .unwrap(),
        )
        .await
        .unwrap();
    recognize(&target, &f.author).await;
    recognize(&target, &f.owner).await;
    for (node, requests) in [
        (
            &f.receiver,
            vec![
                Social::SetAskContact {
                    choice: ContactConsent {
                        organ: organ.clone(),
                        ask: true,
                        answer: true,
                        forward: false,
                    },
                },
                Social::ConfigureGossip { enabled: true },
                Social::SetGossipContact {
                    choice: gossip::ContactConsent {
                        organ: organ.clone(),
                        send: true,
                        receive: false,
                    },
                },
            ],
        ),
        (
            target.as_ref(),
            vec![
                Social::ConfigureAsk { enabled: true },
                Social::SetAskContact {
                    choice: ContactConsent {
                        organ: f.receiver_organ.clone(),
                        ask: false,
                        answer: true,
                        forward: false,
                    },
                },
            ],
        ),
    ] {
        for request in requests {
            node.social_command(request, None, nucleus::execution::now())
                .await
                .unwrap();
        }
    }
    let policy = Policy {
        enabled: true,
        senders: all_known(),
        readers: all_known(),
        ..Default::default()
    };
    target
        .act(
            action(
                &operator,
                "forward:receiver-policy",
                Command::Custody {
                    request: Custody::SetPolicy {
                        policy,
                        expected_version: 0,
                    },
                },
            ),
            Some(operator.uid.clone()),
        )
        .await
        .unwrap();
    let wire = Arc::new(Wire {
        source: f.receiver_endpoint.clone(),
        target_endpoint: endpoint.clone(),
        target: target.clone(),
        offline: AtomicBool::new(false),
        calls: AtomicUsize::new(0),
        frames: Mutex::new(Vec::new()),
    });
    f.receiver.attach_social_network(wire.clone());
    (target, operator, organ, endpoint, wire)
}

async fn enable_forwarding(f: &Fixture, peer: &str) {
    let policy = Policy {
        enabled: true,
        senders: all_known(),
        readers: all_known(),
        forwarding: model::Policy {
            enabled: true,
            peers: Filter {
                include: vec![Condition {
                    identities: vec![Identity::Organ(peer.into())],
                    ..Default::default()
                }],
                exclude: vec![],
            },
        },
        ..Default::default()
    };
    f.receiver
        .act(
            action(
                &f.owner,
                "forward:review-policy",
                Command::Custody {
                    request: Custody::SetPolicy {
                        policy,
                        expected_version: 1,
                    },
                },
            ),
            Some(f.owner.uid.clone()),
        )
        .await
        .unwrap();
}

async fn cached_paths(node: &Engine, operator: &support::Person, f: &Fixture) -> Value {
    let mut policy =
        nucleus::trust_analysis::paths::Policy::for_anchor(Identity::Person(f.author.uid.clone()));
    policy.allow_unilateral = true;
    policy.targets.include.push(Condition {
        identities: vec![Identity::Person(f.owner.uid.clone())],
        ..Default::default()
    });
    node.act(
        action(
            operator,
            &nucleus::new_uid("cached-path"),
            Command::ContactPaths {
                policy,
                as_organ: false,
            },
        ),
        Some(operator.uid.clone()),
    )
    .await
    .unwrap()
    .data
    .unwrap()["trust"]["result"]
        .clone()
}

async fn cached_claim_withdrawal(f: &Fixture, target: &Engine, operator: &support::Person) {
    use nucleus::trust_analysis::{
        claims::{self, Authorship, Comparator, Descriptor, Metric},
        query::{CounterpartyBasis, EvidenceLevel, Query},
    };
    let now = f.now;
    let mut claim = claims::Document {
        statement: claims::Statement {
            category: Default::default(),
            title: "Public activity".into(),
            issuer: Identity::Person(f.author.uid.clone()),
            claim: nucleus::new_uid("r"),
            revision: 1,
            descriptor: Descriptor {
                subject: Identity::Person(f.author.uid.clone()),
                role: nucleus::transfer::outcome::Role::Giver,
                measurement: nucleus::transfer::receipt::Measurement {
                    meaning: nucleus::vocabulary::reference::MeaningReference {
                        vocabulary_uid: nucleus::new_uid("g"),
                        concept_uid: nucleus::new_uid("c"),
                        revision: 1,
                        definition_hash: "a".repeat(64),
                    },
                    unit: None,
                    object_commitment: None,
                },
                level: EvidenceLevel::NeedLinkedReceiver,
                metric: Metric::Quantity,
                counterparty_basis: CounterpartyBasis::Organ,
                from: None,
                until: None,
            },
            comparator: Comparator::AtLeast,
            amount: nucleus::DecimalValue::parse_inferred("100").unwrap(),
            as_of: now,
            issued_at: now,
            expires_at: now + 3600,
            withdrawn: false,
            audience: Audience::Public,
            authorship: Authorship::Manual,
        },
        key_id: String::new(),
        public_key: String::new(),
        signature: String::new(),
    };
    engine::trust_analysis::sign_claim_document(&mut claim, &f.author.signer).unwrap();
    let imported = target
        .act(
            action(
                operator,
                "cached-claim:import",
                Command::Claims {
                    request: claims::commands::Command::Import {
                        document: serde_json::to_string(&claim).unwrap(),
                        as_organ: false,
                    },
                },
            ),
            Some(operator.uid.clone()),
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let inspect = |id: &str| {
        action(
            operator,
            id,
            Command::Claims {
                request: claims::commands::Command::Inspect {
                    record: imported.clone(),
                    as_organ: false,
                },
            },
        )
    };
    let old = target
        .act(inspect("cached-claim:before"), Some(operator.uid.clone()))
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(old["trust"]["current_retained"], true);
    claim.statement.revision = 2;
    claim.statement.withdrawn = true;
    claim.statement.amount = nucleus::DecimalValue::parse_inferred("0").unwrap();
    claim.signature.clear();
    engine::trust_analysis::sign_claim_document(&mut claim, &f.author.signer).unwrap();
    let artifact = Artifact::Claim(claim);
    let offer = f.offer(&artifact);
    let id = f.begin(&offer).await;
    f.send(&id, &artifact).await;
    f.wire(PublicRequest::CustodyFinish { upload: id })
        .await
        .unwrap();
    assert_eq!(f.receiver.social_gossip_once().await.unwrap(), 1);
    let old = target
        .act(inspect("cached-claim:after"), Some(operator.uid.clone()))
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(old["trust"]["current_retained"], false);
    let checking = target
        .act(
            action(
                operator,
                "cached-claim:check",
                Command::Claims {
                    request: claims::commands::Command::Check {
                        record: imported,
                        sources: Query::for_subject(Identity::Person(f.author.uid.clone())),
                        as_organ: false,
                    },
                },
            ),
            Some(operator.uid.clone()),
        )
        .await;
    assert!(checking.is_err());
}

#[test]
fn consented_gossip_forwards_public_originals_keeps_controls_and_discovers_retained_copies_without_import()
 {
    support::run_async_test("evidence-forwarding", || async {
        let f = fixture(true).await;
        let (target, operator, organ, endpoint, wire) = setup_target(&f).await;
        let offer = f.offer(&f.artifact);
        let id = f.begin(&offer).await;
        f.send(&id, &f.artifact).await;
        f.wire(PublicRequest::CustodyFinish { upload: id })
            .await
            .unwrap();
        assert_eq!(f.receiver.social_gossip_once().await.unwrap(), 0);
        assert_eq!(wire.calls.load(Ordering::SeqCst), 0);
        enable_forwarding(&f, &organ).await;
        assert_eq!(f.receiver.social_gossip_once().await.unwrap(), 1);
        let copied: Vec<u8> = store::sqlx::query_scalar(
            "SELECT body FROM evidence_custody_payload WHERE artifact_hash=?",
        )
        .bind(offer.artifact.as_str())
        .fetch_one(&target.store.pool)
        .await
        .unwrap();
        assert_eq!(copied, f.artifact.bytes().unwrap());
        let state: String =
            store::sqlx::query_scalar("SELECT state FROM record_evidence_forwarding")
                .fetch_one(&f.receiver.store.pool)
                .await
                .unwrap();
        assert_eq!(state, "stored");
        assert_eq!(f.receiver.social_gossip_once().await.unwrap(), 0);
        let records: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM contact_disclosure_record")
                .fetch_one(&target.store.pool)
                .await
                .unwrap();
        assert_eq!(records, 0);
        assert_eq!(
            cached_paths(&target, &operator, &f).await["paths"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let now = nucleus::execution::now().timestamp();
        let request = Request {
            id: nucleus::new_uid("ask"),
            question: Question::Contacts {
                subject: Identity::Person(f.author.uid.clone()),
            },
            issued_at: now,
            deadline: now + 15,
            after: None,
        };
        let page = target
            .social_public_frame(
                &f.receiver_endpoint,
                &endpoint,
                &serde_json::to_vec(&PublicRequest::DiscoverEvidence {
                    document: Box::new(request),
                })
                .unwrap(),
                now,
            )
            .await
            .unwrap();
        assert_eq!(page["page"]["headers"][0]["hash"], offer.artifact.as_str());
        let mut withdrawn = f.artifact.clone();
        if let Artifact::Contact(document) = &mut withdrawn {
            document.statement.revision = 2;
            document.statement.withdrawn = true;
            document.signature.clear();
            engine::trust_analysis::sign_contact_document(document, &f.author.signer).unwrap();
        }
        let withdrawal = f.offer(&withdrawn);
        let id = f.begin(&withdrawal).await;
        f.send(&id, &withdrawn).await;
        f.wire(PublicRequest::CustodyFinish { upload: id })
            .await
            .unwrap();
        wire.offline.store(true, Ordering::SeqCst);
        assert_eq!(f.receiver.social_gossip_once().await.unwrap(), 0);
        let pending:i64=store::sqlx::query_scalar("SELECT COUNT(*) FROM record_evidence_forwarding WHERE control=1 AND state='pending' AND attempts=1").fetch_one(&f.receiver.store.pool).await.unwrap();
        assert_eq!(pending, 1);
        wire.offline.store(false, Ordering::SeqCst);
        store::sqlx::query(
            "UPDATE record_evidence_forwarding SET next_attempt=0 WHERE state='pending'",
        )
        .execute(&f.receiver.store.pool)
        .await
        .unwrap();
        assert_eq!(f.receiver.social_gossip_once().await.unwrap(), 1);
        let body: Vec<u8> = store::sqlx::query_scalar(
            "SELECT body FROM evidence_custody_payload WHERE artifact_hash=?",
        )
        .bind(withdrawal.artifact.as_str())
        .fetch_one(&target.store.pool)
        .await
        .unwrap();
        assert_eq!(body, withdrawn.bytes().unwrap());
        assert!(
            cached_paths(&target, &operator, &f).await["paths"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            wire.frames
                .lock()
                .unwrap()
                .iter()
                .filter(|frame| frame["request"] == "custody-begin")
                .count(),
            3
        );
        let credits: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_receipt_record")
                .fetch_one(&target.store.pool)
                .await
                .unwrap();
        assert_eq!(credits, 0);
        cached_claim_withdrawal(&f, &target, &operator).await;
    });
}
