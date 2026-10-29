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
                viewport: lince_interface::host::OutputViewport { rotation: Default::default(),
                    id: lince_interface::host::OutputId(1),
                    bounds: lince_interface::host::Rectangle::new([0.0; 2], [1280.0, 720.0])
                        .unwrap(),
                    usable: lince_interface::host::Rectangle::new([0.0; 2], [1280.0, 720.0]),
                    physical_size: [1280, 720],
                    raster: lince_interface::host::raster::RasterMapping { origin: (lince_interface::host::Rectangle::new([0.0; 2], [1280.0, 720.0])
                        .unwrap()).origin(), scale: [1.0; 2] },
                    workspace: 1,
                },
            },
            Output {
                name: "Showoff-2".into(),
                viewport: lince_interface::host::OutputViewport { rotation: Default::default(),
                    id: lince_interface::host::OutputId(2),
                    bounds: lince_interface::host::Rectangle::new([1280.0, 0.0], [800.0, 600.0])
                        .unwrap(),
                    usable: lince_interface::host::Rectangle::new([1280.0, 0.0], [800.0, 600.0]),
                    physical_size: [800, 600],
                    raster: lince_interface::host::raster::RasterMapping { origin: (lince_interface::host::Rectangle::new([1280.0, 0.0], [800.0, 600.0])
                        .unwrap()).origin(), scale: [1.0; 2] },
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
    let initial_view = *canvas
        .app
        .world()
        .get::<native::canvas::CanvasView>(root)
        .unwrap();
    assert!(native::laboratory::normal(canvas.app.world()));
    assert!(
        native::shortcuts::desktop_bindings(canvas.app.world())
            .iter()
            .any(|binding| {
                binding.action == DesktopAction::ToggleFocus
                    && binding.key == KeyCode::Backspace
                    && binding.modifiers.super_key
                    && !binding.modifiers.alt
                    && !binding.modifiers.control
                    && !binding.modifiers.shift
            })
    );
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
    assert_eq!(
        canvas
            .world_mut()
            .query::<&native::map_castle::MapSand>()
            .iter(canvas.app.world())
            .count(),
        1
    );
    let shader = canvas
        .world_mut()
        .query_filtered::<Entity, With<native::shader_effects::Surface>>()
        .single(canvas.app.world())
        .unwrap();
    let shader_deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        canvas.probe_frame().unwrap();
        let state = native::shader_effects::surface_reading(canvas.app.world(), shader);
        if state
            .as_ref()
            .is_some_and(|state| state.ready && state.image_available && state.enabled)
        {
            break;
        }
        assert!(
            std::time::Instant::now() < shader_deadline,
            "The showoff WGSL mesh did not become ready: {state:?}"
        );
    }
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
    let before = probe_pixels(canvas.probe_frame().unwrap().dmabuf().unwrap()).unwrap();
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
    let colored_model_pixels = |pixels: &[u8], rectangle: Rect| {
        pixels
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
            .count()
    };
    assert!(
        colored_model_pixels(&before, rectangle) > 100,
        "The GLB did not contribute visible colored pixels"
    );
    capture("flat", &before);
    let (map, map_position, map_camera) = canvas
        .world_mut()
        .query::<(
            Entity,
            &native::map_castle::MapSand,
            &native::canvas::CanvasItem,
        )>()
        .iter(canvas.app.world())
        .map(|(entity, map, item)| (entity, item.position, map.camera))
        .next()
        .unwrap();
    *canvas
        .world_mut()
        .get_mut::<native::canvas::CanvasView>(root)
        .unwrap() = native::canvas::CanvasView {
        center: map_position,
        zoom: 0.8,
    };
    canvas.probe_frame().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !canvas
        .app
        .world()
        .resource::<lince_interface::map_view::Scenes>()
        .0
        .contains_key(&map)
    {
        assert!(
            std::time::Instant::now() < deadline,
            "The native map did not render"
        );
        canvas.probe_frame().unwrap();
    }
    for _ in 0..5 {
        canvas.probe_frame().unwrap();
    }
    capture(
        "map",
        &probe_pixels(canvas.probe_frame().unwrap().dmabuf().unwrap()).unwrap(),
    );
    overview_case(&mut canvas, root, space, model);
    assert_eq!(counts(canvas.world_mut()), (6, 1, 1));
    let viewport = canvas.world_mut().query_filtered::<(Entity, &UiGlobalTransform), (With<ImageNode>, With<bevy::ui::RelativeCursorPosition>)>()
        .iter(canvas.app.world()).find_map(|(entity, transform)| descendant(canvas.app.world(), entity, map).then(|| transform.to_scale_angle_translation().2.to_array())).unwrap();
    let world = canvas.app.world();
    let owner_center = world
        .get::<UiGlobalTransform>(map)
        .unwrap()
        .to_scale_angle_translation()
        .2;
    let owner_size = world.get::<ComputedNode>(map).unwrap().size();
    let relative = (Vec2::from_array(viewport) - owner_center) / owner_size + Vec2::splat(0.5);
    let bounds = native::topology::presentation::bounds(world, map).unwrap();
    let viewport = (bounds.min + relative * bounds.size()).to_array();
    canvas.pointer(viewport).unwrap();
    for _ in 0..2 {
        canvas.probe_frame().unwrap();
    }
    canvas
        .scroll(
            [0.0, 1.0],
            bevy::input::mouse::MouseScrollUnit::Line,
            bevy::input::touch::TouchPhase::Moved,
        )
        .unwrap();
    for _ in 0..4 {
        canvas.probe_frame().unwrap();
    }
    assert_ne!(
        canvas
            .app
            .world()
            .get::<native::map_castle::MapSand>(map)
            .unwrap()
            .camera
            .zoom,
        map_camera.zoom
    );
    *canvas
        .world_mut()
        .get_mut::<native::canvas::CanvasView>(root)
        .unwrap() = initial_view;
    canvas.probe_frame().unwrap();
    let other_workspace = canvas
        .app
        .world()
        .resource::<native::output_views::OutputViews>()
        .outputs
        .iter()
        .find(|output| output.id == lince_interface::host::OutputId(2))
        .unwrap()
        .workspace;
    {
        let mut spaces = canvas
            .world_mut()
            .get_mut::<native::workspace::Workspaces>(root)
            .unwrap();
        let other = spaces
            .entries
            .iter_mut()
            .find(|entry| entry.id == other_workspace)
            .unwrap();
        other.center = [1e12, -1e12];
        other.desktop.set_mode(WorkspaceLayout::Free);
    }
    canvas
        .focus_output(lince_interface::host::OutputId(2))
        .unwrap();
    for _ in 0..4 {
        canvas.probe_frame().unwrap();
    }
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::canvas::CanvasView>(root)
            .unwrap()
            .center,
        bevy::math::DVec2::new(1e12, -1e12)
    );
    assert_ne!(
        canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .active,
        space
    );
    let frame = canvas.probe_frame().unwrap();
    let passive = frame
        .outputs
        .iter()
        .find(|output| output.id == lince_interface::host::OutputId(1))
        .unwrap();
    let pixels = probe_pixels(passive.dmabuf()).unwrap();
    assert!(
        colored_model_pixels(&pixels, rectangle) > 100,
        "The GLB disappeared when focus moved to the other output"
    );
    drop(frame);
    canvas
        .focus_output(lince_interface::host::OutputId(1))
        .unwrap();
    for _ in 0..4 {
        canvas.probe_frame().unwrap();
    }
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::workspace::Workspaces>(root)
            .unwrap()
            .active,
        space
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
    let frame = super::shortcuts::settle(&mut canvas);
    capture("before-note-drag", &probe_pixels(frame.dmabuf().unwrap()).unwrap());
    let note = canvas
        .world_mut()
        .query::<(Entity, &native::sand_store::StoredSand)>()
        .iter(canvas.app.world())
        .find(|(_, sand)| sand.kind == native::sand_store::SandKind::EditableText)
        .unwrap()
        .0;
    let corners =
        native::topology::presentation::projected_corners(canvas.app.world(), note).unwrap();
    let point = [0.5, 0.25, 0.75, 0.1, 0.9]
        .into_iter()
        .flat_map(|u| {
            [0.5, 0.25, 0.75, 0.1, 0.9].into_iter().map(move |v| {
                std::array::from_fn(|axis| {
                    corners[0][axis] * (1.0 - u) * (1.0 - v)
                        + corners[1][axis] * u * (1.0 - v)
                        + corners[2][axis] * u * v
                        + corners[3][axis] * (1.0 - u) * v
                })
            })
        })
        .find(|point| {
            point[0] >= 0.0
                && point[0] < 1280.0
                && point[1] >= 0.0
                && point[1] < 720.0
                && !frame
                    .input
                    .controls
                    .iter()
                    .any(|bounds| bounds.contains(*point))
        })
        .expect("The note has no exposed draggable point beside the edit panel")
        .map(|axis| axis as f32);
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
    for layout in [WorkspaceLayout::Tiled, WorkspaceLayout::Free] {
        canvas
            .desktop_action(DesktopAction::SetLayout(layout))
            .unwrap();
        for _ in 0..3 {
            canvas.probe_frame().unwrap();
        }
        canvas
            .desktop_action(DesktopAction::Focus(
                lince_interface::host::Direction::Right,
            ))
            .unwrap();
        canvas.probe_frame().unwrap();
        assert!(canvas.geometry().unwrap().focused.is_some());
        canvas.desktop_action(DesktopAction::Unfocus).unwrap();
        canvas.probe_frame().unwrap();
        assert!(canvas.geometry().unwrap().focused.is_none());
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
        assert_eq!(
            canvas
                .app
                .world()
                .get::<native::canvas::CanvasItem>(model)
                .unwrap()
                .position,
            model_position
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
    canvas.probe_frame().unwrap();
    native::canvas_controls::CanvasAction::Recenter.apply(canvas.world_mut(), root);
    canvas.probe_frame().unwrap();
    assert_eq!(
        canvas
            .app
            .world()
            .get::<native::canvas::CanvasView>(root)
            .unwrap()
            .center,
        bevy::math::DVec2::ZERO,
        "The shared canvas recenter action did not restore the origin"
    );
    native::topology::ui::TopologyAction::ToggleView.apply(canvas.world_mut(), root);
    for _ in 0..5 {
        canvas.probe_frame().unwrap();
    }
    assert!(
        canvas
            .world_mut()
            .query::<(&native::shader_effects::RecordMesh, &Visibility)>()
            .iter(canvas.app.world())
            .any(|(owner, visibility)| owner.0 == shader && *visibility == Visibility::Visible),
        "The WGSL geometry was not placed in the 3D scene"
    );
    let model_bounds = native::topology::presentation::bounds(canvas.app.world(), model).unwrap();
    let spatial_pixels = probe_pixels(canvas.probe_frame().unwrap().dmabuf().unwrap()).unwrap();
    assert!(
        colored_model_pixels(&spatial_pixels, model_bounds) > 100,
        "The GLB disappeared when switching to 3D"
    );
    capture("spatial", &spatial_pixels);
    overview_case(&mut canvas, root, space, model);
    let geometry = canvas.probe_frame().unwrap().geometry.unwrap();
    if !geometry
        .items
        .iter()
        .any(|item| item.mapping.corners.is_some())
    {
        let entities: Vec<_> = canvas
            .world_mut()
            .query_filtered::<Entity, With<native::sand_store::StoredSand>>()
            .iter(canvas.app.world())
            .collect();
        let surfaces: Vec<_> = entities
            .into_iter()
            .map(|entity| {
                (
                    canvas
                        .app
                        .world()
                        .get::<native::canvas::CanvasItem>(entity)
                        .map(|item| (item.position, item.size)),
                    canvas
                        .app
                        .world()
                        .get::<native::topology::presentation::Surface>(entity)
                        .map(|surface| surface.visible),
                    native::topology::presentation::displayed(canvas.app.world(), entity),
                    native::topology::presentation::projected_corners(canvas.app.world(), entity),
                )
            })
            .collect();
        let mut image = b"P6\n1280 720\n255\n".to_vec();
        for pixel in spatial_pixels.chunks_exact(4) {
            image.extend_from_slice(&pixel[..3]);
        }
        std::fs::write(directory.path().join("spatial.ppm"), image).unwrap();
        let path = directory.keep();
        panic!("No projected surfaces: {surfaces:?}; geometry: {geometry:?}; artifacts: {path:?}");
    }
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
    let after = probe_pixels(canvas.probe_frame().unwrap().dmabuf().unwrap()).unwrap();
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
            .query::<&native::map_castle::MapSand>()
            .iter(restored.app.world())
            .count(),
        1
    );
    let effect = restored
        .world_mut()
        .query::<&native::shader_effects::Surface>()
        .single(restored.app.world())
        .unwrap();
    assert!(
        matches!(&effect.effect.mode, nucleus::shader_effect::spatial::Mode::Mesh(settings) if settings.geometry.is_some() && settings.deformation.is_some())
    );
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
            .query::<&Name>()
            .iter(restored.app.world())
            .filter(|name| name.as_str() == "showoff controls")
            .count(),
        0
    );
    restored.finish().unwrap();
}

