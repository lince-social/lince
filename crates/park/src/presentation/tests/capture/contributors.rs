use super::*;
use bevy::pbr::StandardMaterial;
use nucleus::shader_effect::{Effect, spatial::{GeneratedGeometry, MeshSettings, Mode}};

fn effect(private: bool) -> Effect {
    let program = if private { "fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(1.0, 0.0, 0.7, 1.0); }" }
        else { "fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(0.0, 1.0, 0.0, 1.0); }" };
    Effect { mode:Mode::Mesh(MeshSettings { geometry:Some(GeneratedGeometry { source:nucleus::shader_effect::spatial::GENERATED_SPHERE.into(), vertices:32 * 20 * 6 }), ..default() }),
        programs:std::collections::BTreeMap::from([("capture".into(), program.into())]), fallback:"capture".into(), ..default() }
}

fn shader(canvas: &mut Presentation, root: Entity, position: [f64; 2], private_color: bool) -> Entity {
    canvas.world_mut().spawn((CanvasItem { position:bevy::math::DVec2::from_array(position), size:Vec2::new(300.0, 240.0) }, LayoutBox::new(Vec2::new(300.0, 240.0)),
        Node::default(), WorkspaceMember(1), ChildOf(root), native::shader_effects::Surface { effect:effect(private_color), strength:1.0, motion:false, frozen:true, ..default() })).id()
}

fn descendant(world: &World, mut entity: Entity, ancestor: Entity) -> bool {
    for _ in 0..native::layout::MAX_DEPTH {
        if entity == ancestor { return true; }
        let Some(parent) = world.get::<ChildOf>(entity) else { return false; };
        entity = parent.parent();
    }
    false
}

