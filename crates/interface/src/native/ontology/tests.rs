use super::*;
use crate::native::sand_panel::tests::{app, connect, settle};
use bevy::text::EditableText;

#[tokio::test]
async fn knowledge_sources_search_read_and_show_source_revision_through_native_controls() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let mut app = app();
    connect(&mut app, engine);
    app.add_plugins(OntologyPlugin);
    let owner = app.world_mut().spawn(Node::default()).id();
    populate(app.world_mut(), owner, owner);
    settle(&mut app, |world| {
        world
            .get::<OntologySand>(owner)
            .unwrap()
            .ready
            .iter()
            .all(|ready| *ready)
    })
    .await;
    Command::Tab(5).apply(app.world_mut(), owner);
    apply(&mut app, owner, Command::KnowledgeSources).await;
    assert_eq!(
        app.world()
            .get::<OntologySand>(owner)
            .unwrap()
            .knowledge_sources[0]["id"],
        "test"
    );
    name(&mut app, owner, "Temperature");
    apply(&mut app, owner, Command::KnowledgeSearch(0)).await;
    settle(&mut app, |world| {
        !world
            .get::<OntologySand>(owner)
            .unwrap()
            .knowledge_results
            .is_empty()
    })
    .await;
    apply(
        &mut app,
        owner,
        Command::KnowledgeRead("test".into(), "reading".into()),
    )
    .await;
    settle(&mut app, |world| {
        world
            .get::<OntologySand>(owner)
            .unwrap()
            .output
            .as_ref()
            .is_some_and(|output| output["result"]["item"]["id"] == "reading")
    })
    .await;
    assert_eq!(
        app.world()
            .get::<OntologySand>(owner)
            .unwrap()
            .output
            .as_ref()
            .unwrap()["result"]["item"]["revision"],
        "1"
    );
    apply(&mut app, owner, Command::KnowledgeClear).await;
}

#[tokio::test]
async fn native_knowledge_save_export_and_offline_inspection_preserve_record_selection() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let mut app = app();
    connect(&mut app, engine.clone());
    app.add_plugins(OntologyPlugin);
    let owner = app.world_mut().spawn(Node::default()).id();
    populate(app.world_mut(), owner, owner);
    settle(&mut app, |world| {
        world
            .get::<OntologySand>(owner)
            .unwrap()
            .ready
            .iter()
            .all(|ready| *ready)
    })
    .await;
    Command::Tab(5).apply(app.world_mut(), owner);
    Command::Choose(6, organ).apply(app.world_mut(), owner);
    for id in ["reference", "reading"] {
        apply(
            &mut app,
            owner,
            Command::KnowledgeRead("test".into(), id.into()),
        )
        .await;
        settle(&mut app, |world| {
            world
                .get::<OntologySand>(owner)
                .unwrap()
                .knowledge_item
                .as_ref()
                .is_some_and(|item| item["id"] == id)
        })
        .await;
    }
    apply(&mut app, owner, Command::KnowledgePreviewSave).await;
    assert_eq!(
        app.world()
            .get::<OntologySand>(owner)
            .unwrap()
            .knowledge_preview
            .as_ref()
            .unwrap()["preview"]["can_save"],
        true
    );
    apply(&mut app, owner, Command::KnowledgeSave).await;
    let uid = app.world().get::<OntologySand>(owner).unwrap().choices[3].clone();
    apply(&mut app, owner, Command::KnowledgeExport).await;
    let package = app.world().get::<OntologySand>(owner).unwrap().package;
    assert!(
        app.world()
            .get::<EditableText>(package)
            .unwrap()
            .value()
            .to_string()
            .contains("knowledge_bundle")
    );
    apply(&mut app, owner, Command::KnowledgeClear).await;
    apply(&mut app, owner, Command::KnowledgeInspectSaved).await;
    let view = app.world().get::<OntologySand>(owner).unwrap();
    assert_eq!(view.choices[3], uid);
    assert_eq!(view.knowledge_item.as_ref().unwrap()["revision"], "1");
}

