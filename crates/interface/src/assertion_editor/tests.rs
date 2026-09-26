use super::*;
use crate::protein_area::{
    Config, Source,
    tests::{fixture, until},
};

#[test]
fn live_catalogue_updates_preserve_chips_and_only_change_resolved_names() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(crate::theme::Typography(Handle::default()))
        .add_plugins(AssertionEditorPlugin);
    let parent = app.world_mut().spawn(Node::default()).id();
    spawn(
        app.world_mut(),
        parent,
        RecordBinding {
            area: parent,
            uid: "r_task".into(),
            source: Source::Local,
        },
        &json!({"assertions":[{"uid":"a_link", "predicate":"depends-on", "object":"r_project"}]}),
        false,
    );
    let (sender, _receiver) = tokio::sync::mpsc::channel(2);
    app.world_mut()
        .resource_mut::<Subscriptions>()
        .0
        .insert(parent, ("catalogue".into(), sender));
    let chip = app.world().get::<Field>(parent).unwrap().chips[0];
    let text = app.world().get::<Chip>(chip).unwrap().text;
    let children = app.world().get::<Children>(chip).unwrap().to_vec();
    for (slug, head) in [
        ("project", "Project"),
        ("project", "Renamed"),
        ("renamed", "Renamed"),
    ] {
        receive(
            app.world_mut(),
            &ServerMessage::Update {
                id: "catalogue".into(),
                rows: vec![json!({"uid":"r_project", "slug":slug, "head":head})],
            },
        );
        assert_eq!(app.world().get::<Field>(parent).unwrap().chips, vec![chip]);
        assert_eq!(
            app.world().get::<Children>(chip).unwrap().to_vec(),
            children
        );
        assert_eq!(
            app.world().get::<Text>(text).unwrap().0,
            format!("#depends-on @{slug}")
        );
    }
    let flow = app.world().get::<Field>(parent).unwrap().flow;
    let text_tick = app
        .world()
        .entity(text)
        .get_ref::<Text>()
        .unwrap()
        .last_changed();
    let children_tick = app
        .world()
        .entity(flow)
        .get_ref::<Children>()
        .unwrap()
        .last_changed();
    app.world_mut().increment_change_tick();
    receive(
        app.world_mut(),
        &ServerMessage::Update {
            id: "catalogue".into(),
            rows: vec![json!({"uid":"r_project", "slug":"renamed", "head":"Renamed"})],
        },
    );
    assert_eq!(
        app.world()
            .entity(text)
            .get_ref::<Text>()
            .unwrap()
            .last_changed(),
        text_tick
    );
    assert_eq!(
        app.world()
            .entity(flow)
            .get_ref::<Children>()
            .unwrap()
            .last_changed(),
        children_tick
    );
}

#[test]
fn draft_tokens_wrap_remove_in_place_and_keep_unfinished_text() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(crate::theme::Typography(Handle::default()))
        .add_plugins(AssertionEditorPlugin);
    let parent = app.world_mut().spawn(Node::default()).id();
    spawn(
        app.world_mut(),
        parent,
        RecordBinding {
            area: parent,
            uid: String::new(),
            source: Source::Local,
        },
        &json!({}),
        true,
    );
    let input = input(app.world(), parent).unwrap();
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("#planned, #depends-on @project: 1.25 @day");
    submit(app.world_mut(), parent);
    let field = app.world().get::<Field>(parent).unwrap();
    assert_eq!(field.values.len(), 2);
    assert_eq!(
        app.world().get::<Node>(field.flow).unwrap().flex_wrap,
        FlexWrap::Wrap
    );
    assert_eq!(
        app.world().get::<Children>(field.flow).unwrap().last(),
        Some(&input)
    );
    let uid = field.values[0]["uid"].as_str().unwrap().to_owned();
    let chip = field.chips[0];
    let close = app.world().get::<Chip>(chip).unwrap().close;
    assert_eq!(
        *app.world().get::<Visibility>(close).unwrap(),
        Visibility::Hidden
    );
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(close, FocusCause::Navigated);
    app.update();
    assert_eq!(
        *app.world().get::<Visibility>(close).unwrap(),
        Visibility::Inherited
    );
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("#cost:");
    Remove(uid).apply(app.world_mut(), parent);
    assert_eq!(value(app.world(), input), "#cost:");
    assert_eq!(app.world().get::<Field>(parent).unwrap().values.len(), 1);
    submit(app.world_mut(), parent);
    assert_eq!(value(app.world(), input), "#cost:");
    assert!(draft(app.world(), parent).is_err());
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("#cost: 9007199254740993.125");
    let assertions = draft(app.world(), parent).unwrap();
    assert_eq!(assertions.len(), 2);
    assert_eq!(assertions[0].object.as_deref(), Some("project"));
    assert_eq!(
        assertions[1].quantity.as_deref(),
        Some("9007199254740993.125")
    );
}

