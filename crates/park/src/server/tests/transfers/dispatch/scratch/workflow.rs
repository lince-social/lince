use super::*;
use lince_interface::native::shortcuts::{Settings, Shortcut};

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn real_scratch_properties_long_chooser_cancel_saved_chords_and_focus_return_keep_the_same_records_at_scaled_widths() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for (size, scale) in [([1000, 720], 1.25)] {
        let path = directory.path().join(size[0].to_string());
        std::fs::create_dir(&path).unwrap();
        let engine = persisted(&runtime, &path);
        let (mut fixture, windows, _buffers, ids) = windowed_fullscreen::create(&runtime, &path, size, scale, engine, true);
        action(&mut fixture, DesktopAction::SetLayout(WorkspaceLayout::Tiled));
        let (_, application_id) = application(&mut fixture, ids[0]);
        let (inspector, _, uid) = windowed_fullscreen::inspect(&mut fixture, ids[0]);
        property(&mut fixture, inspector, "Mark as scratch");
        assert!(association(&mut fixture, application_id).unwrap().1.shown());
        property(&mut fixture, inspector, "Hide this scratch item");
        assert!(!association(&mut fixture, application_id).unwrap().1.shown());
        action(&mut fixture, DesktopAction::ShowScratch);
        fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&windows[0].surface));
        property(&mut fixture, inspector, "Hide this scratch item");
        assert!(!association(&mut fixture, application_id).unwrap().1.shown());
        let mut notes = Vec::new();
        for _ in 0..16 {
            let note = spawn_note(&mut fixture, "A retained scratch note with a long distinguishable label", "Keep this native scratch body");
            select_native(&mut fixture, note.1);
            action(&mut fixture, DesktopAction::MarkScratch);
            action(&mut fixture, DesktopAction::HideScratch);
            notes.push(note);
        }
        windowed_fullscreen::capture_shortcut(&mut fixture, "item.scratch-show", "scratch-show", 31, false);
        assert_eq!(fixture.inspect(|canvas| canvas.world_mut().resource::<Settings>().text(Shortcut::ScratchShow).to_owned()), "Unbound");
        windowed_fullscreen::capture_shortcut(&mut fixture, "item.scratch-show", "scratch-show", 31, true);
        windowed_fullscreen::capture_shortcut(&mut fixture, "item.scratch-hide", "scratch-hide", 35, true);
        assert_eq!(fixture.inspect(|canvas| canvas.world_mut().resource::<Settings>().text(Shortcut::ScratchShow).to_owned()), "Mod+Alt+S");
        assert_eq!(fixture.inspect(|canvas| canvas.world_mut().resource::<Settings>().text(Shortcut::ScratchHide).to_owned()), "Mod+Alt+H");
        windowed_fullscreen::select(&mut fixture, ids[1]);
        let before = state(&mut fixture);
        windowed_fullscreen::chord_key(&mut fixture, 31);
        choose(&mut fixture, notes[15].0);
        assert_eq!(state(&mut fixture), before);
        activate(&mut fixture, "Cancel scratch selection");
        assert_eq!(state(&mut fixture), before);
        assert!(absent(&mut fixture, "park.scratch-chooser"));
        windowed_fullscreen::select(&mut fixture, ids[1]);
        windowed_fullscreen::chord_key(&mut fixture, 31);
        let choice = choose(&mut fixture, notes[15].0);
        let (_, points) = fixture.inspect(move |canvas| canvas.native_test_points(choice)).unwrap();
        assert!(points.iter().all(|point| point.iter().all(|value| value.is_finite())));
        let _ = appearance::pixels(&mut fixture, &format!("scratch-last-choice-{}", size[0]));
        let buttons = fixture.client.button_recipients.len();
        confirm(&mut fixture);
        assert_eq!(fixture.client.button_recipients.len(), buttons, "A scratch chooser interaction leaked into an ordinary client");
        assert!(association(&mut fixture, notes[15].1).unwrap().1.shown());
        assert_eq!(content(&mut fixture, notes[15].0), "Keep this native scratch body");
        windowed_fullscreen::chord_key(&mut fixture, 35);
        fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&windows[1].surface));
        assert!(!association(&mut fixture, notes[15].1).unwrap().1.shown());
        windowed_fullscreen::chord_key(&mut fixture, 31);
        let (application, _) = application(&mut fixture, ids[0]);
        choose(&mut fixture, application);
        confirm(&mut fixture);
        fixture.until(|fixture| fixture.client.keyboard_focus.as_ref() == Some(&windows[0].surface));
        property(&mut fixture, inspector, "Hide this scratch item");
        assert!(!association(&mut fixture, application_id).unwrap().1.shown());
        property(&mut fixture, inspector, "Unmark and restore home");
        assert!(association(&mut fixture, application_id).is_none());
        assert!(fixture.inspect(move |canvas| canvas.world_mut().get::<native::full_record::window::Metadata>(inspector)
            .is_some_and(|metadata| metadata.uid == uid && metadata.placement == application_id && metadata.live)));
        unique(&mut fixture);
        super::super::configuration::application_input(&mut fixture, ids[1], &windows[1]);
        fixture.desktop.worker.stop().unwrap();
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn actual_cell_restart_restores_native_scratch_hidden_at_its_home_with_owned_content_without_reseeding_or_launching() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path();
    let engine = persisted(&runtime, path);
    let weak = std::sync::Arc::downgrade(&engine);
    let (mut fixture, _, _, _) = windowed_fullscreen::create(&runtime, path, [1000, 720], 1.25, engine.clone(), false);
    action(&mut fixture, DesktopAction::SetLayout(WorkspaceLayout::Tiled));
    let parent = spawn_note(&mut fixture, "Retained scratch parent", "The original scratch parent body");
    let child = spawn_note(&mut fixture, "Retained scratch child", "The original owned scratch body");
    fixture.inspect(move |canvas| { canvas.world_mut().entity_mut(child.0).insert(native::layout::desktop::TransientParent(parent.0)); });
    select_native(&mut fixture, parent.1);
    action(&mut fixture, DesktopAction::MarkScratch);
    action(&mut fixture, DesktopAction::HideScratch);
    let other = spawn_note(&mut fixture, "Another retained scratch", "Keep the other scratch association independent");
    select_native(&mut fixture, other.1);
    action(&mut fixture, DesktopAction::MarkScratch);
    action(&mut fixture, DesktopAction::HideScratch);
    action(&mut fixture, DesktopAction::Workspace(2));
    action(&mut fixture, DesktopAction::SetLayout(WorkspaceLayout::Tiled));
    action(&mut fixture, DesktopAction::ShowScratch);
    choose(&mut fixture, parent.0);
    confirm(&mut fixture);
    assert_eq!(association(&mut fixture, parent.1).unwrap().0, 2);
    assert!(association(&mut fixture, parent.1).unwrap().1.shown());
    assert_eq!(association(&mut fixture, parent.1).unwrap().1.home_workspace(), 1);
    assert!(fixture.inspect(move |canvas| canvas.world_mut().get::<native::workspace::WorkspaceMember>(child.0).is_some_and(|member| member.0 == 2)));
    let expected_parent = content(&mut fixture, parent.0);
    let expected_child = content(&mut fixture, child.0);
    fixture.desktop.worker.stop().unwrap();
    drop(fixture);
    drop(engine);
    released(&runtime, weak);
    let engine = persisted(&runtime, path);
    let (mut restored, _, _, _) = windowed_fullscreen::create(&runtime, path, [1000, 720], 1.25, engine, false);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if association(&mut restored, parent.1).is_some() { break; }
        assert!(Instant::now() < deadline, "Normal Startup did not restore the native scratch association");
        std::thread::sleep(Duration::from_millis(2));
    }
    let association = association(&mut restored, parent.1).unwrap();
    assert_eq!(association.0, 1);
    assert_eq!(association.1.home_workspace(), 1);
    assert!(!association.1.shown());
    action(&mut restored, DesktopAction::Workspace(1));
    let parent_entity = native_entity(&mut restored, parent.1);
    let child_entity = native_entity(&mut restored, child.1);
    assert_eq!(content(&mut restored, parent_entity), expected_parent);
    assert_eq!(content(&mut restored, child_entity), expected_child);
    assert!(restored.inspect(move |canvas| {
        let world = canvas.world_mut();
        world.get::<native::layout::desktop::TransientParent>(child_entity).is_some_and(|owner| owner.0 == parent_entity)
            && world.get::<native::workspace::WorkspaceMember>(child_entity).is_some_and(|member| member.0 == 1)
    }));
    assert!(!restored.desktop.current.as_ref().unwrap().frame.geometry.as_ref().unwrap().items.iter()
        .any(|item| item.id == ItemId::Native(parent.1) || item.id == ItemId::Native(child.1)));
    assert!(restored.desktop.server.windows().is_empty());
    unique(&mut restored);
    action(&mut restored, DesktopAction::ShowScratch);
    choose(&mut restored, parent_entity);
    confirm(&mut restored);
    action(&mut restored, DesktopAction::UnmarkScratch);
    assert_eq!(content(&mut restored, parent_entity), expected_parent);
    unique(&mut restored);
    restored.desktop.worker.stop().unwrap();
}
