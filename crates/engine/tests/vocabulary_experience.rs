use engine::{Engine, actions::Action};
use nucleus::{RecordKind, vocabulary::Command};
use serde_json::{Value, json};

async fn command(engine: &Engine, request: Command) -> engine::actions::ActionOutcome {
    engine
        .act(Action::Vocabulary { request }, None)
        .await
        .unwrap()
}

async fn meaning(engine: &Engine) -> (String, String) {
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let vocabulary = command(
        engine,
        Command::CreateVocabulary {
            name: "Work".into(),
            organ,
        },
    )
    .await
    .created
    .unwrap();
    let concept = command(
        engine,
        Command::CreateConcept {
            vocabulary: vocabulary.clone(),
            name: "work".into(),
            language: "en".into(),
            definition: "A chosen unit of work".into(),
            derived_from: None,
        },
    )
    .await
    .created
    .unwrap();
    (vocabulary, concept)
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
async fn measurement_selection_pins_definition_versions_without_transfer_history_or_writes() {
    let engine = Engine::open_memory().await.unwrap();
    let (vocabulary, concept) = meaning(&engine).await;
    let unit = command(
        &engine,
        Command::CreateConcept {
            vocabulary: vocabulary.clone(),
            name: "work unit".into(),
            language: "en".into(),
            definition: "One measured work item".into(),
            derived_from: None,
        },
    )
    .await
    .created
    .unwrap();
    let before: (i64, i64) =
        store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact)")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
    let changes = engine.watch_query_changes();
    let selected = command(
        &engine,
        Command::InspectMeasurement {
            concept: concept.clone(),
            unit: Some(unit.clone()),
        },
    )
    .await;
    assert!(selected.created.is_none());
    assert!(selected.facts.is_empty());
    assert!(!changes.has_changed().unwrap());
    let measurement: nucleus::transfer::receipt::Measurement =
        serde_json::from_value(selected.data.unwrap()["measurement"].clone()).unwrap();
    measurement.validate().unwrap();
    assert_eq!(measurement.meaning.vocabulary_uid, vocabulary);
    assert_eq!(measurement.meaning.concept_uid, concept);
    assert_eq!(measurement.meaning.revision, 1);
    assert_eq!(measurement.unit.as_ref().unwrap().concept_uid, unit);
    let after: (i64, i64) =
        store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact)")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    command(
        &engine,
        Command::ReviseDefinition {
            concept: concept.clone(),
            expected_revision: 1,
            definition: "A revised activity".into(),
        },
    )
    .await;
    let selected = command(
        &engine,
        Command::InspectMeasurement {
            concept: concept.clone(),
            unit: None,
        },
    )
    .await
    .data
    .unwrap();
    let newer: nucleus::transfer::receipt::Measurement =
        serde_json::from_value(selected["measurement"].clone()).unwrap();
    assert_eq!(newer.meaning.revision, 2);
    assert_ne!(
        newer.meaning.definition_hash,
        measurement.meaning.definition_hash
    );
    assert_eq!(measurement.meaning.revision, 1);
    assert!(
        engine
            .act(
                Action::Vocabulary {
                    request: Command::InspectMeasurement {
                        concept: concept.clone(),
                        unit: None
                    }
                },
                Some(nucleus::new_uid("r"))
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .act(
                Action::Vocabulary {
                    request: Command::InspectMeasurement {
                        concept: "unknown meaning".into(),
                        unit: None
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
                Action::Vocabulary {
                    request: Command::InspectMeasurement {
                        concept,
                        unit: Some(nucleus::new_uid("c"))
                    }
                },
                None
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn saved_queries_keep_selected_concepts_after_wording_changes_and_ambiguity() {
    let engine = Engine::open_memory().await.unwrap();
    let (vocabulary, concept) = meaning(&engine).await;
    let subject = record(&engine, RecordKind::Plain, "One work").await;
    engine
        .act(
            Action::SetIdentity {
                subject: subject.clone(),
                predicate: Some(concept.clone()),
            },
            None,
        )
        .await
        .unwrap();
    let saved=engine.act(Action::SaveProtein {slug:"selected-work".into(),head:"Work".into(),ast:json!({"source":"record","where":[{"all":[{"concept_in":"work"}]}],"order":[{"desc":"#work"}]})},None).await.unwrap().created.unwrap();
    let ast = store::records::get_extension(&engine.store.pool, &saved, "lince.protein")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ast["where"][0]["all"][0]["concept_in"], concept);
    assert_eq!(ast["order"][0]["desc"], format!("#{concept}"));
    let term = store::vocabulary::terms(&engine.store.pool, None)
        .await
        .unwrap()
        .into_iter()
        .find(|term| term.concept_uid == concept)
        .unwrap();
    command(
        &engine,
        Command::EditTerm {
            term: term.uid,
            expected_revision: term.revision,
            name: "labor".into(),
            language: "en".into(),
            preferred: true,
        },
    )
    .await;
    command(
        &engine,
        Command::SetLanguage {
            language: "ar".into(),
        },
    )
    .await;
    let query = serde_json::from_value(ast.clone()).unwrap();
    assert_eq!(
        protein::execute(&engine.store, &query).await.unwrap()[0]["uid"],
        subject
    );
    command(
        &engine,
        Command::CreateConcept {
            vocabulary,
            name: "labor".into(),
            language: "en".into(),
            definition: "A different meaning".into(),
            derived_from: None,
        },
    )
    .await;
    assert!(
        engine
            .act(
                Action::Vocabulary {
                    request: Command::BindQuery {
                        query: json!({"source":"record","where":[{"concept_in":"labor"}]})
                            .to_string()
                    }
                },
                None
            )
            .await
            .is_err()
    );
    assert_eq!(
        protein::execute(&engine.store, &query).await.unwrap()[0]["uid"],
        subject
    );
}

#[tokio::test]
async fn meaning_inspector_filters_uses_by_the_readers_record_policy() {
    let engine = Engine::open_memory().await.unwrap();
    let (_, concept) = meaning(&engine).await;
    let visible = record(&engine, RecordKind::Plain, "Visible").await;
    let hidden = record(&engine, RecordKind::Plain, "Hidden secret").await;
    for subject in [&visible, &hidden] {
        engine
            .act(
                Action::SetIdentity {
                    subject: subject.clone(),
                    predicate: Some(concept.clone()),
                },
                None,
            )
            .await
            .unwrap();
    }
    let actor = record(&engine, RecordKind::Person, "Reader").await;
    let role = store::auth::ensure_role(&engine.store.pool, "Meaning readers")
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
                roles: vec!["Meaning readers".into()],
                expected_revision: 0,
            },
            None,
        )
        .await
        .unwrap();
    engine
        .act(
            Action::SetRolePolicy {
                role: "Meaning readers".into(),
                expected_revision: 0,
                policy: json!({"read":{"uid_eq":visible},"grants":[]}),
            },
            None,
        )
        .await
        .unwrap();
    let data = engine
        .act(
            Action::Vocabulary {
                request: Command::InspectMeaning { concept, offset: 0 },
            },
            Some(actor),
        )
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(data["meaning"]["visible_uses"], 1);
    assert_eq!(data["meaning"]["uses"][0]["uid"], visible);
    assert!(!data.to_string().contains("Hidden secret"));
}

#[tokio::test]
async fn translation_packages_add_wording_idempotently_without_changing_definitions_or_features() {
    let source = Engine::open_memory().await.unwrap();
    let (vocabulary, concept) = meaning(&source).await;
    let revision = store::linguas::get(&source.store.pool, &vocabulary)
        .await
        .unwrap()
        .unwrap()
        .revision;
    let publication = command(
        &source,
        Command::Publish {
            vocabulary: vocabulary.clone(),
            expected_revision: revision,
        },
    )
    .await
    .data
    .unwrap()["publication"]
        .to_string();
    let destination = Engine::open_memory().await.unwrap();
    let organ = store::organs::local(&destination.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    command(
        &destination,
        Command::AdoptPackage {
            package: publication,
            organ,
        },
    )
    .await;
    command(
        &source,
        Command::AddTerm {
            concept: concept.clone(),
            name: "عمل".into(),
            language: "ar".into(),
            preferred: true,
            personal: false,
        },
    )
    .await;
    command(
        &source,
        Command::UseWording {
            concept: concept.clone(),
            wording: "Private wording".into(),
        },
    )
    .await;
    let mut package = command(&source, Command::ExportTranslations { vocabulary })
        .await
        .data
        .unwrap()["translations"]
        .clone();
    assert!(!package.to_string().contains("Private wording"));
    let before = store::vocabulary::feature_bindings(&destination.store.pool)
        .await
        .unwrap();
    command(
        &destination,
        Command::PreviewTranslations {
            package: package.to_string(),
        },
    )
    .await;
    for _ in 0..2 {
        command(
            &destination,
            Command::ImportTranslations {
                package: package.to_string(),
            },
        )
        .await;
    }
    let terms = store::vocabulary::terms(&destination.store.pool, None)
        .await
        .unwrap();
    assert_eq!(
        terms
            .iter()
            .filter(|term| term.concept_uid == concept && term.name == "عمل")
            .count(),
        1
    );
    assert_eq!(
        store::vocabulary::feature_bindings(&destination.store.pool)
            .await
            .unwrap(),
        before
    );
    package["dictionaries"][0]["definitions"][0]["definition"] = json!("Different meaning");
    assert!(
        destination
            .act(
                Action::Vocabulary {
                    request: Command::ImportTranslations {
                        package: package.to_string()
                    }
                },
                None
            )
            .await
            .is_err()
    );
    let mut forged: Value = command(
        &source,
        Command::ExportTranslations {
            vocabulary: store::linguas::LOCAL_UID.into(),
        },
    )
    .await
    .data
    .unwrap()["translations"]
        .clone();
    forged["feature_bindings"] = json!([{"feature":"assignment","write_concept_uid":concept}]);
    assert!(store::vocabulary::read_translations(&forged.to_string()).is_err());
}