#[test]
fn knowledge_controls_keep_keyboard_access_multilingual_text_and_bounded_content() {
    let mut app = app();
    let owner = app
        .world_mut()
        .spawn(Node {
            width: px(320),
            height: px(640),
            ..default()
        })
        .id();
    populate(app.world_mut(), owner, owner);
    Command::Tab(5).apply(app.world_mut(), owner);
    let controls = app
        .world_mut()
        .query_filtered::<Entity, With<bevy::ui_widgets::Button>>()
        .iter(app.world())
        .collect::<Vec<_>>();
    assert!(controls.len() > 20);
    for entity in controls {
        assert!(
            app.world()
                .get::<bevy::input_focus::tab_navigation::TabIndex>(entity)
                .is_some()
        );
    }
    {
        let mut view = app.world_mut().get_mut::<OntologySand>(owner).unwrap();
        view.ready = [true; 4];
        view.knowledge_item = Some(
            serde_json::json!({"title":"العربية 日本語","content":"café ".repeat(100000),"source":"test","revision":"1"}),
        );
    }
    ui::render(app.world_mut(), owner);
    let texts = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect::<Vec<_>>();
    assert!(texts.iter().any(|text| text.contains("العربية 日本語")));
    assert!(texts.iter().all(|text| text.chars().count() <= 16000));
    assert!(app.world().entities().count_spawned() < 900);
}

#[tokio::test]
async fn terminology_context_and_private_wording_use_native_controls() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let mut app = app();
    connect(&mut app, engine.clone());
    app.add_plugins(OntologyPlugin);
    let owner = app.world_mut().spawn(Node::default()).id();
    populate(app.world_mut(), owner, owner);
    settle(&mut app, |world| {
        world
            .get::<OntologySand>(owner)
            .unwrap()
            .ready
            .iter()
            .all(|ready| *ready)
    })
    .await;
    Command::Tab(3).apply(app.world_mut(), owner);
    Command::Choose(1, nucleus::vocabulary::TOPOS_UID.into()).apply(app.world_mut(), owner);
    let field = app.world().get::<OntologySand>(owner).unwrap().context;
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text("Travel");
    apply(&mut app, owner, Command::Context).await;
    name(&mut app, owner, "My destination");
    apply(&mut app, owner, Command::UseWording).await;
    assert_eq!(
        store::vocabulary::display_names(&engine.store.pool, None)
            .await
            .unwrap()[nucleus::vocabulary::TOPOS_UID],
        "My destination"
    );
    apply(&mut app, owner, Command::RestoreOriginal).await;
    assert_eq!(
        store::vocabulary::display_names(&engine.store.pool, None)
            .await
            .unwrap()[nucleus::vocabulary::TOPOS_UID],
        "tópos"
    );
}

