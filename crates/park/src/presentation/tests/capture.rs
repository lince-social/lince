use super::*;
use lince_interface::{host::{capture::{Demand, PolicyGate}, DesktopAction, OutputId, WorkspaceLayout}, native::{canvas::CanvasItem, layout::LayoutBox, workspace::WorkspaceMember}};
use std::{collections::BTreeSet, time::{Duration, Instant}};

mod contributors;
mod aliases;

#[derive(Resource, Default)]
struct Updates(u64);

fn count_updates(mut updates: ResMut<Updates>) { updates.0 += 1; }

pub(super) fn pixels(frame: &Frame, output: OutputId) -> Vec<u8> {
    let stamp = frame.capture_source().expect("the capture frame has no policy stamp");
    let deadline = Instant::now() + Duration::from_secs(3);
    while !frame.capture_ready(output).unwrap() {
        assert!(Instant::now() < deadline, "The filtered GPU frame never became available");
        std::thread::sleep(Duration::from_millis(1));
    }
    probe_pixels(frame.capture_dmabuf(output, stamp.revision, stamp.mapping_revision).unwrap()).unwrap()
}

fn magenta(pixels: &[u8]) -> usize {
    pixels.chunks_exact(4).filter(|pixel| pixel[0] > 220 && pixel[1] < 40 && pixel[2] > 140).count()
}

fn green(pixels: &[u8]) -> usize {
    pixels.chunks_exact(4).filter(|pixel| pixel[0] < 40 && pixel[1] > 220 && pixel[2] < 40).count()
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn native_capture_omits_private_pixels_preserves_public_updates_and_runs_one_main_update() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut canvas = Presentation::new([1000, 720], directory.path().join("assets"), WakeSignal::new(|| {})).unwrap();
    super::shortcuts::settle(&mut canvas);
    let root = canvas.canvas_root().unwrap();
    native::workspace_config::set_physics(canvas.world_mut(), root, 1, false);
    canvas.desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free)).unwrap();
    let dimensions = Vec2::new(260.0, 180.0);
    let mut items = Vec::new();
    for (position, color, private) in [([-170.0, 0.0], Color::srgb(1.0, 0.0, 0.7), true), ([170.0, 0.0], Color::srgb(0.0, 1.0, 0.0), false)] {
        let mut layout = LayoutBox::new(dimensions);
        layout.capture_excluded = private;
        items.push(canvas.world_mut().spawn((CanvasItem { position: bevy::math::DVec2::from_array(position), size: dimensions }, layout, Node::default(), BackgroundColor(color), WorkspaceMember(1), ChildOf(root))).id());
    }
    let gate = PolicyGate::default();
    gate.invalidate();
    canvas.world_mut().insert_resource(gate);
    canvas.world_mut().insert_resource(Demand { outputs: BTreeSet::from([OutputId(1)]), ..Default::default() });
    canvas.app.init_resource::<Updates>().add_systems(Update, count_updates);
    for spatial in [false, true] {
        canvas.world_mut().get_mut::<native::topology::view::View>(root).unwrap().spatial = spatial;
        for mode in [WorkspaceLayout::Free, WorkspaceLayout::Tiled] {
            canvas.desktop_action(DesktopAction::SetLayout(mode)).unwrap();
            let frame = super::shortcuts::settle(&mut canvas);
            let owner = probe_pixels(frame.dmabuf().unwrap()).unwrap();
            let captured = pixels(&frame, OutputId(1));
            assert!(magenta(&owner) > 200, "Private owner content disappeared in {mode:?}/{spatial}");
            assert!(green(&owner) > 200, "Public owner content disappeared in {mode:?}/{spatial}");
            assert_eq!(magenta(&captured), 0, "Private pixels leaked in {mode:?}/{spatial}");
            assert!(green(&captured) > 200, "Public capture was blank in {mode:?}/{spatial}");
            let before = canvas.app.world().resource::<Updates>().0;
            let frame = canvas.probe_frame().unwrap();
            assert_eq!(canvas.app.world().resource::<Updates>().0, before + 1);
            assert_eq!(magenta(&pixels(&frame, OutputId(1))), 0);
        }
    }
    canvas.world_mut().get_mut::<BackgroundColor>(items[1]).unwrap().0 = Color::srgb(0.0, 1.0, 1.0);
    let frame = super::shortcuts::settle(&mut canvas);
    let captured = pixels(&frame, OutputId(1));
    assert!(captured.chunks_exact(4).filter(|pixel| pixel[0] < 40 && pixel[1] > 220 && pixel[2] > 220).count() > 200);
    assert_eq!(magenta(&captured), 0);
    canvas.world_mut().insert_resource(Demand::default());
    let frame = super::shortcuts::settle(&mut canvas);
    assert!(frame.capture_dmabuf(OutputId(1), frame.capture_source().unwrap().revision, 0).is_none());
    assert!(magenta(&probe_pixels(frame.dmabuf().unwrap()).unwrap()) > 200);
    canvas.finish().unwrap();
}

