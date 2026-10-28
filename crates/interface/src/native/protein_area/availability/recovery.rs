use super::*;
use bevy::text::EditableText;

#[test]
fn permission_refresh_keeps_identity_and_draft_and_ignores_the_old_subscription() {
    let (mut app, _, owner) = super::super::tests::fixture();
    let uid = nucleus::new_uid("r");
    app.world_mut()
        .get_mut::<InfluenceArea>(owner)
        .unwrap()
        .protein = Some(crate::native::full_record::config(&uid, Source::Local));
    let data = serde_json::json!({"uid":uid,"head":"Owner title","body":"Owner description","kind":"plain","quantity":0,"assertions":[],"threads":[]});
    super::super::tests::authoring_record(app.world_mut(), owner, data.clone());
    let row = app.world().resource::<Runtime>().areas[&owner].row_entities[&uid];
    let editor = app
        .world_mut()
        .query::<(Entity, &EditableText, &RecordBinding)>()
        .iter(app.world())
        .find(|(_, text, binding)| {
            binding.uid == uid && text.value().to_string() == "Owner description"
        })
        .unwrap()
        .0;
    app.world_mut()
        .get_mut::<EditableText>(editor)
        .unwrap()
        .editor
        .set_text("My unsaved draft");
    app.world_mut()
        .resource_mut::<Runtime>()
        .areas
        .get_mut(&owner)
        .unwrap()
        .subscription = Some("old-record".into());
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Error {
            id: "old-record".into(),
            message: "Permission denied".into(),
            code: Some("permission_denied".into()),
        },
    );
    rows::reconcile(app.world_mut(), owner);
    let panel = app.world().get::<RowPanel>(row).unwrap().0;
    let label = app.world().get::<Panel>(panel).unwrap().label;
    assert!(
        app.world()
            .get::<Text>(label)
            .unwrap()
            .0
            .contains("Permission denied")
    );
    let binding = app.world().get::<RecordBinding>(editor).unwrap().clone();
    Refresh(binding.clone()).apply(app.world_mut(), row);
    let subscription = app.world().resource::<Runtime>().areas[&owner]
        .subscription
        .clone()
        .unwrap();
    assert_ne!(subscription, "old-record");
    assert!(app.world().resource::<Runtime>().areas[&owner].refreshing);
    assert_eq!(
        app.world().resource::<Runtime>().areas[&owner]
            .pending
            .len(),
        2
    );
    Refresh(binding).apply(app.world_mut(), row);
    assert_eq!(
        app.world().resource::<Runtime>().areas[&owner]
            .pending
            .len(),
        2
    );
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Snapshot {
            id: "old-record".into(),
            rows: vec![data.clone()],
        },
    );
    assert!(
        app.world().resource::<Runtime>().areas[&owner]
            .data
            .is_empty()
    );
    assert!(app.world().resource::<Runtime>().areas[&owner].refreshing);
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Snapshot {
            id: subscription,
            rows: vec![data],
        },
    );
    rows::reconcile(app.world_mut(), owner);
    assert!(!app.world().resource::<Runtime>().areas[&owner].refreshing);
    assert_eq!(
        app.world().resource::<Runtime>().areas[&owner].row_entities[&uid],
        row
    );
    assert_eq!(
        app.world()
            .get::<EditableText>(editor)
            .unwrap()
            .value()
            .to_string(),
        "My unsaved draft"
    );
    assert_eq!(
        app.world().get::<Node>(panel).unwrap().display,
        Display::None
    );
    assert!(
        app.world()
            .get::<Children>(row)
            .unwrap()
            .iter()
            .all(|child| app.world().get::<Hidden>(child).is_none())
    );
    assert_eq!(
        app.world()
            .resource::<bevy::input_focus::InputFocus>()
            .get(),
        None
    );
}

#[test]
fn a_canonical_inspector_rejects_another_record_in_the_matching_subscription() {
    let (mut app, _, owner) = super::super::tests::fixture();
    let uid = nucleus::new_uid("r");
    let other = nucleus::new_uid("r");
    app.world_mut()
        .get_mut::<InfluenceArea>(owner)
        .unwrap()
        .protein = Some(crate::native::full_record::config(&uid, Source::Local));
    super::super::tests::authoring_record(
        app.world_mut(),
        owner,
        serde_json::json!({"uid":uid,"body":"Captured description"}),
    );
    let row = app.world().resource::<Runtime>().areas[&owner].row_entities[&uid];
    app.world_mut()
        .resource_mut::<Runtime>()
        .areas
        .get_mut(&owner)
        .unwrap()
        .subscription = Some("own-record".into());
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Snapshot {
            id: "own-record".into(),
            rows: vec![serde_json::json!({"uid":other,"body":"Wrong description"})],
        },
    );
    rows::reconcile(app.world_mut(), owner);
    let state = &app.world().resource::<Runtime>().areas[&owner];
    assert!(!state.ready);
    assert!(state.data.is_empty());
    assert!(state.order.is_empty());
    assert_eq!(state.status, "Invalid Record rows");
    assert_eq!(state.row_entities.len(), 1);
    assert_eq!(state.row_entities[&uid], row);
    assert!(
        app.world_mut()
            .query::<&RecordBinding>()
            .iter(app.world())
            .all(|binding| binding.uid != other)
    );
}

#[test]
fn ordinary_multi_record_views_still_remove_results_that_stop_matching() {
    let (mut app, _, owner) = super::super::tests::fixture();
    let uid = nucleus::new_uid("r");
    let mut config = crate::native::full_record::config(&uid, Source::Local);
    config.draft.query["limit"] = serde_json::json!(2);
    app.world_mut()
        .get_mut::<InfluenceArea>(owner)
        .unwrap()
        .protein = Some(config);
    super::super::tests::authoring_record(
        app.world_mut(),
        owner,
        serde_json::json!({"uid":uid,"body":"Description"}),
    );
    let row = app.world().resource::<Runtime>().areas[&owner].row_entities[&uid];
    let state = app
        .world_mut()
        .resource_mut::<Runtime>()
        .into_inner()
        .areas
        .get_mut(&owner)
        .unwrap();
    state.data.clear();
    state.dirty = true;
    rows::reconcile(app.world_mut(), owner);
    assert!(app.world().get_entity(row).is_err());
    assert!(
        app.world().resource::<Runtime>().areas[&owner]
            .row_entities
            .is_empty()
    );
}
