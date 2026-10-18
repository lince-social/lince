use super::*;
use nucleus::{
    social::{Command as Social, PublicRequest, ask, gossip},
    trust_analysis::{
        Condition, Filter, Identity, IdentityContext,
        commands::Command as Trust,
        custody::{Command as Custody, Policy, forwarding},
        evidence,
        query::{self, Query},
        serving::{
            Artifact, Command as Serving, Request as ServeRequest, Scope as ServeScope,
            Source as ServeSource, SourceKind,
        },
    },
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

struct Wire {
    source: String,
    endpoint: String,
    target: Arc<Engine>,
    offline: AtomicBool,
}

#[async_trait::async_trait]
impl engine::social::Network for Wire {
    async fn request(
        &self,
        endpoint: &str,
        request: PublicRequest,
    ) -> Result<Value, engine::EngineError> {
        assert_eq!(endpoint, self.endpoint);
        if self.offline.load(Ordering::SeqCst) {
            return Err(engine::EngineError::Consequence(
                "scripted relay outage".into(),
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

fn known() -> Filter {
    Filter {
        include: vec![Condition {
            known: Some(true),
            ..Default::default()
        }],
        exclude: vec![],
    }
}

async fn administrator(node: &Engine, person: &Person) {
    let role = store::auth::ensure_role(&node.store.pool, "Custody topology operator")
        .await
        .unwrap();
    for (resource, operation) in [
        ("record", "read"),
        ("record", "create"),
        ("record", "update"),
        ("record", "delete"),
        ("organ", "update"),
    ] {
        let permission = store::auth::ensure_permission(&node.store.pool, resource, operation)
            .await
            .unwrap();
        store::auth::grant(&node.store.pool, role, permission)
            .await
            .unwrap();
    }
    store::auth::create_credential(&node.store.pool, &person.uid, &person.uid, "fixture", role)
        .await
        .unwrap();
    let mut tx = store::write_tx(&node.store.pool).await.unwrap();
    let mut roles = store::person_roles::ids_on(&mut tx, &person.uid).await.unwrap();
    let access = store::auth::person_access_on(&mut tx, &person.uid).await.unwrap().unwrap();
    roles.push(role);
    store::person_roles::replace_on(&mut tx, &person.uid, &roles, access.revision).await.unwrap();
    tx.commit().await.unwrap();
    node.set_signer(person.signer.clone()).await.unwrap();
}

async fn connect(node: &Engine, owner: &Person, other: &Engine, endpoint: &str) {
    node.adopt_introduction(&other.introduction().await.unwrap(), 1)
        .await
        .unwrap();
    let organ = store::organs::local(&other.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    store::organs::set_trust(&node.store.pool, &organ, "known")
        .await
        .unwrap();
    store::organs::set_node_id(&node.store.pool, &organ, Some(endpoint))
        .await
        .unwrap();
    store::replica::make_own_root(&node.store.pool, &organ)
        .await
        .unwrap();
    store::visibility::grant(&node.store.pool, "actor", Some(&owner.uid), &organ)
        .await
        .unwrap();
    node.adopt_roster(&other.roster_of(&organ).await.unwrap().unwrap()).await.unwrap();
    for command in [
        Social::ConfigureAsk { enabled: true },
        Social::SetAskContact {
            choice: ask::ContactConsent {
                organ,
                ask: true,
                answer: true,
                forward: false,
            },
        },
    ] {
        node.social_command(command, None, nucleus::execution::now())
            .await
            .unwrap();
    }
}

async fn policy(node: &Engine, owner: &Person, target: Option<&str>) {
    let policy = Policy {
        enabled: true,
        senders: known(),
        readers: known(),
        forwarding: forwarding::Policy {
            enabled: target.is_some(),
            peers: Filter {
                include: target
                    .map(|uid| {
                        vec![Condition {
                            identities: vec![Identity::Organ(uid.into())],
                            ..Default::default()
                        }]
                    })
                    .unwrap_or_default(),
                exclude: vec![],
            },
        },
        ..Default::default()
    };
    node.act(
        Action::TrustAnalysis {
            person: Some(owner.uid.clone()),
            request_id: nucleus::new_uid("custody-topology"),
            request: Trust::Custody {
                request: Custody::SetPolicy {
                    policy,
                    expected_version: 0,
                },
            },
        },
        Some(owner.uid.clone()),
    )
    .await
    .unwrap();
    if let Some(target) = target {
        for request in [
            Social::ConfigureGossip { enabled: true },
            Social::SetGossipContact {
                choice: gossip::ContactConsent {
                    organ: target.into(),
                    send: true,
                    receive: false,
                },
            },
        ] {
            node.social_command(request, None, nucleus::execution::now())
                .await
                .unwrap();
        }
    }
}

async fn approve(node: &Engine, owner: &Person, record: &str) {
    node.set_signer(owner.signer.clone()).await.unwrap();
    let make = |request| Action::TrustAnalysis {
        person: Some(owner.uid.clone()),
        request_id: nucleus::new_uid("custody-source"),
        request: Trust::Serving { request },
    };
    let preview = node
        .act(
            make(Serving::Preview {
                request: ServeRequest {
                    source: ServeSource {
                        record: record.into(),
                        kind: SourceKind::Receipt,
                    },
                    scope: ServeScope::Fixed,
                    as_organ: false,
                    audience: known(),
                    lifetime_seconds: 3600,
                },
            }),
            Some(owner.uid.clone()),
        )
        .await
        .unwrap()
        .data
        .unwrap()["trust"]
        .clone();
    node.act(
        make(Serving::Publish {
            review: Box::new(serde_json::from_value(preview["review"].clone()).unwrap()),
            expected_review: serde_json::from_value(preview["hash"].clone()).unwrap(),
        }),
        Some(owner.uid.clone()),
    )
    .await
    .unwrap();
}

async fn copied(node: &Engine) -> Document {
    let bytes:Vec<u8>=store::sqlx::query_scalar("SELECT p.body FROM evidence_custody_payload p JOIN evidence_custody_commitment c ON c.artifact_hash=p.artifact_hash ORDER BY c.sequence DESC LIMIT 1").fetch_one(&node.store.pool).await.unwrap();
    let Artifact::Receipt(document) =
        Artifact::parse(&bytes, nucleus::execution::now().timestamp()).unwrap()
    else {
        panic!("Expected an unchanged receipt original")
    };
    node.verify_transfer_receipt_document(&document, nucleus::execution::now())
        .await
        .unwrap();
    *document
}

fn amounts(
    local: &Document,
    local_record: &str,
    local_version: u64,
    copied: &Document,
) -> Vec<String> {
    use nucleus::transfer::receipt_history::Provenance;
    let now = nucleus::execution::now().timestamp();
    let proofs = evidence::receipts(
        &[
            evidence::Receipt {
                document: local,
                provenance: Provenance::Record {
                    record: local_record.into(),
                    version: local_version,
                },
            },
            evidence::Receipt {
                document: copied,
                provenance: Provenance::Artifact {
                    hash: Artifact::Receipt(Box::new(copied.clone())).hash().unwrap(),
                },
            },
        ],
        now,
    )
    .unwrap();
    let scope = &copied.public_bundle().unwrap().publication.scope;
    let mut query = Query::for_subject(Identity::Organ(scope.giver.clone()));
    query.issuers.include.push(Condition {
        identities: vec![Identity::Organ(scope.receiver.clone())],
        ..Default::default()
    });
    let contexts = BTreeMap::from_iter(
        [
            scope.source.origin.clone(),
            scope.giver.clone(),
            scope.receiver.clone(),
        ]
        .map(|uid| {
            (
                Identity::Organ(uid),
                IdentityContext {
                    known: true,
                    readable: true,
                    ..Default::default()
                },
            )
        }),
    );
    query::analyze(
        proofs
            .iter()
            .map(evidence::ReceiptGroup::source_evidence)
            .collect(),
        &contexts,
        None,
        &query,
    )
    .unwrap()
    .groups
    .into_iter()
    .filter_map(|group| group.raw_quantity.map(|quantity| quantity.canonical()))
    .collect()
}

async fn local_analysis(node: &Engine, owner: &Person, original: &Document) -> Value {
    let scope = &original.public_bundle().unwrap().publication.scope;
    let mut query = Query::for_subject(Identity::Organ(scope.giver.clone()));
    query.issuers.include.push(Condition {
        identities: vec![Identity::Organ(scope.receiver.clone())],
        ..Default::default()
    });
    node.set_signer(owner.signer.clone()).await.unwrap();
    let result = node
        .act(
            Action::TrustAnalysis {
                person: Some(owner.uid.clone()),
                request_id: nucleus::new_uid("cached-receipt-query"),
                request: Trust::AnalyzeQuery {
                    query,
                    as_organ: false,
                },
            },
            Some(owner.uid.clone()),
        )
        .await
        .unwrap()
        .data
        .unwrap();
    result["trust"]["analysis"].clone()
}

async fn local_amounts(node: &Engine, owner: &Person, original: &Document) -> Vec<String> {
    local_analysis(node, owner, original).await["groups"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|group| group["raw_quantity"]["value"].as_str().map(str::to_owned))
        .collect()
}

pub(super) async fn forwarded_receipts(
    setup: &PublicationFixture,
    source_record: &str,
    receiver_record: &str,
    received: &str,
    local: &Engine,
    local_record: &str,
) {
    let f = &setup.fixture;
    administrator(&f.engine, &f.giver).await;
    let relay = Arc::new(support::engine().await);
    let target = Arc::new(support::engine().await);
    let _relay_authority = Authorizer::new(&relay, 94).await;
    let _target_authority = Authorizer::new(&target, 95).await;
    let relay_owner = support::person(&relay, "receipt.relay.operator").await;
    let target_owner = support::person(&target, "receipt.target.operator").await;
    administrator(&relay, &relay_owner).await;
    administrator(&target, &target_owner).await;
    let source_endpoint = iroh::SecretKey::from_bytes(&[91; 32]).public().to_string();
    let relay_endpoint = iroh::SecretKey::from_bytes(&[94; 32]).public().to_string();
    let target_endpoint = iroh::SecretKey::from_bytes(&[95; 32]).public().to_string();
    connect(&f.engine, &f.giver, &relay, &relay_endpoint).await;
    connect(&relay, &relay_owner, &f.engine, &source_endpoint).await;
    connect(&relay, &relay_owner, &target, &target_endpoint).await;
    connect(&target, &target_owner, &relay, &relay_endpoint).await;
    connect(&target, &target_owner, &f.engine, &source_endpoint).await;
    let relay_organ = store::organs::local(&relay.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let target_organ = store::organs::local(&target.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    policy(&f.engine, &f.giver, Some(&relay_organ)).await;
    policy(&relay, &relay_owner, Some(&target_organ)).await;
    policy(&target, &target_owner, None).await;
    let first = Arc::new(Wire {
        source: source_endpoint,
        endpoint: relay_endpoint.clone(),
        target: relay.clone(),
        offline: AtomicBool::new(false),
    });
    let second = Arc::new(Wire {
        source: relay_endpoint,
        endpoint: target_endpoint,
        target: target.clone(),
        offline: AtomicBool::new(false),
    });
    f.engine.attach_social_network(first.clone());
    relay.attach_social_network(second.clone());
    approve(&f.engine, &f.giver, source_record).await;
    assert_eq!(f.engine.social_gossip_once().await.unwrap(), 1);
    assert_eq!(relay.social_gossip_once().await.unwrap(), 1);
    let local_document = document(local, local_record).await;
    let local_version = store::transfer_receipts::get(&local.store.pool, local_record)
        .await
        .unwrap()
        .version;
    let initial = copied(&target).await;
    assert_eq!(
        amounts(&local_document, local_record, local_version, &initial),
        vec!["12.000000000000000001"]
    );
    assert_eq!(
        local_amounts(&target, &target_owner, &initial).await,
        vec!["12.000000000000000001"]
    );
    let mut previous = received.to_owned();
    let mut receiver_record = receiver_record.to_owned();
    for (index, amount, withdrawn) in [(0, "4", false), (1, "0", true)] {
        f.engine
            .set_signer(f.receiver.signer.clone())
            .await
            .unwrap();
        let mut statement = f.statement(
            Role::Receiver,
            amount,
            Some(&setup.binding),
            Some(&previous),
        );
        if let nucleus::transfer::outcome::Request::State {
            withdrawn: flag, ..
        } = &mut statement
        {
            *flag = withdrawn;
        }
        let outcome = f
            .engine
            .act(
                f.action(
                    &f.receiver,
                    &format!("custody:correction-{index}"),
                    statement,
                ),
                Some(f.receiver.uid.clone()),
            )
            .await
            .unwrap()
            .created
            .unwrap();
        receiver_record = setup
            .publish(
                &receiver_record,
                &outcome,
                Role::Receiver,
                &format!("custody:publish-{index}"),
            )
            .await;
        let merged = import(
            &f.engine,
            &f.giver,
            &format!("custody:merge-{index}"),
            &document(&f.engine, &receiver_record).await,
        )
        .await;
        approve(&f.engine, &f.giver, &merged).await;
        assert_eq!(f.engine.social_gossip_once().await.unwrap(), 1);
        second.offline.store(true, Ordering::SeqCst);
        assert_eq!(relay.social_gossip_once().await.unwrap(), 0);
        second.offline.store(false, Ordering::SeqCst);
        store::sqlx::query(
            "UPDATE record_evidence_forwarding SET next_attempt=0 WHERE state='pending'",
        )
        .execute(&relay.store.pool)
        .await
        .unwrap();
        assert_eq!(relay.social_gossip_once().await.unwrap(), 1);
        let current = copied(&target).await;
        let measured = amounts(&local_document, local_record, local_version, &current);
        assert_eq!(
            local_amounts(&target, &target_owner, &current).await,
            measured
        );
        if withdrawn {
            assert!(measured.is_empty());
        } else {
            assert_eq!(measured, vec!["4"]);
            let hidden: String = store::sqlx::query_scalar("SELECT record_uid FROM evidence_custody_commitment ORDER BY sequence DESC LIMIT 1").fetch_one(&target.store.pool).await.unwrap();
            target.act(Action::DeleteRecord {target:hidden.clone()},Some(target_owner.uid.clone())).await.unwrap();
            let before: (i64,i64,i64,i64) = store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM identity_key),(SELECT COUNT(*) FROM transfer_occurrence)").fetch_one(&target.store.pool).await.unwrap();
            let result = local_analysis(&target,&target_owner,&current).await;
            assert!(result["groups"].as_array().unwrap().is_empty());
            assert!(!serde_json::to_string(&result).unwrap().contains(&hidden));
            let after: (i64,i64,i64,i64) = store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM identity_key),(SELECT COUNT(*) FROM transfer_occurrence)").fetch_one(&target.store.pool).await.unwrap();
            assert_eq!(before,after);
        }
        for private in [
            &f.stock,
            &f.selection.need,
            &f.transfer.transfer,
            &f.giver.uid,
            &f.receiver.uid,
        ] {
            assert!(!serde_json::to_string(&current).unwrap().contains(private));
        }
        previous = outcome;
    }
    for node in [relay.as_ref(), target.as_ref()] {
        let credits: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_receipt_record")
                .fetch_one(&node.store.pool)
                .await
                .unwrap();
        assert_eq!(credits, 0);
        let occurrences: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM transfer_occurrence")
                .fetch_one(&node.store.pool)
                .await
                .unwrap();
        assert_eq!(occurrences, 0);
    }
}
