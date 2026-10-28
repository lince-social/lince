use super::*;
use crate::native::canvas::CanvasItem;
use bevy::text::EditableText;

fn begin() -> (App, Entity, String, String) {
    let (mut app, _, owner) = super::super::tests::fixture();
    let uid = nucleus::new_uid("r");
    let config = crate::native::full_record::config(&uid, Source::Local);
    app.world_mut()
        .get_mut::<InfluenceArea>(owner)
        .unwrap()
        .protein = Some(config.clone());
    start(app.world_mut(), owner, config);
    let subscription = app.world().resource::<Runtime>().areas[&owner]
        .subscription
        .clone()
        .unwrap();
    rows::reconcile(app.world_mut(), owner);
    (app, owner, uid, subscription)
}

fn data(uid: &str) -> Value {
    serde_json::json!({"uid":uid,"head":"Loaded title","body":"Loaded description","kind":"plain","quantity":0,"assertions":[],"threads":[]})
}

fn saved_query(world: &mut World, owner: Entity) {
    let expected = world
        .get::<InfluenceArea>(owner)
        .unwrap()
        .protein
        .clone()
        .unwrap();
    let uid = target(&expected).unwrap();
    let root = world.get::<ChildOf>(owner).unwrap().parent();
    let snapshot = crate::native::canvas_host::capture(world, root).unwrap();
    let snapshot: nucleus::canvas::Snapshot =
        serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    let identity = &world
        .get::<crate::native::canvas_host::Identity>(owner)
        .unwrap()
        .0;
    let placement = snapshot
        .placements
        .iter()
        .find(|placement| &placement.id == identity)
        .unwrap();
    let nucleus::canvas::Component::Native {
        kind,
        settings,
        bindings,
    } = &placement.component
    else {
        panic!(
            "The inspector lost its source Area configuration: {:?}",
            placement.component
        );
    };
    assert_eq!(kind, "area");
    assert!(bindings.contains(&uid));
    let configuration: Value =
        serde_json::from_str(settings["configuration"].as_str().unwrap()).unwrap();
    let actual: Config = serde_json::from_value(configuration["protein"].clone()).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn inspector_snapshots_preserve_the_nested_query_and_record_source() {
    for source in [Source::Local, Source::Organ("my-organ".into())] {
        let (mut app, _, owner) = super::super::tests::fixture();
        let uid = nucleus::new_uid("r");
        app.world_mut()
            .get_mut::<InfluenceArea>(owner)
            .unwrap()
            .protein = Some(crate::native::full_record::config(&uid, source));
        saved_query(app.world_mut(), owner);
    }
}

#[test]
fn initial_loading_has_no_authoritative_data_and_populates_the_same_card() {
    let (mut app, owner, uid, subscription) = begin();
    let row = app.world().resource::<Runtime>().areas[&owner].row_entities[&uid];
    let panel = app.world().get::<RowPanel>(row).unwrap().0;
    let label = app.world().get::<Panel>(panel).unwrap().label;
    assert_eq!(app.world().get::<Text>(label).unwrap().0, "Loading Record…");
    assert!(
        app.world().resource::<Runtime>().areas[&owner]
            .data
            .is_empty()
    );
    assert!(app.world().get::<rows::Loading>(row).is_some());
    assert!(
        app.world()
            .get::<crate::native::area::RecordProperties>(row)
            .is_none()
    );
    assert_eq!(app.world().get::<rows::Row>(row).unwrap().data, Value::Null);
    assert_eq!(
        crate::native::canvas_host::authoring::owner(app.world(), row),
        owner
    );
    saved_query(app.world_mut(), owner);
    assert!(
        app.world()
            .get::<crate::native::canvas_host::Identity>(row)
            .is_none()
    );
    assert_eq!(
        app.world_mut()
            .query::<&EditableText>()
            .iter(app.world())
            .count(),
        0
    );
    let previous = *app.world().get::<CanvasItem>(row).unwrap();
    rows::commit_edits(app.world_mut());
    assert_eq!(
        app.world().get::<Node>(panel).unwrap().display,
        Display::Flex
    );
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Snapshot {
            id: subscription,
            rows: vec![data(&uid)],
        },
    );
    rows::reconcile(app.world_mut(), owner);
    assert_eq!(
        app.world().resource::<Runtime>().areas[&owner].row_entities[&uid],
        row
    );
    assert!(app.world().get::<rows::Loading>(row).is_none());
    assert!(app.world().get_entity(panel).is_err());
    let loaded_panel = app.world().get::<RowPanel>(row).unwrap().0;
    assert_eq!(
        app.world().get::<ChildOf>(loaded_panel).unwrap().parent(),
        row
    );
    assert_eq!(
        app.world().get::<Node>(loaded_panel).unwrap().display,
        Display::None
    );
    assert_eq!(
        app.world()
            .get::<crate::native::area::RecordProperties>(row)
            .unwrap()
            .0["uid"],
        uid
    );
    let current = app.world().get::<CanvasItem>(row).unwrap();
    assert_eq!(current.position, previous.position);
    assert_eq!(current.size, previous.size);
    assert!(
        app.world_mut()
            .query::<&EditableText>()
            .iter(app.world())
            .count()
            > 0
    );
    assert!(
        app.world()
            .resource::<bevy::input_focus::InputFocus>()
            .get()
            .is_none()
    );
}

