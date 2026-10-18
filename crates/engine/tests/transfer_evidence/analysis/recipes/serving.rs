use super::*;
use nucleus::{
    social::{Command as Social, PublicRequest, ask::ContactConsent},
    trust_analysis::{
        Condition, Filter,
        discovery::{Question, Request as Discovery},
        serving::{self as model, Artifact, Chunk, Command as Serving, Scope, SourceKind},
    },
};

pub(super) struct Prepared {
    endpoint: String,
    original: nucleus::karma::CanonicalHash,
    request: Discovery,
    approval: String,
    _remote: Engine,
    _authority: Authorizer,
}

fn action(person: &Person, request: Serving) -> Action {
    Action::TrustAnalysis {
        person: Some(person.uid.clone()),
        request_id: nucleus::new_uid("rolling-test"),
        request: Command::Serving { request },
    }
}

pub(super) async fn prepare(
    node: &Engine,
    person: &Person,
    recipe: &str,
    document: &claim::Document,
) -> Prepared {
    let remote = support::engine().await;
    let authority = Authorizer::new(&remote, 76).await;
    let endpoint = iroh::SecretKey::from_bytes(&[76; 32]).public().to_string();
    let organ = authority.root.actor_uid.clone();
    store::organs::add_contact(&node.store.pool, &organ, None, "Independent reader", "", 1)
        .await
        .unwrap();
    store::organs::set_trust(&node.store.pool, &organ, "known")
        .await
        .unwrap();
    store::organs::set_node_id(&node.store.pool, &organ, Some(&endpoint))
        .await
        .unwrap();
    store::replica::make_own_root(&node.store.pool, &organ)
        .await
        .unwrap();
    store::visibility::grant(&node.store.pool, "actor", Some(&person.uid), &organ)
        .await
        .unwrap();
    node.publish_root_key(&authority.root).await.unwrap();
    let roster = remote
        .publish_roster(
            &authority.root,
            vec![engine::roster::CellEntry {
                cell_uid: authority.cell.clone(),
                node_id: endpoint.clone(),
                label: "Independent reader".into(),
                operational_key: authority.writer.public_key_b64(),
                sealing_key: None,
                front_door: false,
                capabilities: engine::roster::full_capabilities(),
            }],
        )
        .await
        .unwrap();
    node.adopt_roster(&roster).await.unwrap();
    for request in [
        Social::ConfigureAsk { enabled: true },
        Social::SetAskContact {
            choice: ContactConsent {
                organ,
                ask: false,
                answer: true,
                forward: false,
            },
        },
    ] {
        node.social_command(request, None, nucleus::execution::now())
            .await
            .unwrap();
    }
    let request = model::Request {
        source: model::Source {
            record: recipe.into(),
            kind: SourceKind::Claim,
        },
        scope: Scope::Recipe,
        as_organ: false,
        audience: Filter {
            include: vec![Condition::default()],
            exclude: Vec::new(),
        },
        lifetime_seconds: 3 * 86400,
    };
    let reviewed = node
        .act(
            action(person, Serving::Preview { request }),
            Some(person.uid.clone()),
        )
        .await
        .unwrap()
        .data
        .unwrap()["trust"]
        .clone();
    let review: model::Review = serde_json::from_value(reviewed["review"].clone()).unwrap();
    assert!(review.expires_at > document.statement.expires_at);
    let approval = node
        .act(
            action(
                person,
                Serving::Publish {
                    review: Box::new(review),
                    expected_review: serde_json::from_value(reviewed["hash"].clone()).unwrap(),
                },
            ),
            Some(person.uid.clone()),
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let original = Artifact::Claim(document.clone()).hash().unwrap();
    let now = nucleus::execution::now().timestamp();
    let request = Discovery {
        id: nucleus::new_uid("ask"),
        question: Question::ClaimSeries {
            issuer: document.statement.issuer.clone(),
            claim: document.statement.claim.clone(),
        },
        issued_at: now,
        deadline: now + 15,
        after: None,
    };
    let prepared = Prepared {
        endpoint,
        original,
        request,
        approval,
        _remote: remote,
        _authority: authority,
    };
    check(node, person, recipe, document, &prepared).await;
    prepared
}

async fn query(
    node: &Engine,
    prepared: &Prepared,
    request: PublicRequest,
) -> Result<Value, engine::EngineError> {
    node.social_public_request(
        &prepared.endpoint,
        &iroh::SecretKey::from_bytes(&[75; 32]).public().to_string(),
        request,
        nucleus::execution::now().timestamp(),
    )
    .await
}

async fn check(
    node: &Engine,
    person: &Person,
    recipe: &str,
    document: &claim::Document,
    prepared: &Prepared,
) {
    let before: (i64,i64,i64) = store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM transfer_occurrence)").fetch_one(&node.store.pool).await.unwrap();
    let now = nucleus::execution::now().timestamp();
    let mut request = prepared.request.clone();
    request.id = nucleus::new_uid("ask");
    request.issued_at = now;
    request.deadline = now + 15;
    let value = query(
        node,
        prepared,
        PublicRequest::DiscoverEvidence {
            document: Box::new(request.clone()),
        },
    )
    .await
    .unwrap();
    let page: nucleus::trust_analysis::discovery::Page =
        serde_json::from_value(value["page"].clone()).unwrap();
    page.validate(&request, now).unwrap();
    assert_eq!(page.headers.len(), 1);
    let hash = Artifact::Claim(document.clone()).hash().unwrap();
    assert_eq!(page.headers[0].hash, hash);
    assert_eq!(page.headers[0].expires_at, document.statement.expires_at);
    let value = query(
        node,
        prepared,
        PublicRequest::FetchEvidence {
            hash: hash.clone(),
            offset: 0,
        },
    )
    .await
    .unwrap();
    let chunk: Chunk = serde_json::from_value(value["chunk"].clone()).unwrap();
    let artifact = model::assemble(&hash, &[chunk], now).unwrap();
    assert_eq!(artifact, Artifact::Claim(document.clone()));
    let bytes = String::from_utf8(artifact.bytes().unwrap()).unwrap();
    for private in [recipe, &person.uid, &prepared.approval] {
        if private != document.statement.issuer.uid() {
            assert!(!bytes.contains(private));
        }
    }
    let after: (i64,i64,i64) = store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM transfer_occurrence)").fetch_one(&node.store.pool).await.unwrap();
    assert_eq!(before, after);
}

pub(super) async fn check_updated(
    node: &Engine,
    person: &Person,
    recipe: &str,
    document: &claim::Document,
    prepared: &Prepared,
) {
    let now = document.statement.issued_at;
    let clock = nucleus::execution::Execution::new([128; 32], now * 1000).unwrap();
    clock
        .scope(async {
            assert_ne!(
                prepared.original,
                Artifact::Claim(document.clone()).hash().unwrap()
            );
            assert!(
                query(
                    node,
                    prepared,
                    PublicRequest::FetchEvidence {
                        hash: prepared.original.clone(),
                        offset: 0
                    }
                )
                .await
                .is_err()
            );
            check(node, person, recipe, document, prepared).await;
        })
        .await;
}
