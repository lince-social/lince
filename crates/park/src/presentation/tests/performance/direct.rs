use super::*;
use crate::graphics::{
    GpuContext,
    direct::{Pool, Prepared},
};
use bevy::{
    asset::AssetId,
    render::{Render, RenderSystems},
};
use lince_interface::host::OutputId;
use smithay::backend::{
    allocator::{Buffer, Fourcc},
    egl::{EGLContext, EGLDevice, EGLDisplay},
    renderer::{
        Bind, ExportMem, Frame as _, ImportDma, Offscreen, Renderer,
        gles::{GlesRenderbuffer, GlesRenderer},
    },
};
use std::collections::{BTreeMap, HashMap};

#[derive(Resource)]
struct Targets {
    gpu: Arc<GpuContext>,
    originals: Vec<(AssetId<Image>, GpuImage)>,
    direct: bool,
    active_direct: bool,
    pool: Option<Pool>,
    cached: Vec<(smithay::backend::allocator::dmabuf::Dmabuf, GpuImage)>,
    exported: Vec<smithay::backend::allocator::dmabuf::Dmabuf>,
    epoch: u8,
    setup_ms: f64,
}

fn prepare(
    mut targets: ResMut<Targets>,
    mut images: ResMut<RenderAssets<GpuImage>>,
    mut commands: Commands,
) {
    let start = Instant::now();
    let targets = &mut *targets;
    let prepared = if targets.direct {
        let sources = targets
            .originals
            .iter()
            .map(|(_, image)| wgpu::Texture::clone(&image.texture))
            .collect::<Vec<_>>();
        if let Some(pool) = &mut targets.pool {
            pool.acquire(&targets.gpu, &sources).unwrap()
        } else {
            Some(Prepared::new(&targets.gpu, &sources).unwrap())
        }
    } else {
        None
    };
    if let Some(pool) = &targets.pool {
        targets.cached.retain(|(buffer, _)| pool.contains(buffer));
    }
    targets.active_direct = prepared.is_some();
    if let Some(prepared) = prepared {
        targets.exported = (0..targets.originals.len())
            .map(|index| prepared.dmabuf(index).clone())
            .collect();
        for (index, (id, original)) in targets.originals.iter().enumerate() {
            let buffer = prepared.dmabuf(index);
            let cached = targets
                .pool
                .is_some()
                .then(|| {
                    targets
                        .cached
                        .iter()
                        .find(|(cached, _)| cached == buffer)
                        .map(|(_, image)| image.clone())
                })
                .flatten();
            let image = if let Some(cached) = cached {
                cached
            } else {
                let mut image = original.clone();
                image.texture = prepared.texture(index).clone().into();
                image.texture_view = prepared.view(index).clone().into();
                if targets.pool.is_some() {
                    targets.cached.push((buffer.clone(), image.clone()));
                }
                image
            };
            images.insert(*id, image);
        }
        commands.insert_resource(prepared);
    } else {
        targets.exported.clear();
        for (id, original) in &targets.originals {
            if images.get(*id).unwrap().texture.id() != original.texture.id() {
                images.insert(*id, original.clone());
            }
        }
    }
    targets.setup_ms = start.elapsed().as_secs_f64() * 1000.0;
    targets.epoch = targets.epoch.checked_add(1).unwrap();
}

fn fraction(value: u8) -> [u8; 2] {
    if value == 0 {
        return [0, 0];
    }
    let power = 7 - value.leading_zeros();
    ((((power + 7) << 10) | ((u32::from(value) - (1 << power)) << (10 - power))) as u16)
        .to_le_bytes()
}

fn stamps(size: [u32; 2]) -> [[u32; 2]; 2] {
    [
        [size[0] / 2 - 276, size[1] / 2 - 36],
        [size[0] / 2 - 276, size[1] / 2 + 28],
    ]
}

