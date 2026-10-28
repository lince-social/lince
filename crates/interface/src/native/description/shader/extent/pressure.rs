use super::*;
use crate::native::{
    actions::Action,
    canvas::{CanvasItem, CanvasView},
    container::BoxRoot,
    edit_mode::EditAction,
    layout::{LayoutBox, desktop::DefaultLayout},
    sand_store::{SandKind, StoredSand, spawn_sand},
    shader_effects::{Background, OpenSurface, Surface as EffectSurface},
    topology::surface_budget::{BYTE_BUDGET, Diagnostics, NativeRaster, residency::Usage},
    workspace::Workspaces,
};
use bevy::{
    app::PluginsState,
    asset::RenderAssetUsages,
    render::{
        RenderPlugin,
        gpu_readback::{Readback, ReadbackComplete},
    },
};
use std::path::Path;

#[derive(Resource, Default)]
struct Pixels(Vec<u8>);

fn resize(world: &mut World, owner: Entity, size: Vec2) {
    world.get_mut::<CanvasItem>(owner).unwrap().size = size;
    if let Some(mut layout) = world.get_mut::<LayoutBox>(owner) {
        layout.rules = crate::layout::Rules::fixed(size);
    }
    if let Some(content) = world.get::<StoredSand>(owner).and_then(|sand| sand.content)
        && let Some(mut text) = world.get_mut::<crate::native::sand_text::SandText>(content)
    {
        text.size = [(size.x - 32.0).max(16.0), (size.y - 32.0).max(16.0)];
    }
}

fn step(app: &mut App) {
    app.update();
    app.sub_app(bevy::render::RenderApp)
        .world()
        .resource::<bevy::render::renderer::RenderDevice>()
        .poll(bevy::render::render_resource::PollType::Poll)
        .unwrap();
    if let Some(budget) = app.world().get_resource::<Diagnostics>() {
        assert!(budget.granted_bytes <= BYTE_BUDGET);
    }
    if let Some(usage) = app.world().get_resource::<Usage>() {
        let usage = usage.snapshot();
        assert!(!usage.lower_bound);
        assert!(usage.peak_bytes <= BYTE_BUDGET * 4, "{usage:?}");
    }
}

