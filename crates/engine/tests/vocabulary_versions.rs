use engine::{Engine, actions::Action};
use nucleus::{RecordKind, vocabulary::Command};
use serde_json::json;

async fn command(engine: &Engine, request: Command) -> engine::actions::ActionOutcome {
    engine
        .act(Action::Vocabulary { request }, None)
        .await
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
async fn vocabulary_commands_publish_adopt_revise_and_retain_saved_assertion_meaning() {
    let engine = Engine::open_memory().await.unwrap();
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let vocabulary = command(
        &engine,
        Command::CreateVocabulary {
            name: "Kitchen".into(),
            organ: organ.clone(),
        },
    )
    .await
    .created
    .unwrap();
    let concept = command(
        &engine,
        Command::CreateConcept {
            vocabulary: vocabulary.clone(),
            name: "meal".into(),
            language: "en".into(),
            definition: "One prepared meal".into(),
            derived_from: None,
        },
    )
    .await
    .created
    .unwrap();
    let subject = record(&engine, RecordKind::Plain, "Lunch").await;
    let assertion = engine
        .act(
            Action::AssertRecord {
                subject: subject.clone(),
                predicate: concept.clone(),
                object: None,
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    command(
        &engine,
        Command::ReviseDefinition {
            concept: concept.clone(),
            expected_revision: 1,
            definition: "One prepared meal for one person".into(),
        },
    )
    .await;
    assert_eq!(
        store::assertions::get(&engine.store.pool, &assertion)
            .await
            .unwrap()
            .unwrap()
            .predicate_revision,
        1
    );
    let revision = store::linguas::get(&engine.store.pool, &vocabulary)
        .await
        .unwrap()
        .unwrap()
        .revision;
    let outcome = command(
        &engine,
        Command::Publish {
            vocabulary: vocabulary.clone(),
            expected_revision: revision,
        },
    )
    .await;
    let publication = outcome.created.unwrap();
    let package = outcome.data.unwrap()["publication"].to_string();
    let other = Engine::open_memory().await.unwrap();
    let other_organ = store::organs::local(&other.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    command(
        &other,
        Command::PreviewPackage {
            package: package.clone(),
        },
    )
    .await;
    command(
        &other,
        Command::AdoptPackage {
            package,
            organ: other_organ,
        },
    )
    .await;
    command(
        &engine,
        Command::Withdraw {
            publication: publication.clone(),
        },
    )
    .await;
    let saved = command(&engine, Command::InspectPublication { publication }).await;
    assert_eq!(
        saved.data.unwrap()["publication"]["dictionaries"][0]["definitions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let sources = store::vocabulary::sources(&other.store.pool).await.unwrap();
    assert_eq!(sources[&concept].definition, "One prepared meal");
    assert!(
        other
            .act(
                Action::Vocabulary {
                    request: Command::ReviseDefinition {
                        concept,
                        expected_revision: 2,
                        definition: "Overwrite another Organ".into()
                    }
                },
                None
            )
            .await
            .is_err()
    );
    let history = command(
        &engine,
        Command::RecordHistory {
            record: subject,
            offset: 0,
        },
    )
    .await;
    assert_eq!(
        history.data.unwrap()["history"]["versions"][0]["content"]["head"],
        "Lunch"
    );
}

#[tokio::test]
async fn readers_cannot_publish_or_revise_and_record_history_obeys_read_policies() {
    let engine = Engine::open_memory().await.unwrap();
    let actor = record(&engine, RecordKind::Person, "Reader").await;
    let visible = record(&engine, RecordKind::Plain, "Visible").await;
    let hidden = record(&engine, RecordKind::Plain, "Hidden").await;
    let role = store::auth::ensure_role(&engine.store.pool, "History readers")
        .await
        .unwrap();
    let read = store::auth::ensure_permission(&engine.store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, read)
        .await
        .unwrap();
    engine
        .act(
            Action::AssignRoles {
                person: actor.clone(),
                roles: vec!["History readers".into()],
                expected_revision: 0,
            },
            None,
        )
        .await
        .unwrap();
    engine
        .act(
            Action::SetRolePolicy {
                role: "History readers".into(),
                expected_revision: 0,
                policy: json!({"read":{"uid_eq":visible},"grants":[]}),
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        engine
            .act(
                Action::Vocabulary {
                    request: Command::RecordHistory {
                        record: visible,
                        offset: 0
                    }
                },
                Some(actor.clone())
            )
            .await
            .is_ok()
    );
    assert!(
        engine
            .act(
                Action::Vocabulary {
                    request: Command::RecordHistory {
                        record: hidden,
                        offset: 0
                    }
                },
                Some(actor.clone())
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .act(
                Action::Vocabulary {
                    request: Command::Publish {
                        vocabulary: store::linguas::LOCAL_UID.into(),
                        expected_revision: 1
                    }
                },
                Some(actor.clone())
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .act(
                Action::Vocabulary {
                    request: Command::ReviseDefinition {
                        concept: nucleus::vocabulary::TOPOS_UID.into(),
                        expected_revision: 1,
                        definition: "Changed".into()
                    }
                },
                Some(actor)
            )
            .await
            .is_err()
    );
}
