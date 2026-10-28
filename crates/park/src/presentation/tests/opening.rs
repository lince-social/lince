use super::recent::map;
use super::shortcuts::{capture, key, settle};
use super::transfers::output;
use super::*;
use lince_interface::host::{DesktopAction, OutputId, SurfaceId, WorkspaceLayout};

fn focused(canvas: &Presentation, root: Entity) -> Option<[u8; 16]> {
    let spaces = canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap();
    spaces
        .entries
        .iter()
        .find(|space| space.id == spaces.active)
        .unwrap()
        .desktop
        .focused()
        .map(|id| id.0)
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn background_mapping_dialog_closure_and_activation_preserve_the_workspace_and_output_assignments()
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
        canvas
            .outputs(vec![output(1, 0.0, size), output(2, 1100.0, size)])
            .unwrap();
        let root = canvas.canvas_root().unwrap();
        let workspaces = native::workspace::numbers(canvas.app.world(), root);
        canvas
            .activate_workspace(root.to_bits(), workspaces[0])
            .unwrap();
        canvas
            .desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free))
            .unwrap();
        native::workspace_config::set_physics(canvas.world_mut(), root, workspaces[0], false);
        let note = native::sand_store::spawn_sand(
            canvas.world_mut(),
            root,
            workspaces[0],
            native::sand_store::SandKind::EditableText,
            "Opening keeps this note",
            bevy::math::DVec2::ZERO,
        );
        let area = native::area::spawn_area(
            canvas.world_mut(),
            root,
            workspaces[0],
            native::area::InfluenceArea::new(
                native::area::AreaShape::Square,
                bevy::math::DVec2::splat(5000.0),
                bevy::math::DVec2::splat(200.0),
            ),
        )
        .unwrap();
        settle(&mut canvas);
        let editor = canvas
            .app
            .world()
            .get::<native::sand_store::StoredSand>(note)
            .unwrap()
            .content
            .unwrap();
        native::layout::desktop::focus_item(canvas.world_mut(), note);
        canvas
            .world_mut()
            .get_mut::<bevy::text::EditableText>(editor)
            .unwrap()
            .editor
            .set_text("Retained native draft");
        canvas
            .world_mut()
            .resource_mut::<bevy::input_focus::InputFocus>()
            .set(editor, bevy::input_focus::FocusCause::Navigated);
        settle(&mut canvas);
        let before_view = *canvas
            .app
            .world()
            .get::<native::canvas::CanvasView>(root)
            .unwrap();
        let note_id = canvas
            .app
            .world()
            .get::<native::layout::LayoutBox>(note)
            .unwrap()
            .id;
        canvas
            .open_window(SurfaceId(91), "Foreground parent", "test.generic")
            .unwrap();
        settle(&mut canvas);
        assert_eq!(focused(&canvas, root), Some(note_id));
        map(&mut canvas, SurfaceId(91), [30, 110, 170, 255]);
        let mapped = settle(&mut canvas);
        assert_eq!(focused(&canvas, root), Some(note_id));
        assert_eq!(
            canvas
                .app
                .world()
                .resource::<bevy::input_focus::InputFocus>()
                .get(),
            Some(editor)
        );
        let view = canvas
            .app
            .world()
            .get::<native::canvas::CanvasView>(root)
            .unwrap();
        assert_eq!(
            (view.center, view.zoom),
            (before_view.center, before_view.zoom)
        );
        assert!(
            !mapped
                .windows
                .iter()
                .find(|window| window.surface == SurfaceId(91))
                .unwrap()
                .state
                .maximized
        );
        canvas.activate_application(SurfaceId(91)).unwrap();
        settle(&mut canvas);
        canvas.window_request(
            SurfaceId(91),
            lince_interface::host::WindowRequest {
                maximized: Some(true),
                ..default()
            },
        );
        let maximized = settle(&mut canvas);
        assert!(
            maximized
                .windows
                .iter()
                .find(|window| window.surface == SurfaceId(91))
                .unwrap()
                .state
                .maximized
        );
        canvas.window_request(
            SurfaceId(91),
            lince_interface::host::WindowRequest {
                maximized: Some(false),
                ..default()
            },
        );
        let normal = settle(&mut canvas);
        let normal_size = normal
            .windows
            .iter()
            .find(|window| window.surface == SurfaceId(91))
            .unwrap()
            .size;
        let parent = canvas.windows[&SurfaceId(91)].entity;
        let parent_id = canvas
            .app
            .world()
            .get::<native::layout::LayoutBox>(parent)
            .unwrap()
            .id;
        canvas.desktop_action(DesktopAction::ToggleTiling).unwrap();
        let tiled = settle(&mut canvas);
        let parent_size = tiled
            .windows
            .iter()
            .find(|window| window.surface == SurfaceId(91))
            .unwrap()
            .size;
        canvas
            .open_window(SurfaceId(92), "Background window", "test.generic")
            .unwrap();
        map(&mut canvas, SurfaceId(92), [150, 70, 40, 255]);
        let background = settle(&mut canvas);
        assert_eq!(focused(&canvas, root), Some(parent_id));
        assert_eq!(
            background
                .windows
                .iter()
                .find(|window| window.surface == SurfaceId(91))
                .unwrap()
                .size,
            parent_size
        );
        canvas
            .open_window(SurfaceId(93), "Parent-owned dialog", "test.generic")
            .unwrap();
        let child = canvas.windows[&SurfaceId(93)].entity;
        canvas
            .world_mut()
            .entity_mut(child)
            .insert(native::layout::desktop::TransientParent(parent));
        map(&mut canvas, SurfaceId(93), [30, 170, 80, 255]);
        settle(&mut canvas);
        canvas.activate_application(SurfaceId(93)).unwrap();
        settle(&mut canvas);
        canvas.close_window(SurfaceId(93));
        let closed = settle(&mut canvas);
        assert_eq!(focused(&canvas, root), Some(parent_id));
        assert_eq!(
            closed.geometry.as_ref().unwrap().focused,
            Some(lince_interface::host::ItemId::Application(SurfaceId(91)))
        );
        canvas.close_window(SurfaceId(92));
        settle(&mut canvas);
        assert_eq!(focused(&canvas, root), Some(parent_id));
        canvas.desktop_action(DesktopAction::ToggleTiling).unwrap();
        let restored = settle(&mut canvas);
        assert_eq!(
            restored
                .windows
                .iter()
                .find(|window| window.surface == SurfaceId(91))
                .unwrap()
                .size,
            normal_size
        );
        canvas
            .activate_workspace(root.to_bits(), workspaces[1])
            .unwrap();
        canvas
            .open_window(SurfaceId(94), "Other monitor", "test.generic")
            .unwrap();
        map(&mut canvas, SurfaceId(94), [170, 70, 170, 255]);
        settle(&mut canvas);
        canvas.activate_application(SurfaceId(94)).unwrap();
        settle(&mut canvas);
        canvas.window_request(
            SurfaceId(94),
            lince_interface::host::WindowRequest {
                minimized: Some(true),
                ..default()
            },
        );
        settle(&mut canvas);
        canvas
            .activate_workspace(root.to_bits(), workspaces[0])
            .unwrap();
        let assignments: Vec<_> = canvas
            .output_state
            .assignments
            .outputs()
            .iter()
            .map(|output| (output.id, output.active))
            .collect();
        canvas
            .desktop_action(DesktopAction::ToggleOverview)
            .unwrap();
        settle(&mut canvas);
        assert!(native::layout::desktop::overview::active(
            canvas.app.world()
        ));
        canvas.activate_application(SurfaceId(94)).unwrap();
        let activated = settle(&mut canvas);
        assert!(!native::layout::desktop::overview::active(
            canvas.app.world()
        ));
        assert_eq!(canvas.output_state.assignments.focused(), Some(OutputId(2)));
        assert_eq!(
            canvas
                .output_state
                .assignments
                .outputs()
                .iter()
                .map(|output| (output.id, output.active))
                .collect::<Vec<_>>(),
            assignments
        );
        assert!(
            !activated
                .windows
                .iter()
                .find(|window| window.surface == SurfaceId(94))
                .unwrap()
                .state
                .minimized
        );
        key(&mut canvas, KeyCode::AltLeft, true);
        key(&mut canvas, KeyCode::Tab, true);
        settle(&mut canvas);
        assert!(native::layout::desktop::recent::active(canvas.app.world()));
        canvas.activate_application(SurfaceId(91)).unwrap();
        settle(&mut canvas);
        assert!(!native::layout::desktop::recent::active(canvas.app.world()));
        key(&mut canvas, KeyCode::Tab, false);
        key(&mut canvas, KeyCode::AltLeft, false);
        let frame = settle(&mut canvas);
        assert_eq!(focused(&canvas, root), Some(parent_id));
        assert!(!frame.input.keyboard_modal);
        capture(&format!("opening-activation-{}", size[0]), size, &frame);
        canvas.close_window(SurfaceId(91));
        canvas.close_window(SurfaceId(94));
        settle(&mut canvas);
        assert!(canvas.app.world().get_entity(note).is_ok());
        assert!(canvas.app.world().get_entity(area).is_ok());
        assert_eq!(
            canvas
                .app
                .world()
                .get::<bevy::text::EditableText>(editor)
                .unwrap()
                .editor
                .text(),
            "Retained native draft"
        );
        assert_eq!(
            native::workspace::numbers(canvas.app.world(), root),
            workspaces
        );
    }
}