fn overview_case(canvas: &mut Presentation, root: Entity, workspace: u64, model: Entity) {
    let spatial = *canvas
        .app
        .world()
        .get::<native::topology::view::View>(root)
        .unwrap();
    let view = *canvas
        .app
        .world()
        .get::<native::canvas::CanvasView>(root)
        .unwrap();
    for (key, pressed) in [
        (KeyCode::SuperLeft, true),
        (KeyCode::Space, true),
        (KeyCode::Space, false),
        (KeyCode::SuperLeft, false),
    ] {
        super::shortcuts::key(canvas, key, pressed);
    }
    let frame = super::shortcuts::settle(canvas);
    assert!(frame.input.keyboard_modal);
    let world = canvas.world_mut();
    let preview = world
        .query::<(Entity, &Name, &ImageNode)>()
        .iter(world)
        .find(|(_, name, _)| name.as_str() == format!("overview.workspace.{workspace}"))
        .map(|(entity, _, image)| (entity, image.image.clone()))
        .unwrap();
    let (camera, transform, layers) = world.query::<(&Camera, &GlobalTransform, &bevy::camera::RenderTarget, &bevy::camera::visibility::RenderLayers)>().iter(world)
        .find(|(_, _, target, _)| matches!(target, bevy::camera::RenderTarget::Image(image) if image.handle == preview.1)).map(|(camera, transform, _, layers)| (camera, transform, layers.clone())).unwrap();
    let point = camera
        .world_to_viewport(
            transform,
            world.get::<GlobalTransform>(model).unwrap().translation(),
        )
        .unwrap();
    let node = world.get::<ComputedNode>(preview.0).unwrap();
    let origin = world
        .get::<UiGlobalTransform>(preview.0)
        .unwrap()
        .transform_point2(Vec2::ZERO)
        - node.size() * 0.5;
    let image_size = world
        .resource::<Assets<Image>>()
        .get(&preview.1)
        .unwrap()
        .size()
        .as_vec2();
    let point = origin + point / image_size * node.size();
    assert!(origin.y >= 0.0 && origin.y + node.size().y <= 720.0, "The current workspace preview was not fully revealed: {origin:?}, {:?}", node.size());
    let rectangle = Rect::from_center_size(point, Vec2::splat(24.0));
    let pixels = probe_pixels(frame.dmabuf().unwrap()).unwrap();
    let colored = pixels
        .chunks_exact(4)
        .enumerate()
        .filter(|(index, pixel)| {
            rectangle.contains(Vec2::new((index % 1280) as f32, (index / 1280) as f32))
                && pixel[..3]
                    .iter()
                    .max()
                    .unwrap()
                    .abs_diff(*pixel[..3].iter().min().unwrap())
                    > 60
        })
        .count();
    assert!(
        colored > 10,
        "The actual GLB is missing from the workspace preview at {point:?}: {colored}"
    );
    if spatial.spatial {
        assert!(
            world
                .query::<(
                    &native::shader_effects::RecordMesh,
                    &bevy::camera::visibility::RenderLayers,
                    &Visibility
                )>()
                .iter(world)
                .any(|(mesh, rendered, visibility)| {
                    world
                        .get::<native::workspace::WorkspaceMember>(mesh.0)
                        .is_some_and(|member| member.0 == workspace)
                        && rendered.intersects(&layers)
                        && *visibility == Visibility::Visible
                }),
            "The spatial WGSL mesh is missing from overview's render layers"
        );
    }
    capture(
        if spatial.spatial {
            "overview-spatial"
        } else {
            "overview-flat"
        },
        &pixels,
    );
    super::shortcuts::key(canvas, KeyCode::Escape, true);
    super::shortcuts::key(canvas, KeyCode::Escape, false);
    assert!(!super::shortcuts::settle(canvas).input.keyboard_modal);
    let restored = canvas
        .app
        .world()
        .get::<native::canvas::CanvasView>(root)
        .unwrap();
    assert_eq!(restored.center, view.center);
    assert_eq!(restored.zoom, view.zoom);
    assert_eq!(
        *canvas
            .app
            .world()
            .get::<native::topology::view::View>(root)
            .unwrap(),
        spatial
    );
}

fn capture(name: &str, pixels: &[u8]) {
    let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let mut image = b"P6\n1280 720\n255\n".to_vec();
    for pixel in pixels.chunks_exact(4) {
        image.extend_from_slice(&pixel[..3]);
    }
    std::fs::write(directory.join(format!("showoff-{name}.ppm")), image).unwrap();
}

fn descendant(world: &World, mut entity: Entity, parent: Entity) -> bool {
    while let Some(relation) = world.get::<ChildOf>(entity) {
        entity = relation.parent();
        if entity == parent {
            return true;
        }
    }
    false
}
