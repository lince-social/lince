use super::*;

fn composer(private: bool) -> (World, Entity) {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::theme::Typography>();
    let binding = RecordBinding {
        area: world.spawn_empty().id(),
        uid: "record".into(),
        source: crate::protein_area::Source::Local,
    };
    let owner = world.spawn(Node::default()).id();
    let input = world
        .spawn(crate::sand::text_editor(
            "Inspect these files",
            world.resource::<crate::theme::Typography>(),
            0,
        ))
        .id();
    let status = crate::edit_mode::label(&mut world, owner, "", 12.0);
    draft(
        &mut world,
        owner,
        input,
        status,
        "attachment-test",
        &binding,
        private,
    );
    (world, owner)
}

#[test]
fn all_conversation_composers_keep_files_and_dictation_without_private_record_controls() {
    for private in [false, true] {
        let (mut world, owner) = composer(private);
        let labels: Vec<_> = world
            .query::<&Text>()
            .iter(&world)
            .map(|text| text.0.clone())
            .collect();
        for label in [
            "Attach files",
            "Paste image",
            "Dictate",
            "Stop and transcribe",
            "Cancel dictation",
        ] {
            assert!(labels.iter().any(|text| text == label), "{label}");
        }
        assert_eq!(
            world.get::<Draft>(owner).unwrap().reference.is_none(),
            private
        );
    }
}

#[test]
fn six_file_formats_keep_bytes_remove_selection_and_survive_failed_sends() {
    let directory = tempfile::tempdir().unwrap();
    let (mut world, owner) = composer(true);
    let fixtures = [
        ("note.txt", "text/plain"),
        ("table.csv", "text/csv"),
        ("document.pdf", "application/pdf"),
        ("photo.png", "image/png"),
        ("audio.wav", "audio/wav"),
        ("video.mp4", "video/mp4"),
    ];
    let mut parts = Vec::new();
    for (index, (name, mime_type)) in fixtures.into_iter().enumerate() {
        let bytes = [
            include_bytes!("../../tests/fixtures/fiote/note.txt").as_slice(),
            include_bytes!("../../tests/fixtures/fiote/table.csv").as_slice(),
            include_bytes!("../../tests/fixtures/fiote/document.pdf").as_slice(),
            include_bytes!("../../tests/fixtures/fiote/photo.png").as_slice(),
            include_bytes!("../../tests/fixtures/fiote/audio.wav").as_slice(),
            include_bytes!("../../tests/fixtures/fiote/video.mp4").as_slice(),
        ][index].to_vec();
        let path = directory.path().join(name);
        std::fs::write(&path, &bytes).unwrap();
        let part = read_file(path, false).unwrap();
        assert!(
            matches!(&part, MessagePart::Attachment { name: held, mime_type: mime, data }
            if held == name && mime == mime_type && nucleus::message::decode(data).unwrap() == bytes)
        );
        parts.push(part);
    }
    append(&mut world, owner, parts.clone());
    assert_eq!(contents(&world, owner).unwrap(), parts);
    Remove(1).apply(&mut world, owner);
    parts.remove(1);
    world.get_mut::<Draft>(owner).unwrap().locked = true;
    sent(&mut world, owner, false);
    assert_eq!(contents(&world, owner).unwrap(), parts);
    append(&mut world, owner, Vec::new());
    assert_eq!(contents(&world, owner).unwrap(), parts);
    let oversized = directory.path().join("oversized.bin");
    std::fs::write(&oversized, vec![0; nucleus::message::MAX_CONTENT_BYTES + 1]).unwrap();
    assert!(read_file(oversized, false).is_err());
    assert_eq!(contents(&world, owner).unwrap(), parts);
    sent(&mut world, owner, true);
    assert!(contents(&world, owner).unwrap().is_empty());
}

#[test]
fn box_drops_do_not_attach_to_a_focused_composer_and_stale_hover_does_not_block_them() {
    use bevy::ecs::system::RunSystemOnce;
    use bevy::picking::{backend::HitData, hover::HoverMap, pointer::PointerId};
    let (world, owner) = composer(true);
    let mut app = App::new();
    *app.world_mut() = world;
    app.init_resource::<bevy::ecs::schedule::Schedules>();
    app.add_plugins(crate::external_drop::ExternalDropPlugin);
    let world = app.world_mut();
    let root = world
        .spawn((
            crate::container::BoxRoot,
            crate::workspace::Workspaces::default(),
            crate::canvas::CanvasView::default(),
            ComputedNode {
                size: Vec2::new(800.0, 600.0),
                ..default()
            },
            UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::new(
                400.0, 300.0,
            ))),
        ))
        .id();
    world.entity_mut(owner).insert(ChildOf(root));
    let input = world.get::<Draft>(owner).unwrap().input;
    world.entity_mut(input).insert((
        ChildOf(owner),
        ComputedNode {
            size: Vec2::new(200.0, 100.0),
            ..default()
        },
        UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::new(
            150.0, 150.0,
        ))),
    ));
    world.init_resource::<bevy::input_focus::InputFocus>();
    world
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(input, bevy::input_focus::FocusCause::Pressed);
    world.init_resource::<HoverMap>();
    let mut window = Window::default();
    window.set_cursor_position(Some(Vec2::new(500.0, 350.0)));
    let window = world.spawn(window).id();
    let event = bevy::window::FileDragAndDrop::DroppedFile {
        window,
        path_buf: "/tmp/box-image.png".into(),
    };
    world.write_message(event.clone());
    world.run_system_once(dropped).unwrap();
    assert!(world.get::<Picking>(owner).is_none());
    world
        .resource_mut::<HoverMap>()
        .entry(PointerId::Mouse)
        .or_default()
        .insert(input, HitData::new(root, 0.0, None, None));
    assert!(crate::external_drop::receives(world, window));
    world
        .get_mut::<Window>(window)
        .unwrap()
        .set_cursor_position(Some(Vec2::new(150.0, 150.0)));
    assert!(!crate::external_drop::receives(world, window));
    world.write_message(event);
    world.run_system_once(dropped).unwrap();
    assert!(world.get::<Picking>(owner).is_some());
}
