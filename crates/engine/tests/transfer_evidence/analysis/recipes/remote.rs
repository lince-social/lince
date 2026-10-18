use super::*;
use nucleus::{
    social::{self, PublicRequest},
    transfer::receipt::{self, Content, Document},
    trust_analysis::{
        remote::{self as model, Command as Remote},
        serving::{self, Command as Serving},
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Host {
    engine: Arc<Engine>,
    source: String,
    requests: AtomicUsize,
}

#[async_trait::async_trait]
impl engine::social::Network for Host {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, engine::EngineError> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        self.engine.social_public_frame(&self.source, destination, &serde_json::to_vec(&request).unwrap(), nucleus::execution::now().timestamp()).await
    }
}

async fn serve(host: &Engine, owner: &Person, document: Document) {
    host.set_signer(owner.signer.clone()).await.unwrap();
    let record = host.act(Action::TransferReceipt {person:Some(owner.uid.clone()),request_id:nucleus::new_uid("fragment-import"),request:receipt::Command::Import {document:serde_json::to_string(&document).unwrap()}}, Some(owner.uid.clone())).await.unwrap().created.unwrap();
    let execute = |request| Action::TrustAnalysis {person:Some(owner.uid.clone()),request_id:nucleus::new_uid("fragment-serving"),request:Command::Serving {request}};
    let preview = host.act(execute(Serving::Preview {request:serving::Request {source:serving::Source {record,kind:serving::SourceKind::Receipt},scope:serving::Scope::Fixed,as_organ:false,audience:nucleus::trust_analysis::Filter {include:vec![nucleus::trust_analysis::Condition::default()],exclude:vec![]},lifetime_seconds:3600}}), Some(owner.uid.clone())).await.unwrap().data.unwrap()["trust"].clone();
    host.act(execute(Serving::Publish {review:Box::new(serde_json::from_value(preview["review"].clone()).unwrap()),expected_review:serde_json::from_value(preview["hash"].clone()).unwrap()}), Some(owner.uid.clone())).await.unwrap();
}

async fn permit(node: &Engine, person: &Person) {
    let role = store::auth::ensure_role(&node.store.pool, "Distributed evidence reader").await.unwrap();
    for operation in ["read", "create"] {
        let permission = store::auth::ensure_permission(&node.store.pool, "record", operation).await.unwrap();
        store::auth::grant(&node.store.pool, role, permission).await.unwrap();
    }
    store::auth::create_credential(&node.store.pool, &person.uid, &person.uid, "fixture", role).await.unwrap();
    node.set_signer(person.signer.clone()).await.unwrap();
}

async fn peer(node: &Engine, owner: &Person, other: &Engine, endpoint: &str) {
    node.adopt_introduction(&other.introduction().await.unwrap(), 1).await.unwrap();
    let organ = store::organs::local(&other.store.pool).await.unwrap().unwrap().uid;
    store::organs::set_trust(&node.store.pool, &organ, "known").await.unwrap();
    store::organs::set_node_id(&node.store.pool, &organ, Some(endpoint)).await.unwrap();
    store::replica::make_own_root(&node.store.pool, &organ).await.unwrap();
    store::visibility::grant(&node.store.pool, "actor", Some(&owner.uid), &organ).await.unwrap();
    node.adopt_roster(&other.roster_of(&organ).await.unwrap().unwrap()).await.unwrap();
    for request in [social::Command::ConfigureAsk {enabled:true},social::Command::SetAskContact {choice:social::ask::ContactConsent {organ,ask:true,answer:true,forward:false}}] {
        node.social_command(request,None,nucleus::execution::now()).await.unwrap();
    }
}

fn command(person: &Person, request: Remote) -> Action {
    Action::TrustAnalysis {
        person: Some(person.uid.clone()),
        request_id: nucleus::new_uid("remote-test"),
        request: Command::Remote { request },
    }
}

async fn run(node: &Engine, person: &Person, request: Remote) -> Value {
    node.act(command(person, request), Some(person.uid.clone()))
        .await
        .unwrap()
        .data
        .unwrap()["trust"]
        .clone()
}

pub(super) async fn verify_original_receipt_fragments(
    source: &Engine,
    origin: &Engine,
    query: &Query,
) {
    let node = support::engine().await;
    let _authority = Authorizer::new(&node, 77).await;
    let viewer = support::person(&node, "remote.fragments.viewer").await;
    permit(&node,&viewer).await;
    let host = Arc::new(support::engine().await);
    let _host_authority = Authorizer::new(&host, 79).await;
    let host_owner = support::person(&host,"fragment.host").await;
    permit(&host,&host_owner).await;
    for other in [source, origin] {
        node.adopt_introduction(&other.introduction().await.unwrap(), 1)
            .await
            .unwrap();
        host.adopt_introduction(&other.introduction().await.unwrap(), 1).await.unwrap();
    }
    let source_organ = store::organs::local(&host.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    store::organs::set_trust(&node.store.pool, &source_organ, "known")
        .await
        .unwrap();
    let endpoint = iroh::SecretKey::from_bytes(&[79; 32]).public().to_string();
    let reader_endpoint = iroh::SecretKey::from_bytes(&[77; 32]).public().to_string();
    peer(&node,&viewer,&host,&endpoint).await;
    peer(&host,&host_owner,&node,&reader_endpoint).await;
    store::organs::set_node_id(&node.store.pool, &source_organ, Some(&endpoint))
        .await
        .unwrap();
    let receipt: String = store::sqlx::query_scalar("SELECT record_uid FROM transfer_receipt_record WHERE kind='publication' ORDER BY created_at LIMIT 1").fetch_one(&source.store.pool).await.unwrap();
    let saved = store::transfer_receipts::get(&source.store.pool, &receipt)
        .await
        .unwrap();
    let mut fragment = saved.document.public_bundle().unwrap().clone();
    let mut need = fragment.clone();
    need.publication
        .events
        .retain(|signed| matches!(signed.event.content, Content::Need { .. }));
    fragment
        .publication
        .events
        .retain(|signed| !matches!(signed.event.content, Content::Need { .. }));
    assert!(!need.publication.events.is_empty());
    assert!(!fragment.publication.events.is_empty());
    for organ in std::collections::BTreeSet::from([
        &source_organ,
        &fragment.publication.scope.source.origin,
        &fragment.publication.scope.giver,
        &fragment.publication.scope.receiver,
    ]) {
        for (engine, person) in [(&node, &viewer), (host.as_ref(), &host_owner)] {
            store::replica::make_own_root(&engine.store.pool, organ)
                .await
                .unwrap();
            store::visibility::grant(&engine.store.pool, "actor", Some(&person.uid), organ)
                .await
                .unwrap();
        }
    }
    for request in [
        social::Command::ConfigureAsk { enabled: true },
        social::Command::SetAskContact {
            choice: social::ask::ContactConsent {
                organ: source_organ,
                ask: true,
                answer: false,
                forward: false,
            },
        },
    ] {
        node.social_command(request, None, nucleus::execution::now())
            .await
            .unwrap();
    }
    serve(&host,&host_owner,Document::ProofFragment(fragment)).await;
    let network = Arc::new(Host {
        engine:host.clone(),
        source:reader_endpoint,
        requests: AtomicUsize::new(0),
    });
    node.attach_social_network(network.clone());
    let reviewed = run(
        &node,
        &viewer,
        Remote::Preview {
            draft: model::Draft {
                query: query.clone(),
                as_organ: false,
                linked_claims: Vec::new(),
                include_contacts: false,
                receipt_sources: Vec::new(),
                contact_subjects: Vec::new(),
            },
        },
    )
    .await;
    assert_eq!(reviewed["kind"], "remote_review", "{reviewed}");
    let started = run(
        &node,
        &viewer,
        Remote::Start {
            review: Box::new(serde_json::from_value(reviewed["review"].clone()).unwrap()),
            expected_review: serde_json::from_value(reviewed["hash"].clone()).unwrap(),
        },
    )
    .await;
    let id = started["job"].as_str().unwrap();
    node.refresh_trust_requests_once().await.unwrap();
    let progress = run(&node, &viewer, Remote::Poll { job: id.into() }).await;
    assert!(progress["endpoints"].as_array().unwrap().iter().all(|endpoint| endpoint["state"] == "answered"), "{progress}");
    let incomplete = run(&node, &viewer, Remote::Verify { job: id.into() }).await;
    assert_eq!(incomplete["receipts"].as_array().unwrap().len(), 1, "{incomplete}");
    assert_eq!(incomplete["receipts"][0]["partial"], true);
    assert!(incomplete["analysis"]["groups"].as_array().unwrap().is_empty());
    assert!(!incomplete["analysis"]["withheld"].as_array().unwrap().is_empty());
    let initial: (i64,i64,i64,i64) = store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM identity_key),(SELECT COUNT(*) FROM transfer_occurrence)").fetch_one(&node.store.pool).await.unwrap();
    let traffic = network.requests.load(Ordering::SeqCst);
    serve(&host,&host_owner,Document::ProofFragment(need.clone())).await;
    let refreshed = run(
        &node,
        &viewer,
        Remote::Preview {
            draft: model::Draft {
                query: query.clone(),
                as_organ: false,
                linked_claims: Vec::new(),
                include_contacts: false,
                receipt_sources: serde_json::from_value(incomplete["receipt_sources"].clone())
                    .unwrap(),
                contact_subjects: Vec::new(),
            },
        },
    )
    .await;
    assert!(
        refreshed["review"]["questions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|question| question["kind"] == "receipt_source")
    );
    assert_eq!(network.requests.load(Ordering::SeqCst), traffic);
    run(&node, &viewer, Remote::Forget { job: id.into() }).await;
    let started = run(
        &node,
        &viewer,
        Remote::Start {
            review: Box::new(serde_json::from_value(refreshed["review"].clone()).unwrap()),
            expected_review: serde_json::from_value(refreshed["hash"].clone()).unwrap(),
        },
    )
    .await;
    let id = started["job"].as_str().unwrap();
    node.refresh_trust_requests_once().await.unwrap();
    let complete = run(&node, &viewer, Remote::Verify { job: id.into() }).await;
    assert_eq!(complete["receipts"][0]["partial"], false);
    assert_eq!(complete["analysis"]["groups"][0]["raw_quantity"]["value"], "12.000000000000000001");
    assert_eq!(complete["analysis"]["groups"][0]["occurrences"], 1);
    let unchanged: (i64,i64,i64,i64) = store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM identity_key),(SELECT COUNT(*) FROM transfer_occurrence)").fetch_one(&node.store.pool).await.unwrap();
    assert_eq!(initial, unchanged);
    assert!(network.requests.load(Ordering::SeqCst) > traffic);
    let retained = node
        .act(
            Action::TransferReceipt {
                person: Some(viewer.uid.clone()),
                request_id: "remote:retain-need".into(),
                request: receipt::Command::Import {
                    document: serde_json::to_string(&Document::ProofFragment(need)).unwrap(),
                },
            },
            Some(viewer.uid.clone()),
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let before: (i64,i64,i64,i64) = store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM identity_key),(SELECT COUNT(*) FROM transfer_occurrence)").fetch_one(&node.store.pool).await.unwrap();
    let traffic = network.requests.load(Ordering::SeqCst);
    let merged = run(&node, &viewer, Remote::Verify { job: id.into() }).await;
    assert_eq!(merged["receipts"][0]["partial"], false);
    assert_eq!(merged["analysis"]["groups"][0]["raw_quantity"]["value"], "12.000000000000000001");
    assert_eq!(merged["analysis"]["groups"][0]["occurrences"], 1);
    let protocol_originals = merged["analysis"]["protocol"].as_array().unwrap().iter().flat_map(|row| row["originals"].as_array().unwrap()).collect::<Vec<_>>();
    assert!(protocol_originals.iter().any(|original| original["provenance"] == "record" && original["record"] == retained));
    let reported = merged["analysis"]["rows"].as_array().unwrap().iter().find(|row| row["selected"] == true).unwrap();
    assert!(reported["originals"].as_array().unwrap().iter().any(|original| original["provenance"] == "artifact" && original.get("record").is_none()));
    let copies = merged["receipts"][0]["history"]["outcomes"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|outcome| outcome["copies"].as_array().unwrap())
        .collect::<Vec<_>>();
    assert!(
        copies
            .iter()
            .any(|copy| copy["provenance"] == "record" && copy["record"] == retained)
    );
    assert!(copies.iter().any(|copy| copy["provenance"] == "artifact"
        && copy.get("record").is_none()
        && copy["content"]["quantity"]["value"] == "12.000000000000000001"));
    assert_eq!(network.requests.load(Ordering::SeqCst), traffic);
    let after: (i64,i64,i64,i64) = store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM identity_key),(SELECT COUNT(*) FROM transfer_occurrence)").fetch_one(&node.store.pool).await.unwrap();
    assert_eq!(before, after);
    assert_eq!(after.3, 0);
}