#[track_caller]
fn wait(app: &mut App, predicate: impl Fn(&World) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        step(app);
        if predicate(app.world()) {
            return;
        }
        if Instant::now() >= deadline {
            let captures: Vec<_> = app
                .world_mut()
                .query::<(Entity, &crate::native::topology::presentation::Surface)>()
                .iter(app.world())
                .map(|(entity, surface)| {
                    (
                        entity,
                        surface.pixels,
                        app.world()
                            .resource::<crate::native::topology::surface_render::CompletedCaptures>(
                            )
                            .contains(&surface.image),
                        app.world()
                            .get::<crate::native::topology::surface_render::CaptureReady>(
                                surface.camera,
                            )
                            .map(|ready| ready.0),
                    )
                })
                .collect();
            let sources: Vec<_> = app
                .world_mut()
                .query::<(Entity, &EffectPreview, &Raster)>()
                .iter(app.world())
                .map(|(entity, preview, raster)| {
                    (
                        entity,
                        preview.ready,
                        preview.painted,
                        preview.error.clone(),
                        *raster,
                    )
                })
                .collect();
            panic!(
                "The pressure scenario did not settle: {:?}; captures: {captures:?}; sources: {sources:?}",
                app.world().resource::<Usage>().snapshot()
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn capture(app: &mut App, image: &Handle<Image>, name: &str) {
    let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") else {
        return;
    };
    app.world_mut().resource_mut::<Pixels>().0.clear();
    let entity = app
        .world_mut()
        .spawn(Readback::texture(image.clone()))
        .observe(|event: On<ReadbackComplete>, mut pixels: ResMut<Pixels>| {
            pixels.0 = event.data.clone();
        })
        .id();
    wait(app, |world| !world.resource::<Pixels>().0.is_empty());
    app.world_mut().despawn(entity);
    let mut ppm = b"P6\n1280 720\n255\n".to_vec();
    for pixel in app.world().resource::<Pixels>().0.chunks_exact(4) {
        ppm.extend_from_slice(&pixel[..3]);
    }
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(Path::new(&directory).join(format!("{name}.ppm")), ppm).unwrap();
}

fn record(
    app: &App,
    live: Entity,
    editor: Entity,
    stage: &str,
    samples: &mut Vec<serde_json::Value>,
) {
    let world = app.world();
    let budget = world.resource::<Diagnostics>();
    let usage = world.resource::<Usage>().snapshot();
    let native = world.get::<NativeRaster>(live).unwrap();
    let source = world.get::<Raster>(editor).unwrap();
    let live_source = world
        .get::<Raster>(world.get::<Background>(live).unwrap().preview)
        .unwrap();
    samples.push(serde_json::json!({
        "stage": stage,
        "budget_bytes": BYTE_BUDGET,
        "fixture_texture_ceiling_bytes": BYTE_BUDGET * 4,
        "granted_bytes": budget.granted_bytes,
        "limited_targets": budget.limited_targets,
        "native_requested": native.requested.to_array(),
        "native_granted": native.pixels.to_array(),
        "editor_requested": source.requested.to_array(),
        "editor_granted": source.pixels.to_array(),
        "live_source_requested": live_source.requested.to_array(),
        "live_source_granted": live_source.pixels.to_array(),
        "current_texture_bytes": usage.current_bytes,
        "retained_image_bytes": usage.held_bytes,
        "pending_gpu_bytes": usage.inflight_bytes,
        "estimated_cache_bytes": usage.cached_estimate_bytes,
        "peak_texture_bytes": usage.peak_bytes,
        "partial": usage.lower_bound,
    }));
}

fn readout(world: &mut World) -> Entity {
    world
        .query::<(Entity, &Name)>()
        .iter(world)
        .find(|(_, name)| name.as_str() == "rendering.diagnostics")
        .map(|(entity, _)| entity)
        .unwrap()
}

fn reveal(app: &mut App, entity: Entity) {
    for _ in 0..8 {
        let world = app.world();
        let center = world
            .get::<UiGlobalTransform>(entity)
            .unwrap()
            .transform_point2(Vec2::ZERO);
        let Some(clip) = world.get::<CalculatedClip>(entity).map(|clip| clip.clip) else {
            return;
        };
        if clip.contains(center) {
            return;
        }
        let mut parent = world.get::<ChildOf>(entity).map(ChildOf::parent);
        let mut scroll = None;
        for _ in 0..64 {
            let Some(current) = parent else { break };
            if world.get::<ScrollPosition>(current).is_some() {
                scroll = Some(current);
                break;
            }
            parent = world.get::<ChildOf>(current).map(ChildOf::parent);
        }
        let scroll = scroll.expect("The Park readout needs its ordinary scrollable panel");
        app.world_mut()
            .get_mut::<ScrollPosition>(scroll)
            .unwrap()
            .0
            .y += center.y - clip.center().y;
        for _ in 0..3 {
            step(app);
        }
    }
    panic!("The rendering readout was not reachable");
}

#[test]
fn gpu_native_pressure_preserves_the_inspected_target_and_recovers_quality_after_resize_and_close()
{
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    let directory = tempfile::tempdir().unwrap();
    let plugins = DefaultPlugins
        .build()
        .disable::<bevy::winit::WinitPlugin>()
        .disable::<bevy::log::LogPlugin>()
        .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>()
        .set(WindowPlugin {
            primary_window: Some(Window {
                resolution: (1280, 720).into(),
                ..default()
            }),
            exit_condition: bevy::window::ExitCondition::DontExit,
            ..default()
        })
        .set(RenderPlugin {
            synchronous_pipeline_compilation: true,
            ..default()
        });
    let mut app = crate::native::app::presentation_app(directory.path().join("assets"), plugins);
    app.insert_resource(bevy::render::error_handler::RenderErrorHandler(
        |error, _, _| {
            panic!("Pressure rendering failed: {error:?}");
        },
    ));
    assert!(
        app.world()
            .get_resource::<crate::native::workspace::WorkspaceFile>()
            .is_none()
    );
    crate::native::laboratory::isolate(app.world_mut());
    app.insert_resource(DefaultLayout(crate::host::WorkspaceLayout::Free));
    app.init_resource::<Pixels>();
    app.add_systems(Startup, crate::native::app::canvas);
    let deadline = Instant::now() + Duration::from_secs(30);
    while app.plugins_state() != PluginsState::Ready {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    app.finish();
    app.cleanup();
    let mut image = Image::new_target_texture(1280, 720, TextureFormat::Rgba8UnormSrgb, None);
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    image.asset_usage = RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD;
    let image = app.world_mut().resource_mut::<Assets<Image>>().add(image);
    app.insert_resource(crate::native::app::OutputTarget(RenderTarget::Image(
        image.clone().into(),
    )));
    for _ in 0..4 {
        step(&mut app);
    }
    let root = app
        .world_mut()
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(app.world())
        .unwrap();
    let workspace = app.world().get::<Workspaces>(root).unwrap().active;
    crate::native::workspace_config::set_physics(app.world_mut(), root, workspace, false);
    *app.world_mut().get_mut::<CanvasView>(root).unwrap() = CanvasView::default();
    let mut sands = Vec::new();
    for index in 0..16 {
        let sand = spawn_sand(
            app.world_mut(),
            root,
            workspace,
            SandKind::Text,
            &format!("Pressure view {index}"),
            bevy::math::DVec2::new(-1000.0, 0.0),
        );
        resize(app.world_mut(), sand, Vec2::new(1280.0, 720.0));
        sands.push(sand);
    }
    let live = sands.remove(0);
    let effect = Effect {
        programs: [("check".into(), "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { let cell = vec2<u32>(floor(uv * inputs.size / 8.0)); let value = f32((cell.x + cell.y) % 2u); return vec4<f32>(0.1 + value * 0.7, 0.3, 0.8, 1.0); }".into())].into(),
        rules: Vec::new(), fallback: "check".into(), ..default()
    };
    app.world_mut().entity_mut(live).insert(EffectSurface {
        effect,
        preset: "custom".into(),
        motion: false,
        strength: 1.0,
        glow: 0.0,
        ..default()
    });
    wait(&mut app, |world| world.get::<Background>(live).is_some());
    let source = app.world().get::<Background>(live).unwrap().preview;
    OpenSurface(live).apply(app.world_mut(), live);
    let editor = app
        .world_mut()
        .query::<(Entity, &EffectPreview)>()
        .iter(app.world())
        .find(|(entity, state)| {
            state.priority
                && crate::native::shader_effects::inspected_source(app.world(), *entity)
                    == Some(source)
        })
        .map(|(entity, _)| entity)
        .unwrap();
    let owner = app.world().get::<ChildOf>(editor).unwrap().parent();
    resize(app.world_mut(), owner, Vec2::new(700.0, 680.0));
    for _ in 0..4 {
        step(&mut app);
    }
    crate::native::layout::desktop::focus_item(app.world_mut(), owner);
    wait(&mut app, |world| {
        world
            .get::<NativeRaster>(live)
            .is_some_and(|raster| raster.budget_limited)
            && world.get::<Raster>(editor).is_some_and(|raster| {
                raster.visible && !raster.budget_limited && !raster.device_limited
            })
            && world
                .get::<EffectPreview>(editor)
                .is_some_and(|state| state.ready && state.painted >= 3)
            && world
                .get::<EffectPreview>(source)
                .is_some_and(|state| state.ready && state.painted >= 3)
    });
    let source_image = effect_image(app.world(), source).unwrap();
    let editor_image = effect_image(app.world(), editor).unwrap();
    let source_requested = app.world().get::<Raster>(source).unwrap().requested;
    let requested = app.world().get::<NativeRaster>(live).unwrap().requested;
    let original_grant = app.world().get::<NativeRaster>(live).unwrap().pixels;
    let label = app.world().get::<Preview>(editor).unwrap().label;
    assert!(
        app.world()
            .get::<Text>(label)
            .unwrap()
            .0
            .contains("Live Sand:")
    );
    assert!(
        app.world()
            .get::<Text>(label)
            .unwrap()
            .0
            .contains("Sand capture")
    );
    let mut samples = Vec::new();
    record(&app, live, editor, "pressure", &mut samples);
    reveal(&mut app, label);
    capture(&mut app, &image, "pressure-inspector");
    EditAction::Open.apply(app.world_mut(), root);
    EditAction::Park.apply(app.world_mut(), root);
    for _ in 0..4 {
        step(&mut app);
    }
    let diagnostics = readout(app.world_mut());
    let limited = app.world().resource::<Diagnostics>().limited_targets;
    assert!(limited > 0);
    assert!(
        app.world()
            .get::<Text>(diagnostics)
            .unwrap()
            .0
            .contains(&format!("{limited} reduced targets"))
    );
    reveal(&mut app, diagnostics);
    capture(&mut app, &image, "pressure-settings");
    EditAction::Close.apply(app.world_mut(), root);
    for (index, size) in [
        Vec2::new(1160.0, 660.0),
        Vec2::new(1440.0, 800.0),
        Vec2::new(1280.0, 720.0),
    ]
    .into_iter()
    .cycle()
    .take(6)
    .enumerate()
    {
        for sand in &sands {
            resize(app.world_mut(), *sand, size);
        }
        for _ in 0..8 {
            step(&mut app);
        }
        eprintln!(
            "Resized pressure stage {index}: {:?}",
            app.world().resource::<Usage>().snapshot()
        );
        wait(&mut app, |world| {
            world.resource::<Usage>().snapshot().retained_bytes() == 0
        });
        assert!(
            app.world()
                .get::<NativeRaster>(live)
                .unwrap()
                .budget_limited
        );
        assert_eq!(effect_image(app.world(), source).unwrap(), source_image);
        assert_eq!(effect_image(app.world(), editor).unwrap(), editor_image);
        assert_eq!(
            app.world().get::<Raster>(source).unwrap().requested,
            source_requested
        );
        assert_eq!(
            app.world().get::<NativeRaster>(live).unwrap().requested,
            requested
        );
        if index % 3 == 2 {
            assert_eq!(
                app.world().get::<NativeRaster>(live).unwrap().pixels,
                original_grant
            );
        }
        record(&app, live, editor, &format!("resize-{index}"), &mut samples);
    }
    for sand in sands {
        app.world_mut().despawn(sand);
    }
    wait(&mut app, |world| {
        world
            .get::<NativeRaster>(live)
            .is_some_and(|raster| !raster.budget_limited && raster.pixels == raster.requested)
    });
    for _ in 0..8 {
        step(&mut app);
    }
    wait(&mut app, |world| {
        world.resource::<Usage>().snapshot().retained_bytes() == 0
    });
    assert_eq!(
        crate::native::shader_effects::inspected_source(app.world(), editor),
        Some(source)
    );
    assert!(
        !app.world()
            .get::<Text>(label)
            .unwrap()
            .0
            .contains("Sand capture")
    );
    assert_eq!(effect_image(app.world(), source).unwrap(), source_image);
    record(&app, live, editor, "recovered", &mut samples);
    capture(&mut app, &image, "pressure-recovered");
    EditAction::Open.apply(app.world_mut(), root);
    EditAction::Park.apply(app.world_mut(), root);
    for _ in 0..4 {
        step(&mut app);
    }
    let diagnostics = readout(app.world_mut());
    assert!(
        app.world()
            .get::<Text>(diagnostics)
            .unwrap()
            .0
            .contains("0 reduced targets")
    );
    assert_eq!(
        app.world().get::<NativeRaster>(live).unwrap().requested,
        requested
    );
    app.world_mut()
        .entity_mut(live)
        .insert(crate::native::sand_placement::Pinned {
            anchor: [0.5; 2],
            scale: 1.0,
        });
    wait(&mut app, |world| {
        world
            .get::<crate::native::topology::presentation::Surface>(live)
            .is_none()
            && world.get::<NativeRaster>(live).is_none()
    });
    app.world_mut()
        .entity_mut(live)
        .remove::<crate::native::sand_placement::Pinned>();
    wait(&mut app, |world| {
        world
            .get::<NativeRaster>(live)
            .is_some_and(|raster| raster.pixels == requested && raster.requested == requested)
    });
    assert_eq!(effect_image(app.world(), source).unwrap(), source_image);
    app.world_mut().despawn(owner);
    app.world_mut().despawn(live);
    drop(source_image);
    drop(editor_image);
    wait(&mut app, |world| {
        world.resource::<Usage>().snapshot().total_bytes() == 0
    });
    let usage = app.world().resource::<Usage>().snapshot();
    assert_eq!(usage.textures, 0);
    samples.push(serde_json::json!({
        "stage": "all_closed",
        "tracked_texture_bytes": usage.total_bytes(),
        "tracked_textures": usage.textures,
        "peak_texture_bytes": usage.peak_bytes,
        "fixture_texture_ceiling_bytes": BYTE_BUDGET * 4,
    }));
    if let Some(directory) = std::env::var_os("LINCE_PARK_TEST_ARTIFACTS") {
        std::fs::write(
            Path::new(&directory).join("pressure-metrics.json"),
            serde_json::to_vec_pretty(&samples).unwrap(),
        )
        .unwrap();
    }
    assert!(app.world().resource::<Usage>().snapshot().peak_bytes > 0);
    assert!(app.world().get_entity(live).is_err());
}
