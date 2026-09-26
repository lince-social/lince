use super::*;
use crate::sand_panel::tests::{app, connect, settle};
use bevy::text::EditableText;

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
    assert_eq!(app.world().get::<Text>(status).unwrap().0, "Live");
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
    apply(&mut app, owner, Command::Delete).await;
    assert!(
        !app.world().get::<OntologySand>(owner).unwrap().rows[0]
            .iter()
            .any(|row| row["uid"] == lingua)
    );
    app.world_mut().despawn(owner);
    settle(&mut app, |world| {
        world.resource::<Subscriptions>().0.is_empty()
    })
    .await;
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
        .init_resource::<crate::laboratory::Laboratory>();
    app.world_mut()
        .resource_mut::<crate::laboratory::Laboratory>()
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
