use super::*;
use super::{displays::{labelled, write}, shortcuts::{click, key, named_button, settle}};
use lince_interface::{
    host::{capture::{Demand, PolicyGate}, DesktopAction, ItemId, OutputId, WorkspaceLayout},
    native::{actions::Action, canvas::{CanvasItem, CanvasView}, layout::LayoutBox, workspace::WorkspaceMember},
};
use std::{collections::{BTreeMap, BTreeSet}, path::Path, time::{Duration, Instant}};

mod geometry;
mod reference;
mod transmission;
mod utilities;

const SHADOW: &str = "Shadow radius (0–32 logical units)";
const BLUR: &str = "Background blur radius (0–24 logical units)";
const FADE: &str = "Appearance fade (0–200 milliseconds)";

pub(super) fn open(canvas: &mut Presentation) {
    let root = canvas.canvas_root().unwrap();
    native::edit_mode::EditAction::Open.apply(canvas.world_mut(), root);
    native::edit_mode::EditAction::General.apply(canvas.world_mut(), root);
    settle(canvas);
}

pub(super) fn apply(canvas: &mut Presentation, caption: &str, value: &str, size: [u32; 2]) {
    write(canvas, caption, value, size);
    key(canvas, KeyCode::Enter, true);
    key(canvas, KeyCode::Enter, false);
}

pub(super) fn launch(path: &Path, size: [u32; 2]) -> Presentation {
    let mut canvas = Presentation::new(size, path.parent().unwrap().join("assets"), WakeSignal::new(|| {})).unwrap();
    canvas.world_mut().insert_resource(native::workspace::WorkspaceFile::new(path.into()));
    settle(&mut canvas);
    canvas
}

fn scale(canvas: &mut Presentation, size: [u32; 2], scale: f64) {
    let mut server = crate::server::Server::new(size.map(|value| value as i32)).unwrap();
    server.configure_scaled_outputs(&[crate::server::Monitor {
        id: OutputId(1), name: "appearance-fixture".into(), origin: [0, 0], size: size.map(|value| value as i32),
    }], &BTreeMap::from([(OutputId(1), scale)])).unwrap();
    canvas.outputs(server.output_viewports().into_iter().map(|(name, viewport)| Output { name, viewport }).collect()).unwrap();
    settle(canvas);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn rendered_appearance_fields_apply_cancel_and_reload_through_real_workspace_startup() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    for (size, output_scale) in [([1000, 720], 1.25), ([360, 720], 1.5)] {
        let profile = directory.path().join(size[0].to_string());
        std::fs::create_dir_all(&profile).unwrap();
        let path = profile.join("workspace.json");
        let mut canvas = launch(&path, size);
        scale(&mut canvas, size, output_scale);
        open(&mut canvas);
        for (caption, value) in [(SHADOW, "12"), (BLUR, "8"), (FADE, "120")] { apply(&mut canvas, caption, value, size); }
        write(&mut canvas, BLUR, "25", size);
        key(&mut canvas, KeyCode::Enter, true);
        key(&mut canvas, KeyCode::Enter, false);
        let field = labelled(&mut canvas, BLUR);
        assert_eq!(canvas.app.world().get::<bevy::text::EditableText>(field).unwrap().value(), "25");
        key(&mut canvas, KeyCode::Escape, true);
        key(&mut canvas, KeyCode::Escape, false);
        assert_eq!(canvas.app.world().get::<bevy::text::EditableText>(field).unwrap().value(), "8");
        let frame = settle(&mut canvas);
        super::shortcuts::capture(&format!("appearance-controls-{}-{output_scale}", size[0]), size, &frame);
        canvas.finish().unwrap();
        drop(canvas);
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["shader_settings"]["compositor"]["shadow_radius"].as_f64(), Some(12.0));
        assert_eq!(saved["shader_settings"]["compositor"]["blur_radius"].as_f64(), Some(8.0));
        assert_eq!(saved["shader_settings"]["compositor"]["appearance_fade_ms"].as_u64(), Some(120));
        let mut canvas = launch(&path, size);
        scale(&mut canvas, size, output_scale);
        open(&mut canvas);
        for (caption, value) in [(SHADOW, "12"), (BLUR, "8"), (FADE, "120")] {
            let field = labelled(&mut canvas, caption);
            assert_eq!(canvas.app.world().get::<bevy::text::EditableText>(field).unwrap().value(), value);
        }
        canvas.finish().unwrap();
    }
}

