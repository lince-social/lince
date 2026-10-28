use super::*;
use lince_interface::native::shortcuts::{Settings, Shortcut};

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn rendered_compact_properties_chooser_header_and_saved_rebound_actions_keep_the_captured_members_at_scaled_widths() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for (size, scale) in [([1000, 720], 1.25), ([360, 720], 1.5)] {
        let path = directory.path().join(size[0].to_string());
        std::fs::create_dir(&path).unwrap();
        let engine = persisted(&runtime, &path);
        let (mut fixture, windows, _buffers, ids) = windowed_fullscreen::create(&runtime, &path, size, scale, engine, true);
        action(&mut fixture, DesktopAction::SetLayout(WorkspaceLayout::Tiled));
        let (third, third_id, _) = spawn_note(&mut fixture, "Native compact member", "A distinct native compact member");
        let (_, neighbour, _) = spawn_note(&mut fixture, "Independent neighbour", "Keep this neighbour outside the compact group");
        let (first, first_id) = application(&mut fixture, ids[0]);
        let (second, second_id) = application(&mut fixture, ids[1]);
        let (inspector, captured, uid) = windowed_fullscreen::inspect(&mut fixture, ids[0]);
        assert_eq!(captured, first_id);
        windowed_fullscreen::capture_shortcut(&mut fixture, "item.group-next", "group-next", 49, false);
        assert_eq!(fixture.inspect(|canvas| canvas.world_mut().resource::<Settings>().text(Shortcut::CompactNext).to_owned()), "Unbound");
        windowed_fullscreen::capture_shortcut(&mut fixture, "item.group-next", "group-next", 49, true);
        assert_eq!(fixture.inspect(|canvas| canvas.world_mut().resource::<Settings>().text(Shortcut::CompactNext).to_owned()), "Mod+Alt+N");
        let before = state(&mut fixture);
        property(&mut fixture, inspector, "Create a compact group");
        choose(&mut fixture, second);
        choose(&mut fixture, third);
        assert_eq!(state(&mut fixture), before);
        activate(&mut fixture, "Cancel compact changes");
        assert_eq!(state(&mut fixture), before);
        property(&mut fixture, inspector, "Create a compact group");
        choose(&mut fixture, second);
        confirm(&mut fixture, false);
        assert_eq!(group(&mut fixture, first_id).unwrap().members, [PlacementId(first_id), PlacementId(second_id)]);
        let header = named(&mut fixture, "desktop.compact-members");
        let height = fixture.inspect(move |canvas| {
            let node = canvas.world_mut().get::<ComputedNode>(header).unwrap();
            node.size().y * node.inverse_scale_factor
        });
        assert!((height - lince_interface::layout::desktop::COMPACT_HEADER_HEIGHT as f32).abs() <= 1.0 / scale as f32);
        property(&mut fixture, inspector, "Add compact members");
        choose(&mut fixture, third);
        confirm(&mut fixture, true);
        let expected = [PlacementId(first_id), PlacementId(second_id), PlacementId(third_id)];
        assert_eq!(group(&mut fixture, first_id).unwrap().members, expected);
        property(&mut fixture, inspector, "Move member later");
        assert_eq!(group(&mut fixture, first_id).unwrap().members, [PlacementId(second_id), PlacementId(first_id), PlacementId(third_id)]);
        property(&mut fixture, inspector, "Move member earlier");
        assert_eq!(group(&mut fixture, first_id).unwrap().members, expected);
        let expected_uid = uid.clone();
        assert!(fixture.inspect(move |canvas| canvas.world_mut().get::<native::full_record::window::Metadata>(inspector)
            .is_some_and(|metadata| metadata.uid == expected_uid && metadata.placement == first_id && metadata.live)));
        windowed_fullscreen::select(&mut fixture, ids[0]);
        windowed_fullscreen::chord_key(&mut fixture, 49);
        fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&windows[1].surface));
        assert_eq!(group(&mut fixture, first_id).unwrap().active, PlacementId(second_id));
        let buttons = fixture.client.button_recipients.len();
        selector(&mut fixture, third);
        assert_eq!(group(&mut fixture, first_id).unwrap().active, PlacementId(third_id));
        assert_eq!(fixture.client.button_recipients.len(), buttons, "A selector click leaked into the application below the header");
        selector(&mut fixture, first);
        fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&windows[0].surface));
        let _ = appearance::pixels(&mut fixture, &format!("compact-controls-{}", size[0]));
        action(&mut fixture, DesktopAction::SetLayout(WorkspaceLayout::Free));
        assert!(absent(&mut fixture, "desktop.compact-members"));
        assert_eq!(group(&mut fixture, first_id).unwrap().members, expected);
        for id in [first_id, second_id, third_id, neighbour] {
            assert!(fixture.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().items.iter().any(|item|
                item.id == if id == first_id { ItemId::Application(ids[0]) } else if id == second_id { ItemId::Application(ids[1]) } else { ItemId::Native(id) }));
        }
        action(&mut fixture, DesktopAction::SetLayout(WorkspaceLayout::Tiled));
        assert_eq!(group(&mut fixture, first_id).unwrap().active, PlacementId(first_id));
        windowed_fullscreen::select(&mut fixture, ids[0]);
        action(&mut fixture, DesktopAction::DetachCompactMember);
        assert!(group(&mut fixture, first_id).is_none());
        assert_eq!(group(&mut fixture, second_id).unwrap().members, [PlacementId(second_id), PlacementId(third_id)]);
        windowed_fullscreen::select(&mut fixture, ids[1]);
        action(&mut fixture, DesktopAction::DissolveCompactGroup);
        assert!(group(&mut fixture, second_id).is_none());
        unique(&mut fixture);
        super::super::configuration::application_input(&mut fixture, ids[0], &windows[0]);
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn ordinary_native_group_save_and_actual_cell_restart_restore_content_identity_order_and_active_member_without_reseeding() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path();
    let engine = persisted(&runtime, path);
    let weak = std::sync::Arc::downgrade(&engine);
    let (mut fixture, _, _, _) = windowed_fullscreen::create(&runtime, path, [1000, 720], 1.25, engine.clone(), false);
    action(&mut fixture, DesktopAction::SetLayout(WorkspaceLayout::Tiled));
    let first = spawn_note(&mut fixture, "First retained note", "First retained native body");
    let second = spawn_note(&mut fixture, "Second retained note", "Second retained native body");
    let third = spawn_note(&mut fixture, "Third retained note", "Third retained native body");
    let values = [first, second, third];
    select_native(&mut fixture, first.1);
    action(&mut fixture, DesktopAction::CreateCompactGroup);
    choose(&mut fixture, second.0);
    choose(&mut fixture, third.0);
    confirm(&mut fixture, false);
    selector(&mut fixture, second.0);
    action(&mut fixture, DesktopAction::MoveCompactMember(Direction::Right));
    let expected = group(&mut fixture, first.1).unwrap();
    assert_eq!(expected.members, [PlacementId(first.1), PlacementId(third.1), PlacementId(second.1)]);
    assert_eq!(expected.active, PlacementId(second.1));
    let saved = values.map(|(entity, id, field)| {
        let body = fixture.inspect(move |canvas| canvas.world_mut().get::<bevy::text::EditableText>(field).unwrap().value().to_string());
        (entity, id, body)
    });
    fixture.desktop.worker.stop().unwrap();
    drop(fixture);
    drop(engine);
    released(&runtime, weak);
    let engine = persisted(&runtime, path);
    let (mut restored, _, _, _) = windowed_fullscreen::create(&runtime, path, [1000, 720], 1.25, engine, false);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if group(&mut restored, first.1).as_ref() == Some(&expected) { break; }
        assert!(Instant::now() < deadline, "The real Startup owner did not restore the saved native compact relation");
        std::thread::sleep(Duration::from_millis(2));
    }
    for (_, id, body) in saved {
        let actual = restored.inspect(move |canvas| {
            let world = canvas.world_mut();
            let (entity, sand) = world.query::<(Entity, &native::layout::LayoutBox, &native::sand_store::StoredSand)>().iter(world)
                .find_map(|(entity, layout, sand)| (layout.id == id).then_some((entity, sand.content.unwrap()))).unwrap();
            (entity, world.get::<bevy::text::EditableText>(sand).unwrap().value().to_string())
        });
        assert_eq!(actual.1, body);
    }
    assert!(restored.desktop.server.windows().is_empty());
    unique(&mut restored);
    action(&mut restored, DesktopAction::DissolveCompactGroup);
    assert!(group(&mut restored, first.1).is_none());
    restored.desktop.worker.stop().unwrap();
}
