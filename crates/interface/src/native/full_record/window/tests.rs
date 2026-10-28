use super::*;
use bevy::{ecs::system::RunSystemOnce, input_focus::InputFocus, text::EditableText};

#[test]
fn metadata_updates_and_closure_preserve_the_actual_record_editor_draft_and_binding() {
    let (mut app, _, owner) = crate::native::protein_area::tests::fixture();
    let uid = nucleus::new_uid("r");
    app.world_mut()
        .get_mut::<crate::native::area::InfluenceArea>(owner)
        .unwrap()
        .protein = Some(config(&uid, Source::Local));
    app.world_mut().entity_mut(owner).insert(Metadata {
        placement: [9; 16],
        uid: uid.clone(),
        title: "Client title".into(),
        app_id: "example.client".into(),
        live: true,
        state: "tiled".into(),
    });
    crate::native::protein_area::tests::authoring_record(
        app.world_mut(),
        owner,
        json!({"uid":uid, "head":"Owner title", "body":"Owner description", "kind":"plain", "quantity":0, "assertions":[], "threads":[]}),
    );
    let header = app
        .world_mut()
        .query::<(Entity, &Header)>()
        .iter(app.world())
        .find(|(_, header)| header.0 == owner)
        .unwrap()
        .0;
    let editor = app
        .world_mut()
        .query::<(Entity, &EditableText, &RecordBinding)>()
        .iter(app.world())
        .find(|(_, text, binding)| {
            text.value().to_string() == "Owner description" && binding.uid == uid
        })
        .unwrap()
        .0;
    app.world_mut()
        .get_mut::<EditableText>(editor)
        .unwrap()
        .editor
        .set_text("Unsaved owner description");
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(editor, bevy::input_focus::FocusCause::Pressed);
    assert!(crate::native::protein_area::authoring_draft(
        app.world(),
        editor
    ));
    for live in [true, false] {
        let mut metadata = app.world_mut().get_mut::<Metadata>(owner).unwrap();
        metadata.title = "Changed client title".into();
        metadata.live = live;
        metadata.state = "floating".into();
        app.world_mut().run_system_once(refresh).unwrap();
        let text = &app.world().get::<Text>(header).unwrap().0;
        assert!(text.contains("Changed client title"));
        assert!(text.contains(if live {
            "floating"
        } else {
            "closed or unavailable"
        }));
        assert_eq!(
            app.world()
                .get::<EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "Unsaved owner description"
        );
        assert_eq!(app.world().get::<RecordBinding>(editor).unwrap().uid, uid);
        assert_eq!(app.world().resource::<InputFocus>().get(), Some(editor));
        assert!(crate::native::protein_area::authoring_draft(
            app.world(),
            editor
        ));
        assert!(app.world().get::<EditableText>(header).is_none());
        assert!(
            app.world()
                .get::<crate::native::actions::ActionButton>(header)
                .is_none()
        );
    }
}

#[test]
fn unrelated_or_remote_records_cannot_receive_a_local_application_metadata_header() {
    let mut world = World::new();
    let uid = nucleus::new_uid("r");
    let area = world
        .spawn(Metadata {
            placement: [1; 16],
            uid: uid.clone(),
            title: "Client".into(),
            app_id: "example.client".into(),
            live: true,
            state: "tiled".into(),
        })
        .id();
    let row = world.spawn_empty().id();
    for (uid, source) in [
        (nucleus::new_uid("r"), Source::Local),
        (uid, Source::Organ("https://remote.example".into())),
    ] {
        populate(&mut world, row, &RecordBinding { area, uid, source });
    }
    assert_eq!(world.query::<&Header>().iter(&world).count(), 0);
}