#[tokio::test]
async fn definition_history_publication_adoption_and_record_history_use_native_controls() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let mut records = Vec::new();
    for (kind, head) in [
        (nucleus::RecordKind::Organ, "Kitchen team"),
        (nucleus::RecordKind::Plain, "Lunch"),
    ] {
        records.push(
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
            .uid,
        );
    }
    let mut app = app();
    connect(&mut app, engine.clone());
    app.add_plugins(OntologyPlugin);
    let owner = app.world_mut().spawn(Node::default()).id();
    populate(app.world_mut(), owner, owner);
    settle(&mut app, |world| {
        world
            .get::<OntologySand>(owner)
            .unwrap()
            .ready
            .iter()
            .all(|ready| *ready)
    })
    .await;
    name(&mut app, owner, "Kitchen");
    apply(&mut app, owner, Command::Create).await;
    let vocabulary = uid(&app, owner, 0, "Kitchen");
    Command::Choose(0, vocabulary.clone()).apply(app.world_mut(), owner);
    Command::Tab(3).apply(app.world_mut(), owner);
    name(&mut app, owner, "meal");
    let field = app.world().get::<OntologySand>(owner).unwrap().definition;
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text("A prepared meal");
    apply(&mut app, owner, Command::Create).await;
    let concept = uid(&app, owner, 1, "meal");
    Command::Choose(1, concept.clone()).apply(app.world_mut(), owner);
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text("A prepared meal for one person");
    apply(&mut app, owner, Command::ReviseDefinition).await;
    apply(&mut app, owner, Command::PreviewFeature(true, true)).await;
    apply(&mut app, owner, Command::ApplyFeature).await;
    assert_eq!(
        store::vocabulary::feature_write(&engine.store.pool, "assignment")
            .await
            .unwrap(),
        concept
    );
    apply(&mut app, owner, Command::InspectMeaning(0)).await;
    let definitions = store::vocabulary::definition_versions(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(definitions[&concept].len(), 2);
    Command::Tab(0).apply(app.world_mut(), owner);
    apply(&mut app, owner, Command::ExportTranslations).await;
    apply(&mut app, owner, Command::PreviewTranslations).await;
    apply(&mut app, owner, Command::ImportTranslations).await;
    apply(&mut app, owner, Command::Publish).await;
    let publication = app
        .world()
        .get::<OntologySand>(owner)
        .unwrap()
        .output
        .as_ref()
        .unwrap()["publication"]["uid"]
        .as_str()
        .unwrap()
        .to_owned();
    Command::Choose(6, records[0].clone()).apply(app.world_mut(), owner);
    apply(&mut app, owner, Command::Adopt).await;
    let saved: bool = store::sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM organ_vocabulary WHERE organ_uid=? AND publication_uid=?)",
    )
    .bind(&records[0])
    .bind(&publication)
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert!(saved);
    apply(&mut app, owner, Command::Withdraw).await;
    apply(&mut app, owner, Command::InspectPublicationAt(publication)).await;
    assert_eq!(
        app.world()
            .get::<OntologySand>(owner)
            .unwrap()
            .output
            .as_ref()
            .unwrap()["publication"]["name"],
        "Kitchen"
    );
    Command::Tab(4).apply(app.world_mut(), owner);
    Command::Choose(3, records[1].clone()).apply(app.world_mut(), owner);
    apply(&mut app, owner, Command::History(0)).await;
    assert_eq!(
        app.world()
            .get::<OntologySand>(owner)
            .unwrap()
            .output
            .as_ref()
            .unwrap()["history"]["versions"][0]["content"]["head"],
        "Lunch"
    );
}

fn name(app: &mut App, owner: Entity, value: &str) {
    let field = app.world().get::<OntologySand>(owner).unwrap().name;
    app.world_mut()
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text(value);
}

fn uid(app: &App, owner: Entity, source: usize, name: &str) -> String {
    app.world().get::<OntologySand>(owner).unwrap().rows[source]
        .iter()
        .find(|row| row["name"] == name)
        .unwrap()["uid"]
        .as_str()
        .unwrap()
        .into()
}

async fn apply(app: &mut App, owner: Entity, command: Command) {
    command.apply(app.world_mut(), owner);
    assert!(
        app.world()
            .get::<OntologySand>(owner)
            .unwrap()
            .pending
            .is_some()
    );
    settle(app, |world| {
        let view = world.get::<OntologySand>(owner).unwrap();
        view.pending.is_none() && view.ready.iter().all(|ready| *ready)
    })
    .await;
    let status = app.world().get::<OntologySand>(owner).unwrap().status;
    let status = &app.world().get::<Text>(status).unwrap().0;
    assert!(
        [
            "Live",
            "Saved",
            "Loading knowledge…",
            "Knowledge loaded",
            "Temporary content cleared"
        ]
        .contains(&status.as_str()),
        "{status}"
    );
}

