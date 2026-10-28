use super::commands::{named, type_command};
use super::shortcuts::{capture, click, key, named_button, settle};
use super::*;
use lince_interface::{
    host::{DesktopAction, HostRequest, SurfaceId},
    i18n::Token,
};
use native::actions::Action;

fn chord(canvas: &mut Presentation, trigger: KeyCode, shifted: bool) -> Frame {
    canvas
        .key(KeyCode::SuperLeft, Key::Super, None, true, false)
        .unwrap();
    if shifted {
        canvas
            .key(KeyCode::ShiftLeft, Key::Shift, None, true, false)
            .unwrap();
    }
    let text = match trigger {
        KeyCode::KeyE => "E",
        KeyCode::KeyF => "f",
        KeyCode::KeyN => "n",
        _ => unreachable!(),
    };
    canvas
        .key(
            trigger,
            Key::Character(text.into()),
            Some(text.into()),
            true,
            false,
        )
        .unwrap();
    canvas
        .key(trigger, Key::Character(text.into()), None, false, false)
        .unwrap();
    if shifted {
        canvas
            .key(KeyCode::ShiftLeft, Key::Shift, None, false, false)
            .unwrap();
    }
    canvas
        .key(KeyCode::SuperLeft, Key::Super, None, false, false)
        .unwrap();
    settle(canvas)
}

