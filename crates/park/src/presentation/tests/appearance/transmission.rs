use super::*;
use bevy::{
    pbr::{MeshViewBindGroup, ScreenSpaceTransmission, Transmissive3d, ViewTransmissionTexture},
    render::{Render, RenderSystems, render_phase::ViewSortedRenderPhases, sync_world::RenderEntity, view::ExtractedView},
};

#[derive(Resource, Default)]
struct OwnerBindings(BTreeMap<Entity, [String; 5]>);

fn bindings(texture: &ViewTransmissionTexture, group: &MeshViewBindGroup) -> [String; 5] {
    [format!("{:?}", texture.texture.id()), format!("{:?}", group.main.id()), format!("{:?}", group.binding_array.id()),
        format!("{:?}", group.empty.id()), format!("{:?}", group.main_offsets)]
}

fn remember_owner_bindings(
    external: Option<Res<native::capture::render::ExternalPass>>,
    mut snapshot: ResMut<OwnerBindings>,
    views: Query<(Entity, &ViewTransmissionTexture, &MeshViewBindGroup)>,
) {
    if external.is_some() { return; }
    snapshot.0 = views.iter().map(|(entity, texture, group)| (entity, bindings(texture, group))).collect();
}

fn green(pixels: &[u8]) -> usize {
    pixels.chunks_exact(4).filter(|pixel| pixel[0] < 20 && pixel[1] > 230 && pixel[2] < 20).count()
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn real_glass_transmission_keeps_its_texture_and_sharp_foreground_when_backdrop_blur_is_enabled() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let size = [1000, 720];
    let mut canvas = launch(&directory.path().join("transmission.json"), size);
    let root = canvas.canvas_root().unwrap();
    native::workspace_config::set_physics(canvas.world_mut(), root, 1, false);
    canvas.desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free)).unwrap();
    *canvas.world_mut().get_mut::<CanvasView>(root).unwrap() = CanvasView::default();
    let camera = canvas.app.world().resource::<native::topology::presentation::SceneCamera>().0;
    canvas.app.sub_app_mut(RenderApp).init_resource::<OwnerBindings>()
        .add_systems(Render, remember_owner_bindings.after(RenderSystems::Prepare).before(RenderSystems::Render));
    canvas.world_mut().entity_mut(camera).insert((ScreenSpaceTransmission::default(), Msaa::Off));
    let background = canvas.world_mut().resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial {
        base_color: Color::srgb(0.9, 0.08, 0.04), unlit: true, ..default()
    });
    let mesh = canvas.world_mut().resource_mut::<Assets<Mesh>>().add(Cuboid::new(320.0, 10.0, 280.0));
    canvas.world_mut().spawn((Mesh3d(mesh), MeshMaterial3d(background.clone()), Transform::from_xyz(120.0, -20.0, 0.0)));
    let glass = canvas.world_mut().resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial {
        base_color: Color::WHITE, specular_transmission: 0.95, thickness: 3.0,
        ior: 1.15, perceptual_roughness: 0.05, ..default()
    });
    let mesh = canvas.world_mut().resource_mut::<Assets<Mesh>>().add(Sphere::new(110.0));
    canvas.world_mut().spawn((Mesh3d(mesh), MeshMaterial3d(glass), Transform::from_xyz(120.0, 110.0, 0.0)));
    let front = item(&mut canvas, root, Vec2::new(180.0, 140.0), [120.0, 0.0], Color::srgba(0.0, 0.0, 0.0, 0.2), 2, false);
    canvas.world_mut().entity_mut(front).insert(native::topology::Spatial { elevation: 320.0, ..default() });
    canvas.world_mut().spawn((Node { position_type: PositionType::Absolute, left: px(85), top: px(15), width: px(10), height: px(110), ..default() },
        BackgroundColor(Color::srgb(0.0, 1.0, 0.0)), ChildOf(front)));
    let id = ItemId::Native(canvas.app.world().get::<LayoutBox>(front).unwrap().id);
    let deadline = Instant::now() + Duration::from_secs(60);
    let (off, geometry, transmission) = loop {
        let frame = canvas.probe_frame().unwrap();
        let pixels = probe_pixels(frame.dmabuf().unwrap()).unwrap();
        let render_camera = canvas.app.world().get::<RenderEntity>(camera).unwrap().id();
        let render = canvas.app.sub_app(RenderApp).world();
        let view = render.get::<ExtractedView>(render_camera);
        let phase = view.and_then(|view| render.resource::<ViewSortedRenderPhases<Transmissive3d>>().get(&view.retained_view_entity));
        if green(&pixels) > 600 && phase.is_some_and(|phase| !phase.items.is_empty())
            && let Some(texture) = render.get::<ViewTransmissionTexture>(render_camera) {
            let geometry = frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == id).unwrap().clone();
            break (pixels, geometry, texture.texture.clone());
        }
        assert!(Instant::now() < deadline, "The real transmissive material and sharp foreground did not become visible");
    };
    open(&mut canvas);
    apply(&mut canvas, BLUR, "8", size);
    close(&mut canvas, size);
    let deadline = Instant::now() + Duration::from_secs(15);
    let on = loop {
        let frame = canvas.probe_frame().unwrap();
        let pixels = probe_pixels(frame.dmabuf().unwrap()).unwrap();
        let changed = off.chunks_exact(4).zip(pixels.chunks_exact(4)).enumerate().filter(|(index, (before, after))| {
            let point = [(index % size[0] as usize) as f64 + 0.5, (index / size[0] as usize) as f64 + 0.5];
            geometry.surface_point(point).is_some() && (0..3).any(|channel| before[channel].abs_diff(after[channel]) > 4)
        }).count();
        if decorations(&mut canvas, front) == 1 && changed > 40 {
            assert_eq!(frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == id), Some(&geometry));
            break pixels;
        }
        assert!(Instant::now() < deadline, "Backdrop blur never changed actual pixels beside the real transmission pass: {changed} changed pixels");
    };
    assert!(green(&on).abs_diff(green(&off)) <= 4, "The blur pass moved ahead of transmission or changed sharp foreground pixels");
    let render_camera = canvas.app.world().get::<RenderEntity>(camera).unwrap().id();
    let render = canvas.app.sub_app(RenderApp).world();
    assert_eq!(render.get::<ViewTransmissionTexture>(render_camera).unwrap().texture.id(), transmission.id(), "Backdrop blur replaced the transmission texture");
    let origin = geometry.mapping.surface.origin();
    let extent = geometry.mapping.surface.size();
    let point = geometry.mapping.canvas_point([origin[0] + extent[0] * 0.5 + 100.0, origin[1] + extent[1] * 0.5]).unwrap();
    let point = point.map(|value| value.floor() as u32);
    assert!(point[0] < size[0] && point[1] < size[1]);
    let index = (point[1] as usize * size[0] as usize + point[0] as usize) * 4;
    let before = &on[index..index + 4];
    assert!(before[0] > before[2].saturating_add(30), "The declared glass reference did not transmit the actual red backing: {before:?}");
    canvas.world_mut().resource_mut::<Assets<StandardMaterial>>().get_mut(&background).unwrap().base_color = Color::srgb(0.03, 0.08, 0.95);
    let deadline = Instant::now() + Duration::from_secs(15);
    let changed = loop {
        let frame = canvas.probe_frame().unwrap();
        let pixels = probe_pixels(frame.dmabuf().unwrap()).unwrap();
        let after = &pixels[index..index + 4];
        if after[2] > after[0].saturating_add(30) && before[0].abs_diff(after[0]) > 30 { break pixels; }
        assert!(Instant::now() < deadline, "The actual glass stopped following fresh backing content after blur: {after:?}");
    };
    assert!(green(&changed).abs_diff(green(&off)) <= 4);
    super::super::layers::capture("appearance-glass-off", size, &off);
    super::super::layers::capture("appearance-glass-on", size, &on);
    super::super::layers::capture("appearance-glass-fresh", size, &changed);
    let private = item(&mut canvas, root, Vec2::new(140.0, 120.0), [-220.0, 0.0], Color::srgb(1.0, 0.0, 1.0), 3, true);
    let gate = PolicyGate::default();
    gate.invalidate();
    canvas.world_mut().insert_resource(gate);
    canvas.world_mut().insert_resource(Demand { outputs: BTreeSet::from([OutputId(1)]), ..Default::default() });
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let frame = canvas.probe_frame().unwrap();
        let owner = probe_pixels(frame.dmabuf().unwrap()).unwrap();
        if owner.chunks_exact(4).filter(|pixel| pixel[0] > 230 && pixel[1] < 30 && pixel[2] > 230).count() > 200
            && frame.capture_source().is_some() {
            let safe = super::super::capture::pixels(&frame, OutputId(1));
            assert_eq!(safe.chunks_exact(4).filter(|pixel| pixel[0] > 230 && pixel[1] < 30 && pixel[2] > 230).count(), 0);
            assert!(green(&safe).abs_diff(green(&owner)) <= 4);
            let render_camera = canvas.app.world().get::<RenderEntity>(camera).unwrap().id();
            let render = canvas.app.sub_app(RenderApp).world();
            let normal = &render.resource::<OwnerBindings>().0[&render_camera];
            let restored = bindings(render.get::<ViewTransmissionTexture>(render_camera).unwrap(), render.get::<MeshViewBindGroup>(render_camera).unwrap());
            assert_eq!(&restored, normal, "Filtered rendering retained capture transmission textures or material view bindings in the owner view");
            assert_eq!(render.get::<ViewTransmissionTexture>(render_camera).unwrap().texture.id(), transmission.id());
            for channel in 0..4 { assert!(owner[index + channel].abs_diff(safe[index + channel]) <= 2, "Filtered capture changed public glass pixels"); }
            super::super::layers::capture("appearance-glass-filtered", size, &safe);
            break;
        }
        assert!(Instant::now() < deadline, "The private-source filtered glass capture never became ready");
    }
    canvas.world_mut().despawn(private);
    canvas.world_mut().insert_resource(Demand::default());
    drop(canvas.probe_frame().unwrap());
    canvas.finish().unwrap();
}