fn frame(canvas: &mut Presentation) -> (Frame, Vec<u8>) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let updates = canvas.app.world().resource::<Updates>().0;
        let frame = canvas.probe_frame().unwrap();
        assert_eq!(canvas.app.world().resource::<Updates>().0, updates + 1, "Capture performed another main update for a detached contributor");
        if frame.capture_source().is_some_and(|stamp| frame.capture_dmabuf(OutputId(1), stamp.revision, stamp.mapping_revision).is_some()) {
            let pixels = pixels(&frame, OutputId(1));
            return (frame, pixels);
        }
        assert!(Instant::now() < deadline, "The actual imported/shader contributor never produced an eligible capture");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn imported_glb_and_generated_wgsl_meshes_follow_their_real_sources_in_capture_and_a_replacement_keeps_its_public_pixels() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut canvas = Presentation::new([1000, 720], directory.path().join("assets"), WakeSignal::new(|| {})).unwrap();
    super::super::shortcuts::settle(&mut canvas);
    let root = canvas.canvas_root().unwrap();
    native::workspace_config::set_physics(canvas.world_mut(), root, 1, false);
    canvas.desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free)).unwrap();
    let path = directory.path().join("private-source.glb");
    std::fs::write(&path, native::laboratory::showoff::MODEL).unwrap();
    let assets = canvas.app.world().resource::<native::topology::assets::AssetDirectory>().0.clone();
    let asset = native::topology::assets::copy_import(&path, &assets).unwrap();
    let model = native::topology::assets::spawn(canvas.world_mut(), root, 1, bevy::math::DVec2::new(-240.0, 120.0), asset);
    canvas.world_mut().entity_mut(model).insert(native::topology::assets::CenteredAsset);
    let mesh = shader(&mut canvas, root, [-240.0, -160.0], true);
    let public = super::super::appearance::item(&mut canvas, root, Vec2::new(240.0, 320.0), [230.0, 0.0], Color::srgb(0.0, 1.0, 0.0), 0, false);
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        canvas.probe_frame().unwrap();
        if canvas.app.world().get::<native::topology::assets::Ready>(model).is_some()
            && native::shader_effects::surface_reading(canvas.app.world(), mesh).is_some_and(|reading| reading.ready && reading.image_available && reading.error.is_empty()) { break; }
        assert!(Instant::now() < deadline, "The real GLB or generated WGSL mesh did not become ready");
    }
    let (camera, _) = shader_target(&mut canvas, mesh).expect("The generated WGSL preview must declare its semantic source");
    let world = canvas.world_mut();
    if descendant(world, camera, mesh) {
        let transform = world.get::<GlobalTransform>(camera).unwrap().compute_transform();
        world.entity_mut(camera).remove::<ChildOf>().insert(transform);
    }
    assert!(!descendant(world, camera, mesh));
    assert!(native::capture::semantic_sources(world, camera).contains(&mesh), "The detached renderer must resolve its actual source rather than a guessed parent or title");
    let world = canvas.world_mut();
    let scene = world.get::<native::topology::assets::ImportedScene>(model).unwrap().0;
    let materials = world.query::<(Entity, &MeshMaterial3d<StandardMaterial>)>().iter(world).filter_map(|(entity, material)| descendant(world, entity, scene).then_some(material.0.clone())).collect::<Vec<_>>();
    assert!(!materials.is_empty(), "The fixture must use real imported GLB mesh materials");
    for handle in materials {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let mut material = materials.get_mut(&handle).unwrap();
        material.base_color = Color::srgb(1.0, 0.0, 0.7);
        material.base_color_texture = None;
        material.unlit = true;
    }
    let gate = PolicyGate::default();
    gate.invalidate();
    canvas.world_mut().insert_resource(gate.clone());
    canvas.world_mut().insert_resource(Demand { outputs: BTreeSet::from([OutputId(1)]), ..Default::default() });
    canvas.app.init_resource::<Updates>().add_systems(Update, count_updates);
    for spatial in [false, true] {
        canvas.world_mut().get_mut::<native::topology::view::View>(root).unwrap().spatial = spatial;
        for mode in [WorkspaceLayout::Free, WorkspaceLayout::Tiled] {
            canvas.desktop_action(DesktopAction::SetLayout(mode)).unwrap();
            for (target, other) in [(model, mesh), (mesh, model)] {
                canvas.world_mut().get_mut::<Node>(other).unwrap().display = Display::None;
                canvas.world_mut().get_mut::<Node>(target).unwrap().display = Display::Flex;
                canvas.world_mut().get_mut::<LayoutBox>(target).unwrap().capture_excluded = false;
                gate.invalidate();
                let (before, output) = frame(&mut canvas);
                assert!(magenta(&probe_pixels(before.dmabuf().unwrap()).unwrap()) > 100, "The actual {target:?} model had no distinctive owner pixels in {mode:?}/{spatial}");
                assert!(magenta(&output) > 100, "An explicitly public imported/WGSL source was blanket-redacted");
                canvas.world_mut().get_mut::<LayoutBox>(target).unwrap().capture_excluded = true;
                gate.invalidate();
                let (after, output) = frame(&mut canvas);
                assert!(magenta(&probe_pixels(after.dmabuf().unwrap()).unwrap()) > 100, "Exclusion removed the local owner source");
                assert_eq!(magenta(&output), 0, "Detached imported/WGSL pixels leaked in {mode:?}/{spatial}");
                assert!(green(&output) > 100, "The public control disappeared with its unrelated private source");
            }
        }
    }
    canvas.world_mut().get_mut::<Node>(model).unwrap().display = Display::Flex;
    canvas.world_mut().get_mut::<LayoutBox>(model).unwrap().capture_excluded = true;
    canvas.world_mut().get_mut::<Node>(public).unwrap().display = Display::None;
    assert!(canvas.world_mut().despawn(mesh));
    let replacement = shader(&mut canvas, root, [230.0, 0.0], false);
    gate.invalidate();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let (owner, output) = frame(&mut canvas);
        if native::shader_effects::surface_reading(canvas.app.world(), replacement).is_some_and(|reading| reading.ready && reading.image_available) {
            assert!(magenta(&probe_pixels(owner.dmabuf().unwrap()).unwrap()) > 100);
            assert_eq!(magenta(&output), 0);
            assert!(green(&output) > 100, "A replacement with a new semantic source retained private history or lost its public pixels");
            break;
        }
        assert!(Instant::now() < deadline, "The real replacement shader never became capture-ready");
    }
    canvas.world_mut().insert_resource(Demand::default());
    canvas.finish().unwrap();
}
