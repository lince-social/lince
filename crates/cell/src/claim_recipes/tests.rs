use super::*;
use base64::Engine as _;
use engine::{actions::Action, trust::Signer};
use nucleus::trust_analysis::{
    Identity,
    claims::{
        self,
        recipes::{self, commands::Command as Request},
    },
    commands::Command,
    contacts::Audience,
    query::{CounterpartyBasis, EvidenceLevel, Query},
};

fn action(person: &str, id: &str, request: Request) -> Action {
    Action::TrustAnalysis {
        person: Some(person.into()),
        request_id: id.into(),
        request: Command::Claims {
            request: claims::commands::Command::Recipes { request },
        },
    }
}

async fn wait_output(engine: &Engine, record: &str) -> String {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let output: Option<String> = store::sqlx::query_scalar(
                "SELECT output_record_uid FROM profile_claim_recipe_runtime WHERE record_uid=?",
            )
            .bind(record)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
            if let Some(output) = output {
                return output;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn supervisor_publishes_without_a_person_private_key_coalesces_wakes_and_stops() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let organ = store::organs::ensure_local(&engine.store.pool, "http://recipe-service.test")
        .await
        .unwrap();
    let cell = store::cells::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap();
    let person = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Person,
                head: "Recipe controller".into(),
                body: String::new(),
                quantity: 1.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let person_key = Signer::generate(&person, "recipe-controller");
    engine.set_signer(person_key.clone()).await.unwrap();
    let role = store::auth::ensure_role(&engine.store.pool, "Recipe controller")
        .await
        .unwrap();
    for operation in ["read", "create", "update"] {
        let permission = store::auth::ensure_permission(&engine.store.pool, "record", operation)
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, permission)
            .await
            .unwrap();
    }
    store::auth::create_credential(&engine.store.pool, &person, &person, "test", role)
        .await
        .unwrap();
    let root = Signer::generate(&organ.uid, engine::roster::ROOT_KEY_ID);
    engine.publish_root_key(&root).await.unwrap();
    let publisher = Signer::generate(&organ.uid, &engine::roster::cell_key_id(&cell.uid));
    engine.set_organ_signer(publisher.clone()).await.unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(publisher.public_key_b64())
        .unwrap();
    let node_id = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    engine
        .publish_roster(
            &root,
            vec![engine::roster::CellEntry {
                cell_uid: cell.uid,
                node_id,
                label: "Publishing Cell".into(),
                operational_key: publisher.public_key_b64(),
                sealing_key: None,
                front_door: false,
                capabilities: engine::roster::full_capabilities(),
            }],
        )
        .await
        .unwrap();
    let descriptor = claims::Descriptor {
        subject: Identity::Person(person.clone()),
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
        metric: claims::Metric::Quantity,
        counterparty_basis: CounterpartyBasis::Organ,
        from: None,
        until: None,
    };
    let mut inputs = Query::for_subject(Identity::Person(person.clone()));
    inputs.rely_on_all_visible = true;
    let draft = recipes::commands::Draft {
        claim: claims::commands::Draft {
            category: Default::default(),
            issuer: Identity::Person(person.clone()),
            claim: None,
            title: "Local automatic history".into(),
            descriptor,
            comparator: claims::Comparator::AtLeast,
            amount: String::new(),
            as_of: None,
            lifetime_seconds: 3600,
            expected_revision: 0,
            withdrawn: false,
            audience: Audience::Public,
            key_id: person_key.key_id.clone(),
            public_key: person_key.public_key_b64(),
        },
        recipe: None,
        expected_revision: 0,
        inputs,
        permit_seconds: 86400,
        refresh_seconds: 60,
        start_enabled: true,
    };
    let preview = engine
        .act(
            action(&person, "service:preview", Request::Preview { draft }),
            Some(person.clone()),
        )
        .await
        .unwrap()
        .data
        .unwrap();
    let mut review: recipes::commands::Review =
        serde_json::from_value(preview["trust"]["review"].clone()).unwrap();
    engine::trust_analysis::sign_recipe_permit(&mut review.recipe.permit, &person_key).unwrap();
    let record = engine
        .act(
            action(
                &person,
                "service:save",
                Request::Save {
                    review,
                    expected_review: serde_json::from_value(preview["trust"]["hash"].clone())
                        .unwrap(),
                },
            ),
            Some(person.clone()),
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine.set_signer(publisher).await.unwrap();
    let worker = spawn(engine.clone());
    let output = wait_output(&engine, &record).await;
    let saved =
        store::profile_claims::get_on(&mut *engine.store.pool.acquire().await.unwrap(), &output)
            .await
            .unwrap();
    assert!(saved.document.statement.withdrawn);
    assert_eq!(saved.fact.actor_uid.as_deref(), Some(organ.uid.as_str()));
    let checked: String = store::sqlx::query_scalar(
        "SELECT checked_at FROM profile_claim_recipe_runtime WHERE record_uid=?",
    )
    .bind(&record)
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "An unrelated local edit".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let latest: String = store::sqlx::query_scalar(
                "SELECT checked_at FROM profile_claim_recipe_runtime WHERE record_uid=?",
            )
            .bind(&record)
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
            if latest != checked {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let outputs: i64 = store::sqlx::query_scalar(
        "SELECT COUNT(*) FROM profile_claim_record WHERE generation_recipe_uid=?",
    )
    .bind(&record)
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(outputs, 1);
    worker.abort();
    assert!(worker.await.unwrap_err().is_cancelled());
}