fn cached_shader(private: bool) -> nucleus::shader_effect::Effect {
    let source = if private {
        "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(1.0, 0.0, 0.7, 1.0); }"
    } else {
        "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(0.0, 1.0, 0.0, 1.0); }"
    };
    nucleus::shader_effect::Effect {
        programs: [("solid".into(), source.into())].into(),
        fallback: "solid".into(),
        ..default()
    }
}

fn shader_target(canvas: &mut Presentation, sand: Entity) -> Option<(Entity, bevy::asset::AssetId<Image>)> {
    let world = canvas.world_mut();
    world.query::<(Entity, &Camera, &native::capture::render::TargetSource, &RenderTarget)>()
        .iter(world)
        .find_map(|(camera, _, source, target)| {
            let RenderTarget::Image(image) = target else { return None; };
            let mut current = source.0;
            for _ in 0..32 {
                if current == sand { return Some((camera, image.handle.id())); }
                current = world.get::<ChildOf>(current)?.parent();
            }
            None
        })
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn capture_retains_public_frozen_shader_pixels_and_restores_private_targets_without_another_update() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut canvas = Presentation::new([1000, 720], directory.path().join("assets"), WakeSignal::new(|| {})).unwrap();
    super::shortcuts::settle(&mut canvas);
    let root = canvas.canvas_root().unwrap();
    native::workspace_config::set_physics(canvas.world_mut(), root, 1, false);
    canvas.desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free)).unwrap();
    let dimensions = Vec2::new(280.0, 200.0);
    let sands: Vec<_> = [true, false].into_iter().enumerate().map(|(index, private)| {
        let mut layout = LayoutBox::new(dimensions);
        layout.capture_excluded = private;
        canvas.world_mut().spawn((
            CanvasItem { position: bevy::math::DVec2::new(if index == 0 { -180.0 } else { 180.0 }, 0.0), size: dimensions },
            layout, Node::default(), WorkspaceMember(1), ChildOf(root),
            native::shader_effects::Surface { effect: cached_shader(private), motion: false, frozen: true, strength: 1.0, ..default() },
        )).id()
    }).collect();
    let deadline = Instant::now() + Duration::from_secs(60);
    let targets = loop {
        canvas.probe_frame().unwrap().wait_for_probe().unwrap();
        let targets: Option<Vec<_>> = sands.iter().map(|sand| shader_target(&mut canvas, *sand)).collect();
        if let Some(targets) = targets
            && sands.iter().all(|sand| native::shader_effects::surface_reading(canvas.app.world(), *sand)
                .is_some_and(|reading| reading.ready && reading.image_available && reading.error.is_empty()))
            && targets.iter().all(|(camera, _)| !canvas.app.world().get::<Camera>(*camera).unwrap().is_active)
        { break targets; }
        assert!(Instant::now() < deadline, "The private/public shader targets never reached their frozen cached state");
        std::thread::sleep(Duration::from_millis(2));
    };
    let before: Vec<_> = targets.iter().map(|(_, image)| super::layers::image_pixels(&canvas, *image)).collect();
    assert!(magenta(&before[0].1) > 200);
    assert!(green(&before[1].1) > 200);
    let gate = PolicyGate::default();
    gate.invalidate();
    canvas.world_mut().insert_resource(gate);
    canvas.world_mut().insert_resource(Demand { outputs: BTreeSet::from([OutputId(1)]), ..Default::default() });
    canvas.app.init_resource::<Updates>().add_systems(Update, count_updates);
    for _ in 0..20 {
        let updates = canvas.app.world().resource::<Updates>().0;
        let frame = canvas.probe_frame().unwrap();
        assert_eq!(canvas.app.world().resource::<Updates>().0, updates + 1);
        let captured = pixels(&frame, OutputId(1));
        assert_eq!(magenta(&captured), 0);
        assert!(green(&captured) > 200, "A public frozen preview disappeared from external capture");
        let owner = probe_pixels(frame.dmabuf().unwrap()).unwrap();
        assert!(magenta(&owner) > 200);
        assert!(green(&owner) > 200);
        for ((camera, image), expected) in targets.iter().zip(&before) {
            assert!(!canvas.app.world().get::<Camera>(*camera).unwrap().is_active);
            assert_eq!(&super::layers::image_pixels(&canvas, *image), expected);
        }
    }
    canvas.world_mut().insert_resource(Demand::default());
    let frame = super::shortcuts::settle(&mut canvas);
    assert!(magenta(&probe_pixels(frame.dmabuf().unwrap()).unwrap()) > 200);
    canvas.finish().unwrap();
}

