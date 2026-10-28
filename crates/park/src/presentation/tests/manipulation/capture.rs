use super::super::shortcuts::{capture_button, click, key, named_button};
use super::*;
use bevy::{
    input::mouse::MouseButton,
    picking::pointer::{PointerButton, PointerId},
};

#[derive(Resource, Default)]
struct Delivered {
    presses: Vec<PointerButton>,
    releases: Vec<PointerButton>,
}

fn observe(canvas: &mut Presentation) {
    canvas
        .app
        .init_resource::<Delivered>()
        .add_observer(
            |event: On<Pointer<Press>>, mut delivered: ResMut<Delivered>| {
                if event.pointer_id == PointerId::Mouse
                    && event.entity == event.original_event_target()
                    && matches!(
                        event.button,
                        PointerButton::Secondary | PointerButton::Middle
                    )
                {
                    delivered.presses.push(event.button);
                }
            },
        )
        .add_observer(
            |event: On<Pointer<bevy::picking::events::Release>>,
             mut delivered: ResMut<Delivered>| {
                if event.pointer_id == PointerId::Mouse
                    && event.entity == event.original_event_target()
                    && matches!(
                        event.button,
                        PointerButton::Secondary | PointerButton::Middle
                    )
                {
                    delivered.releases.push(event.button);
                }
            },
        );
}

fn activate(canvas: &mut Presentation, control: Entity) -> Frame {
    canvas
        .world_mut()
        .trigger(bevy::ui_widgets::Activate { entity: control });
    settle(canvas)
}

fn text(canvas: &Presentation, editor: Entity) -> String {
    canvas
        .app
        .world()
        .get::<bevy::text::EditableText>(editor)
        .unwrap()
        .value()
        .to_string()
}