#[tokio::test]
async fn vocabulary_hierarchy_and_assertions_round_trip_through_cell() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let mut records = vec![];
    for head in ["Build API", "Project"] {
        records.push(
            engine
                .act(
                    engine::actions::Action::CreateRecord {
                        slug: None,
                        kind: nucleus::RecordKind::Plain,
                        head: head.into(),
                        body: String::new(),
                        quantity: 0.0,
                    },
                    None,
                )
                .await
                .unwrap()
                .created
                .unwrap(),
        );
    }
    let mut app = app();
    connect(&mut app, engine);
    app.add_plugins(OntologyPlugin);
    let owner = app.world_mut().spawn(Node::default()).id();
    populate(app.world_mut(), owner, owner);
    settle(&mut app, |world| {
        world
            .get::<OntologySand>(owner)
            .unwrap()
            .ready
            .iter()
            .all(|ready| *ready)
    })
    .await;
    name(&mut app, owner, "Team");
    Command::Choose(5, "shared".into()).apply(app.world_mut(), owner);
    apply(&mut app, owner, Command::Create).await;
    let lingua = uid(&app, owner, 0, "Team");
    Command::Choose(0, lingua.clone()).apply(app.world_mut(), owner);
    name(&mut app, owner, "Team vocabulary");
    apply(&mut app, owner, Command::Rename).await;
    assert_eq!(uid(&app, owner, 0, "Team vocabulary"), lingua);
    Command::Tab(1).apply(app.world_mut(), owner);
    name(&mut app, owner, "task");
    apply(&mut app, owner, Command::Create).await;
    let parent = uid(&app, owner, 1, "task");
    name(&mut app, owner, "backend");
    apply(&mut app, owner, Command::Create).await;
    let concept = uid(&app, owner, 1, "backend");
    Command::Choose(1, concept.clone()).apply(app.world_mut(), owner);
    Command::Choose(2, parent.clone()).apply(app.world_mut(), owner);
    apply(&mut app, owner, Command::Parent(true)).await;
    assert!(
        app.world().get::<OntologySand>(owner).unwrap().rows[1]
            .iter()
            .any(|row| row["uid"] == concept
                && row["parents"]
                    .as_array()
                    .unwrap()
                    .contains(&Value::String(parent.clone())))
    );
    apply(&mut app, owner, Command::Parent(false)).await;
    Command::Choose(0, "g_local".into()).apply(app.world_mut(), owner);
    apply(&mut app, owner, Command::Include(true)).await;
    Command::Choose(0, lingua.clone()).apply(app.world_mut(), owner);
    apply(&mut app, owner, Command::Include(false)).await;
    name(&mut app, owner, "server");
    apply(&mut app, owner, Command::Rename).await;
    assert_eq!(uid(&app, owner, 1, "server"), concept);
    Command::Tab(2).apply(app.world_mut(), owner);
    Command::Choose(3, records[0].clone()).apply(app.world_mut(), owner);
    apply(&mut app, owner, Command::Assert).await;
    Command::Choose(4, records[1].clone()).apply(app.world_mut(), owner);
    apply(&mut app, owner, Command::Assert).await;
    assert!(
        app.world().get::<OntologySand>(owner).unwrap().rows[3]
            .iter()
            .any(|row| row["subject"] == records[0] && row["object"] == records[1])
    );
    apply(&mut app, owner, Command::Identity).await;
    assert!(
        app.world().get::<OntologySand>(owner).unwrap().rows[3]
            .iter()
            .any(|row| row["subject"] == records[0]
                && row["role"] == "identity"
                && row["object"].is_null())
    );
    let assertions: Vec<_> = app.world().get::<OntologySand>(owner).unwrap().rows[3]
        .iter()
        .map(|row| row["uid"].as_str().unwrap().to_owned())
        .collect();
    for assertion in assertions {
        apply(&mut app, owner, Command::Retract(assertion)).await;
    }
    assert!(app.world().get::<OntologySand>(owner).unwrap().rows[3].is_empty());
    Command::Tab(1).apply(app.world_mut(), owner);
    Command::Delete.apply(app.world_mut(), owner);
    settle(&mut app, |world| {
        world.get::<OntologySand>(owner).unwrap().pending.is_none()
    })
    .await;
    let view = app.world().get::<OntologySand>(owner).unwrap();
    assert!(
        app.world()
            .get::<Text>(view.status)
            .unwrap()
            .0
            .contains("assertion history")
    );
    Command::Refresh.apply(app.world_mut(), owner);
    settle(&mut app, |world| {
        world
            .get::<OntologySand>(owner)
            .unwrap()
            .ready
            .iter()
            .all(|ready| *ready)
    })
    .await;
    let view = app.world().get::<OntologySand>(owner).unwrap();
    assert!(view.rows[1].iter().any(|row| {
        row["uid"] == concept
            && row["name"] == "server"
            && row["linguas"]
                .as_array()
                .unwrap()
                .contains(&Value::String("g_local".into()))
    }));
    Command::Choose(1, parent.clone()).apply(app.world_mut(), owner);
    apply(&mut app, owner, Command::Delete).await;
    assert!(
        !app.world().get::<OntologySand>(owner).unwrap().rows[1]
            .iter()
            .any(|row| row["uid"] == parent)
    );
    Command::Tab(0).apply(app.world_mut(), owner);
    Command::Delete.apply(app.world_mut(), owner);
    settle(&mut app, |world| {
        world.get::<OntologySand>(owner).unwrap().pending.is_none()
    })
    .await;
    let status = app.world().get::<OntologySand>(owner).unwrap().status;
    assert!(
        app.world()
            .get::<Text>(status)
            .unwrap()
            .0
            .contains("source of retained Concepts")
    );
    assert!(
        app.world().get::<OntologySand>(owner).unwrap().rows[0]
            .iter()
            .any(|row| row["uid"] == lingua)
    );
    app.world_mut().despawn(owner);
    settle(&mut app, |world| {
        world.resource::<Subscriptions>().0.is_empty()
    })
    .await;
}

