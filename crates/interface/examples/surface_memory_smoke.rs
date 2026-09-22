use bevy::{
    math::DVec2,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        camera::ExtractedCamera,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        renderer::RenderDevice,
        texture::GpuImage,
        view::{
            ViewDepthTexture, ViewTarget,
            screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
        },
    },
    winit::WinitSettings,
};
use lince_interface::{
    canvas::{CanvasItem, CanvasView},
    container::BoxRoot,
    topology::{presentation::Surface, view::View},
    workspace::{WorkspaceMember, Workspaces},
};
use std::{collections::HashMap, time::Instant};

fn memory() -> (String, u64) {
    let resident = std::fs::read_to_string("/proc/self/status")
        .unwrap_or_default()
        .lines()
        .find(|line| line.starts_with("VmRSS:"))
        .unwrap_or("VmRSS unavailable")
        .to_string();
    let mut clients = HashMap::new();
    if let Ok(entries) = std::fs::read_dir("/proc/self/fdinfo") {
        for entry in entries.flatten() {
            let info = std::fs::read_to_string(entry.path()).unwrap_or_default();
            let field = |prefix| {
                info.lines()
                    .find_map(|line| line.strip_prefix(prefix))
                    .and_then(|value| value.split_whitespace().next())
                    .and_then(|value| value.parse::<u64>().ok())
            };
            if let (Some(client), Some(bytes)) =
                (field("drm-client-id:"), field("drm-total-system0:"))
            {
                clients.insert(client, bytes);
            }
        }
    }
    (resident, clients.values().sum())
}

#[derive(Resource)]
struct Trial {
    frame: u32,
    started: Instant,
    peak: u64,
    root: Option<Entity>,
    last_frame: Instant,
    frame_times: Vec<(u32, f64)>,
}

#[derive(Resource, Default, Clone, ExtractResource)]
struct ReleasedCaptures(Vec<AssetId<Image>>);

fn verify_targets(
    cameras: Query<(
        &ExtractedCamera,
        Option<&ViewTarget>,
        Option<&ViewDepthTexture>,
        Option<&Camera2d>,
        Option<&bevy::ui_render::UiCameraView>,
        Option<&bevy::render::view::visibility::RenderVisibleEntities>,
    )>,
    views: Query<&bevy::render::view::ExtractedView>,
    phases: Res<bevy::render::render_phase::ViewSortedRenderPhases<bevy::ui_render::TransparentUi>>,
    device: Res<RenderDevice>,
    released: Res<ReleasedCaptures>,
    images: Res<RenderAssets<GpuImage>>,
    mut frame: Local<u32>,
) {
    for (camera, color, depth, scene_camera, _, visible_meshes) in &cameras {
        if camera.order == -1 {
            assert!(
                color.is_none(),
                "UI captures must not allocate intermediate color buffers"
            );
            assert!(
                depth.is_none(),
                "UI captures must not allocate depth buffers"
            );
            assert!(
                scene_camera.is_none(),
                "UI captures must not process scene geometry"
            );
            assert!(
                visible_meshes.is_none(),
                "UI captures must not collect scene meshes"
            );
        }
    }
    *frame += 1;
    if *frame == 40 {
        let mut counts = [0; 3];
        for (camera, _, _, _, ui, _) in &cameras {
            if camera.order != -1 {
                continue;
            }
            counts[0] += 1;
            if let Some(ui) = ui.and_then(|ui| views.get(ui.0).ok()) {
                counts[1] += 1;
                counts[2] += phases
                    .get(&ui.retained_view_entity)
                    .map_or(0, |p| p.items.len());
            }
        }
        println!("Capture cameras, UI views, draw items: {counts:?}");
        assert_eq!(counts[0], 1000);
        assert_eq!(counts[1], 1000);
        assert_eq!(
            counts[2], 0,
            "Unchanged captures must skip UI geometry preparation"
        );
    }
    if *frame >= 189 {
        assert_eq!(released.0.len(), 1000);
        assert!(
            released.0.iter().all(|id| images.get(*id).is_none()),
            "Deleted captures must release their GPU images"
        );
    }
    if *frame == 40
        && let Some(report) = device.wgpu_device().generate_allocator_report()
    {
        let mut allocations: HashMap<String, (u64, usize)> = HashMap::new();
        for allocation in report.allocations {
            let entry = allocations.entry(allocation.name).or_default();
            entry.0 += allocation.size;
            entry.1 += 1;
        }
        let mut allocations: Vec<_> = allocations.into_iter().collect();
        allocations.sort_by_key(|(_, (bytes, _))| std::cmp::Reverse(*bytes));
        println!(
            "GPU allocator: {} bytes allocated, {} reserved; largest allocations: {:?}",
            report.total_allocated_bytes,
            report.total_reserved_bytes,
            &allocations[..allocations.len().min(12)]
        );
    }
}