pub(super) fn item(canvas: &mut Presentation, root: Entity, size: Vec2, position: [f64; 2], color: Color, z: i32, private: bool) -> Entity {
    let mut layout = LayoutBox::new(size);
    layout.capture_excluded = private;
    canvas.world_mut().spawn((CanvasItem { position: bevy::math::DVec2::from_array(position), size }, layout,
        Node::default(), BackgroundColor(color), WorkspaceMember(1), ZIndex(z), ChildOf(root))).id()
}

pub(super) fn close(canvas: &mut Presentation, size: [u32; 2]) {
    let button = labelled(canvas, lince_interface::i18n::Token::CloseEditMode.text());
    super::displays::reveal(canvas, button);
    click(canvas, button, size);
    settle(canvas);
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn actual_backdrop_blur_changes_translucent_pixels_preserves_sharp_content_and_excludes_private_sources() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let size = [800, 600];
    let mut canvas = launch(&directory.path().join("workspace.json"), size);
    let root = canvas.canvas_root().unwrap();
    native::workspace_config::set_physics(canvas.world_mut(), root, 1, false);
    canvas.desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free)).unwrap();
    *canvas.world_mut().get_mut::<CanvasView>(root).unwrap() = CanvasView::default();
    let backdrop = item(&mut canvas, root, Vec2::new(560.0, 360.0), [0.0; 2], Color::BLACK, 0, false);
    for index in 0..70 {
        canvas.world_mut().spawn((Node { position_type: PositionType::Absolute, left: px(index as f32 * 8.0), top: px(0), width: px(4), height: percent(100), ..default() },
            BackgroundColor(Color::WHITE), ChildOf(backdrop)));
    }
    let front = item(&mut canvas, root, Vec2::new(360.0, 240.0), [0.0; 2], Color::srgba(0.0, 0.0, 0.0, 0.3), 2, false);
    canvas.world_mut().spawn((Node { position_type: PositionType::Absolute, left: px(175), top: px(40), width: px(10), height: px(160), ..default() },
        BackgroundColor(Color::srgb(0.0, 1.0, 0.0)), ChildOf(front)));
    let id = ItemId::Native(canvas.app.world().get::<LayoutBox>(front).unwrap().id);
    let green = |pixels: &[u8]| pixels.chunks_exact(4).filter(|pixel| pixel[0] < 20 && pixel[1] > 230 && pixel[2] < 20).count();
    let ready = Instant::now() + Duration::from_secs(60);
    let (baseline, off) = loop {
        let frame = settle(&mut canvas);
        let pixels = probe_pixels(frame.dmabuf().unwrap()).unwrap();
        if green(&pixels) > 200 { break (frame, pixels); }
        assert!(Instant::now() < ready, "The sharp reference did not become visible after startup");
        std::thread::sleep(Duration::from_millis(5));
    };
    let geometry = baseline.geometry.as_ref().unwrap().items.iter().find(|item| item.id == id).unwrap().clone();
    let sharp = green(&off);
    assert!(sharp > 200, "The actual sharp reference was not visible");
    open(&mut canvas);
    apply(&mut canvas, BLUR, "8", size);
    close(&mut canvas, size);
    let deadline = Instant::now() + Duration::from_secs(15);
    let on = loop {
        let frame = settle(&mut canvas);
        let pixels = probe_pixels(frame.dmabuf().unwrap()).unwrap();
        let changed = off.chunks_exact(4).zip(pixels.chunks_exact(4)).enumerate().filter(|(index, (before, after))| {
            let point = [(index % size[0] as usize) as f64 + 0.5, (index / size[0] as usize) as f64 + 0.5];
            geometry.surface_point(point).is_some()
                && before[1].abs_diff(after[1]) > 12 && before[0].abs_diff(before[2]) < 4
        }).count();
        assert_eq!(frame.geometry.as_ref().unwrap().items.iter().find(|item| item.id == id), Some(&geometry));
        if changed > 200 { break pixels; }
        assert!(Instant::now() < deadline, "The real blur pipeline never changed the translucent backdrop");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(green(&on).abs_diff(sharp) <= 4, "Backdrop blur changed sharp foreground pixels");
    super::layers::capture("appearance-blur-off", size, &off);
    super::layers::capture("appearance-blur-on", size, &on);
    let private = item(&mut canvas, root, Vec2::new(200.0, 150.0), [-90.0, 0.0], Color::srgb(1.0, 0.0, 0.8), 1, true);
    let gate = PolicyGate::default();
    gate.invalidate();
    canvas.world_mut().insert_resource(gate);
    canvas.world_mut().insert_resource(Demand { outputs: BTreeSet::from([OutputId(1)]), ..Default::default() });
    let frame = settle(&mut canvas);
    let owner = probe_pixels(frame.dmabuf().unwrap()).unwrap();
    let public = super::capture::pixels(&frame, OutputId(1));
    assert!(owner.chunks_exact(4).zip(public.chunks_exact(4)).filter(|(a, b)| a[0].abs_diff(b[0]) > 20 || a[2].abs_diff(b[2]) > 20).count() > 200);
    canvas.world_mut().get_mut::<Node>(private).unwrap().display = Display::None;
    let frame = settle(&mut canvas);
    let reference = super::capture::pixels(&frame, OutputId(1));
    assert_eq!(public.len(), reference.len());
    assert!(public.iter().zip(&reference).all(|(actual, expected)| actual.abs_diff(*expected) <= 2), "A private source contributed to a safe backdrop or retained history");
    let mut compared = 0;
    for (index, (actual, expected)) in reference.chunks_exact(4).zip(on.chunks_exact(4)).enumerate() {
        let point = [(index % size[0] as usize) as f64 + 0.5, (index / size[0] as usize) as f64 + 0.5];
        let Some(local) = geometry.surface_point(point) else { continue; };
        let origin = geometry.mapping.surface.origin();
        let extent = geometry.mapping.surface.size();
        let inside = (0..2).all(|axis| (0.15..=0.85).contains(&((local[axis] - origin[axis]) / extent[axis])));
        if inside {
            assert!((0..4).all(|channel| actual[channel].abs_diff(expected[channel]) <= 2), "Filtered capture dropped or changed public backdrop blur at {point:?}");
            compared += 1;
        }
    }
    assert!(compared > 200, "No actual blurred public interior was compared");
    super::layers::capture("appearance-public-filtered", size, &public);
    super::layers::capture("appearance-public-reference", size, &reference);
    canvas.finish().unwrap();
}