#[tokio::test]
async fn translated_terms_are_managed_through_the_native_ontology_and_cell() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let mut app = app();
    connect(&mut app, engine.clone());
    app.add_plugins(OntologyPlugin);
    let owner = app.world_mut().spawn(Node::default()).id();
    populate(app.world_mut(), owner, owner);
    settle(&mut app, |world| {
        world
            .get::<OntologySand>(owner)
            .unwrap()
            .ready
            .iter()
            .all(|ready| *ready)
    })
    .await;
    Command::Tab(3).apply(app.world_mut(), owner);
    Command::Choose(1, nucleus::vocabulary::TOPOS_UID.into()).apply(app.world_mut(), owner);
    name(&mut app, owner, "onde");
    let field = app.world().get::<OntologySand>(owner).unwrap().language;
    app.world_mut()
        .get_mut::<bevy::text::EditableText>(field)
        .unwrap()
        .editor
        .set_text("pt-BR");
    Command::Preferred.apply(app.world_mut(), owner);
    apply(&mut app, owner, Command::AddTerm).await;
    let term = app.world().get::<OntologySand>(owner).unwrap().rows[1]
        .iter()
        .find(|row| row["uid"] == nucleus::vocabulary::TOPOS_UID)
        .unwrap()["terms"]
        .as_array()
        .unwrap()
        .iter()
        .find(|term| term["name"] == "onde")
        .unwrap()["uid"]
        .as_str()
        .unwrap()
        .to_owned();
    apply(&mut app, owner, Command::Language).await;
    assert_eq!(uid(&app, owner, 1, "onde"), nucleus::vocabulary::TOPOS_UID);
    Command::Term(term.clone()).apply(app.world_mut(), owner);
    name(&mut app, owner, "morada");
    apply(&mut app, owner, Command::EditTerm).await;
    assert_eq!(
        uid(&app, owner, 1, "morada"),
        nucleus::vocabulary::TOPOS_UID
    );
    apply(&mut app, owner, Command::RetireTerm).await;
    assert_eq!(
        uid(&app, owner, 1, "localização"),
        nucleus::vocabulary::TOPOS_UID
    );
    assert_eq!(
        store::vocabulary::sources(&engine.store.pool)
            .await
            .unwrap()[nucleus::vocabulary::TOPOS_UID]
            .original_name,
        "tópos"
    );
}