fn exercise(world: &mut World) {
    {
        let mut trial = world.resource_mut::<Trial>();
        trial.frame += 1;
        let sample = (
            trial.frame,
            trial.last_frame.elapsed().as_secs_f64() * 1000.0,
        );
        trial.frame_times.push(sample);
        trial.last_frame = Instant::now();
    }
    let frame = world.resource::<Trial>().frame;
    if frame == 5 {
        let root = world
            .query_filtered::<Entity, With<BoxRoot>>()
            .single(world)
            .unwrap();
        world.resource_mut::<Trial>().root = Some(root);
        let title_font = world
            .resource::<lince_interface::theme::Typography>()
            .text(24.0);
        let row_font = world
            .resource::<lince_interface::theme::Typography>()
            .text(18.0);
        for index in 0..1000 {
            let size = if index % 2 == 0 {
                Vec2::splat(300.0)
            } else {
                Vec2::new(340.0, 640.0)
            };
            let sand = world
                .spawn((
                    Node {
                        width: px(size.x),
                        height: px(size.y),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(px(12)),
                        row_gap: px(8),
                        ..default()
                    },
                    CanvasItem {
                        position: DVec2::new(
                            (index % 32) as f64 * 380.0,
                            (index / 32) as f64 * 720.0,
                        ),
                        size,
                    },
                    BackgroundColor(if index % 2 == 0 {
                        Color::srgb(0.25, 0.35, 0.65)
                    } else {
                        Color::srgb(0.2, 0.55, 0.4)
                    }),
                    WorkspaceMember(1),
                    ChildOf(root),
                ))
                .id();
            world.spawn((
                Text::new(format!("Surface {index}")),
                title_font.clone(),
                TextColor(Color::WHITE),
                ChildOf(sand),
            ));
            if index % 2 != 0 {
                for row in 0..4 {
                    world.spawn((
                        Text::new(format!("Row {row} · Text, layout and clipping")),
                        row_font.clone(),
                        TextColor(Color::WHITE),
                        ChildOf(sand),
                    ));
                }
            }
        }
        world.get_mut::<CanvasView>(root).unwrap().center = DVec2::new(5800.0, 11000.0);
        world.get_mut::<CanvasView>(root).unwrap().set_zoom(0.04);
    }
    let Some(root) = world.resource::<Trial>().root else {
        return;
    };
    if frame == 25 {
        world.entity_mut(root).insert(View {
            spatial: true,
            position: [5800.0, 24000.0, 28000.0],
            pitch: -0.95,
            ..default()
        });
    }
    if frame == 45 {
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("/tmp/lince-surface-memory.png"))
            .observe(|event: On<ScreenshotCaptured>| {
                let pixels = event.image.clone().try_into_dynamic().unwrap().to_rgb8();
                let colored = pixels
                    .pixels()
                    .filter(|pixel| {
                        pixel[1] > 90 && f32::from(pixel[1]) > f32::from(pixel[0]) * 1.4
                            || pixel[2] > 110 && f32::from(pixel[2]) > f32::from(pixel[0]) * 1.4
                    })
                    .count();
                assert!(
                    colored > 10000,
                    "Surface captures must be visible: {colored} colored pixels"
                );
            });
    }
    if (50..114).contains(&frame) {
        let angle = (frame - 50) as f32 * 0.31;
        world.entity_mut(root).insert(View {
            spatial: true,
            position: [
                5800.0 + f64::from(angle.sin()) * 10000.0,
                1000.0 + f64::from(angle.cos().abs()) * 18000.0,
                11000.0 + f64::from(angle.cos()) * 10000.0,
            ],
            yaw: angle,
            pitch: -0.7,
            ..default()
        });
    }
    if frame == 120 {
        world.get_mut::<Workspaces>(root).unwrap().active = 2;
    }
    if frame == 115 {
        world.entity_mut(root).insert(View {
            spatial: true,
            position: [0.0, 500.0, 500.0],
            pitch: -std::f32::consts::FRAC_PI_4,
            ..default()
        });
    }
    if frame == 118 {
        let entity = world
            .query::<(Entity, &CanvasItem)>()
            .iter(world)
            .find(|(_, item)| item.position == DVec2::ZERO)
            .unwrap()
            .0;
        let bounds = lince_interface::topology::presentation::bounds(world, entity).unwrap();
        let scale = world
            .query::<&Window>()
            .single(world)
            .unwrap()
            .resolution
            .scale_factor();
        let min = (bounds.min * scale).max(Vec2::ZERO).as_uvec2();
        let max = (bounds.max * scale).max(Vec2::ZERO).as_uvec2();
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("/tmp/lince-surface-text.png"))
            .observe(move |event: On<ScreenshotCaptured>| {
                let width = event.image.width();
                let data = event.image.data.as_ref().unwrap();
                let mut text_pixels = 0;
                for y in min.y..max.y.min(event.image.height()) {
                    for x in min.x..max.x.min(width) {
                        let pixel = &data[((y * width + x) * 4) as usize..][..3];
                        text_pixels += usize::from(pixel.iter().all(|value| *value > 210));
                    }
                }
                assert!(
                    text_pixels > 30,
                    "Close-up text must remain visible: {text_pixels} text pixels"
                );
            });
    }
    if (140..156).contains(&frame) {
        world.get_mut::<Workspaces>(root).unwrap().active = 1;
        world.get_mut::<View>(root).unwrap().spatial = frame.is_multiple_of(2);
    }
    if frame == 156 {
        world.get_mut::<Workspaces>(root).unwrap().active = 2;
    }
    if frame > 8 {
        let images = world.resource::<Assets<Image>>();
        let surfaces: Vec<_> = world
            .iter_entities()
            .filter_map(|entity| entity.get::<Surface>())
            .collect();
        assert_eq!(surfaces.len(), if frame <= 165 { 1000 } else { 0 });
        let mut bytes = 0;
        let mut visible = 0;
        for surface in surfaces {
            assert!(images.get(&surface.image).unwrap().data.is_none());
            assert!(
                world
                    .get::<bevy::camera::visibility::VisibleEntities>(surface.camera)
                    .is_none()
            );
            bytes += u64::from(surface.pixels.x) * u64::from(surface.pixels.y) * 4;
            visible += usize::from(surface.visible);
        }
        assert!(bytes <= 128 * 1024 * 1024);
        world.resource_mut::<Trial>().peak = world.resource::<Trial>().peak.max(bytes);
        if frame.is_multiple_of(20) && frame <= 165 {
            let (resident, graphics) = memory();
            let glyphs: usize = world
                .query::<&bevy::text::TextLayoutInfo>()
                .iter(world)
                .map(|layout| layout.glyphs.len())
                .sum();
            assert!(
                glyphs > 10000,
                "The workload must include rendered text: {glyphs}"
            );
            let atlas_bytes = world
                .resource::<bevy::text::FontAtlasSet>()
                .total_bytes(world.resource::<Assets<Image>>());
            println!(
                "frame={frame} visible={visible} glyphs={glyphs} capture_MiB={:.2} atlas_MiB={:.2} elapsed={:.2}s {resident} drm_MiB={:.2}",
                bytes as f64 / 1_048_576.0,
                atlas_bytes as f64 / 1_048_576.0,
                world.resource::<Trial>().started.elapsed().as_secs_f64(),
                graphics as f64 / 1024.0,
            );
        }
        if frame == 130 {
            assert_eq!(bytes, 4000);
        }
        if frame == 165 {
            assert_eq!(bytes, 4000);
            world.resource_mut::<ReleasedCaptures>().0 = world
                .query::<&Surface>()
                .iter(world)
                .map(|surface| surface.image.id())
                .collect();
            let entities: Vec<_> = world
                .query_filtered::<Entity, With<Surface>>()
                .iter(world)
                .collect();
            for entity in entities {
                world.despawn(entity);
            }
        }
        if frame > 165 && frame.is_multiple_of(5) {
            println!(
                "Remaining captures at {frame}: {} CPU images, {} materials",
                world
                    .resource::<ReleasedCaptures>()
                    .0
                    .iter()
                    .filter(|id| world.resource::<Assets<Image>>().get(**id).is_some())
                    .count(),
                world.resource::<Assets<StandardMaterial>>().len()
            );
        }
        if frame == 190 {
            assert_eq!(bytes, 0);
            assert!(
                world
                    .resource::<ReleasedCaptures>()
                    .0
                    .iter()
                    .all(|id| world.resource::<Assets<Image>>().get(*id).is_none())
            );
            assert_eq!(
                world
                    .query::<&lince_interface::topology::presentation::VisualOwner>()
                    .iter(world)
                    .count(),
                0
            );
            for (label, range) in [("stationary 3D", 32..44), ("moving camera", 55..113)] {
                let mut times: Vec<_> = world
                    .resource::<Trial>()
                    .frame_times
                    .iter()
                    .filter(|(frame, _)| range.contains(frame))
                    .map(|(_, time)| *time)
                    .collect();
                times.sort_by(f64::total_cmp);
                println!(
                    "{label}: median={:.2}ms p95={:.2}ms",
                    times[times.len() / 2],
                    times[times.len() * 95 / 100]
                );
            }
            println!(
                "PASS: 1000 mixed UI surfaces, repeated 2D/3D transitions, camera movement, workspace release and deletion; peak capture MiB={:.2}",
                world.resource::<Trial>().peak as f64 / 1_048_576.0
            );
            world.write_message(AppExit::Success);
        }
    }
}

fn main() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _guard = runtime.enter();
    let mut app = lince_interface::app::interface_app();
    app.add_plugins(bevy::log::LogPlugin::default())
        .add_plugins(ExtractResourcePlugin::<ReleasedCaptures>::default())
        .init_resource::<ReleasedCaptures>()
        .insert_resource(WinitSettings::continuous())
        .insert_resource(Trial {
            frame: 0,
            started: Instant::now(),
            peak: 0,
            root: None,
            last_frame: Instant::now(),
            frame_times: Vec::new(),
        })
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(BoxRoot);
        })
        .add_systems(
            Update,
            exercise.after(lince_interface::topology::presentation::synchronize),
        );
    app.sub_app_mut(RenderApp)
        .add_systems(Render, verify_targets.in_set(RenderSystems::Cleanup));
    app.run();
}