#[tokio::test]
async fn saved_assertions_support_links_live_completion_removal_and_failed_drafts() {
    let (mut app, _, owner) = fixture();
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let mut ids = Vec::new();
    for slug in ["task", "project"] {
        ids.push(
            engine
                .act(
                    engine::actions::Action::CreateRecord {
                        slug: Some(slug.into()),
                        kind: nucleus::RecordKind::Plain,
                        head: slug.into(),
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
    for name in ["planned", "depends-on", "cost", "day"] {
        engine
            .act(
                engine::actions::Action::CreateConcept {
                    lingua: "g_local".into(),
                    name: name.into(),
                    parents: Vec::new(),
                },
                None,
            )
            .await
            .unwrap();
    }
    app.insert_resource(crate::app::CellHandle(cell::CellRuntime {
        commands: Default::default(),
        engine: engine.clone(),
        store: engine.store.clone(),
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: default(),
        fiote: None,
        information: None,
    }))
    .insert_resource(crate::wake::WakeSignal::new(|| {}))
    .add_plugins(crate::cell_bridge::CellBridgePlugin);
    let mut config = Config::records();
    config.draft.query["where"] = json!([{"uid_eq":ids[0]}]);
    app.world_mut()
        .get_mut::<crate::area::InfluenceArea>(owner)
        .unwrap()
        .protein = Some(config);
    until(&mut app, |world| {
        world
            .resource::<Subscriptions>()
            .0
            .keys()
            .filter_map(|entity| world.get::<Field>(*entity))
            .any(|field| {
                field.concepts.len() >= 4
                    && field.records.iter().any(|row| row["slug"] == "project")
            })
    })
    .await;
    let parent = app
        .world_mut()
        .query_filtered::<Entity, With<Field>>()
        .single(app.world())
        .unwrap();
    let input = input(app.world(), parent).unwrap();
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("#planned, #depends-on @project: 9007199254740993.125 @day");
    submit(app.world_mut(), parent);
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("#cost:");
    until(&mut app, |world| {
        world.get::<Field>(parent).unwrap().values.len() == 2
            && !world.get::<Field>(parent).unwrap().pending
    })
    .await;
    assert_eq!(value(app.world(), input), "#cost:");
    let field = app.world().get::<Field>(parent).unwrap();
    let link = field
        .values
        .iter()
        .find(|value| value["predicate"] == "depends-on")
        .unwrap();
    assert_eq!(link["object"], ids[1]);
    assert_eq!(link["quantity"], "9007199254740993.125");
    let uid = link["uid"].as_str().unwrap().to_owned();
    Remove(uid).apply(app.world_mut(), parent);
    until(&mut app, |world| {
        world.get::<Field>(parent).unwrap().values.len() == 1
            && !world.get::<Field>(parent).unwrap().pending
    })
    .await;
    assert_eq!(value(app.world(), input), "#cost:");
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("#missing-concept");
    submit(app.world_mut(), parent);
    until(&mut app, |world| {
        !world.get::<Field>(parent).unwrap().pending
    })
    .await;
    assert_eq!(value(app.world(), input), "#missing-concept");
    assert_eq!(app.world().get::<Field>(parent).unwrap().values.len(), 1);
    let notice = app.world().get::<Field>(parent).unwrap().notice;
    assert!(!app.world().get::<Text>(notice).unwrap().0.is_empty());
    app.world_mut().despawn(parent);
    app.update();
    assert!(
        !app.world()
            .resource::<Subscriptions>()
            .0
            .contains_key(&parent)
    );
}