#[test]
fn an_initial_permission_failure_retries_the_captured_uid_without_creating_editors() {
    let (mut app, owner, uid, subscription) = begin();
    let row = app.world().resource::<Runtime>().areas[&owner].row_entities[&uid];
    let binding = app.world().get::<RecordBinding>(row).unwrap().clone();
    app.world_mut()
        .entity_mut(owner)
        .insert(crate::native::full_record::window::Metadata {
            placement: [7; 16],
            uid: uid.clone(),
            title: "Original application".into(),
            app_id: "test.application".into(),
            live: true,
            state: "active".into(),
        });
    crate::native::full_record::window::populate(app.world_mut(), row, &binding);
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Error {
            id: subscription.clone(),
            message: "Permission denied".into(),
            code: Some("permission_denied".into()),
        },
    );
    rows::reconcile(app.world_mut(), owner);
    rows::commit_edits(app.world_mut());
    let panel = app.world().get::<RowPanel>(row).unwrap().0;
    let label = app.world().get::<Panel>(panel).unwrap().label;
    assert_eq!(
        app.world().get::<Text>(label).unwrap().0,
        "Permission denied."
    );
    let header = app
        .world_mut()
        .query::<(Entity, &crate::native::full_record::window::Header)>()
        .iter(app.world())
        .next()
        .unwrap()
        .0;
    assert_eq!(
        app.world().get::<Node>(header).unwrap().display,
        Display::Flex
    );
    assert!(
        app.world()
            .get::<Text>(header)
            .unwrap()
            .0
            .contains("Original application")
    );
    assert_eq!(
        app.world_mut()
            .query::<&EditableText>()
            .iter(app.world())
            .count(),
        0
    );
    Refresh(binding).apply(app.world_mut(), row);
    let state = &app.world().resource::<Runtime>().areas[&owner];
    let fresh = state.subscription.clone().unwrap();
    assert_ne!(fresh, subscription);
    assert!(state.pending.iter().all(|message| matches!(
        message,
        ClientMessage::Subscribe { .. } | ClientMessage::Unsubscribe { .. }
    )));
    assert!(state.data.is_empty());
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Snapshot {
            id: subscription,
            rows: vec![data(&uid)],
        },
    );
    rows::reconcile(app.world_mut(), owner);
    assert!(app.world().get::<rows::Loading>(row).is_some());
    assert!(
        app.world()
            .get::<crate::native::area::RecordProperties>(row)
            .is_none()
    );
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Snapshot {
            id: fresh,
            rows: vec![data(&uid)],
        },
    );
    rows::reconcile(app.world_mut(), owner);
    assert_eq!(
        app.world().resource::<Runtime>().areas[&owner].row_entities[&uid],
        row
    );
    assert!(app.world().get::<rows::Loading>(row).is_none());
    assert_eq!(app.world().get::<RecordBinding>(row).unwrap().uid, uid);
}

#[test]
fn field_presentations_leave_status_and_application_metadata_outside_editable_data() {
    let (mut app, owner, uid, subscription) = begin();
    app.world_mut()
        .entity_mut(owner)
        .insert(crate::native::full_record::window::Metadata {
            placement: [7; 16],
            uid: uid.clone(),
            title: "Original application".into(),
            app_id: "test.application".into(),
            live: true,
            state: "active".into(),
        });
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Snapshot {
            id: subscription.clone(),
            rows: vec![data(&uid)],
        },
    );
    rows::reconcile(app.world_mut(), owner);
    let row = app.world().resource::<Runtime>().areas[&owner].row_entities[&uid];
    app.world_mut()
        .get_mut::<super::super::presentation::State>(row)
        .unwrap()
        .view = Some(lince_interface::presentation::Presentation {
        name: "Description only".into(),
        layout: lince_interface::presentation::Layout::Castle,
        fields: vec!["body".into()],
        fills: Default::default(),
        settings: Default::default(),
    });
    rows::commit_edits(app.world_mut());
    let header = app
        .world_mut()
        .query::<(Entity, &crate::native::full_record::window::Header)>()
        .iter(app.world())
        .next()
        .unwrap()
        .0;
    assert_eq!(app.world().get::<ChildOf>(header).unwrap().parent(), row);
    assert_eq!(
        app.world().get::<Node>(header).unwrap().display,
        Display::Flex
    );
    let panel = app.world().get::<RowPanel>(row).unwrap().0;
    assert_eq!(app.world().get::<ChildOf>(panel).unwrap().parent(), row);
    assert_eq!(
        app.world().get::<Node>(panel).unwrap().display,
        Display::None
    );
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Update {
            id: subscription,
            rows: vec![],
        },
    );
    rows::reconcile(app.world_mut(), owner);
    rows::commit_edits(app.world_mut());
    assert_eq!(app.world().get::<ChildOf>(panel).unwrap().parent(), row);
    assert_eq!(app.world().get::<ChildOf>(header).unwrap().parent(), row);
    assert_eq!(
        app.world().get::<Node>(header).unwrap().display,
        Display::Flex
    );
    assert_eq!(
        app.world().get::<Node>(panel).unwrap().display,
        Display::Flex
    );
    assert!(
        app.world()
            .get::<crate::native::area::RecordProperties>(row)
            .is_none()
    );
    for child in app
        .world()
        .get::<Children>(row)
        .unwrap()
        .iter()
        .filter(|child| !chrome(app.world(), *child))
    {
        assert!(
            app.world()
                .get::<Node>(child)
                .is_none_or(|node| node.display == Display::None)
        );
    }
}