#[derive(Clone, Debug, serde::Serialize)]
struct Resources {
    assets: [usize; 3],
    gpu: [[usize; 2]; 5],
    allocated_bytes: Option<u64>,
    descriptors: usize,
}

fn resources(canvas: &mut Presentation) -> Resources {
    for _ in 0..16 { drop(canvas.probe_frame().unwrap()); }
    canvas.renderer.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(Duration::from_secs(3)) }).unwrap();
    let report = canvas.renderer._instance.generate_report().expect("The native WGPU resource report is unavailable");
    let gpu = [report.hub.textures, report.hub.texture_views, report.hub.buffers, report.hub.bind_groups, report.hub.samplers]
        .map(|resource| [resource.num_kept_from_user, resource.num_allocated]);
    let world = canvas.app.world();
    Resources {
        assets: [world.resource::<Assets<Image>>().len(), world.resource::<Assets<Mesh>>().len(), world.resource::<Assets<StandardMaterial>>().len()],
        gpu,
        allocated_bytes: canvas.renderer.device.generate_allocator_report().map(|report| report.total_allocated_bytes),
        descriptors: std::fs::read_dir("/proc/self/fd").unwrap().count(),
    }
}

fn decorations(canvas: &mut Presentation, owner: Entity) -> usize {
    let world = canvas.world_mut();
    let Some(material) = world.get::<native::topology::presentation::Surface>(owner).map(|surface| surface.material.clone()) else { return 0; };
    world.query_filtered::<(&native::topology::presentation::VisualOwner, &ChildOf), With<Mesh3d>>().iter(world).filter(|(visual, parent)| {
        visual.0 == owner && world.get::<MeshMaterial3d<StandardMaterial>>(parent.parent()).is_some_and(|parent| parent.0 == material)
    }).count()
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn twenty_real_gpu_appearance_cycles_retire_closed_items_and_preserve_bounded_resources() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let size = [1000, 720];
    let mut canvas = launch(&directory.path().join("workspace.json"), size);
    let root = canvas.canvas_root().unwrap();
    native::workspace_config::set_physics(canvas.world_mut(), root, 1, false);
    canvas.desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free)).unwrap();
    *canvas.world_mut().get_mut::<CanvasView>(root).unwrap() = CanvasView::default();
    let gate = PolicyGate::default();
    gate.invalidate();
    canvas.world_mut().insert_resource(gate.clone());
    canvas.world_mut().insert_resource(Demand { outputs: BTreeSet::from([OutputId(1)]), ..Default::default() });
    open(&mut canvas);
    let mut samples = Vec::new();
    let mut pending_closes = 0;
    let mut pending_capture_closes = 0;
    for cycle in 0..20 {
        let owner = item(&mut canvas, root, Vec2::new(240.0, 180.0), [-120.0, 0.0], Color::srgba(0.0, 0.8, 0.4, 0.6), 1, false);
        let private = item(&mut canvas, root, Vec2::new(120.0, 100.0), [240.0, 0.0], Color::srgb(1.0, 0.0, 1.0), 2, true);
        apply(&mut canvas, SHADOW, "12", size);
        apply(&mut canvas, BLUR, "8", size);
        let deadline = Instant::now() + Duration::from_secs(15);
        while decorations(&mut canvas, owner) != 2 {
            drop(canvas.probe_frame().unwrap());
            assert!(Instant::now() < deadline, "Both actual decorations were not created in cycle {cycle}");
        }
        canvas.world_mut().get_mut::<Node>(owner).unwrap().display = Display::None;
        drop(settle(&mut canvas));
        assert_eq!(decorations(&mut canvas, owner), 0);
        canvas.world_mut().get_mut::<Node>(owner).unwrap().display = Display::Flex;
        drop(settle(&mut canvas));
        assert_eq!(decorations(&mut canvas, owner), 2);
        let deadline = Instant::now() + Duration::from_secs(15);
        let pending = loop {
            let frame = canvas.frame().unwrap();
            if frame.capture_source().is_some_and(|stamp| frame.capture_dmabuf(OutputId(1), stamp.revision, stamp.mapping_revision).is_some()) { break frame; }
            assert!(Instant::now() < deadline, "The actual filtered resource-cycle image did not become available");
            std::thread::sleep(Duration::from_millis(2));
        };
        pending_closes += usize::from(!pending.ready().unwrap());
        let stamp = pending.capture_source().expect("The real filtered resource-cycle frame has no capture policy stamp");
        let stamp = (stamp.revision, stamp.mapping_revision);
        assert!(pending.capture_dmabuf(OutputId(1), stamp.0, stamp.1).is_some(), "The filtered resource-cycle image was not exported");
        pending_capture_closes += usize::from(!pending.capture_ready(OutputId(1)).unwrap());
        gate.invalidate();
        assert!(canvas.world_mut().despawn(owner));
        assert!(canvas.world_mut().despawn(private));
        let replacement = canvas.frame().unwrap();
        let current = replacement.capture_source().expect("The replacement frame did not publish its capture policy");
        assert_ne!(current.revision, stamp.0);
        assert!(pending.capture_dmabuf(OutputId(1), current.revision, current.mapping_revision).is_none(), "A retained image inherited the replacement policy identity");
        pending.wait_for_probe().unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !pending.capture_ready(OutputId(1)).unwrap() {
            assert!(Instant::now() < deadline, "An exported filtered image was destroyed or stopped completing after producer closure");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(pending.capture_dmabuf(OutputId(1), stamp.0, stamp.1).is_some());
        drop(pending);
        drop(replacement);
        apply(&mut canvas, SHADOW, "0", size);
        apply(&mut canvas, BLUR, "0", size);
        let sample = resources(&mut canvas);
        assert!(canvas.app.world().get_entity(owner).is_err());
        samples.push(sample);
    }
    if let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        std::fs::create_dir_all(&directory).unwrap();
        let report = serde_json::json!({
            "cycles": 20,
            "warmup_cycles": 10,
            "pending_closes": pending_closes,
            "pending_capture_closes": pending_capture_closes,
            "gpu_order": ["textures", "texture_views", "buffers", "bind_groups", "samplers"],
            "gpu_fields": ["kept_from_user", "allocated_ids"],
            "samples": &samples,
        });
        std::fs::write(std::path::Path::new(&directory).join("appearance-resources.json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    assert!(pending_closes > 0, "No actual unfinished GPU frame was observed before closing a producer");
    assert!(pending_capture_closes > 0, "No actual unfinished filtered GPU export was observed before closing its producers and changing policy");
    let ceiling = &samples[10];
    for (cycle, sample) in samples.iter().enumerate().skip(11) {
        for index in 0..sample.assets.len() {
            assert!(sample.assets[index] <= ceiling.assets[index], "Native assets grew after warmup in cycle {cycle}: {samples:?}");
        }
        for (index, counters) in sample.gpu.iter().enumerate() {
            for counter in 0..2 {
                assert!(counters[counter] <= ceiling.gpu[index][counter], "Live WGPU resources grew after warmup in cycle {cycle}: {samples:?}");
            }
        }
        if let (Some(actual), Some(ceiling)) = (sample.allocated_bytes, ceiling.allocated_bytes) {
            assert!(actual <= ceiling, "GPU allocated bytes grew after warmup in cycle {cycle}: {samples:?}");
        }
    }
    let late = &samples[10..];
    let monotonic_descriptors = late.windows(2).all(|pair| pair[1].descriptors >= pair[0].descriptors)
        && late.last().unwrap().descriptors > late[0].descriptors;
    assert!(!monotonic_descriptors, "Descriptors grew monotonically after warmup: {samples:?}");
    canvas.finish().unwrap();
}

fn companions() -> Vec<Output> {
    let first = super::transfers::output(1, 0.0, [640, 480]);
    let mut second = super::transfers::output(2, 640.0, [640, 480]);
    second.viewport.workspace = 2;
    vec![first, second]
}

fn alpha(canvas: &Presentation, owner: Entity) -> f32 {
    let world = canvas.app.world();
    let surface = world.get::<native::topology::presentation::Surface>(owner).unwrap();
    world.resource::<Assets<StandardMaterial>>().get(&surface.material).unwrap().base_color.alpha()
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn reduced_motion_protection_and_last_output_loss_retire_effects_without_losing_companion_content_or_preferences() {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(32 * 1024 * 1024).build().unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workspace.json");
    let size = [640, 480];
    let mut canvas = launch(&path, size);
    canvas.outputs(companions()).unwrap();
    drop(settle(&mut canvas));
    let root = canvas.canvas_root().unwrap();
    let workspaces = canvas.app.world().get::<native::workspace::Workspaces>(root).unwrap().entries.iter().map(|space| space.id).collect::<Vec<_>>();
    for workspace in workspaces { native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false); }
    canvas.desktop_action(DesktopAction::SetLayout(WorkspaceLayout::Free)).unwrap();
    let first = item(&mut canvas, root, Vec2::new(240.0, 180.0), [0.0; 2], Color::srgb(0.0, 1.0, 0.0), 1, false);
    let second = item(&mut canvas, root, Vec2::new(240.0, 180.0), [0.0; 2], Color::srgb(0.1, 0.25, 1.0), 1, false);
    canvas.world_mut().entity_mut(second).insert(WorkspaceMember(2));
    open(&mut canvas);
    for (caption, value) in [(SHADOW, "12"), (BLUR, "8"), (FADE, "120")] { apply(&mut canvas, caption, value, size); }
    let motion = named_button(&mut canvas, "Shader reduced motion: Off");
    super::displays::reveal(&mut canvas, motion);
    click(&mut canvas, motion, size);
    let changed = named_button(&mut canvas, "Shader reduced motion: On");
    assert_eq!(motion, changed, "A simple appearance toggle replaced its control instead of updating its state");
    close(&mut canvas, size);
    let ids = [first, second].map(|owner| canvas.app.world().get::<LayoutBox>(owner).unwrap().id);
    for spatial in [false, true] {
        canvas.world_mut().get_mut::<native::topology::view::View>(root).unwrap().spatial = spatial;
        let ready = Instant::now() + Duration::from_secs(15);
        let frame = loop {
            let frame = settle(&mut canvas);
            if [first, second].iter().all(|owner| decorations(&mut canvas, *owner) == 2)
                && frame.outputs.iter().any(|output| output.id == OutputId(2)) { break frame; }
            assert!(Instant::now() < ready, "Appearance did not prepare both actual output views");
        };
        for owner in [first, second] { assert_eq!(alpha(&canvas, owner), 1.0, "Reduced motion left a transient fade"); }
        let primary = probe_pixels(frame.dmabuf().unwrap()).unwrap();
        let secondary = probe_pixels(frame.outputs.iter().find(|output| output.id == OutputId(2)).unwrap().dmabuf()).unwrap();
        assert!(primary.chunks_exact(4).filter(|pixel| pixel[0] < 30 && pixel[1] > 220 && pixel[2] < 30).count() > 100);
        assert!(secondary.chunks_exact(4).filter(|pixel| pixel[0] < 60 && pixel[1] < 100 && pixel[2] > 220).count() > 100);
        super::layers::capture(&format!("appearance-companion-primary-{spatial}"), size, &primary);
        super::layers::capture(&format!("appearance-companion-secondary-{spatial}"), size, &secondary);
        canvas.world_mut().insert_resource(lince_interface::host::control::Status { available: true, active: false, locked: true, ..Default::default() });
        drop(settle(&mut canvas));
        for owner in [first, second] { assert_eq!(decorations(&mut canvas, owner), 0); }
        canvas.world_mut().insert_resource(lince_interface::host::control::Status { available: true, active: true, ..Default::default() });
        drop(settle(&mut canvas));
        for owner in [first, second] { assert_eq!(decorations(&mut canvas, owner), 2); }
    }
    canvas.outputs(Vec::new()).unwrap();
    assert!(settle(&mut canvas).dmabuf().is_none());
    for owner in [first, second] { assert_eq!(decorations(&mut canvas, owner), 0); }
    canvas.outputs(companions()).unwrap();
    canvas.focus(true).unwrap();
    drop(settle(&mut canvas));
    for (index, owner) in [first, second].into_iter().enumerate() {
        assert_eq!(canvas.app.world().get::<LayoutBox>(owner).unwrap().id, ids[index]);
        assert_eq!(decorations(&mut canvas, owner), 2);
        assert_eq!(alpha(&canvas, owner), 1.0);
    }
    canvas.finish().unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["shader_settings"]["reduced_motion"].as_bool(), Some(true));
    assert_eq!(saved["shader_settings"]["compositor"]["shadow_radius"].as_f64(), Some(12.0));
    assert_eq!(saved["shader_settings"]["compositor"]["blur_radius"].as_f64(), Some(8.0));
    assert_eq!(saved["shader_settings"]["compositor"]["appearance_fade_ms"].as_u64(), Some(120));
}
