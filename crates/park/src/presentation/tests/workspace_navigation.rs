use super::commands::{named, type_command};
use super::shortcuts::{capture, capture_button, click, key, named_button, settle};
use super::transfers::output;
use super::*;
use lince_interface::host::{DesktopAction, HostRequest, OutputId};
use native::actions::Action;
use std::sync::mpsc::Receiver;

fn submit(canvas: &mut Presentation, root: Entity, path: &str) -> Frame {
    native::operation::OpenOperation.apply(canvas.world_mut(), root);
    settle(canvas);
    type_command(canvas, path);
    key(canvas, KeyCode::Enter, true);
    key(canvas, KeyCode::Enter, false);
    settle(canvas)
}

fn receive(
    canvas: &mut Presentation,
    receiver: &Receiver<HostRequest>,
    action: DesktopAction,
) -> Frame {
    assert_eq!(receiver.try_recv(), Ok(HostRequest::Action(action)));
    assert!(receiver.try_recv().is_err());
    canvas.desktop_action(action).unwrap();
    settle(canvas)
}

fn selection(canvas: &Presentation, input: Entity) -> [usize; 2] {
    let input = canvas
        .app
        .world()
        .get::<bevy::text::EditableText>(input)
        .unwrap();
    [
        input.editor.raw_selection().anchor().index(),
        input.editor.raw_selection().focus().index(),
    ]
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn workspace_navigation_operation_commands_preserve_editor_drafts_outputs_and_live_availability() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for size in [[360, 700]] {
        let mut canvas = Presentation::new(
            size,
            directory.path().join(format!("assets-{}", size[0])),
            WakeSignal::new(|| {}),
        )
        .unwrap();
        settle(&mut canvas);
        canvas
            .outputs(vec![output(1, 0.0, size), output(2, 1000.0, [640, 480])])
            .unwrap();
        let root = canvas.canvas_root().unwrap();
        canvas.create_workspace(root.to_bits()).unwrap();
        let ids = native::workspace::numbers(canvas.app.world(), root);
        assert_eq!(ids.len(), 3);
        canvas.activate_workspace(root.to_bits(), ids[0]).unwrap();
        settle(&mut canvas);
        let sand = native::sand_store::spawn_sand(
            canvas.world_mut(),
            root,
            ids[0],
            native::sand_store::SandKind::EditableText,
            "Retained note",
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
            .set_text("Retained editor draft");
        settle(&mut canvas);
        native::layout::desktop::focus_item(canvas.world_mut(), sand);
        canvas
            .world_mut()
            .resource_mut::<bevy::input_focus::InputFocus>()
            .set(editor, bevy::input_focus::FocusCause::Navigated);
        settle(&mut canvas);
        let (sender, receiver) = std::sync::mpsc::sync_channel(16);
        canvas.host_requests(sender);
        submit(&mut canvas, root, "/workspace/next");
        let frame = receive(&mut canvas, &receiver, DesktopAction::NextWorkspace);
        assert_eq!(frame.output, Some(OutputId(2)));
        assert_eq!(
            canvas
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .active,
            ids[1]
        );
        submit(&mut canvas, root, "/workspace/previous");
        let frame = receive(&mut canvas, &receiver, DesktopAction::PreviousWorkspace);
        assert_eq!(frame.output, Some(OutputId(1)));
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
                .get::<bevy::text::EditableText>(editor)
                .unwrap()
                .value(),
            "Retained editor draft"
        );
        let view = *canvas
            .app
            .world()
            .get::<native::canvas::CanvasView>(root)
            .unwrap();
        let preferences = canvas.output_state.assignments.preferences().to_vec();
        submit(&mut canvas, root, "/workspace/move-later");
        let frame = receive(&mut canvas, &receiver, DesktopAction::MoveWorkspaceLater);
        assert_eq!(frame.output, Some(OutputId(1)));
        assert_eq!(
            native::workspace::numbers(canvas.app.world(), root),
            vec![ids[1], ids[0], ids[2]]
        );
        assert_eq!(canvas.output_state.assignments.preferences(), preferences);
        assert_eq!(
            canvas
                .app
                .world()
                .resource::<bevy::input_focus::InputFocus>()
                .get(),
            Some(editor)
        );
        let current = canvas
            .app
            .world()
            .get::<native::canvas::CanvasView>(root)
            .unwrap();
        assert_eq!((current.center, current.zoom), (view.center, view.zoom));
        native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
        native::edit_mode::EditAction::Workspaces.apply(canvas.world_mut(), root);
        settle(&mut canvas);
        let field = {
            let world = canvas.world_mut();
            world
                .query::<(Entity, &native::edit_mode::EditField)>()
                .iter(world)
                .find(|(_, field)| **field == native::edit_mode::EditField::WorkspaceName)
                .map(|(entity, _)| entity)
                .unwrap()
        };
        click(&mut canvas, field, size);
        canvas.text(" owner draft".into()).unwrap();
        settle(&mut canvas);
        let draft = canvas
            .app
            .world()
            .get::<bevy::text::EditableText>(field)
            .unwrap()
            .value()
            .to_string();
        let caret = selection(&canvas, field);
        submit(&mut canvas, root, "/workspace/move-earlier");
        let frame = receive(&mut canvas, &receiver, DesktopAction::MoveWorkspaceEarlier);
        assert_eq!(native::workspace::numbers(canvas.app.world(), root), ids);
        assert_eq!(
            canvas
                .app
                .world()
                .resource::<bevy::input_focus::InputFocus>()
                .get(),
            Some(field)
        );
        assert_eq!(
            canvas
                .app
                .world()
                .get::<bevy::text::EditableText>(field)
                .unwrap()
                .value(),
            draft.as_str()
        );
        assert_eq!(selection(&canvas, field), caret);
        assert_eq!(
            canvas
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .entries[0]
                .name,
            draft.trim()
        );
        capture(&format!("workspace-reordered-{}", size[0]), size, &frame);
        native::edit_mode::EditAction::Close.apply(canvas.world_mut(), root);
        native::operation::OpenOperation.apply(canvas.world_mut(), root);
        settle(&mut canvas);
        type_command(&mut canvas, "/workspace/next");
        let input = named(&mut canvas, "operation.input");
        native::workspace::navigation::reorder(canvas.world_mut(), root, 1).unwrap();
        native::workspace::navigation::reorder(canvas.world_mut(), root, 1).unwrap();
        settle(&mut canvas);
        key(&mut canvas, KeyCode::Enter, true);
        key(&mut canvas, KeyCode::Enter, false);
        let frame = settle(&mut canvas);
        assert!(receiver.try_recv().is_err());
        assert_eq!(
            canvas
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .active,
            ids[0]
        );
        assert_eq!(
            canvas
                .app
                .world()
                .get::<bevy::text::EditableText>(input)
                .unwrap()
                .value(),
            "/workspace/next"
        );
        assert_eq!(
            canvas
                .app
                .world()
                .resource::<bevy::input_focus::InputFocus>()
                .get(),
            Some(input)
        );
        let status = named(&mut canvas, "operation.status");
        assert!(
            canvas
                .app
                .world()
                .get::<Text>(status)
                .unwrap()
                .0
                .contains("last workspace")
        );
        capture(
            &format!("workspace-navigation-edge-{}", size[0]),
            size,
            &frame,
        );
        key(&mut canvas, KeyCode::Escape, true);
        key(&mut canvas, KeyCode::Escape, false);
        native::laboratory::LaboratoryAction::Open.apply(canvas.world_mut(), root);
        settle(&mut canvas);
        let temporary = native::laboratory::scope(canvas.app.world()).unwrap().2;
        assert_eq!(
            native::workspace::navigation::destination(canvas.app.world(), root, -1),
            Ok(ids[2])
        );
        canvas
            .desktop_action(DesktopAction::PreviousWorkspace)
            .unwrap();
        settle(&mut canvas);
        assert!(native::laboratory::scope(canvas.app.world()).is_none());
        assert_eq!(
            canvas
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .active,
            ids[2]
        );
        assert!(!native::workspace::numbers(canvas.app.world(), root).contains(&temporary));
        canvas.desktop_action(DesktopAction::Workspace(9)).unwrap();
        settle(&mut canvas);
        for _ in 0..7 {
            canvas.create_workspace(root.to_bits()).unwrap();
            settle(&mut canvas);
        }
        canvas.activate_workspace(root.to_bits(), ids[0]).unwrap();
        native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
        native::edit_mode::EditAction::Park.apply(canvas.world_mut(), root);
        settle(&mut canvas);
        let open = named_button(&mut canvas, "Edit all shortcuts");
        super::displays::reveal(&mut canvas, open);
        click(&mut canvas, open, size);
        settle(&mut canvas);
        let binding = capture_button(&mut canvas, "workspace.move-later");
        super::displays::reveal(&mut canvas, binding);
        click(&mut canvas, binding, size);
        key(&mut canvas, KeyCode::SuperLeft, true);
        key(&mut canvas, KeyCode::F8, true);
        key(&mut canvas, KeyCode::F8, false);
        key(&mut canvas, KeyCode::SuperLeft, false);
        key(&mut canvas, KeyCode::Enter, true);
        key(&mut canvas, KeyCode::Enter, false);
        let save = named_button(&mut canvas, "Save all changes");
        click(&mut canvas, save, size);
        let frame = settle(&mut canvas);
        assert!(frame.input.bindings.iter().any(|binding| binding.action
            == DesktopAction::MoveWorkspaceLater
            && binding.key == KeyCode::F8
            && binding.modifiers.super_key));
        let toast =
            super::displays::labelled(&mut canvas, lince_interface::i18n::Token::CloseToast.text());
        click(&mut canvas, toast, size);
        settle(&mut canvas);
        assert!(canvas.app.world().get_entity(toast).is_err());
        native::edit_mode::EditAction::Workspaces.apply(canvas.world_mut(), root);
        settle(&mut canvas);
        let field = {
            let world = canvas.world_mut();
            world
                .query::<(Entity, &native::edit_mode::EditField)>()
                .iter(world)
                .find(|(_, field)| **field == native::edit_mode::EditField::WorkspaceName)
                .map(|(entity, _)| entity)
                .unwrap()
        };
        super::displays::reveal(&mut canvas, field);
        click(&mut canvas, field, size);
        let order = native::workspace::numbers(canvas.app.world(), root);
        let position = order.iter().position(|id| *id == ids[0]).unwrap();
        for _ in position..order.len() - 1 {
            key(&mut canvas, KeyCode::SuperLeft, true);
            key(&mut canvas, KeyCode::F8, true);
            key(&mut canvas, KeyCode::F8, false);
            key(&mut canvas, KeyCode::SuperLeft, false);
            receive(&mut canvas, &receiver, DesktopAction::MoveWorkspaceLater);
        }
        let frame = settle(&mut canvas);
        capture(
            &format!("workspace-reorder-scrolled-{}", size[0]),
            size,
            &frame,
        );
        assert_eq!(
            native::workspace::numbers(canvas.app.world(), root).last(),
            Some(&ids[0])
        );
        assert_eq!(
            canvas
                .app
                .world()
                .resource::<bevy::input_focus::InputFocus>()
                .get(),
            Some(field)
        );
        let point = canvas
            .app
            .world()
            .get::<UiGlobalTransform>(field)
            .unwrap()
            .translation;
        assert!(
            point.y >= 0.0
                && point.y < size[1] as f32
                && canvas
                    .app
                    .world()
                    .get::<bevy::ui::CalculatedClip>(field)
                    .is_none_or(|clip| clip.clip.contains(point)),
            "Reordering scrolled the focused name editor out of view: {point:?}"
        );
        let panel = canvas
            .app
            .world()
            .get::<native::edit_mode::EditMode>(root)
            .unwrap()
            .panel;
        let before = canvas.app.world().get::<ScrollPosition>(panel).unwrap().0.y;
        assert!(before > 0.0);
        let point = canvas
            .app
            .world()
            .get::<UiGlobalTransform>(panel)
            .unwrap()
            .translation;
        canvas.pointer(point.to_array()).unwrap();
        canvas
            .scroll(
                [0.0, 120.0],
                bevy::input::mouse::MouseScrollUnit::Pixel,
                bevy::input::touch::TouchPhase::Moved,
            )
            .unwrap();
        settle(&mut canvas);
        let scrolled = canvas.app.world().get::<ScrollPosition>(panel).unwrap().0.y;
        assert!(scrolled < before);
        settle(&mut canvas);
        assert_eq!(
            canvas.app.world().get::<ScrollPosition>(panel).unwrap().0.y,
            scrolled
        );
        assert_eq!(
            canvas
                .app
                .world()
                .resource::<bevy::input_focus::InputFocus>()
                .get(),
            Some(field)
        );
        canvas.finish().unwrap();
    }
}
