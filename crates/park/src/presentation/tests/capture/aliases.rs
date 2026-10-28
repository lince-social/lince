use super::*;
use lince_interface::host::{OutputViewport, Rectangle};
use native::{layout::Rules, topology::presentation::Surface};

fn card(canvas: &mut Presentation, root: Entity, workspace: u64, size: Vec2) -> (Entity, Entity) {
    let card = canvas.world_mut().spawn((
        CanvasItem { position: bevy::math::DVec2::ZERO, size }, LayoutBox::new(size),
        Node::default(), BackgroundColor(Color::srgb(0.0, 1.0, 0.0)), WorkspaceMember(workspace), ChildOf(root),
    )).id();
    let mut layout = LayoutBox::new(Vec2::splat(160.0));
    layout.capture_excluded = true;
    let child = canvas.world_mut().spawn((Node { width: px(160), height: px(160), ..default() }, layout,
        BackgroundColor(Color::srgb(1.0, 0.0, 0.7)), ChildOf(card))).id();
    (card, child)
}

fn displayed(canvas: &Presentation, card: Entity) -> Handle<Image> {
    let world = canvas.app.world();
    let surface = world.get::<Surface>(card).unwrap();
    world.resource::<Assets<StandardMaterial>>().get(&surface.material).unwrap().base_color_texture.clone().unwrap()
}

fn pause(canvas: &mut Presentation, card: Entity) -> Entity {
    let camera = canvas.app.world().get::<Surface>(card).unwrap().camera;
    canvas.app.insert_resource(PausedSurface(Some(camera))).init_resource::<Updates>()
        .add_systems(Update, count_updates)
        .add_systems(PostUpdate, pause_surface.after(native::topology::presentation::synchronize).before(bevy::camera::CameraUpdateSystems));
    camera
}

fn resize(canvas: &mut Presentation, card: Entity, size: Vec2) {
    canvas.world_mut().get_mut::<CanvasItem>(card).unwrap().size = size;
    canvas.world_mut().get_mut::<LayoutBox>(card).unwrap().rules = Rules::fixed(size);
}

fn outputs(workspace: u64, companion: bool) -> Vec<Output> {
    (1..=if companion { 2 } else { 1 }).map(|id| {
        let bounds = Rectangle::new([640.0 * (id - 1) as f64, 0.0], [640.0, 480.0]).unwrap();
        Output { name: format!("Capture-Preview-{id}"), viewport: OutputViewport {
            id: OutputId(id), bounds, usable: Some(bounds), physical_size: [640, 480],
            raster: lince_interface::host::raster::RasterMapping { origin: bounds.origin(), scale: [1.0; 2] },
            rotation: Default::default(), workspace: if id == 1 { 1 } else { workspace },
        } }
    }).collect()
}