fn stamp(targets: Res<Targets>, images: Res<RenderAssets<GpuImage>>) {
    let pixel = [
        fraction(targets.epoch),
        fraction(255 - targets.epoch),
        fraction(targets.epoch ^ 128),
        [0, 0x3c],
    ];
    let pixel = pixel.into_iter().flatten().collect::<Vec<_>>();
    let bytes = pixel.repeat(64);
    for (id, _) in &targets.originals {
        let texture = &images.get(*id).unwrap().texture;
        for [x, y] in stamps([texture.width(), texture.height()]) {
            targets.gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x, y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(64),
                    rows_per_image: Some(8),
                },
                wgpu::Extent3d {
                    width: 8,
                    height: 8,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}

fn install(canvas: &mut Presentation) {
    let mut ids = canvas
        .output_state
        .targets
        .iter()
        .filter(|(id, _)| Some(**id) != canvas.output_state.assignments.focused())
        .map(|(_, target)| target.image.id())
        .collect::<Vec<_>>();
    ids.push(canvas.target.id());
    let assets = canvas
        .app
        .sub_app(RenderApp)
        .world()
        .resource::<RenderAssets<GpuImage>>();
    let originals = ids
        .into_iter()
        .map(|id| {
            let image = assets.get(id).unwrap().clone();
            assert_eq!(image.texture.format(), wgpu::TextureFormat::Rgba16Float);
            (id, image)
        })
        .collect();
    canvas
        .app
        .sub_app_mut(RenderApp)
        .insert_resource(Targets {
            gpu: canvas.renderer.gpu.clone(),
            originals,
            direct: false,
            active_direct: false,
            pool: std::env::var_os("LINCE_PARK_DIRECT_POOL")
                .is_some()
                .then(|| Pool::new(6, 256 * 1024 * 1024)),
            cached: Vec::new(),
            exported: Vec::new(),
            epoch: 0,
            setup_ms: 0.0,
        })
        .add_systems(
            Render,
            prepare
                .in_set(RenderSystems::PrepareAssets)
                .after(bevy::render::render_asset::prepare_assets::<GpuImage>),
        )
        .add_systems(Render, stamp.after(RenderSystems::Render));
}

fn buffers(frame: &Frame) -> Vec<(OutputId, &smithay::backend::allocator::dmabuf::Dmabuf)> {
    let mut buffers = frame
        .outputs
        .iter()
        .map(|output| (output.id, output.dmabuf()))
        .collect::<Vec<_>>();
    buffers.push((frame.output.unwrap(), frame.dmabuf().unwrap()));
    buffers
}

struct Consumer {
    renderer: GlesRenderer,
    composition: crate::desktop::renderer::Composition,
    targets: HashMap<OutputId, GlesRenderbuffer>,
}

impl Consumer {
    fn new() -> Self {
        let device = EGLDevice::enumerate()
            .unwrap()
            .find(|device| !device.is_software())
            .unwrap();
        let display = unsafe { EGLDisplay::new(device) }.unwrap();
        let context = EGLContext::new(&display).unwrap();
        let renderer = unsafe { GlesRenderer::new(context) }.unwrap();
        Self {
            renderer,
            composition: Default::default(),
            targets: Default::default(),
        }
    }

    fn consume(&mut self, frame: &Frame) -> serde_json::Value {
        assert!(frame.ready().unwrap());
        let started = Instant::now();
        self.composition.canvas(&mut self.renderer, frame).unwrap();
        let imported = started.elapsed().as_secs_f64() * 1000.0;
        let mut draws = 0.0;
        let mut waits = 0.0;
        for (id, source) in buffers(frame) {
            let texture = self.renderer.import_dmabuf(source, None).unwrap();
            let size = source.size();
            let physical =
                smithay::utils::Size::<i32, smithay::utils::Physical>::from((size.w, size.h));
            if !self.targets.contains_key(&id) {
                self.targets.insert(
                    id,
                    self.renderer.create_buffer(Fourcc::Abgr8888, size).unwrap(),
                );
            }
            let target = self.targets.get_mut(&id).unwrap();
            assert_eq!(target.size(), size);
            let start = Instant::now();
            let fence = {
                let mut framebuffer = self.renderer.bind(target).unwrap();
                let mut render = self
                    .renderer
                    .render(
                        &mut framebuffer,
                        physical,
                        smithay::utils::Transform::Normal,
                    )
                    .unwrap();
                render
                    .clear(
                        smithay::backend::renderer::Color32F::BLACK,
                        &[smithay::utils::Rectangle::from_size(physical)],
                    )
                    .unwrap();
                render
                    .render_texture_at(
                        &texture,
                        (0, 0).into(),
                        1,
                        1.0,
                        smithay::utils::Transform::Normal,
                        &[smithay::utils::Rectangle::from_size(physical)],
                        &[],
                        1.0,
                    )
                    .unwrap();
                render.finish().unwrap()
            };
            draws += start.elapsed().as_secs_f64() * 1000.0;
            let start = Instant::now();
            fence.wait().unwrap();
            waits += start.elapsed().as_secs_f64() * 1000.0;
        }
        serde_json::json!({"import_ms":imported,"draw_submit_ms":draws,"gpu_wait_ms":waits,"complete_ms":started.elapsed().as_secs_f64()*1000.0})
    }
}

fn cpu() -> f64 {
    let now = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
    now.tv_sec as f64 * 1000.0 + now.tv_nsec as f64 / 1_000_000.0
}

fn render(canvas: &mut Presentation, pixels: bool) -> Frame {
    let frame = if pixels {
        canvas.pixel_frame().unwrap()
    } else {
        canvas.frame().unwrap()
    };
    frame.wait_for_probe().unwrap();
    let targets = canvas.app.sub_app(RenderApp).world().resource::<Targets>();
    if targets.active_direct {
        let actual = buffers(&frame);
        assert_eq!(actual.len(), targets.exported.len());
        for ((_, actual), expected) in actual.into_iter().zip(&targets.exported) {
            assert_eq!(
                actual, expected,
                "The direct frame was copied or used a different target epoch"
            );
        }
    } else {
        assert!(targets.exported.is_empty());
    }
    frame
}

fn snapshot(frame: &Frame) -> BTreeMap<OutputId, Vec<u8>> {
    let mut consumer = Consumer::new();
    consumer.consume(frame);
    buffers(frame)
        .into_iter()
        .map(|(id, image)| {
            let mut framebuffer = consumer
                .renderer
                .bind(consumer.targets.get_mut(&id).unwrap())
                .unwrap();
            let mapping = consumer
                .renderer
                .copy_framebuffer(
                    &mut framebuffer,
                    smithay::utils::Rectangle::from_size(image.size()),
                    Fourcc::Abgr8888,
                )
                .unwrap();
            let pixels = consumer.renderer.map_texture(&mapping).unwrap().to_vec();
            (id, pixels)
        })
        .collect()
}

fn check_snapshot(frame: &Frame, reference: &BTreeMap<OutputId, Vec<u8>>, epoch: u8) {
    let mut actual = snapshot(frame);
    let expected = [epoch, 255 - epoch, epoch ^ 128];
    for (id, image) in buffers(frame) {
        let bytes = actual.get_mut(&id).unwrap();
        let size = image.size();
        for [x, y] in stamps([size.w as u32, size.h as u32]) {
            let offset = ((y + 4) as usize * size.w as usize + (x + 4) as usize) * 4;
            assert!(
                bytes[offset..offset + 3].iter().zip(expected).all(
                    |(actual, expected)| (i32::from(*actual)
                        - (f64::from(expected) * 255.0 / 256.0).round() as i32)
                        .abs()
                        <= 1
                ),
                "Epoch {epoch} lost its pixels on output {id:?}: {:?}",
                &bytes[offset..offset + 3]
            );
            for y in y..y + 8 {
                let offset = (y as usize * size.w as usize + x as usize) * 4;
                bytes[offset..offset + 32].copy_from_slice(&reference[&id][offset..offset + 32]);
            }
        }
    }
    assert_eq!(
        &actual, reference,
        "Retained scene pixels changed at epoch {epoch}"
    );
}

#[test]
#[ignore = "private native direct-target and GLES handoff comparison; hardware Vulkan/EGL; no concurrent compilation"]
fn native_direct_targets_preserve_retained_pixels_and_compare_complete_handoff_cost() {
    let checks = native::laboratory::run_interface_checks().unwrap();
    assert!(
        checks[0]
            .name
            .ends_with("::protein_area_filter_visible_and_selectable")
    );
    assert!(
        checks.iter().all(|result| result.error.is_none()),
        "{checks:?}"
    );
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let path = std::path::PathBuf::from(
        std::env::var_os("LINCE_PARK_TEST_ARTIFACTS").expect("Set private artifacts"),
    );
    std::fs::create_dir_all(&path).unwrap();
    let mut reports = Vec::new();
    for count in [1, 2] {
        for pixels in [false, true] {
            let name = format!("outputs-{count}-pixels-{pixels}");
            let (mut canvas, _) =
                super::offscreen::fixture_with_color(directory.path(), &name, 0, true);
            assert!(canvas.linear_output);
            canvas
                .world_mut()
                .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                    std::time::Duration::from_millis(16),
                ));
            canvas
                .outputs(
                    [[1920, 1080], [1366, 768]]
                        .into_iter()
                        .take(count)
                        .enumerate()
                        .map(|(index, size)| {
                            let mut output = super::super::transfers::output(
                                index as u64 + 1,
                                if index == 0 { 0.0 } else { 1920.0 },
                                size,
                            );
                            output.viewport.workspace = index as u64 + 1;
                            output
                        })
                        .collect(),
                )
                .unwrap();
            for _ in 0..12 {
                drop(canvas.probe_frame().unwrap());
            }
            let reference_frame = canvas.probe_frame().unwrap();
            let reference = snapshot(&reference_frame);
            assert_eq!(reference.len(), count);
            for (id, image) in buffers(&reference_frame) {
                let size = image.size();
                let bytes = reference.get(&id).unwrap();
                let green = bytes
                    .chunks_exact(4)
                    .filter(|pixel| pixel[0] < 30 && pixel[1] > 230 && pixel[2] < 30)
                    .count();
                let mut glyphs = 0;
                for index in 0..8 {
                    let x = size.w / 2 - 252 - 34 + index * 72 + 4;
                    let y = size.h / 2 - 24;
                    let mut label_green = 0;
                    let mut label_ink = 0;
                    for y in y..y + 48 {
                        for x in x..x + 60 {
                            let offset = (y as usize * size.w as usize + x as usize) * 4;
                            label_ink += usize::from(
                                bytes[offset..offset + 3]
                                    .iter()
                                    .all(|channel| *channel < 80),
                            );
                            label_green += usize::from(
                                bytes[offset] < 30
                                    && bytes[offset + 1] > 230
                                    && bytes[offset + 2] < 30,
                            );
                        }
                    }
                    assert!(
                        label_green > 200 && label_ink > 20,
                        "The native label region lacks its glyphs/green background: {index}, green {label_green}, ink {label_ink}"
                    );
                    glyphs += label_ink;
                }
                assert!(
                    green > 20000 && glyphs > 200,
                    "The reference is not a visible native scene: green {green}, glyphs {glyphs}"
                );
            }
            drop(reference_frame);
            install(&mut canvas);
            let mut consumer = Consumer::new();
            for _ in 0..4 {
                consumer.consume(&render(&mut canvas, pixels));
            }
            let mut samples = Vec::new();
            let mut retained = Vec::new();
            let mut order = [false, true, true, false, true, false, false, true];
            if std::env::var_os("LINCE_PARK_DIRECT_REVERSE").is_some() {
                order.reverse();
            }
            for (block, direct) in order.into_iter().enumerate() {
                canvas
                    .app
                    .sub_app_mut(RenderApp)
                    .world_mut()
                    .resource_mut::<Targets>()
                    .direct = direct;
                for _ in 0..4 {
                    consumer.consume(&render(&mut canvas, pixels));
                }
                let mut last = None;
                for round in 0..8 {
                    let before = cpu();
                    let started = Instant::now();
                    let frame = render(&mut canvas, pixels);
                    let ready = started.elapsed().as_secs_f64() * 1000.0;
                    let native_cpu = cpu() - before;
                    let consumed = consumer.consume(&frame);
                    let epoch = canvas
                        .app
                        .sub_app(RenderApp)
                        .world()
                        .resource::<Targets>()
                        .epoch;
                    samples.push(serde_json::json!({"block":block,"round":round,"direct":direct,"epoch":epoch,"frame_ready_ms":ready,"native_cpu_ms":native_cpu,"target_setup_ms":canvas.app.sub_app(RenderApp).world().resource::<Targets>().setup_ms,"actual_direct":canvas.app.sub_app(RenderApp).world().resource::<Targets>().active_direct,"pool":canvas.app.sub_app(RenderApp).world().resource::<Targets>().pool.as_ref().map(Pool::stats),"gles":consumed,"complete_ms":started.elapsed().as_secs_f64()*1000.0,"process_cpu_ms":cpu()-before}));
                    last = Some((epoch, frame));
                }
                retained.push((direct, last.unwrap()));
            }
            for (_, (epoch, frame)) in &retained {
                check_snapshot(frame, &reference, *epoch);
            }
            let mut handoffs = Vec::new();
            for direct in [false, true] {
                canvas
                    .app
                    .sub_app_mut(RenderApp)
                    .world_mut()
                    .resource_mut::<Targets>()
                    .direct = direct;
                for round in 0..8 {
                    let previous = render(&mut canvas, pixels);
                    let previous_epoch = canvas
                        .app
                        .sub_app(RenderApp)
                        .world()
                        .resource::<Targets>()
                        .epoch;
                    let next = if pixels {
                        canvas.pixel_frame().unwrap()
                    } else {
                        canvas.frame().unwrap()
                    };
                    let next_was_pending = !next.ready().unwrap();
                    let next_epoch = canvas
                        .app
                        .sub_app(RenderApp)
                        .world()
                        .resource::<Targets>()
                        .epoch;
                    let consumed = consumer.consume(&previous);
                    next.wait_for_probe().unwrap();
                    check_snapshot(&previous, &reference, previous_epoch);
                    check_snapshot(&next, &reference, next_epoch);
                    handoffs.push(serde_json::json!({"direct":direct,"round":round,"previous_epoch":previous_epoch,"next_epoch":next_epoch,"newer_frame_was_pending":next_was_pending,"gles":consumed}));
                }
            }
            let sizes = [[1920, 1080], [1366, 768]]
                .into_iter()
                .take(count)
                .collect::<Vec<_>>();
            let report = serde_json::json!({"outputs":count,"pixels_only":pixels,"samples":samples,"handoffs_during_newer_work":handoffs,"retained_oracle_frames":retained.len(),"format":"Rgba16Float","pooled":canvas.app.sub_app(RenderApp).world().resource::<Targets>().pool.is_some(),"pool_final":canvas.app.sub_app(RenderApp).world().resource::<Targets>().pool.as_ref().map(Pool::stats),"sizes":sizes});
            std::fs::write(
                path.join(format!("{name}.json")),
                serde_json::to_vec_pretty(&report).unwrap(),
            )
            .unwrap();
            eprintln!(
                "native-direct: outputs {count}, pixels {pixels}, 64 measured frames, retained and interop pixels correct"
            );
            reports.push(report);
            canvas.finish().unwrap();
        }
    }
    let report = serde_json::json!({"scope":"Private test-profile native full/pixel-frame comparison, one/two physical-size simulated outputs, static visible native fixture with text. Fresh initialized direct RGBA16F output allocations replace RenderAssets before view preparation; old frames retained independently. Optional bounded test-only pool with explicit exported-image leases, GPU completion and external reacquisition/clearing; controlled GLES consumers fence before dropping frames. This does not establish every production consumer/teardown contract. No production fast path. Captures, replacement/format transitions, SRGB attachments and clients/Records are excluded. Actual compositor canvas import and representative GLES draw/fence are timed; this excludes final color management, DRM/presentation and user input. Initialization/allocation/registration/target replacement are inside candidate timing. Scene pixel oracles compare representative GLES-converted RGBA8; exact HDR/RGB10 storage has a separate GPU gate. Direct frames prove DMA-BUF identity with the prepared target. Retained image and concurrent-handoff pixel readback follows timing. Existing batched-copy control remains unchanged.","reports":reports});
    std::fs::write(
        path.join("native-direct-targets.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
