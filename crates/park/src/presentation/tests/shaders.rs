use super::layers::{capture, image_pixels};
use super::shortcuts::settle;
use super::*;
use bevy::{asset::AssetId, camera::visibility::RenderLayers, math::DVec2};
use lince_interface::host::{
    DesktopAction, ItemId, OutputId, OutputViewport, Rectangle, WorkspaceLayout,
};
use native::{
    canvas::{CanvasItem, CanvasView},
    layout::LayoutBox,
    workspace::{WorkspaceMember, Workspaces},
};
use nucleus::shader_effect::{
    Effect,
    spatial::{MeshSettings, Mode, Quality, VolumeSettings},
};
use smithay::backend::allocator::Buffer;
use std::time::{Duration, Instant};

fn effect(kind: usize) -> Effect {
    let (mode, source) = match kind {
        0 => (
            Mode::Surface,
            "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { let cell = vec2<u32>(floor(uv * inputs.size / 8.0)); let value = f32((cell.x + cell.y) % 2u); return vec4<f32>(vec3<f32>(value), 1.0); }",
        ),
        1 => (
            Mode::Mesh(MeshSettings::default()),
            "fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> vec4<f32> { let cell = vec2<u32>(floor(point.uv * inputs.size / 8.0)); let value = f32((cell.x + cell.y) % 2u); return vec4<f32>(vec3<f32>(value), 1.0); }",
        ),
        _ => (
            Mode::Volume(VolumeSettings {
                quality: Quality::Low,
                density: 1.0,
            }),
            "fn volume_sample(position: vec3<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(0.15, 0.65, 0.95, 40.0); }",
        ),
    };
    let effect = Effect {
        mode,
        programs: [("reference".into(), source.into())].into(),
        rules: Vec::new(),
        fallback: "reference".into(),
        ..default()
    };
    effect.validate().unwrap();
    effect
}

fn output(scale: f64) -> Output {
    let bounds = Rectangle::new([0.0; 2], [800.0, 600.0]).unwrap();
    Output {
        name: "Shader raster reference".into(),
        viewport: OutputViewport { rotation: Default::default(),
            id: OutputId(1),
            bounds,
            usable: Some(bounds),
            physical_size: [(800.0 * scale) as u32, (600.0 * scale) as u32],
            raster: lince_interface::host::raster::RasterMapping {
                origin: [0.0; 2],
                scale: [scale; 2],
            },
            workspace: 1,
        },
    }
}

fn source(canvas: &mut Presentation) -> (Entity, AssetId<Image>) {
    canvas
        .world_mut()
        .query::<(Entity, &Camera2d, &RenderLayers, &RenderTarget)>()
        .iter(canvas.app.world())
        .find_map(|(entity, _, layers, target)| {
            let RenderTarget::Image(image) = target else {
                return None;
            };
            layers
                .intersects(&RenderLayers::layer(30))
                .then_some((entity, image.handle.id()))
        })
        .unwrap()
}

fn wait(canvas: &mut Presentation, sand: Entity, camera: Entity, kind: usize) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        canvas.probe_frame().unwrap();
        let world = canvas.app.world();
        let ready = native::shader_effects::surface_reading(world, sand)
            .is_some_and(|state| state.ready && state.image_available && state.error.is_empty());
        let dimensionality = if kind == 0 {
            world.get::<Camera2d>(camera).is_some()
        } else {
            world.get::<Camera3d>(camera).is_some()
        };
        let volume = world
            .get::<bevy::core_pipeline::prepass::DepthPrepass>(camera)
            .is_some()
            == (kind == 2);
        if ready && dimensionality && volume {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Shader mode {kind} did not become ready: {:?}",
            native::shader_effects::surface_reading(world, sand)
        );
    }
    for _ in 0..8 {
        canvas.probe_frame().unwrap();
    }
}