fn filtered(canvas: &mut Presentation, outputs: &[OutputId]) -> Frame {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let before = canvas.app.world().resource::<Updates>().0;
        let frame = canvas.probe_frame().unwrap();
        assert_eq!(canvas.app.world().resource::<Updates>().0, before + 1);
        frame.wait_for_probe().unwrap();
        if frame.capture_source().is_some_and(|stamp| outputs.iter()
            .all(|output| frame.capture_dmabuf(*output, stamp.revision, stamp.mapping_revision).is_some()))
        {
            for output in outputs {
                let captured = pixels(&frame, *output);
                assert_eq!(magenta(&captured), 0, "Private retained pixels leaked through {output:?}");
                assert!(green(&captured) > 200, "The public retained preview disappeared from {output:?}");
            }
            return frame;
        }
        assert!(Instant::now() < deadline, "The retained preview never acquired a real filtered completion");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn a_real_size_transition_alias_keeps_its_owner_pixels_and_gets_fresh_public_capture_without_another_update() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut canvas = Presentation::new([1000, 720], directory.path().join("assets"), WakeSignal::new(|| {})).unwrap();
    super::super::shortcuts::settle(&mut canvas);
    let root = canvas.canvas_root().unwrap();
    native::workspace_config::set_physics(canvas.world_mut(), root, 1, false);
    canvas.desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free)).unwrap();
    let (item, child) = card(&mut canvas, root, 1, Vec2::new(480.0, 300.0));
    let frame = super::super::shortcuts::settle(&mut canvas);
    assert!(magenta(&probe_pixels(frame.dmabuf().unwrap()).unwrap()) > 200);
    let alias = displayed(&canvas, item);
    let retained = super::super::layers::image_pixels(&canvas, alias.id());
    assert!(magenta(&retained.1) > 200);
    let camera = pause(&mut canvas, item);
    canvas.world_mut().despawn(child);
    resize(&mut canvas, item, Vec2::new(600.0, 375.0));
    let gate = PolicyGate::default();
    gate.invalidate();
    canvas.world_mut().insert_resource(gate.clone());
    canvas.world_mut().insert_resource(Demand { outputs: BTreeSet::from([OutputId(1)]), ..Default::default() });
    for spatial in [false, true] {
        canvas.world_mut().get_mut::<native::topology::view::View>(root).unwrap().spatial = spatial;
        for _ in 0..3 {
            let frame = filtered(&mut canvas, &[OutputId(1)]);
            assert!(magenta(&probe_pixels(frame.dmabuf().unwrap()).unwrap()) > 200);
            assert_eq!(displayed(&canvas, item), alias);
            assert_ne!(canvas.app.world().get::<Surface>(item).unwrap().image, alias);
            assert_eq!(super::super::layers::image_pixels(&canvas, alias.id()), retained);
            assert!(!canvas.app.world().get::<Camera>(camera).unwrap().is_active);
            assert!(gate.read().unwrap().requires_filter());
        }
    }
    canvas.world_mut().resource_mut::<PausedSurface>().0 = None;
    let frame = super::super::shortcuts::settle(&mut canvas);
    assert_eq!(magenta(&probe_pixels(frame.dmabuf().unwrap()).unwrap()), 0);
    assert_ne!(displayed(&canvas, item), alias);
    canvas.world_mut().despawn(item);
    let (replacement, private) = card(&mut canvas, root, 1, Vec2::new(480.0, 300.0));
    canvas.world_mut().despawn(private);
    let frame = filtered(&mut canvas, &[OutputId(1)]);
    assert_ne!(canvas.app.world().get::<Surface>(replacement).unwrap().camera, camera);
    assert!(green(&probe_pixels(frame.dmabuf().unwrap()).unwrap()) > 200);
    canvas.finish().unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn inactive_overview_nested_preview_and_companion_output_follow_the_real_retained_source_through_activation_and_replacement() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut canvas = Presentation::new([640, 480], directory.path().join("assets"), WakeSignal::new(|| {})).unwrap();
    super::super::shortcuts::settle(&mut canvas);
    let root = canvas.canvas_root().unwrap();
    canvas.world_mut().remove_resource::<native::layout::desktop::DefaultLayout>();
    native::workspace::create(canvas.world_mut(), root);
    let workspace = canvas.app.world().get::<native::workspace::Workspaces>(root).unwrap().active;
    for id in [1, workspace] {
        canvas.world_mut().get_mut::<native::workspace::Workspaces>(root).unwrap().entries.iter_mut()
            .find(|entry| entry.id == id).unwrap().desktop.set_mode(WorkspaceLayout::Free);
        native::workspace_config::set_physics(canvas.world_mut(), root, id, false);
    }
    let (item, child) = card(&mut canvas, root, workspace, Vec2::new(400.0, 250.0));
    super::super::shortcuts::settle(&mut canvas);
    let alias = displayed(&canvas, item);
    assert!(magenta(&super::super::layers::image_pixels(&canvas, alias.id()).1) > 200);
    let nested = canvas.world_mut().spawn((CanvasItem { position: bevy::math::DVec2::ZERO, size: Vec2::new(320.0, 200.0) },
        LayoutBox::new(Vec2::new(320.0, 200.0)), Node::default(), WorkspaceMember(1), ChildOf(root))).id();
    let nested_image = canvas.world_mut().spawn((ImageNode::new(alias.clone()), Node { width: percent(100), height: percent(100), ..default() }, ChildOf(nested))).id();
    let camera = pause(&mut canvas, item);
    canvas.world_mut().despawn(child);
    resize(&mut canvas, item, Vec2::new(480.0, 300.0));
    native::workspace::switch_output(canvas.world_mut(), root, 1);
    canvas.outputs(outputs(workspace, false)).unwrap();
    canvas.desktop_action(DesktopAction::ToggleOverview).unwrap();
    let gate = PolicyGate::default();
    gate.invalidate();
    canvas.world_mut().insert_resource(gate);
    for companion in [false, true] {
        if companion { canvas.outputs(outputs(workspace, true)).unwrap(); }
        let requested = if companion { vec![OutputId(1), OutputId(2)] } else { vec![OutputId(1)] };
        canvas.world_mut().insert_resource(Demand { outputs: requested.iter().copied().collect(), ..Default::default() });
        for _ in 0..2 {
            let frame = filtered(&mut canvas, &requested);
            assert!(frame.input.keyboard_modal);
            if !companion {
                assert!(frame.geometry.as_ref().unwrap().outputs.iter().all(|output| output.workspace != workspace),
                    "The supposedly inactive overview source remained active on an output");
            }
            assert!(native::layout::desktop::overview::active(canvas.app.world()));
            let captured = pixels(&frame, OutputId(1));
            for preview in [1, workspace] {
                let rectangle = {
                    let world = canvas.world_mut();
                    let (entity, _, _) = world.query::<(Entity, &Name, &ImageNode)>().iter(world)
                        .find(|(_, name, _)| name.as_str() == format!("overview.workspace.{preview}")).unwrap();
                    Rect::from_center_size(world.get::<UiGlobalTransform>(entity).unwrap().transform_point2(Vec2::ZERO),
                        world.get::<ComputedNode>(entity).unwrap().size())
                };
                let green = captured.chunks_exact(4).enumerate().filter(|(index, pixel)| {
                    rectangle.contains(Vec2::new((index % 640) as f32 + 0.5, (index / 640) as f32 + 0.5))
                        && pixel[0] < 40 && pixel[1] > 220 && pixel[2] < 40
                }).count();
                assert!(green > 100, "Preview {preview} was blank even though another workspace remained public");
            }
            assert_eq!(displayed(&canvas, item), alias);
            assert_eq!(canvas.app.world().get::<ImageNode>(nested_image).unwrap().image, alias);
            assert!(!canvas.app.world().get::<Camera>(camera).unwrap().is_active);
            if companion {
                let owner = frame.outputs.iter().find(|output| output.id == OutputId(2)).unwrap();
                assert!(magenta(&probe_pixels(owner.dmabuf()).unwrap()) > 200);
            }
        }
    }
    canvas.desktop_action(DesktopAction::ToggleOverview).unwrap();
    canvas.world_mut().resource_mut::<PausedSurface>().0 = None;
    canvas.activate_workspace(root.to_bits(), workspace).unwrap();
    let frame = filtered(&mut canvas, &[OutputId(1), OutputId(2)]);
    assert!(!frame.input.keyboard_modal);
    assert_ne!(displayed(&canvas, item), alias);
    let current = displayed(&canvas, item);
    canvas.world_mut().get_mut::<ImageNode>(nested_image).unwrap().image = current;
    canvas.world_mut().despawn(item);
    let (replacement, private) = card(&mut canvas, root, workspace, Vec2::new(400.0, 250.0));
    canvas.world_mut().despawn(private);
    super::super::shortcuts::settle(&mut canvas);
    let image = displayed(&canvas, replacement);
    canvas.world_mut().get_mut::<ImageNode>(nested_image).unwrap().image = image;
    filtered(&mut canvas, &[OutputId(1), OutputId(2)]);
    canvas.finish().unwrap();
}