#[test]
fn selections_are_validated_and_large_catalogs_have_bounded_controls() {
    let mut app = app();
    let owner = app.world_mut().spawn(Node::default()).id();
    populate(app.world_mut(), owner, owner);
    assert!(mutation(app.world(), owner, &Command::Delete).is_err());
    {
        let mut view = app.world_mut().get_mut::<OntologySand>(owner).unwrap();
        view.ready = [true; 4];
        view.rows[0] = vec![serde_json::json!({"uid":"g_local", "name":"local"})];
        view.rows[1] = (0..1000).map(|index| serde_json::json!({"uid":format!("c_{index}"),"name":format!("concept {index}")})).collect();
        view.rows[2] = (0..1000).map(|index| serde_json::json!({"uid":format!("r_{index}"),"head":format!("record {index}")})).collect();
    }
    assert!(
        mutation(app.world(), owner, &Command::Delete)
            .unwrap_err()
            .contains("permanent")
    );
    Command::Tab(2).apply(app.world_mut(), owner);
    ui::render(app.world_mut(), owner);
    assert!(app.world().entities().count_spawned() < 650);
    Command::ChoicePage(3, true).apply(app.world_mut(), owner);
    ui::render(app.world_mut(), owner);
    assert_eq!(
        app.world().get::<OntologySand>(owner).unwrap().choice_pages[3],
        1
    );
    assert!(app.world().entities().count_spawned() < 650);
    Command::Choose(3, "missing".into()).apply(app.world_mut(), owner);
    assert!(mutation(app.world(), owner, &Command::Assert).is_err());
    let ids = app.world().get::<OntologySand>(owner).unwrap().ids.clone();
    receive(
        app.world_mut(),
        owner,
        &ServerMessage::Update {
            id: "stale".into(),
            rows: vec![],
        },
    );
    assert_eq!(
        app.world().get::<OntologySand>(owner).unwrap().rows[2].len(),
        1000
    );
    receive(
        app.world_mut(),
        owner,
        &ServerMessage::Error {
            id: ids[2].clone(),
            code: Some("test".into()),
            message: "denied".into(),
        },
    );
    assert!(!app.world().get::<OntologySand>(owner).unwrap().ready[2]);
    assert!(mutation(app.world(), owner, &Command::Assert).is_err());
}

#[test]
fn laboratory_cannot_send_ontology_changes() {
    let mut app = app();
    let owner = app.world_mut().spawn(Node::default()).id();
    populate(app.world_mut(), owner, owner);
    app.world_mut()
        .init_resource::<crate::native::laboratory::Laboratory>();
    app.world_mut()
        .resource_mut::<crate::native::laboratory::Laboratory>()
        .root = Some(owner);
    app.world_mut()
        .get_mut::<OntologySand>(owner)
        .unwrap()
        .ready = [true; 4];
    name(&mut app, owner, "Preview only");
    Command::Create.apply(app.world_mut(), owner);
    let view = app.world().get::<OntologySand>(owner).unwrap();
    assert!(view.pending.is_none());
    assert!(
        app.world()
            .get::<Text>(view.status)
            .unwrap()
            .0
            .contains("Laboratory")
    );
}