fn linear(value: f64) -> f64 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb(value: f64) -> f64 {
    if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

fn layout(
    canvas: &Presentation,
    root: Entity,
    sand: Entity,
    mode: WorkspaceLayout,
    maximized: bool,
) {
    let world = canvas.app.world();
    let spaces = world.get::<Workspaces>(root).unwrap();
    let model = &spaces
        .entries
        .iter()
        .find(|space| space.id == spaces.active)
        .unwrap()
        .desktop;
    let id = world
        .get::<native::layout::desktop::Managed>(sand)
        .unwrap()
        .id;
    assert_eq!(model.mode(), mode);
    assert_eq!(model.maximized(id), maximized);
}

fn pixels(frame: &Frame, id: ItemId) -> ([u32; 2], Vec<u8>) {
    let output = frame
        .geometry
        .as_ref()
        .unwrap()
        .items
        .iter()
        .find(|item| item.id == id)
        .unwrap()
        .output;
    let buffer = if frame.output.is_none() || frame.output == Some(output) {
        frame.dmabuf().unwrap()
    } else {
        frame
            .outputs
            .iter()
            .find(|candidate| candidate.id == output)
            .unwrap()
            .dmabuf()
    };
    let size = buffer.size();
    (
        [size.w as u32, size.h as u32],
        probe_pixels(buffer).unwrap(),
    )
}

fn verify(
    canvas: &mut Presentation,
    sand: Entity,
    camera: Entity,
    image: AssetId<Image>,
    kind: usize,
    label: &str,
    records: &mut Vec<serde_json::Value>,
) {
    eprintln!("Verifying shader pixels: {label}");
    let id = ItemId::Native(canvas.app.world().get::<LayoutBox>(sand).unwrap().id);
    let background = {
        canvas
            .world_mut()
            .get_mut::<native::shader_effects::Surface>(sand)
            .unwrap()
            .disabled = true;
        for _ in 0..8 {
            canvas.probe_frame().unwrap();
        }
        let frame = settle(canvas);
        let (_, pixels) = pixels(&frame, id);
        canvas
            .world_mut()
            .get_mut::<native::shader_effects::Surface>(sand)
            .unwrap()
            .disabled = false;
        wait(canvas, sand, camera, kind);
        pixels
    };
    wait(canvas, sand, camera, kind);
    let frame = settle(canvas);
    let (size, pixels) = pixels(&frame, id);
    let (source_size, reference) = image_pixels(canvas, image);
    let native = canvas
        .app
        .world()
        .get::<native::topology::presentation::Surface>(sand)
        .unwrap();
    assert_eq!(
        canvas
            .app
            .world()
            .get::<Camera>(camera)
            .unwrap()
            .physical_target_size(),
        Some(UVec2::from_array(source_size))
    );
    assert_eq!(
        canvas
            .app
            .world()
            .get::<Camera>(native.camera)
            .unwrap()
            .physical_target_size(),
        Some(native.pixels)
    );
    let layout = canvas.app.world().get::<LayoutBox>(sand).unwrap();
    let geometry = frame.geometry.as_ref().unwrap();
    let item = geometry
        .items
        .iter()
        .find(|item| item.id == ItemId::Native(layout.id))
        .unwrap();
    let output = geometry
        .outputs
        .iter()
        .find(|output| output.id == item.output)
        .unwrap();
    let scale = output.raster.scale;
    let width = item.mapping.canvas.size()[0] * scale[0];
    let height = item.mapping.canvas.size()[1] * scale[1];
    assert!(
        f64::from(source_size[0]) + 2.0 >= width && f64::from(source_size[1]) + 2.0 >= height,
        "{label}: source {source_size:?} undersamples the projected {width}×{height} rectangle"
    );
    let RenderTarget::Image(target) = canvas.app.world().get::<RenderTarget>(camera).unwrap()
    else {
        panic!("The shader source lost its image target")
    };
    assert_eq!(target.handle.id(), image);
    let mut samples = 0_u64;
    let mut mismatches = 0_u64;
    let mut dark = 0_u64;
    let mut light = 0_u64;
    let mut mapped = 0_u64;
    let mut interior = 0_u64;
    let mut uniform = 0_u64;
    let mut examples = Vec::new();
    for y in (16..size[1].saturating_sub((100.0 * scale[1]) as u32)).step_by(2) {
        for x in (16..size[0].saturating_sub(16)).step_by(2) {
            let logical = output
                .raster
                .logical_point([f64::from(x) + 0.5, f64::from(y) + 0.5]);
            let Some(surface) = item.surface_point(logical) else {
                continue;
            };
            mapped += 1;
            let uv = std::array::from_fn::<_, 2, _>(|axis| {
                (surface[axis] - item.mapping.surface.origin()[axis])
                    / item.mapping.surface.size()[axis]
            });
            let sx = (uv[0] * f64::from(source_size[0]) - 0.5).floor() as i64;
            let sy = (uv[1] * f64::from(source_size[1]) - 0.5).floor() as i64;
            if sx < 2
                || sy < 2
                || sx + 2 >= i64::from(source_size[0])
                || sy + 2 >= i64::from(source_size[1])
            {
                continue;
            }
            interior += 1;
            let mut minimum = [255_u8; 4];
            let mut maximum = [0_u8; 4];
            for dy in -1..=2 {
                for dx in -1..=2 {
                    let index = (((sy + dy) as usize * source_size[0] as usize
                        + (sx + dx) as usize)
                        * 4) as usize;
                    for channel in 0..4 {
                        minimum[channel] = minimum[channel].min(reference[index + channel]);
                        maximum[channel] = maximum[channel].max(reference[index + channel]);
                    }
                }
            }
            if (0..4).any(|channel| maximum[channel] - minimum[channel] > 8) || minimum[3] < 64 {
                continue;
            }
            uniform += 1;
            let index = (y as usize * size[0] as usize + x as usize) * 4;
            let mut expected = [0_u8; 3];
            let mut source = [0_u8; 3];
            for channel in 0..3 {
                source[channel] = minimum[channel] + (maximum[channel] - minimum[channel]) / 2;
                let value = f64::from(source[channel]) / 255.0;
                let alpha = f64::from(minimum[3] + (maximum[3] - minimum[3]) / 2) / 255.0;
                let value = srgb(
                    linear(value) * alpha
                        + linear(f64::from(background[index + channel]) / 255.0) * (1.0 - alpha),
                );
                expected[channel] = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            samples += 1;
            dark += u64::from(source.iter().all(|value| *value < 32));
            light += u64::from(source.iter().all(|value| *value > 223));
            if (0..3).any(|channel| pixels[index + channel].abs_diff(expected[channel]) > 28) {
                mismatches += 1;
                if examples.len() < 5 {
                    examples.push(serde_json::json!({ "pixel": [x, y], "expected": expected, "actual": &pixels[index..index + 4] }));
                }
            }
        }
    }
    let report = serde_json::json!({
        "stage": label, "source_pixels": source_size, "capture_pixels": native.pixels.to_array(),
        "output": item.output.0,
        "projected_pixels": [width, height], "output_pixels": size, "scale": scale,
        "samples": samples, "mismatches": mismatches, "source_dark": dark, "source_light": light,
        "mapped": mapped, "interior": interior, "uniform": uniform,
        "canvas_origin": item.mapping.canvas.origin(), "canvas_size": item.mapping.canvas.size(),
        "surface_origin": item.mapping.surface.origin(), "surface_size": item.mapping.surface.size(),
        "clip": item.clip, "raster_origin": output.raster.origin,
        "source_alpha": [reference.chunks_exact(4).map(|pixel| pixel[3]).min(), reference.chunks_exact(4).map(|pixel| pixel[3]).max()],
        "source_first_pixels": &reference[..16],
        "examples": examples,
    });
    records.push(report.clone());
    capture(&format!("shader-{label}"), size, &pixels);
    capture(&format!("shader-{label}-source"), source_size, &reference);
    if let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        std::fs::write(
            std::path::Path::new(&directory).join("shader-output-metrics.json"),
            serde_json::to_vec_pretty(records).unwrap(),
        )
        .unwrap();
    }
    assert!(
        samples > 256 && mismatches * 100 <= samples,
        "Final shader pixels differ from their source: {report}"
    );
    if kind == 0 {
        assert!(
            dark > 100 && light > 100,
            "The checker reference lost contrast: {report}"
        );
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn shader_sources_match_final_free_tiled_maximized_and_overview_return_pixels_at_fractional_scale() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let mut canvas = Presentation::new(
        [800, 600],
        directory.path().join("assets"),
        WakeSignal::new(|| {}),
    )
    .unwrap();
    settle(&mut canvas);
    let root = canvas.canvas_root().unwrap();
    let workspace = canvas.app.world().get::<Workspaces>(root).unwrap().active;
    native::workspace_config::set_physics(canvas.world_mut(), root, workspace, false);
    canvas.desktop_action(DesktopAction::ToggleTiling).unwrap();
    *canvas.world_mut().get_mut::<CanvasView>(root).unwrap() = CanvasView::default();
    let dimensions = Vec2::new(600.0, 400.0);
    let sand = canvas
        .world_mut()
        .spawn((
            CanvasItem {
                position: DVec2::ZERO,
                size: dimensions,
            },
            LayoutBox::new(dimensions),
            Node::default(),
            WorkspaceMember(workspace),
            Name::new("Shader raster reference"),
            ChildOf(root),
            native::shader_effects::Surface {
                effect: effect(0),
                motion: false,
                strength: 1.0,
                glow: 0.0,
                ..default()
            },
        ))
        .id();
    settle(&mut canvas);
    let (camera, image) = source(&mut canvas);
    native::layout::desktop::focus_item(canvas.world_mut(), sand);
    let mut records = Vec::new();
    for kind in 0..3 {
        canvas
            .world_mut()
            .get_mut::<native::shader_effects::Surface>(sand)
            .unwrap()
            .effect = effect(kind);
        wait(&mut canvas, sand, camera, kind);
        for scale in [1.25] {
            canvas.outputs(vec![output(scale)]).unwrap();
            let prefix = format!("mode-{kind}-scale-{}", (scale * 100.0) as u32);
            verify(
                &mut canvas,
                sand,
                camera,
                image,
                kind,
                &format!("{prefix}-free"),
                &mut records,
            );
            layout(&canvas, root, sand, WorkspaceLayout::Free, false);
            canvas.desktop_action(DesktopAction::ToggleTiling).unwrap();
            verify(
                &mut canvas,
                sand,
                camera,
                image,
                kind,
                &format!("{prefix}-tiled"),
                &mut records,
            );
            layout(&canvas, root, sand, WorkspaceLayout::Tiled, false);
            canvas
                .desktop_action(DesktopAction::ToggleMaximized)
                .unwrap();
            verify(
                &mut canvas,
                sand,
                camera,
                image,
                kind,
                &format!("{prefix}-maximized"),
                &mut records,
            );
            layout(&canvas, root, sand, WorkspaceLayout::Tiled, true);
            canvas
                .desktop_action(DesktopAction::ToggleMaximized)
                .unwrap();
            canvas
                .desktop_action(DesktopAction::ToggleOverview)
                .unwrap();
            assert!(settle(&mut canvas).input.keyboard_modal);
            canvas
                .desktop_action(DesktopAction::ToggleOverview)
                .unwrap();
            verify(
                &mut canvas,
                sand,
                camera,
                image,
                kind,
                &format!("{prefix}-overview-return"),
                &mut records,
            );
            layout(&canvas, root, sand, WorkspaceLayout::Tiled, false);
            canvas.desktop_action(DesktopAction::ToggleTiling).unwrap();
        }
        let mut secondary = output(1.0);
        secondary.name.push_str(" 2");
        secondary.viewport.id = OutputId(2);
        secondary.viewport.bounds = Rectangle::new([800.0, 0.0], [800.0, 600.0]).unwrap();
        secondary.viewport.usable = Some(secondary.viewport.bounds);
        secondary.viewport.raster.origin = secondary.viewport.bounds.origin();
        secondary.viewport.workspace = 2;
        canvas.outputs(vec![output(1.0), secondary]).unwrap();
        canvas.focus_output(OutputId(2)).unwrap();
        verify(
            &mut canvas,
            sand,
            camera,
            image,
            kind,
            &format!("mode-{kind}-passive-output"),
            &mut records,
        );
        canvas.focus_output(OutputId(1)).unwrap();
        canvas.outputs(vec![output(1.0)]).unwrap();
    }
}
