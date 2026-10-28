use super::*;
use bevy::text::EditableText;

#[test]
fn renewed_authentication_uses_the_existing_form_without_losing_the_record_draft() {
    let (mut app, _, owner) = super::super::tests::fixture();
    let uid = nucleus::new_uid("r");
    app.world_mut()
        .get_mut::<InfluenceArea>(owner)
        .unwrap()
        .protein = Some(crate::native::full_record::config(
        &uid,
        Source::Organ("my-organ".into()),
    ));
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
        .set_text("Retained remote draft");
    let state = app
        .world_mut()
        .resource_mut::<Runtime>()
        .into_inner()
        .areas
        .get_mut(&owner)
        .unwrap();
    state.remote = Some("my-organ".into());
    state.subscription = Some("record-authentication".into());
    receive(
        app.world_mut(),
        owner,
        ServerMessage::LiveHello {
            login_required: true,
        },
    );
    rows::reconcile(app.world_mut(), owner);
    login::sync(app.world_mut());
    let panel = app.world().get::<RowPanel>(row).unwrap().0;
    assert!(
        app.world_mut()
            .query::<(&login::Form, &ChildOf)>()
            .iter(app.world())
            .any(|(_, parent)| parent.parent() == panel)
    );
    assert_eq!(
        app.world()
            .get::<EditableText>(editor)
            .unwrap()
            .value()
            .to_string(),
        "Retained remote draft"
    );
    assert!(hidden(app.world(), row));
    receive(
        app.world_mut(),
        owner,
        ServerMessage::SessionAuthenticated {
            id: "authentication".into(),
            session_id: "renewed".into(),
            person: "same-owner".into(),
            key_id: "same-key".into(),
        },
    );
    rows::reconcile(app.world_mut(), owner);
    assert!(app.world().resource::<Runtime>().areas[&owner].refreshing);
    assert!(
        !app.world_mut()
            .query::<(&login::Form, &ChildOf)>()
            .iter(app.world())
            .any(|(_, parent)| parent.parent() == panel)
    );
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Snapshot {
            id: "record-authentication".into(),
            rows: vec![data],
        },
    );
    rows::reconcile(app.world_mut(), owner);
    assert!(!hidden(app.world(), row));
    assert_eq!(
        app.world()
            .get::<crate::native::area::RecordProperties>(row)
            .unwrap()
            .0["uid"],
        uid
    );
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
        "Retained remote draft"
    );
    assert_eq!(
        app.world().get::<Node>(panel).unwrap().display,
        Display::None
    );
}