#[derive(Resource, Default)]
struct PausedSurface(Option<Entity>);

fn pause_surface(paused: Res<PausedSurface>, mut cameras: Query<&mut Camera>) {
    if let Some(entity) = paused.0 && let Ok(mut camera) = cameras.get_mut(entity) { camera.is_active = false; }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn removed_private_child_in_a_paused_cached_surface_is_refreshed_only_for_external_capture() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut canvas = Presentation::new([1000, 720], directory.path().join("assets"), WakeSignal::new(|| {})).unwrap();
    super::shortcuts::settle(&mut canvas);
    let root = canvas.canvas_root().unwrap();
    native::workspace_config::set_physics(canvas.world_mut(), root, 1, false);
    canvas.desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free)).unwrap();
    let dimensions = Vec2::new(480.0, 300.0);
    let card = canvas.world_mut().spawn((
        CanvasItem { position: bevy::math::DVec2::ZERO, size: dimensions }, LayoutBox::new(dimensions),
        Node::default(), BackgroundColor(Color::srgb(0.0, 1.0, 0.0)), WorkspaceMember(1), ChildOf(root),
    )).id();
    let mut exclusion = LayoutBox::new(Vec2::splat(160.0));
    exclusion.capture_excluded = true;
    let child = canvas.world_mut().spawn((
        Node { width: px(160), height: px(160), ..default() }, exclusion,
        BackgroundColor(Color::srgb(1.0, 0.0, 0.7)), ChildOf(card),
    )).id();
    let gate = PolicyGate::default();
    gate.invalidate();
    canvas.world_mut().insert_resource(gate.clone());
    let initial = super::shortcuts::settle(&mut canvas);
    let initial = probe_pixels(initial.dmabuf().unwrap()).unwrap();
    assert!(magenta(&initial) > 200);
    assert!(green(&initial) > 200);
    let camera = {
        let world = canvas.world_mut();
        world.query::<(Entity, &Camera, &native::topology::presentation::VisualOwner)>().iter(world)
            .find(|(_, _, owner)| owner.0 == card).unwrap().0
    };
    canvas.app.insert_resource(PausedSurface(Some(camera))).init_resource::<Updates>()
        .add_systems(Update, count_updates)
        .add_systems(PostUpdate, pause_surface.after(native::topology::presentation::synchronize).before(bevy::camera::CameraUpdateSystems));
    canvas.world_mut().despawn(child);
    canvas.world_mut().insert_resource(Demand { outputs: BTreeSet::from([OutputId(1)]), ..Default::default() });
    for _ in 0..4 {
        let updates = canvas.app.world().resource::<Updates>().0;
        let frame = canvas.probe_frame().unwrap();
        assert_eq!(canvas.app.world().resource::<Updates>().0, updates + 1);
        assert!(gate.read().unwrap().requires_filter(), "A removed private child incorrectly enabled the public owner-image fast path");
        let owner = probe_pixels(frame.dmabuf().unwrap()).unwrap();
        assert!(magenta(&owner) > 200, "Capture changed the owner's retained preview");
        let captured = pixels(&frame, OutputId(1));
        assert_eq!(magenta(&captured), 0);
        assert!(green(&captured) > 500, "The fresh public content disappeared with its removed private child");
        assert!(!canvas.app.world().get::<Camera>(camera).unwrap().is_active);
    }
    canvas.world_mut().resource_mut::<PausedSurface>().0 = None;
    let frame = super::shortcuts::settle(&mut canvas);
    let owner = probe_pixels(frame.dmabuf().unwrap()).unwrap();
    assert_eq!(magenta(&owner), 0);
    assert!(green(&owner) > 500);
    assert!(!gate.read().unwrap().requires_filter(), "Fresh normal rendering retained stale private provenance");
    canvas.finish().unwrap();
}
