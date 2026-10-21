use super::*;
use lince_interface::{
    host::{DesktopAction, WorkspaceLayout},
    native::actions::Action,
};

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn showoff_exercises_rendering_assets_camera_editing_tiling_and_persistence() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let engine = Arc::new(runtime.block_on(engine::Engine::open_memory()).unwrap());
    let cell = CellHandle(cell::CellRuntime {
        commands: cell::terminal::commands::CommandHost::new(directory.path().into()),
        store: engine.store.clone(),
        engine,
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
        fiote: None,
        speech: None,
    });
    let mut canvas = Presentation::new_with_cell(
        [1280, 720],
        directory.path().join("assets"),
        WakeSignal::new(|| {}),
        Some((cell.clone(), directory.path().into())),
    )
    .unwrap();
    canvas
        .world_mut()
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(16),
        ));
    for _ in 0..3 {
        canvas.probe_frame().unwrap();
    }
    canvas
        .outputs(vec![
            Output {
                name: "Showoff-1".into(),
                viewport: lince_interface::host::OutputViewport {
                    id: lince_interface::host::OutputId(1),
                    bounds: lince_interface::host::Rectangle::new([0.0; 2], [1280.0, 720.0])
                        .unwrap(),
                    usable: lince_interface::host::Rectangle::new([0.0; 2], [1280.0, 720.0]),
                    physical_size: [1280, 720],
                    workspace: 1,
                },
            },
            Output {
                name: "Showoff-2".into(),
                viewport: lince_interface::host::OutputViewport {
                    id: lince_interface::host::OutputId(2),
                    bounds: lince_interface::host::Rectangle::new([1280.0, 0.0], [800.0, 600.0])
                        .unwrap(),
                    usable: lince_interface::host::Rectangle::new([1280.0, 0.0], [800.0, 600.0]),
                    physical_size: [800, 600],
                    workspace: 2,
                },
            },
        ])
        .unwrap();
    canvas
        .world_mut()
        .insert_resource(native::laboratory::showoff::Launch);
    for _ in 0..12 {
        canvas.probe_frame().unwrap();
    }
    let root = canvas.canvas_root().unwrap();
    assert!(native::laboratory::normal(canvas.app.world()));
    let space = canvas
        .app
        .world()
        .get::<native::workspace::Workspaces>(root)
        .unwrap()
        .active;
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries
            .iter()
            .find(|entry| entry.id == space)
            .unwrap()
            .name,
        "showoff"
    );
    let counts = |world: &mut World| {
        (
            world
                .query::<&native::sand_store::StoredSand>()
                .iter(world)
                .count(),
            world
                .query::<&native::area::InfluenceArea>()
                .iter(world)
                .count(),
            world
                .query::<&native::topology::assets::ImportedAsset>()
                .iter(world)
                .count(),
        )
    };
    assert_eq!(counts(canvas.world_mut()), (6, 1, 1));
    let start = std::time::Instant::now();
    while !canvas
        .world_mut()
        .query::<&native::topology::assets::Ready>()
        .iter(canvas.app.world())
        .any(|_| true)
    {
        assert!(start.elapsed().as_secs() < 30, "GLB was not rendered");
        canvas.probe_frame().unwrap();
    }
    let before = probe_pixels(canvas.probe_frame().unwrap().dmabuf()).unwrap();
    let model = canvas
        .world_mut()
        .query_filtered::<Entity, With<native::topology::assets::ImportedAsset>>()
        .single(canvas.app.world())
        .unwrap();
    let model_position = canvas
        .app
        .world()
        .get::<native::canvas::CanvasItem>(model)
        .unwrap()
        .position;
    assert_eq!(model_position, bevy::math::DVec2::new(-230.0, 400.0));
    let rectangle = native::topology::presentation::bounds(canvas.app.world(), model).unwrap();
    let colored_model_pixels = before
        .chunks_exact(4)
        .enumerate()
        .filter(|(index, pixel)| {
            let point = Vec2::new((index % 1280) as f32, (index / 1280) as f32);
            rectangle.contains(point)
                && pixel[..3]
                    .iter()
                    .max()
                    .unwrap()
                    .abs_diff(*pixel[..3].iter().min().unwrap())
                    > 60
        })
        .count();
    assert!(
        colored_model_pixels > 100,
        "The GLB did not contribute visible colored pixels"
    );
    assert!(
        before
            .chunks_exact(4)
            .any(|pixel| pixel[0] > 100 && pixel[1] > 60)
    );
    let positions = |world: &mut World| -> Vec<(Entity, bevy::math::DVec2)> {
        world
            .query::<(
                Entity,
                &native::sand_store::StoredSand,
                &native::canvas::CanvasItem,
            )>()
            .iter(world)
            .filter(|(_, sand, _)| sand.kind == native::sand_store::SandKind::Text)
            .map(|(entity, _, item)| (entity, item.position))
            .collect()
    };
    for (index, (_, mut item, _)) in canvas
        .world_mut()
        .query::<(
            Entity,
            &mut native::canvas::CanvasItem,
            &native::area::RecordProperties,
        )>()
        .iter_mut(canvas.app.world_mut())
        .filter(|(_, _, properties)| properties.0["kind"] == "showoff-physics")
        .enumerate()
    {
        item.position = bevy::math::DVec2::new(440.0 + index as f64 * 150.0, 350.0);
    }
    let initial = positions(canvas.world_mut());
    for _ in 0..30 {
        canvas.probe_frame().unwrap();
    }
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::canvas::CanvasItem>(model)
            .unwrap()
            .position,
        model_position
    );
    assert!(
        initial.iter().any(|(entity, position)| canvas
            .app
            .world()
            .get::<native::canvas::CanvasItem>(*entity)
            .unwrap()
            .position
            .distance(*position)
            > 0.1),
        "Physics did not move any pieces"
    );
    let center = canvas
        .app
        .world()
        .get::<native::canvas::CanvasView>(root)
        .unwrap()
        .center;
    canvas.pointer([40.0, 660.0]).unwrap();
    for _ in 0..2 {
        canvas.probe_frame().unwrap();
    }
    canvas
        .button(bevy::input::mouse::MouseButton::Left, true)
        .unwrap();
    canvas.probe_frame().unwrap();
    canvas.pointer([140.0, 660.0]).unwrap();
    for _ in 0..2 {
        canvas.probe_frame().unwrap();
    }
    canvas
        .button(bevy::input::mouse::MouseButton::Left, false)
        .unwrap();
    canvas.probe_frame().unwrap();
    assert!(
        canvas
            .app
            .world()
            .get::<native::canvas::CanvasView>(root)
            .unwrap()
            .center
            .distance(center)
            > 1.0,
        "Dragging the canvas background did not pan the camera"
    );
    let zoom = canvas
        .app
        .world()
        .get::<native::canvas::CanvasView>(root)
        .unwrap()
        .zoom;
    canvas
        .scroll(
            [0.0, 1.0],
            bevy::input::mouse::MouseScrollUnit::Line,
            bevy::input::touch::TouchPhase::Moved,
        )
        .unwrap();
    for _ in 0..2 {
        canvas.probe_frame().unwrap();
    }
    assert_ne!(
        canvas
            .app
            .world()
            .get::<native::canvas::CanvasView>(root)
            .unwrap()
            .zoom,
        zoom
    );
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    canvas.probe_frame().unwrap();
    let note = canvas
        .world_mut()
        .query::<(Entity, &native::sand_store::StoredSand)>()
        .iter(canvas.app.world())
        .find(|(_, sand)| sand.kind == native::sand_store::SandKind::EditableText)
        .unwrap()
        .0;
    let corners =
        native::topology::presentation::projected_corners(canvas.app.world(), note).unwrap();
    let point = std::array::from_fn(|axis| (corners[0][axis] + corners[2][axis]) as f32 * 0.5);
    let before_drag = canvas
        .app
        .world()
        .get::<native::canvas::CanvasItem>(note)
        .unwrap()
        .position;
    canvas.pointer(point).unwrap();
    for _ in 0..2 {
        canvas.probe_frame().unwrap();
    }
    canvas
        .button(bevy::input::mouse::MouseButton::Right, true)
        .unwrap();
    canvas.probe_frame().unwrap();
    canvas.pointer([point[0] + 48.0, point[1] + 24.0]).unwrap();
    for _ in 0..2 {
        canvas.probe_frame().unwrap();
    }
    canvas
        .button(bevy::input::mouse::MouseButton::Right, false)
        .unwrap();
    canvas.probe_frame().unwrap();
    assert!(
        canvas
            .app
            .world()
            .get::<native::canvas::CanvasItem>(note)
            .unwrap()
            .position
            .distance(before_drag)
            > 1.0,
        "Dragging the note did not move the real Sand"
    );
    native::edit_mode::EditAction::Close.apply(canvas.world_mut(), root);
    for layout in [
        WorkspaceLayout::Grid,
        WorkspaceLayout::ScrollingColumns,
        WorkspaceLayout::Free,
    ] {
        canvas
            .desktop_action(DesktopAction::SetLayout(layout))
            .unwrap();
        for _ in 0..3 {
            canvas.probe_frame().unwrap();
        }
        assert_eq!(
            canvas
                .app
                .world()
                .get::<native::workspace::Workspaces>(root)
                .unwrap()
                .entries
                .iter()
                .find(|entry| entry.id == space)
                .unwrap()
                .desktop
                .mode(),
            layout
        );
    }
    canvas.desktop_action(DesktopAction::Unfocus).unwrap();
    canvas.probe_frame().unwrap();
    assert!(canvas.geometry().unwrap().focused.is_none());
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    canvas.probe_frame().unwrap();
    assert!(
        canvas
            .app
            .world()
            .get::<native::edit_mode::EditMode>(root)
            .unwrap()
            .enabled
    );
    native::edit_mode::EditAction::Area(native::area_panel::AreaAction::Add(
        native::area::ShapeKind::Square,
    ))
    .apply(canvas.world_mut(), root);
    canvas.probe_frame().unwrap();
    assert_eq!(counts(canvas.world_mut()), (6, 2, 1));
    native::edit_mode::EditAction::Close.apply(canvas.world_mut(), root);
    native::topology::ui::TopologyAction::ToggleView.apply(canvas.world_mut(), root);
    for _ in 0..5 {
        canvas.probe_frame().unwrap();
    }
    assert!(
        canvas
            .probe_frame()
            .unwrap()
            .geometry
            .unwrap()
            .items
            .iter()
            .any(|item| item.mapping.corners.is_some())
    );
    assert!(
        canvas
            .app
            .world()
            .get::<native::topology::view::View>(root)
            .unwrap()
            .spatial
    );
    let position = canvas
        .app
        .world()
        .get::<native::topology::view::View>(root)
        .unwrap()
        .position;
    canvas
        .key(
            KeyCode::KeyA,
            bevy::input::keyboard::Key::Character("a".into()),
            None,
            true,
            false,
        )
        .unwrap();
    for _ in 0..5 {
        canvas.probe_frame().unwrap();
    }
    canvas
        .key(
            KeyCode::KeyA,
            bevy::input::keyboard::Key::Character("a".into()),
            None,
            false,
            false,
        )
        .unwrap();
    assert_ne!(
        canvas
            .app
            .world()
            .get::<native::topology::view::View>(root)
            .unwrap()
            .position,
        position
    );
    let camera_position = canvas
        .app
        .world()
        .get::<native::topology::view::View>(root)
        .unwrap()
        .position;
    canvas
        .desktop_action(DesktopAction::Focus(
            lince_interface::host::Direction::Right,
        ))
        .unwrap();
    canvas
        .desktop_action(DesktopAction::ToggleFullscreen)
        .unwrap();
    for _ in 0..4 {
        canvas.probe_frame().unwrap();
    }
    assert!(
        canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries
            .iter()
            .find(|entry| entry.id == space)
            .unwrap()
            .desktop
            .fullscreen()
            .is_some()
    );
    assert!(!native::topology::view::spatial(canvas.app.world(), root));
    let camera = canvas
        .app
        .world()
        .resource::<native::topology::presentation::SceneCamera>()
        .0;
    assert!(matches!(
        canvas.app.world().get::<Projection>(camera),
        Some(Projection::Orthographic(_))
    ));
    canvas
        .desktop_action(DesktopAction::ToggleFullscreen)
        .unwrap();
    for _ in 0..4 {
        canvas.probe_frame().unwrap();
    }
    assert!(native::topology::view::spatial(canvas.app.world(), root));
    assert!(matches!(
        canvas.app.world().get::<Projection>(camera),
        Some(Projection::Perspective(_))
    ));
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::topology::view::View>(root)
            .unwrap()
            .position,
        camera_position
    );
    let after = probe_pixels(canvas.probe_frame().unwrap().dmabuf()).unwrap();
    assert!(
        before
            .iter()
            .zip(&after)
            .filter(|(a, b)| a.abs_diff(**b) > 8)
            .count()
            > 1000
    );
    native::topology::ui::TopologyAction::ToggleView.apply(canvas.world_mut(), root);
    canvas.probe_frame().unwrap();
    canvas.finish().unwrap();
    drop(canvas);
    let saved_path = std::fs::read_dir(directory.path().join("interface.snapshots"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .max()
        .unwrap();
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(saved_path).unwrap()).unwrap();
    assert!(
        saved["workspaces"]
            .as_array()
            .unwrap()
            .iter()
            .any(|space| space["name"] == "showoff")
    );
    let mut restored = Presentation::new_with_cell(
        [1280, 720],
        directory.path().join("assets"),
        WakeSignal::new(|| {}),
        Some((cell, directory.path().into())),
    )
    .unwrap();
    restored
        .world_mut()
        .insert_resource(native::laboratory::showoff::Launch);
    for _ in 0..12 {
        restored.probe_frame().unwrap();
    }
    assert_eq!(counts(restored.world_mut()), (6, 2, 1));
    assert_eq!(
        restored
            .world_mut()
            .query::<&native::area::RecordProperties>()
            .iter(restored.app.world())
            .filter(|properties| properties.0["kind"] == "showoff-physics")
            .count(),
        3
    );
    let root = restored.canvas_root().unwrap();
    assert_eq!(
        restored
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .entries
            .iter()
            .filter(|space| space.name == "showoff")
            .count(),
        1
    );
    assert_eq!(
        restored
            .world_mut()
            .query_filtered::<Entity, With<native::laboratory::showoff::Controls>>()
            .iter(restored.app.world())
            .count(),
        1
    );
    restored.finish().unwrap();
}
