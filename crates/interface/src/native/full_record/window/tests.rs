use super::*;
use bevy::{ecs::system::RunSystemOnce, input_focus::InputFocus, text::EditableText};

#[test]
fn restoration_preserves_live_metadata_until_its_identity_changes() {
    let mut world = World::new();
    let owner = world.spawn_empty().id();
    let uid = nucleus::new_uid("r");
    let mut config = config(&uid, Source::Local);
    config.window_record = Some(WindowRecord {
        placement: [1; 16],
        uid,
        title: "Saved title".into(),
        app_id: "example.client".into(),
        creating: false,
    });
    restore(&mut world, owner, &config);
    let mut metadata = config.window_record.as_ref().unwrap().metadata();
    assert_eq!(world.get::<Metadata>(owner), Some(&metadata));
    metadata.title = "Live title".into();
    metadata.live = true;
    metadata.state = "floating".into();
    world.entity_mut(owner).insert(metadata.clone());
    let changed = world
        .entity(owner)
        .get_ref::<Metadata>()
        .unwrap()
        .last_changed();
    world.increment_change_tick();
    restore(&mut world, owner, &config);
    assert_eq!(world.get::<Metadata>(owner), Some(&metadata));
    assert_eq!(
        world
            .entity(owner)
            .get_ref::<Metadata>()
            .unwrap()
            .last_changed(),
        changed
    );
    config.window_record.as_mut().unwrap().placement = [2; 16];
    restore(&mut world, owner, &config);
    assert_eq!(
        world.get::<Metadata>(owner),
        Some(&config.window_record.as_ref().unwrap().metadata())
    );
}

#[test]
fn invalid_window_queries_cannot_insert_or_replace_metadata() {
    let mut world = World::new();
    let uid = nucleus::new_uid("r");
    let mut config = config(&uid, Source::Local);
    config.window_record = Some(WindowRecord {
        placement: [1; 16],
        uid,
        title: "Saved title".into(),
        app_id: "example.client".into(),
        creating: false,
    });
    let empty = world.spawn_empty().id();
    let mut metadata = config.window_record.as_ref().unwrap().metadata();
    metadata.placement = [3; 16];
    metadata.live = true;
    let live = world.spawn(metadata.clone()).id();
    for mutation in 0..4 {
        let mut invalid = config.clone();
        match mutation {
            0 => invalid.source = Source::Organ("remote".into()),
            1 => invalid.draft.query["limit"] = json!(2),
            2 => invalid.window_record.as_mut().unwrap().uid = nucleus::new_uid("r"),
            _ => invalid.window_record.as_mut().unwrap().placement = [0; 16],
        }
        for owner in [empty, live] {
            restore(&mut world, owner, &invalid);
        }
        assert!(world.get::<Metadata>(empty).is_none());
        assert_eq!(world.get::<Metadata>(live), Some(&metadata));
    }
}

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
    let row = app.world().get::<ChildOf>(header).unwrap().parent();
    let captured = app.world().get::<CaptureSource>(row).unwrap();
    assert_eq!(captured.owner, owner);
    assert_eq!(captured.uid, uid);
    assert_eq!(captured.placement, [9; 16]);
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
        assert!(crate::native::capture::excluded_with(
            app.world(),
            editor,
            &std::collections::BTreeSet::from([owner])
        ));
    }
    app.world_mut().get_mut::<Metadata>(owner).unwrap().uid = nucleus::new_uid("r");
    assert!(crate::native::capture::excluded(app.world(), editor));
    populate(
        app.world_mut(),
        row,
        &RecordBinding {
            area: owner,
            uid,
            source: Source::Local,
        },
    );
    assert!(app.world().get::<CaptureSource>(row).is_some());
    assert!(crate::native::capture::excluded(app.world(), editor));
    let binding = RecordBinding {
        area: owner,
        uid: nucleus::new_uid("r"),
        source: Source::Local,
    };
    app.world_mut().entity_mut(row).insert(binding.clone());
    populate(app.world_mut(), row, &binding);
    assert!(app.world().get::<CaptureSource>(row).is_none());
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
        let original_uid = world.get::<Metadata>(area).unwrap().uid.clone();
        world.entity_mut(row).insert(CaptureSource {
            owner: area,
            placement: [1; 16],
            uid: original_uid,
        });
        populate(&mut world, row, &RecordBinding { area, uid, source });
        assert!(world.get::<CaptureSource>(row).is_none());
    }
    assert_eq!(world.query::<&Header>().iter(&world).count(), 0);
}
