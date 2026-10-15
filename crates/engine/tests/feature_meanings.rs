use engine::{Engine, actions::Action};
use nucleus::{RecordKind, vocabulary::Command};
use serde_json::json;

async fn command(engine: &Engine, request: Command) -> engine::actions::ActionOutcome {
    engine
        .act(Action::Vocabulary { request }, None)
        .await
        .unwrap()
}

async fn concept(engine: &Engine, name: &str) -> String {
    command(
        engine,
        Command::CreateConcept {
            vocabulary: "g_local".into(),
            name: name.into(),
            language: "en".into(),
            definition: format!("The chosen meaning of {name}"),
            derived_from: None,
        },
    )
    .await
    .created
    .unwrap()
}

async fn record(engine: &Engine, kind: RecordKind, head: &str) -> String {
    store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind,
            head,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid
}

#[tokio::test]
async fn assignment_accepts_selected_identities_and_keeps_earlier_assignments_when_write_meaning_changes()
 {
    let engine = Engine::open_memory().await.unwrap();
    let task = record(&engine, RecordKind::Plain, "Task").await;
    let one = record(&engine, RecordKind::Person, "One").await;
    let two = record(&engine, RecordKind::Person, "Two").await;
    let original = store::vocabulary::feature_write(&engine.store.pool, "assignment")
        .await
        .unwrap();
    engine
        .act(
            Action::AssertRecord {
                subject: task.clone(),
                predicate: original.clone(),
                object: Some(one.clone()),
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    let custom = concept(&engine, "responsável").await;
    let preview = command(
        &engine,
        Command::PreviewFeature {
            feature: "assignment".into(),
            concept: custom.clone(),
            enabled: true,
            write: true,
        },
    )
    .await
    .data
    .unwrap()["feature_preview"]
        .clone();
    assert_eq!(preview["can_apply"], true);
    command(
        &engine,
        Command::BindFeature {
            feature: "assignment".into(),
            concept: custom.clone(),
            enabled: true,
            write: true,
            expected_revision: preview["before"]["revision"].as_i64().unwrap(),
        },
    )
    .await;
    engine
        .act(
            Action::AssertRecord {
                subject: task.clone(),
                predicate: "feature:assignment".into(),
                object: Some(two),
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    command(
        &engine,
        Command::UseWording {
            concept: custom.clone(),
            wording: "In charge".into(),
        },
    )
    .await;
    let query=serde_json::from_value(json!({"source":"record","where":[{"uid_eq":task}],"fields":["uid","assignees","assertions"]})).unwrap();
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    assert_eq!(rows[0]["assignees"].as_array().unwrap().len(), 2);
    assert!(
        rows[0]["assertions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|assertion| assertion["predicate_uid"] == custom
                && assertion["predicate"] == "In charge")
    );
    assert!(
        engine
            .act(
                Action::Vocabulary {
                    request: Command::BindFeature {
                        feature: "assignment".into(),
                        concept: original,
                        enabled: false,
                        write: false,
                        expected_revision: 1
                    }
                },
                None
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn incompatible_claims_and_unary_assignments_are_rejected() {
    let engine = Engine::open_memory().await.unwrap();
    let task = record(&engine, RecordKind::Plain, "Task").await;
    let custom = concept(&engine, "custom tag").await;
    engine
        .act(
            Action::AssertRecord {
                subject: task.clone(),
                predicate: custom.clone(),
                object: None,
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    let preview = command(
        &engine,
        Command::PreviewFeature {
            feature: "assignment".into(),
            concept: custom.clone(),
            enabled: true,
            write: true,
        },
    )
    .await
    .data
    .unwrap()["feature_preview"]
        .clone();
    assert_eq!(preview["incompatible_assertions"], 1);
    assert_eq!(preview["can_apply"], false);
    assert!(
        engine
            .act(
                Action::Vocabulary {
                    request: Command::BindFeature {
                        feature: "assignment".into(),
                        concept: custom,
                        enabled: true,
                        write: true,
                        expected_revision: 1
                    }
                },
                None
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .act(
                Action::AssertRecord {
                    subject: task,
                    predicate: "feature:assignment".into(),
                    object: None,
                    quantity: None,
                    unit: None
                },
                None
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn containment_uses_all_selected_meanings_without_depends_on_display_terms() {
    let engine = Engine::open_memory().await.unwrap();
    let parent = record(&engine, RecordKind::Plain, "Parent").await;
    let child = record(&engine, RecordKind::Plain, "Child").await;
    let custom = concept(&engine, "inside").await;
    command(
        &engine,
        Command::BindFeature {
            feature: "containment".into(),
            concept: custom.clone(),
            enabled: true,
            write: true,
            expected_revision: 1,
        },
    )
    .await;
    engine
        .act(
            Action::AssertRecord {
                subject: child.clone(),
                predicate: custom.clone(),
                object: Some(parent.clone()),
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    command(
        &engine,
        Command::UseWording {
            concept: custom,
            wording: "Dentro".into(),
        },
    )
    .await;
    let query = serde_json::from_value(
        json!({"source":"record","where":[{"under":{"record":parent}}],"fields":["uid"]}),
    )
    .unwrap();
    assert_eq!(
        protein::execute(&engine.store, &query).await.unwrap()[0]["uid"],
        child
    );
    let grandchild = record(&engine, RecordKind::Plain, "Grandchild").await;
    let selected_write = store::vocabulary::feature_write(&engine.store.pool, "containment")
        .await
        .unwrap();
    let original = store::vocabulary::feature_binding(&engine.store.pool, "containment")
        .await
        .unwrap()
        .concept_uids
        .into_iter()
        .find(|uid| uid != &selected_write)
        .unwrap();
    engine
        .act(
            Action::AssertRecord {
                subject: grandchild,
                predicate: original.clone(),
                object: Some(child),
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    let bound = protein::bind_meanings(&engine.store, &query, None)
        .await
        .unwrap();
    assert_eq!(
        protein::execute(&engine.store, &bound).await.unwrap().len(),
        2
    );
    command(
        &engine,
        Command::BindFeature {
            feature: "containment".into(),
            concept: original,
            enabled: false,
            write: false,
            expected_revision: 2,
        },
    )
    .await;
    assert_eq!(
        protein::execute(&engine.store, &bound).await.unwrap().len(),
        2
    );
    assert_eq!(
        protein::execute(&engine.store, &query).await.unwrap().len(),
        1
    );
}