fn binding(canvas: &Presentation) -> (KeyCode, native::actions::Modifiers) {
    native::shortcuts::desktop_bindings(canvas.app.world())
        .iter()
        .find(|binding| binding.action == DesktopAction::ToggleTiling)
        .map(|binding| (binding.key, binding.modifiers))
        .unwrap()
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn settings_capture_cancels_native_resize_preserves_drafts_and_quarantines_held_buttons() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for size in [[1000, 700], [360, 700]] {
        let data = directory.path().join(format!("capture-{}", size[0]));
        let mut canvas =
            Presentation::new(size, data.join("assets"), WakeSignal::new(|| {})).unwrap();
        canvas
            .world_mut()
            .insert_resource(native::workspace::WorkspaceFile::new(
                data.join("interface.json"),
            ));
        observe(&mut canvas);
        settle(&mut canvas);
        let root = canvas.canvas_root().unwrap();
        canvas
            .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free))
            .unwrap();
        assert!(native::workspace_config::set_physics(
            canvas.world_mut(),
            root,
            1,
            false
        ));
        let center = canvas
            .app
            .world()
            .get::<native::canvas::CanvasView>(root)
            .unwrap()
            .center;
        let target = native::sand_store::spawn_sand(
            canvas.world_mut(),
            root,
            1,
            native::sand_store::SandKind::EditableText,
            "Retained draft",
            center,
        );
        settle(&mut canvas);
        canvas
            .world_mut()
            .entity_mut(target)
            .insert(native::canvas::CanvasItem {
                position: center,
                size: Vec2::splat(96.0),
            });
        canvas
            .world_mut()
            .get_mut::<native::layout::LayoutBox>(target)
            .unwrap()
            .rules = lince_interface::layout::Rules::fixed([96.0; 2]);
        let editor = canvas
            .app
            .world()
            .get::<native::sand_store::StoredSand>(target)
            .unwrap()
            .content
            .unwrap();
        let draft = "Keep this note through settings capture";
        canvas
            .world_mut()
            .get_mut::<bevy::text::EditableText>(editor)
            .unwrap()
            .editor
            .set_text(draft);
        canvas
            .world_mut()
            .resource_mut::<bevy::input_focus::InputFocus>()
            .set(editor, bevy::input_focus::FocusCause::Navigated);
        let initial = settle(&mut canvas);
        let original = *canvas
            .app
            .world()
            .get::<native::canvas::CanvasItem>(target)
            .unwrap();
        let rules = canvas
            .app
            .world()
            .get::<native::layout::LayoutBox>(target)
            .unwrap()
            .rules;
        let model = canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries[0]
            .desktop
            .clone();
        let id = ItemId::Native(
            canvas
                .app
                .world()
                .get::<native::layout::LayoutBox>(target)
                .unwrap()
                .id,
        );
        let original_binding = binding(&canvas);
        let start = point(item_mapping(&initial, id), [0.9; 2]);
        canvas.pointer(start).unwrap();
        key(&mut canvas, KeyCode::SuperLeft, true);
        canvas.button(MouseButton::Right, true).unwrap();
        settle(&mut canvas);
        assert!(manipulation::active(canvas.app.world()));
        key(&mut canvas, KeyCode::SuperLeft, false);
        canvas.pointer([start[0] + 30.0, start[1] + 20.0]).unwrap();
        canvas.button(MouseButton::Middle, true).unwrap();
        settle(&mut canvas);
        assert!(manipulation::active(canvas.app.world()));
        native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
        native::edit_mode::EditAction::Park.apply(canvas.world_mut(), root);
        settle(&mut canvas);
        let open = named_button(&mut canvas, "Edit all shortcuts");
        let modal = activate(&mut canvas, open);
        assert!(modal.input.keyboard_modal);
        assert!(!manipulation::active(canvas.app.world()));
        let capture_control = capture_button(&mut canvas, "layout.toggle-tiling");
        assert!(
            activate(&mut canvas, capture_control)
                .input
                .keyboard_capture
        );
        key(&mut canvas, KeyCode::F2, true);
        key(&mut canvas, KeyCode::F2, false);
        let validate = named_button(&mut canvas, "Validate combination");
        assert!(
            canvas
                .app
                .world()
                .get::<bevy::ui::InteractionDisabled>(validate)
                .is_some()
        );
        key(&mut canvas, KeyCode::ControlLeft, true);
        key(&mut canvas, KeyCode::F2, true);
        key(&mut canvas, KeyCode::F2, false);
        key(&mut canvas, KeyCode::ControlLeft, false);
        assert!(
            canvas
                .app
                .world()
                .get::<bevy::ui::InteractionDisabled>(validate)
                .is_none()
        );
        canvas.button(MouseButton::Right, false).unwrap();
        canvas.button(MouseButton::Middle, false).unwrap();
        let retained = settle(&mut canvas);
        assert!(retained.input.keyboard_capture);
        assert!(
            canvas
                .app
                .world()
                .get::<bevy::ui::InteractionDisabled>(validate)
                .is_none()
        );
        assert_eq!(binding(&canvas), original_binding);
        assert_eq!(text(&canvas, editor), draft);
        assert!(
            canvas
                .app
                .world()
                .resource::<Delivered>()
                .presses
                .is_empty()
        );
        assert!(
            canvas
                .app
                .world()
                .resource::<Delivered>()
                .releases
                .is_empty()
        );
        assert!(!activate(&mut canvas, validate).input.keyboard_capture);
        let control = capture_button(&mut canvas, "layout.toggle-tiling");
        assert!(
            canvas
                .app
                .world()
                .get::<Children>(control)
                .unwrap()
                .iter()
                .any(|child| canvas
                    .app
                    .world()
                    .get::<Text>(child)
                    .is_some_and(|value| value.0 == "Ctrl+F2"))
        );
        assert_eq!(binding(&canvas), original_binding);
        capture(
            &format!("settings-drag-draft-{}", size[0]),
            size,
            &settle(&mut canvas),
        );
        let save = named_button(&mut canvas, "Save all changes");
        super::super::displays::reveal(&mut canvas, save);
        click(&mut canvas, save, size);
        assert!(!settle(&mut canvas).input.keyboard_modal);
        assert_eq!(
            binding(&canvas),
            (
                KeyCode::F2,
                native::actions::Modifiers {
                    control: true,
                    ..native::actions::Modifiers::NONE
                }
            )
        );
        assert!(data.join("interface.lingua").is_file());
        assert_eq!(text(&canvas, editor), draft);
        let current = canvas
            .app
            .world()
            .get::<native::canvas::CanvasItem>(target)
            .unwrap();
        assert_eq!(current.position, original.position);
        assert_eq!(current.size, original.size);
        assert_eq!(
            canvas
                .app
                .world()
                .get::<native::layout::LayoutBox>(target)
                .unwrap()
                .rules,
            rules
        );
        assert_eq!(
            canvas
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .entries[0]
                .desktop,
            model
        );
        native::edit_mode::EditAction::Close.apply(canvas.world_mut(), root);
        let restored = settle(&mut canvas);
        canvas
            .pointer(point(item_mapping(&restored, id), [0.5; 2]))
            .unwrap();
        canvas.button(MouseButton::Right, true).unwrap();
        canvas.button(MouseButton::Right, false).unwrap();
        settle(&mut canvas);
        assert_eq!(
            canvas.app.world().resource::<Delivered>().presses,
            [PointerButton::Secondary]
        );
        assert_eq!(
            canvas.app.world().resource::<Delivered>().releases,
            [PointerButton::Secondary]
        );
        canvas.finish().unwrap();
        drop(canvas);
        let mut reopened =
            Presentation::new(size, data.join("assets"), WakeSignal::new(|| {})).unwrap();
        reopened
            .world_mut()
            .insert_resource(native::workspace::WorkspaceFile::new(
                data.join("interface.json"),
            ));
        settle(&mut reopened);
        assert_eq!(
            binding(&reopened),
            (
                KeyCode::F2,
                native::actions::Modifiers {
                    control: true,
                    ..native::actions::Modifiers::NONE
                }
            )
        );
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn mouse_capture_keeps_other_drag_releases_in_the_batch_and_restores_the_next_context_click() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for size in [[1000, 700], [360, 700]] {
        let data = directory.path().join(format!("mouse-{}", size[0]));
        let mut canvas =
            Presentation::new(size, data.join("assets"), WakeSignal::new(|| {})).unwrap();
        canvas
            .world_mut()
            .insert_resource(native::workspace::WorkspaceFile::new(
                data.join("interface.json"),
            ));
        observe(&mut canvas);
        settle(&mut canvas);
        let root = canvas.canvas_root().unwrap();
        canvas
            .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free))
            .unwrap();
        assert!(native::workspace_config::set_physics(
            canvas.world_mut(),
            root,
            1,
            false
        ));
        let center = canvas
            .app
            .world()
            .get::<native::canvas::CanvasView>(root)
            .unwrap()
            .center;
        let target = native::sand_store::spawn_sand(
            canvas.world_mut(),
            root,
            1,
            native::sand_store::SandKind::Square,
            "Mouse handoff",
            center,
        );
        settle(&mut canvas);
        canvas
            .world_mut()
            .entity_mut(target)
            .insert(native::canvas::CanvasItem {
                position: center,
                size: Vec2::splat(96.0),
            });
        canvas
            .world_mut()
            .get_mut::<native::layout::LayoutBox>(target)
            .unwrap()
            .rules = lince_interface::layout::Rules::fixed([96.0; 2]);
        let initial = settle(&mut canvas);
        let id = ItemId::Native(
            canvas
                .app
                .world()
                .get::<native::layout::LayoutBox>(target)
                .unwrap()
                .id,
        );
        let original = *canvas
            .app
            .world()
            .get::<native::canvas::CanvasItem>(target)
            .unwrap();
        let start = point(item_mapping(&initial, id), [0.9; 2]);
        canvas.pointer(start).unwrap();
        key(&mut canvas, KeyCode::SuperLeft, true);
        canvas.button(MouseButton::Right, true).unwrap();
        settle(&mut canvas);
        assert!(manipulation::active(canvas.app.world()));
        key(&mut canvas, KeyCode::SuperLeft, false);
        canvas.pointer([start[0] + 30.0, start[1] + 20.0]).unwrap();
        canvas.button(MouseButton::Middle, true).unwrap();
        settle(&mut canvas);
        native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
        native::edit_mode::EditAction::Park.apply(canvas.world_mut(), root);
        settle(&mut canvas);
        let open = named_button(&mut canvas, "Edit all shortcuts");
        assert!(activate(&mut canvas, open).input.keyboard_modal);
        assert!(!manipulation::active(canvas.app.world()));
        let control = capture_button(&mut canvas, "pointer.move");
        assert!(activate(&mut canvas, control).input.keyboard_capture);
        key(&mut canvas, KeyCode::ControlLeft, true);
        canvas.button(MouseButton::Left, true).unwrap();
        canvas.button(MouseButton::Right, false).unwrap();
        canvas.button(MouseButton::Middle, false).unwrap();
        settle(&mut canvas);
        canvas.button(MouseButton::Left, false).unwrap();
        key(&mut canvas, KeyCode::ControlLeft, false);
        let validate = named_button(&mut canvas, "Validate combination");
        assert!(
            canvas
                .app
                .world()
                .get::<bevy::ui::InteractionDisabled>(validate)
                .is_none()
        );
        assert!(!activate(&mut canvas, validate).input.keyboard_capture);
        assert!(
            canvas
                .app
                .world()
                .resource::<Delivered>()
                .presses
                .is_empty()
        );
        assert!(
            canvas
                .app
                .world()
                .resource::<Delivered>()
                .releases
                .is_empty()
        );
        capture(
            &format!("mouse-drag-draft-{}", size[0]),
            size,
            &settle(&mut canvas),
        );
        let save = named_button(&mut canvas, "Save all changes");
        super::super::displays::reveal(&mut canvas, save);
        click(&mut canvas, save, size);
        assert!(!settle(&mut canvas).input.keyboard_modal);
        assert!(
            native::shortcuts::pointer_bindings(canvas.app.world())
                .iter()
                .any(|binding| binding.operation == Manipulation::Move
                    && binding.chord
                        == native::shortcuts::PointerChord {
                            button: MouseButton::Left,
                            modifiers: native::actions::Modifiers {
                                control: true,
                                ..native::actions::Modifiers::NONE
                            },
                        })
        );
        let current = canvas
            .app
            .world()
            .get::<native::canvas::CanvasItem>(target)
            .unwrap();
        assert_eq!(current.position, original.position);
        assert_eq!(current.size, original.size);
        native::edit_mode::EditAction::Close.apply(canvas.world_mut(), root);
        let restored = settle(&mut canvas);
        canvas
            .pointer(point(item_mapping(&restored, id), [0.5; 2]))
            .unwrap();
        canvas.button(MouseButton::Right, true).unwrap();
        canvas.button(MouseButton::Right, false).unwrap();
        settle(&mut canvas);
        assert_eq!(
            canvas.app.world().resource::<Delivered>().presses,
            [PointerButton::Secondary]
        );
        assert_eq!(
            canvas.app.world().resource::<Delivered>().releases,
            [PointerButton::Secondary]
        );
    }
}