fn draft(canvas: &Presentation, editor: Entity) -> (String, [usize; 2]) {
    let text = canvas
        .app
        .world()
        .get::<bevy::text::EditableText>(editor)
        .unwrap();
    (
        text.value().to_string(),
        [
            text.editor.raw_selection().anchor().index(),
            text.editor.raw_selection().focus().index(),
        ],
    )
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn logout_confirmation_and_session_defaults_use_real_keys_pointer_cancel_retry_and_retained_drafts()
{
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for size in [[1000, 700], [360, 700]] {
        let mut canvas = Presentation::new(
            size,
            directory.path().join(format!("assets-{}", size[0])),
            WakeSignal::new(|| {}),
        )
        .unwrap();
        settle(&mut canvas);
        let root = canvas.canvas_root().unwrap();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        canvas.host_requests(sender);
        let surface = SurfaceId(71);
        canvas
            .open_window(surface, "Session defaults", "test.generic")
            .unwrap();
        settle(&mut canvas);
        let application = canvas.windows[&surface].entity;
        native::layout::desktop::focus_item(canvas.world_mut(), application);
        let normal = settle(&mut canvas)
            .windows
            .iter()
            .find(|window| window.surface == surface)
            .unwrap()
            .size;
        let frame = chord(&mut canvas, KeyCode::KeyF, false);
        let maximized = frame
            .windows
            .iter()
            .find(|window| window.surface == surface)
            .unwrap();
        assert!(maximized.state.maximized);
        assert!(!maximized.state.fullscreen);
        chord(&mut canvas, KeyCode::KeyF, false);
        let frame = chord(&mut canvas, KeyCode::KeyF, true);
        assert!(
            frame
                .windows
                .iter()
                .find(|window| window.surface == surface)
                .unwrap()
                .state
                .fullscreen
        );
        let frame = chord(&mut canvas, KeyCode::KeyF, true);
        let restored = frame
            .windows
            .iter()
            .find(|window| window.surface == surface)
            .unwrap();
        assert!(!restored.state.fullscreen);
        assert!(!restored.state.maximized);
        assert_eq!(restored.size, normal);
        chord(&mut canvas, KeyCode::KeyN, false);
        assert_eq!(
            receiver.try_recv(),
            Ok(HostRequest::Action(DesktopAction::OpenTerminal))
        );

        let workspace = canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .active;
        let sand = native::sand_store::spawn_sand(
            canvas.world_mut(),
            root,
            workspace,
            native::sand_store::SandKind::EditableText,
            "Session draft",
            bevy::math::DVec2::ZERO,
        );
        let editor = canvas
            .app
            .world()
            .get::<native::sand_store::StoredSand>(sand)
            .unwrap()
            .content
            .unwrap();
        canvas
            .world_mut()
            .get_mut::<bevy::text::EditableText>(editor)
            .unwrap()
            .editor
            .set_text("Retained draft");
        settle(&mut canvas);
        native::layout::desktop::focus_item(canvas.world_mut(), sand);
        canvas
            .world_mut()
            .resource_mut::<bevy::input_focus::InputFocus>()
            .set(editor, bevy::input_focus::FocusCause::Navigated);
        settle(&mut canvas);
        let before = draft(&canvas, editor);
        let view = *canvas
            .app
            .world()
            .get::<native::canvas::CanvasView>(root)
            .unwrap();
        let frame = chord(&mut canvas, KeyCode::KeyE, true);
        assert!(frame.input.keyboard_modal);
        assert!(frame.input.bindings.is_empty());
        assert!(receiver.try_recv().is_err());
        let cancel = named_button(&mut canvas, Token::Cancel.text());
        assert_eq!(
            canvas
                .app
                .world()
                .resource::<bevy::input_focus::InputFocus>()
                .get(),
            Some(cancel)
        );
        capture(&format!("logout-confirmation-{}", size[0]), size, &frame);
        key(&mut canvas, KeyCode::Escape, true);
        key(&mut canvas, KeyCode::Escape, false);
        assert!(!settle(&mut canvas).input.keyboard_modal);
        assert!(receiver.try_recv().is_err());
        assert_eq!(draft(&canvas, editor), before);
        assert_eq!(
            canvas
                .app
                .world()
                .resource::<bevy::input_focus::InputFocus>()
                .get(),
            Some(editor)
        );
        assert_eq!(
            canvas
                .app
                .world()
                .get::<native::canvas::CanvasView>(root)
                .unwrap()
                .center,
            view.center
        );
        assert_eq!(
            canvas
                .app
                .world()
                .get::<native::canvas::CanvasView>(root)
                .unwrap()
                .zoom,
            view.zoom
        );
        chord(&mut canvas, KeyCode::KeyE, true);
        key(&mut canvas, KeyCode::Enter, true);
        key(&mut canvas, KeyCode::Enter, false);
        assert!(!settle(&mut canvas).input.keyboard_modal);
        assert!(receiver.try_recv().is_err());

        native::operation::OpenOperation.apply(canvas.world_mut(), root);
        settle(&mut canvas);
        type_command(&mut canvas, "/session/logout");
        key(&mut canvas, KeyCode::Enter, true);
        key(&mut canvas, KeyCode::Enter, false);
        assert!(settle(&mut canvas).input.keyboard_modal);
        assert!(receiver.try_recv().is_err());
        let overlay = named(&mut canvas, "park.logout.confirmation");
        canvas
            .app
            .world()
            .resource::<native::layout::desktop::HostRequests>()
            .0
            .try_send(HostRequest::Close(surface))
            .unwrap();
        let confirm = named_button(&mut canvas, Token::ShortcutLogout.text());
        click(&mut canvas, confirm, size);
        let frame = settle(&mut canvas);
        assert!(frame.input.keyboard_modal);
        assert_eq!(named(&mut canvas, "park.logout.confirmation"), overlay);
        capture(&format!("logout-busy-{}", size[0]), size, &frame);
        assert_eq!(receiver.try_recv(), Ok(HostRequest::Close(surface)));
        assert!(receiver.try_recv().is_err());
        click(&mut canvas, confirm, size);
        assert!(!settle(&mut canvas).input.keyboard_modal);
        assert_eq!(
            receiver.try_recv(),
            Ok(HostRequest::Action(DesktopAction::Logout))
        );
        assert!(receiver.try_recv().is_err());
        assert!(canvas.app.world().get_entity(overlay).is_err());
        assert_eq!(draft(&canvas, editor), before);
        canvas.finish().unwrap();
    }
}
