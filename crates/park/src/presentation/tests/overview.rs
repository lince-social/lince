use super::shortcuts::{capture, click, key, named_button, settle};
use super::*;
use lince_interface::{
    host::{DesktopAction, WorkspaceLayout},
    native::actions::Action,
};

mod dragging;
mod settings;

fn chord(canvas: &mut Presentation, physical: KeyCode) {
    key(canvas, KeyCode::SuperLeft, true);
    key(canvas, physical, true);
    key(canvas, physical, false);
    key(canvas, KeyCode::SuperLeft, false);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn overview_keyboard_pointer_and_thumbnails_preserve_the_canvas_and_native_content() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let size = [1000, 720];
    let mut canvas = Presentation::new(
        size,
        directory.path().join("assets"),
        WakeSignal::new(|| {}),
    )
    .unwrap();
    settle(&mut canvas);
    let root = canvas.canvas_root().unwrap();
    canvas
        .world_mut()
        .remove_resource::<native::layout::desktop::DefaultLayout>();
    native::workspace::create(canvas.world_mut(), root);
    let second = canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap()
        .active;
    let mut targets = Vec::new();
    for (workspace, color) in [
        (1, Color::srgb(1.0, 0.0, 0.0)),
        (second, Color::srgb(0.0, 1.0, 0.0)),
    ] {
        let mut spaces = canvas
            .world_mut()
            .get_mut::<native::workspace::Workspaces>(root)
            .unwrap();
        spaces
            .entries
            .iter_mut()
            .find(|space| space.id == workspace)
            .unwrap()
            .desktop
            .set_mode(WorkspaceLayout::Free);
        native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false);
        let rectangle = native::canvas::CanvasItem {
            position: bevy::math::DVec2::new(1000.0 * workspace as f64, -500.0),
            size: Vec2::new(240.0, 160.0),
        };
        let layout = native::layout::LayoutBox::new(rectangle.size);
        let entity = canvas
            .world_mut()
            .spawn((
                rectangle,
                layout,
                native::workspace::WorkspaceMember(workspace),
                BackgroundColor(color),
                Name::new(format!("Overview item {workspace}")),
                ChildOf(root),
            ))
            .id();
        targets.push((entity, layout.id, rectangle));
    }
    native::workspace::switch_output(canvas.world_mut(), root, 1);
    settle(&mut canvas);
    canvas
        .world_mut()
        .entity_mut(root)
        .insert(native::canvas_selection::SandSelection(vec![targets[0].0]));
    canvas.desktop_action(DesktopAction::ToggleFocus).unwrap();
    settle(&mut canvas);
    let original = *canvas
        .app
        .world()
        .get::<native::canvas::CanvasView>(root)
        .unwrap();
    chord(&mut canvas, KeyCode::Space);
    let frame = settle(&mut canvas);
    assert!(frame.input.keyboard_modal);
    assert_eq!(frame.input.bindings.len(), 1);
    assert_eq!(
        frame.input.bindings[0].action,
        DesktopAction::ToggleOverview
    );
    assert!(native::layout::desktop::overview::active(
        canvas.app.world()
    ));
    let pixels = probe_pixels(frame.dmabuf().unwrap()).unwrap();
    let colored = |axis: usize| {
        pixels
            .chunks_exact(4)
            .filter(|pixel| {
                pixel[axis] > 180 && pixel[(axis + 1) % 3] < 80 && pixel[(axis + 2) % 3] < 80
            })
            .count()
    };
    assert!(colored(0) > 500, "The active native preview is missing");
    assert!(
        colored(1) > 500,
        "The inactive workspace preview is missing"
    );
    capture("overview-native-wide", size, &frame);
    chord(&mut canvas, KeyCode::Space);
    assert!(!settle(&mut canvas).input.keyboard_modal);
    assert!(!native::layout::desktop::overview::active(
        canvas.app.world()
    ));
    let restored = canvas
        .app
        .world()
        .get::<native::canvas::CanvasView>(root)
        .unwrap();
    assert_eq!(restored.center, original.center);
    assert_eq!(restored.zoom, original.zoom);
    for (entity, identity, original) in &targets {
        assert_eq!(
            canvas
                .app
                .world()
                .get::<native::layout::LayoutBox>(*entity)
                .unwrap()
                .id,
            *identity
        );
        let item = canvas
            .app
            .world()
            .get::<native::canvas::CanvasItem>(*entity)
            .unwrap();
        assert_eq!(item.position, original.position);
        assert_eq!(item.size, original.size);
    }
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    native::edit_mode::EditAction::ToggleAlwaysShowControls.apply(canvas.world_mut(), root);
    native::edit_mode::EditAction::Close.apply(canvas.world_mut(), root);
    let frame = settle(&mut canvas);
    capture("overview-closed-toolbar", size, &frame);
    let open = named_button(
        &mut canvas,
        lince_interface::i18n::Token::ShortcutOverview.text(),
    );
    click(&mut canvas, open, size);
    assert!(settle(&mut canvas).input.keyboard_modal);
    let select = named_button(&mut canvas, &format!("Overview item {second}"));
    let (sender, receiver) = std::sync::mpsc::sync_channel(8);
    canvas.host_requests(sender);
    click(&mut canvas, select, size);
    assert!(settle(&mut canvas).input.keyboard_modal);
    let lince_interface::host::HostRequest::Action(action) = receiver.try_recv().unwrap() else {
        panic!("Overview selection did not use the host action bridge")
    };
    assert_eq!(
        action,
        DesktopAction::SelectPlacement {
            workspace: second,
            item: Some(targets[1].1)
        }
    );
    canvas.desktop_action(action).unwrap();
    assert!(!settle(&mut canvas).input.keyboard_modal);
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .active,
        second
    );
    assert_eq!(
        canvas
            .app
            .world()
            .resource::<bevy::input_focus::InputFocus>()
            .get(),
        Some(targets[1].0)
    );
    chord(&mut canvas, KeyCode::KeyE);
    assert!(
        canvas
            .app
            .world()
            .get::<native::edit_mode::EditMode>(root)
            .unwrap()
            .enabled
    );
    chord(&mut canvas, KeyCode::KeyE);
    assert!(
        !canvas
            .app
            .world()
            .get::<native::edit_mode::EditMode>(root)
            .unwrap()
            .enabled
    );
    for (size, direction, workspace) in [
        ([640, 640], KeyCode::ArrowUp, 1),
        ([360, 720], KeyCode::ArrowDown, second),
    ] {
        canvas.resize(size).unwrap();
        settle(&mut canvas);
        chord(&mut canvas, KeyCode::Space);
        let frame = settle(&mut canvas);
        assert!(frame.input.keyboard_modal);
        capture(&format!("overview-native-{}", size[0]), size, &frame);
        key(&mut canvas, direction, true);
        key(&mut canvas, direction, false);
        key(&mut canvas, KeyCode::ArrowRight, true);
        key(&mut canvas, KeyCode::ArrowRight, false);
        let frame = settle(&mut canvas);
        let focused = canvas
            .app
            .world()
            .resource::<bevy::input_focus::InputFocus>()
            .get()
            .unwrap();
        assert_eq!(
            canvas.app.world().get::<Outline>(focused).unwrap().width,
            px(2)
        );
        capture(&format!("overview-keyboard-{}", size[0]), size, &frame);
        key(&mut canvas, KeyCode::Enter, true);
        key(&mut canvas, KeyCode::Enter, false);
        let lince_interface::host::HostRequest::Action(action) = receiver.try_recv().unwrap()
        else {
            panic!("Keyboard overview selection did not use the host action bridge")
        };
        assert!(
            matches!(action, DesktopAction::SelectPlacement { workspace: chosen, item: Some(_) } if chosen == workspace)
        );
        canvas.desktop_action(action).unwrap();
        assert!(!settle(&mut canvas).input.keyboard_modal);
        chord(&mut canvas, KeyCode::Space);
        key(&mut canvas, KeyCode::Escape, true);
        key(&mut canvas, KeyCode::Escape, false);
        assert!(!settle(&mut canvas).input.keyboard_modal);
        assert_eq!(
            canvas
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .active,
            workspace
        );
    }
    for _ in 0..20 {
        native::workspace::create(canvas.world_mut(), root);
    }
    settle(&mut canvas);
    let cameras = |world: &mut World| {
        world
            .query_filtered::<Entity, With<Camera>>()
            .iter(world)
            .count()
    };
    let baseline_cameras = cameras(canvas.world_mut());
    let baseline_images = canvas.app.world().resource::<Assets<Image>>().len();
    let original = *canvas
        .app
        .world()
        .get::<native::canvas::CanvasView>(root)
        .unwrap();
    chord(&mut canvas, KeyCode::Space);
    canvas.pointer([180.0, 360.0]).unwrap();
    canvas
        .scroll(
            [0.0, 1000.0],
            bevy::input::mouse::MouseScrollUnit::Line,
            bevy::input::touch::TouchPhase::Moved,
        )
        .unwrap();
    settle(&mut canvas);
    let first_y = |canvas: &mut Presentation| {
        let world = canvas.world_mut();
        world
            .query::<(&Name, &UiGlobalTransform)>()
            .iter(world)
            .find(|(name, _)| name.as_str() == "overview.workspace.1")
            .unwrap()
            .1
            .transform_point2(Vec2::ZERO)
            .y
    };
    let top = first_y(&mut canvas);
    for amount in [-15.0, 15.0] {
        for _ in 0..16 {
            canvas
                .scroll(
                    [0.0, amount],
                    bevy::input::mouse::MouseScrollUnit::Line,
                    bevy::input::touch::TouchPhase::Moved,
                )
                .unwrap();
            settle(&mut canvas);
            assert!(cameras(canvas.world_mut()) <= baseline_cameras + 16);
            assert!(canvas.app.world().resource::<Assets<Image>>().len() <= baseline_images + 8);
        }
        if amount < 0.0 {
            assert!(first_y(&mut canvas) < top - 1000.0);
        } else {
            assert!((first_y(&mut canvas) - top).abs() < 1.0);
        }
    }
    let retained = canvas
        .app
        .world()
        .get::<native::canvas::CanvasView>(root)
        .unwrap();
    assert_eq!(retained.center, original.center);
    assert_eq!(retained.zoom, original.zoom);
    chord(&mut canvas, KeyCode::Space);
    settle(&mut canvas);
    assert_eq!(cameras(canvas.world_mut()), baseline_cameras);
    assert!(canvas.app.world().resource::<Assets<Image>>().len() <= baseline_images);
}
