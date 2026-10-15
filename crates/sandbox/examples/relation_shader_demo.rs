use bevy::{
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    winit::WinitSettings,
};
use lince_sandbox::{
    canvas::{CanvasItem, CanvasView},
    container::BoxRoot,
    protein_area::RecordBinding,
};
use nucleus::shader_effect::{Effect, Extension, NAMESPACE};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[path = "relation_shader_demo/controls.rs"]
mod control_review;

#[derive(Resource)]
struct Demo {
    ids: Vec<String>,
    owner: Option<Entity>,
    alternate_owner: Option<Entity>,
    since: Instant,
    stage: u8,
    capturing: bool,
    numeric: bool,
    catalog: bool,
    mesh: bool,
    generated: bool,
    volume: bool,
    lighting: Option<nucleus::canvas::appearance::ShadowQuality>,
    review_controls: bool,
    frame_times: Vec<f64>,
    reference_blue: Option<u8>,
}

#[derive(Component)]
struct DemoLabel;

fn exercise(world: &mut World) {
    if control_review::active(world) {
        return;
    }
    assert!(
        world.resource::<Demo>().since.elapsed() < Duration::from_secs(60),
        "Relation shader demonstration timed out"
    );
    let stage = world.resource::<Demo>().stage;
    let numeric = world.resource::<Demo>().numeric;
    let catalog = world.resource::<Demo>().catalog;
    let mesh = world.resource::<Demo>().mesh;
    let generated = world.resource::<Demo>().generated;
    let volume = world.resource::<Demo>().volume;
    let lighting = world.resource::<Demo>().lighting;
    for mut text in world
        .query_filtered::<&mut Text, With<DemoLabel>>()
        .iter_mut(world)
    {
        text.0 = if lighting.is_some() {
            match stage {
                0 => "2D · Lit WGSL sphere previews · independent Record textures",
                1 => "3D · Generated spheres · two bounded shadow lights",
                2 if numeric => "3D · Protein 20 → 80 · lit mesh data updates",
                _ => "3D · Task completed · its driven light turns off",
            }
        } else if volume {
            match stage {
                0 => "2D · Volume previews above Record titles · bounded WGSL fields",
                1 => "3D · Raymarched volumes · scene depth clips each field",
                2 if numeric => "3D · Protein 20 → 80 brightens the volume · task stays −3",
                _ => "3D · Task quantity −3 → 0 · gray volume stops moving",
            }
        } else if mesh {
            match stage {
                0 => "2D · Real mesh previews above Record titles · WGSL vertex deformation",
                1 if generated => "3D · Compute-generated spheres · WGSL produces every vertex",
                1 => "3D · Actual WGSL-deformed sphere geometry and 3D position shading",
                2 if numeric => "3D · Protein 20 → 80 brightens the mesh · task quantity stays −3",
                _ => "3D · Task quantity −3 → 0 · gray geometry stops moving",
            }
        } else if catalog {
            match stage {
                0 => "2D · Same Records, two Castles · main above, amber effect below",
                1 => "3D · Two independently selected effects from each Record catalog",
                2 if numeric => "3D · Protein 20 → 80 · main brightens, amber stays independent",
                _ => "3D · Task quantity −3 → 0 · main calms, amber selection stays independent",
            }
        } else {
            match stage {
                0 => "2D · Protein gathers 3 linked Records · quantities: −3, 0, 5",
                1 => "3D · Real sphere meshes above Record titles · quantities: −3, 0, 5",
                2 if numeric => "3D · Protein field changed: 20 → 80 · task quantity stays −3",
                _ => "3D · Clean room quantity changed: −3 → 0 · its WGSL switches to calm",
            }
        }
        .into();
    }
    let root = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(world)
        .unwrap();
    if world.resource::<Demo>().owner.is_none() {
        if world
            .get::<lince_sandbox::workspace::Workspaces>(root)
            .is_none()
        {
            return;
        }
        let owner = lince_sandbox::relation_castle::spawn(world, root).unwrap();
        let ids = world.resource::<Demo>().ids.clone();
        let mut area = world
            .get_mut::<lince_sandbox::area::InfluenceArea>(owner)
            .unwrap();
        let config = area.protein.as_mut().unwrap();
        config.shader_effects = true;
        config.shader_placement.sphere = true;
        config.shader_placement.elevation = 150.0;
        config.shader_placement.size = 130.0;
        config.motion = None;
        config.draft.query["where"] = serde_json::json!([{"any":ids.iter().map(|uid| serde_json::json!({"uid_eq":uid})).collect::<Vec<_>>()}]);
        world.get_mut::<CanvasView>(root).unwrap().zoom = 0.9;
        world.resource_mut::<Demo>().owner = Some(owner);
        if catalog {
            let alternate = lince_sandbox::relation_castle::spawn(world, root).unwrap();
            let config = world
                .get::<lince_sandbox::area::InfluenceArea>(owner)
                .unwrap()
                .protein
                .clone()
                .unwrap();
            let mut area = world
                .get_mut::<lince_sandbox::area::InfluenceArea>(alternate)
                .unwrap();
            area.protein = Some(config);
            area.protein.as_mut().unwrap().shader_placement.effect_id = Some("amber".into());
            world.resource_mut::<Demo>().alternate_owner = Some(alternate);
        }
        world.resource_mut::<Demo>().since = Instant::now();
        let font = world
            .resource::<lince_sandbox::theme::Typography>()
            .text(22.0);
        world.spawn((
            DemoLabel,
            Text::new("Loading Protein…"),
            font,
            TextColor(Color::WHITE),
            TextBackgroundColor(Color::BLACK),
            Node {
                position_type: PositionType::Absolute,
                left: px(24),
                bottom: px(24),
                padding: UiRect::all(px(12)),
                ..default()
            },
            ChildOf(root),
        ));
        return;
    }
    let owner = world.resource::<Demo>().owner.unwrap();
    let alternate = world.resource::<Demo>().alternate_owner;
    let rows: Vec<_> = world
        .query::<(Entity, &RecordBinding, &CanvasItem)>()
        .iter(world)
        .filter(|(_, binding, _)| binding.area == owner || Some(binding.area) == alternate)
        .map(|(entity, binding, _)| (entity, binding.uid.clone(), Some(binding.area) == alternate))
        .collect();
    if rows.len() != if catalog { 6 } else { 3 } {
        return;
    }
    for (row, uid, alternate) in &rows {
        let index = world
            .resource::<Demo>()
            .ids
            .iter()
            .position(|id| id == uid)
            .unwrap();
        world.get_mut::<CanvasItem>(*row).unwrap().position = DVec2::new(
            (index as f64 - 1.0) * 410.0,
            if catalog {
                let gap = if lighting.is_some() { 260.0 } else { 180.0 };
                if *alternate { gap } else { -gap }
            } else {
                0.0
            },
        );
        let mut spatial = lince_sandbox::topology::spatial(world, *row);
        spatial.depth = Some(12.0);
        world.entity_mut(*row).insert(spatial);
        if let Some(quality) = lighting {
            use lince_sandbox::topology::appearance::{Light, Material};
            use nucleus::canvas::appearance as model;
            if world.get::<Material>(*row).is_none() {
                world.entity_mut(*row).insert(Material(model::Material {
                    lit: true,
                    color: if *alternate {
                        model::Material::default().color
                    } else {
                        [0.18, 0.18, 0.2, 1.0]
                    },
                    roughness: 0.45,
                    emissive: if *alternate { 0.15 } else { 2.0 },
                    texture: match index {
                        0 => model::Texture::Tiles,
                        1 => model::Texture::Grain,
                        _ => model::Texture::Brushed,
                    },
                    repeat: [2.0, 2.0],
                    ..default()
                }));
                if !*alternate && index != 1 {
                    world.entity_mut(*row).insert(Light(model::Light {
                        position: [if index == 0 { -100.0 } else { 100.0 }, 260.0, 120.0],
                        color: [1.0; 3],
                        intensity: 10000.0,
                        range: 1800.0,
                        drive: if index == 0 {
                            model::LightDrive::Pending
                        } else {
                            model::LightDrive::Constant
                        },
                        shadows: quality,
                        ..default()
                    }));
                }
            }
        }
        if lighting.is_some() {
            let material = world
                .get::<lince_sandbox::topology::appearance::Material>(*row)
                .map(|value| value.0.clone());
            let light = world
                .get::<lince_sandbox::topology::appearance::Light>(*row)
                .map(|value| value.0.clone());
            let area = world.get::<RecordBinding>(*row).unwrap().area;
            let mut area = world
                .get_mut::<lince_sandbox::area::InfluenceArea>(area)
                .unwrap();
            let saved = area.records.entry(uid.clone()).or_default();
            saved.material = material;
            saved.light = light;
        }
        assert!(
            world
                .get::<lince_sandbox::record_presentation::RecordPresentation>(*row)
                .is_some_and(|state| state.hide_filled && !state.expanded)
        );
    }
    if mesh
        && world
            .query::<&lince_sandbox::shader_effects::RecordMesh>()
            .iter(world)
            .count()
            != if catalog { 6 } else { 3 }
    {
        return;
    }
    if lighting.is_some()
        && !world.resource::<Demo>().capturing
        && world.resource::<Demo>().since.elapsed() > Duration::from_secs(4)
    {
        let elapsed = world.resource::<Time<bevy::time::Real>>().delta_secs_f64() * 1000.0;
        let mut demo = world.resource_mut::<Demo>();
        if elapsed.is_finite() && elapsed > 0.0 && demo.frame_times.len() < 2048 {
            demo.frame_times.push(elapsed);
        }
    }
    let demo = world.resource::<Demo>();
    if demo.capturing
        || demo.since.elapsed() < Duration::from_secs(6)
        || lighting.is_some() && demo.frame_times.len() < 16
    {
        return;
    }
    if numeric
        && stage == 2
        && !world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0.contains("Protein: 80"))
    {
        return;
    }
    assert_eq!(
        world
            .query::<&lince_sandbox::arrow_sand::ArrowSand>()
            .iter(world)
            .count(),
        if catalog { 4 } else { 2 }
    );
    assert!(
        !world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0.starts_with("WGSL:"))
    );
    let folder = std::env::args()
        .skip(1)
        .find(|argument| {
            !matches!(
                argument.as_str(),
                "--numeric"
                    | "--catalog"
                    | "--mesh"
                    | "--generated"
                    | "--volume"
                    | "--lighting"
                    | "--lighting-low"
                    | "--lighting-medium"
                    | "--lighting-high"
                    | "--lighting-controls"
            )
        })
        .unwrap_or_else(|| "target".into());
    if let Some(quality) = lighting {
        let lights: Vec<_> = world.query::<&PointLight>().iter(world).collect();
        let pending = stage <= if numeric { 2 } else { 1 };
        assert_eq!(lights.len(), if pending { 2 } else { 1 });
        assert!(lights.iter().all(|light| light.shadow_maps_enabled));
        assert_eq!(
            world.resource::<bevy::light::PointLightShadowMap>().size,
            quality.resolution().unwrap() as usize
        );
    }
    std::fs::create_dir_all(&folder).unwrap();
    let prefix = if lighting.is_some() {
        "lit-generated-shaders"
    } else if volume {
        "volume-shaders"
    } else if generated {
        "generated-shaders"
    } else if mesh && catalog && numeric {
        "mesh-catalog-numeric-shaders"
    } else if mesh && catalog {
        "mesh-catalog-shaders"
    } else if mesh && numeric {
        "mesh-numeric-shaders"
    } else if mesh {
        "mesh-shaders"
    } else if catalog && numeric {
        "catalog-numeric-shaders"
    } else if catalog {
        "catalog-shaders"
    } else if numeric {
        "numeric-shaders"
    } else {
        "relation-shaders"
    };
    if mesh {
        let timings: std::collections::BTreeMap<_, _> = world
            .resource::<bevy::diagnostic::DiagnosticsStore>()
            .iter()
            .filter(|diagnostic| {
                diagnostic.path().as_str().ends_with("elapsed_gpu")
                    || diagnostic.path().as_str().ends_with("elapsed_cpu")
            })
            .filter_map(|diagnostic| {
                diagnostic
                    .average()
                    .map(|value| (diagnostic.path().as_str().to_owned(), value))
            })
            .collect();
        std::fs::write(
            format!("{folder}/{prefix}-timings-{stage}.json"),
            serde_json::to_vec_pretty(&timings).unwrap(),
        )
        .unwrap();
        if let Some(quality) = lighting {
            let mut frames = world.resource::<Demo>().frame_times.clone();
            frames.sort_by(f64::total_cmp);
            assert!(
                frames.len() >= 16,
                "Lighting replay needs at least sixteen settled frame samples"
            );
            let lights = world
                .query::<&PointLight>()
                .iter(world)
                .filter(|light| light.shadow_maps_enabled)
                .count();
            let adapter = world
                .get_resource::<bevy::render::renderer::RenderAdapterInfo>()
                .map(|adapter| format!("{} ({:?})", adapter.name, adapter.backend));
            let performance = serde_json::json!({
                "adapter": adapter,
                "stage": stage,
                "shadow_quality": quality,
                "shadow_resolution": quality.resolution(),
                "shadow_lights": lights,
                "record_meshes": if catalog { 6 } else { 3 },
                "present_mode": "AutoNoVsync",
                "frame_samples": frames.len(),
                "frame_p50_ms": frames[(frames.len() - 1) / 2],
                "frame_p95_ms": frames[(frames.len() as f64 * 0.95).ceil() as usize - 1],
                "frame_max_ms": frames.last(),
                "render_diagnostics_ms": timings,
            });
            std::fs::write(
                format!("{folder}/{prefix}-performance-{stage}.json"),
                serde_json::to_vec_pretty(&performance).unwrap(),
            )
            .unwrap();
        }
    }
    let mut samples = Vec::new();
    if stage > 0 {
        let scale_factor = world
            .query::<&Window>()
            .single(world)
            .unwrap()
            .resolution
            .scale_factor();
        let (camera, transform) = world.query_filtered::<(&Camera, &GlobalTransform), With<lince_sandbox::topology::presentation::WorldCamera>>().single(world).map(|(camera, transform)| (camera.clone(), *transform)).unwrap();
        let bodies: Vec<_> = if mesh {
            world
                .query::<(
                    &lince_sandbox::shader_effects::RecordMesh,
                    &GlobalTransform,
                    &Visibility,
                )>()
                .iter(world)
                .map(|(body, position, visibility)| (body.0, *position, *visibility))
                .collect()
        } else {
            world
                .query::<(
                    &lince_sandbox::shader_effects::RecordSphere,
                    &GlobalTransform,
                    &Visibility,
                )>()
                .iter(world)
                .map(|(body, position, visibility)| (body.0, *position, *visibility))
                .collect()
        };
        for (record, position, visibility) in bodies {
            assert_eq!(visibility, Visibility::Visible);
            let binding = world.get::<RecordBinding>(record).unwrap();
            let uid = &binding.uid;
            samples.push((
                camera
                    .world_to_viewport(&transform, position.translation())
                    .unwrap()
                    * scale_factor,
                uid == &world.resource::<Demo>().ids[0],
                Some(binding.area) == alternate,
            ));
        }
        assert_eq!(samples.len(), if catalog { 6 } else { 3 });
    }
    world.resource_mut::<Demo>().capturing = true;
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(format!("{folder}/{prefix}-{stage}.png")))
        .observe(
            move |event: On<ScreenshotCaptured>, mut commands: Commands| {
                let image = event.image.clone().try_into_dynamic().unwrap().to_rgba8();
                let mut first_blue = None;
                for (point, first, alternate) in &samples {
                    let pixel = image.get_pixel(point.x as u32, point.y as u32);
                    if *alternate {
                        assert!(
                            pixel[0] > pixel[2].saturating_add(30),
                            "The independent amber catalog effect did not render: {pixel:?}"
                        );
                    } else if (stage == 1 || numeric && stage == 2) && *first {
                        first_blue = Some(pixel[2]);
                        assert!(
                            pixel[2] > pixel[0].saturating_add(30),
                            "3D energy sphere did not render: {pixel:?}"
                        );
                    } else {
                        assert!(
                            pixel[0].abs_diff(pixel[2]) < 10 && pixel[0] > 40,
                            "3D calm sphere did not render: {pixel:?}"
                        );
                    }
                }
                let folder = folder.clone();
                commands.queue(move |world: &mut World| {
                    if numeric && stage == 1 {
                        world.resource_mut::<Demo>().reference_blue = first_blue;
                    }
                    if numeric && stage == 2 {
                        assert!(
                            first_blue.unwrap()
                                > world
                                    .resource::<Demo>()
                                    .reference_blue
                                    .unwrap()
                                    .saturating_add(15),
                            "The numeric extension update did not brighten the 3D sphere: before={:?}, after={first_blue:?}",
                            world.resource::<Demo>().reference_blue
                        );
                    }
                    let root = world
                        .query_filtered::<Entity, With<BoxRoot>>()
                        .single(world)
                        .unwrap();
                    if stage == 0 {
                        world
                            .entity_mut(root)
                            .insert(lince_sandbox::topology::view::View {
                                spatial: true,
                                position: if lighting.is_some() { [0.0, 1000.0, 1000.0] } else { [0.0, 650.0, 900.0] },
                                pitch: if lighting.is_some() { -std::f32::consts::FRAC_PI_4 } else { -0.6 },
                                ..default()
                            });
                    } else if numeric && stage == 1 {
                        let uid = world.resource::<Demo>().ids[0].clone();
                        world
                            .get_non_send::<lince_sandbox::cell_bridge::CellBridge>()
                            .unwrap()
                            .outgoing
                            .try_send(cell::ClientMessage::Act {
                                id: "relation-shader-numeric".into(),
                                action: engine::actions::Action::SetExtension {
                                    target: uid,
                                    namespace: "nutrition".into(),
                                    fds: serde_json::json!({"daily":{"protein":80}}),
                                },
                            })
                            .unwrap();
                    } else if stage == 1 || numeric && stage == 2 {
                        let uid = world.resource::<Demo>().ids[0].clone();
                        world
                            .get_non_send::<lince_sandbox::cell_bridge::CellBridge>()
                            .unwrap()
                            .outgoing
                            .try_send(cell::ClientMessage::Act {
                                id: "relation-shader-complete".into(),
                                action: engine::actions::Action::SetQuantity {
                                    target: uid,
                                    value: 0.0,
                                },
                            })
                            .unwrap();
                    } else if world.resource::<Demo>().review_controls {
                        let owner = world.resource::<Demo>().owner.unwrap();
                        let uid = world.resource::<Demo>().ids[0].clone();
                        let row = world.query_filtered::<(Entity, &RecordBinding), With<CanvasItem>>().iter(world).find(|(_, binding)| binding.area == owner && binding.uid == uid).unwrap().0;
                        control_review::start(world, root, row, folder);
                    } else {
                        world.write_message(AppExit::Success);
                    }
                    let mut demo = world.resource_mut::<Demo>();
                    demo.stage += 1;
                    demo.capturing = false;
                    demo.since = Instant::now();
                    demo.frame_times.clear();
                });
            },
        );
}

fn programs(numeric: bool, mesh: bool, generated: bool, volume: bool, lighting: bool) -> Effect {
    let mut effect = Effect::default();
    effect.inputs = nucleus::shader_effect::InputMappings {
        task: Some(nucleus::shader_effect::QuantityRange {
            start: "-3".into(),
            end: "0".into(),
        }),
        progress: Some(nucleus::shader_effect::QuantityRange {
            start: "-3".into(),
            end: "0".into(),
        }),
    };
    effect.programs.insert("negative".into(), "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { let time = inputs.time * f32(inputs.motion_allowed); let bands = 0.5 + 0.5 * sin(uv.x * 32.0 + uv.y * 17.0 + time * 2.0 + abs(inputs.quantity)); return vec4<f32>(0.05 + bands * 0.12, 0.3 + bands * 0.35, 0.65 + bands * 0.35, 1.0); }".into());
    effect.programs.insert("nonnegative".into(), "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { let bands = 0.45 + 0.12 * cos(uv.y * 20.0); return vec4<f32>(vec3<f32>(bands), 1.0); }".into());
    if numeric {
        use nucleus::shader_effect::bindings::{NumericBinding, NumericField, NumericSource};
        effect.bindings = vec![NumericBinding {
            slot: 0,
            label: "Protein".into(),
            source: NumericSource {
                field: NumericField::Extension {
                    namespace: "nutrition".into(),
                    path: vec!["daily".into(), "protein".into()],
                },
                ..default()
            },
        }];
        effect.programs.insert("negative".into(), "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { let level = clamp(inputs.numeric0.x / 100.0, 0.0, 1.0) * f32(inputs.present0.x); let bands = 0.8 + 0.2 * sin(uv.x * 32.0 + uv.y * 17.0 + inputs.time * 2.0); return vec4<f32>(vec3<f32>(0.05, 0.3, 0.9) * (0.3 + 0.7 * level) * bands, 1.0); }".into());
    }
    if mesh {
        use nucleus::shader_effect::spatial::{Deformation, MeshSettings, Mode};
        effect.mode = Mode::Mesh(MeshSettings { deformation: Some(Deformation {
            source: "fn deform_vertex(vertex: MeshVertex, inputs: ShaderInputs) -> MeshVertex { let task_pending = f32(inputs.quantity_present != 0u && inputs.quantity < 0.0); let amount = clamp(inputs.quantity * f32(inputs.quantity_present) * 0.02, -0.12, 0.12); let wave = 0.03 * sin(vertex.position.y * 12.0 + inputs.time * 2.0) * task_pending * f32(inputs.motion_allowed); return MeshVertex(vertex.position + vertex.normal * (amount + wave), vertex.normal, vertex.uv); }".into(), max_displacement: 0.25,
        }), ..default() });
        if generated && let Mode::Mesh(settings) = &mut effect.mode {
            settings.geometry = Some(nucleus::shader_effect::spatial::GeneratedGeometry {
                source: nucleus::shader_effect::spatial::GENERATED_SPHERE.into(),
                vertices: 3840,
            });
        }
        let level = if numeric {
            "0.3 + 0.7 * clamp(inputs.numeric0.x / 100.0, 0.0, 1.0) * f32(inputs.present0.x)"
        } else {
            "1.0"
        };
        let bands = if numeric {
            "1.0"
        } else {
            "0.8 + 0.2 * sin(point.local_position.x * 32.0 + point.local_position.y * 17.0 + inputs.time * 2.0)"
        };
        let illumination = if numeric { "0.8 + 0.2" } else { "0.45 + 0.55" };
        effect.programs.insert("negative".into(), format!("fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> vec4<f32> {{ let bands = {bands}; let light = {illumination} * max(dot(normalize(point.normal), normalize(vec3<f32>(-0.5, 1.0, 0.7))), 0.0); return vec4<f32>(vec3<f32>(0.05, 0.3, 0.9) * ({level}) * bands * light, 1.0); }}"));
        effect.programs.insert("nonnegative".into(), "fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> vec4<f32> { let light = 0.35 + 0.65 * max(dot(normalize(point.normal), normalize(vec3<f32>(-0.5, 1.0, 0.7))), 0.0); return vec4<f32>(vec3<f32>(0.5 * light), 1.0); }".into());
        if lighting {
            effect.programs.insert("negative".into(), format!("fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> vec4<f32> {{ return vec4<f32>(vec3<f32>(0.05, 0.3, 0.9) * ({level}), 1.0); }}"));
            effect.programs.insert("nonnegative".into(), "fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(vec3<f32>(0.1), 1.0); }".into());
        }
    }
    if volume {
        effect.mode = nucleus::shader_effect::spatial::Mode::Volume(
            nucleus::shader_effect::spatial::VolumeSettings {
                density: 8.0,
                ..default()
            },
        );
        let level = if numeric {
            "0.3 + 0.7 * clamp(inputs.numeric0.x / 100.0, 0.0, 1.0) * f32(inputs.present0.x)"
        } else {
            "1.0"
        };
        effect.programs.insert("negative".into(), format!("fn volume_sample(position: vec3<f32>, inputs: ShaderInputs) -> vec4<f32> {{ let radius = 0.33 + 0.02 * sin(inputs.time * 2.0) * f32(inputs.motion_allowed); return vec4<f32>(vec3<f32>(0.05, 0.3, 0.9) * ({level}), f32(length(position) < radius)); }}"));
        effect.programs.insert("nonnegative".into(), "fn volume_sample(position: vec3<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(vec3<f32>(0.45), f32(length(position) < 0.4)); }".into());
        let neutral = effect.programs["nonnegative"].clone();
        effect.programs.insert("neutral".into(), neutral);
    }
    effect.validate().unwrap();
    effect
}

fn main() -> AppExit {
    let review_controls = std::env::args().any(|argument| argument == "--lighting-controls");
    let numeric = review_controls || std::env::args().any(|argument| argument == "--numeric");
    let catalog = std::env::args().any(|argument| argument == "--catalog");
    let volume = std::env::args().any(|argument| argument == "--volume");
    let lighting = std::env::args().find_map(|argument| match argument.as_str() {
        "--lighting-low" => Some(nucleus::canvas::appearance::ShadowQuality::Low),
        "--lighting" | "--lighting-medium" | "--lighting-controls" => {
            Some(nucleus::canvas::appearance::ShadowQuality::Medium)
        }
        "--lighting-high" => Some(nucleus::canvas::appearance::ShadowQuality::High),
        _ => None,
    });
    assert!(
        !volume || lighting.is_none(),
        "The lighting replay uses generated mesh spheres; omit --volume"
    );
    let generated = !volume
        && (lighting.is_some() || std::env::args().any(|argument| argument == "--generated"));
    let mesh = volume || generated || std::env::args().any(|argument| argument == "--mesh");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let _guard = runtime.enter();
    let (engine, ids) = runtime.block_on(async {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let organ = store::organs::ensure_local(&engine.store.pool, "http://relation-shader-demo").await.unwrap().uid;
        engine.set_signer(engine::trust::Signer::generate(&organ, "key")).await.unwrap();
        engine.act(engine::actions::Action::CreateConcept { lingua: "g_local".into(), name: "depends-on".into(), parents: vec![] }, None).await.unwrap();
        let mut ids = Vec::new();
        for (head, quantity) in [("Clean room", -3.0), ("Study WGSL", 0.0), ("Water plants", 5.0)] {
            let uid = engine.act(engine::actions::Action::CreateRecord { slug: None, kind: nucleus::RecordKind::Plain, head: head.into(), body: "Protein gathers this Record with its quantity, links and WGSL extension.".into(), quantity }, None).await.unwrap().created.unwrap();
            if numeric {
                engine.act(engine::actions::Action::SetExtension { target: uid.clone(), namespace: "nutrition".into(), fds: serde_json::json!({"daily":{"protein":20}}) }, None).await.unwrap();
            }
            let mut extension = Extension::single(1, programs(numeric, mesh, generated, volume, lighting.is_some()));
            if catalog {
                let mut amber = Effect::default();
                amber.programs = [("amber".into(), "fn shade(uv: vec2<f32>, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(0.95, 0.3, 0.04, 1.0); }".into())].into();
                amber.rules.clear();
                amber.fallback = "amber".into();
                if mesh {
                    amber.mode = nucleus::shader_effect::spatial::Mode::Mesh(default());
                    amber.programs.insert("amber".into(), "fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> vec4<f32> { let light = 0.35 + 0.65 * max(dot(normalize(point.normal), normalize(vec3<f32>(-0.5, 1.0, 0.7))), 0.0); return vec4<f32>(vec3<f32>(0.95, 0.3, 0.04) * light, 1.0); }".into());
                    if lighting.is_some() {
                        amber.programs.insert("amber".into(), "fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> vec4<f32> { return vec4<f32>(0.95, 0.3, 0.04, 1.0); }".into());
                    }
                }
                extension.effects.insert("amber".into(), nucleus::shader_effect::NamedEffect { label: "Amber".into(), effect: amber });
            }
            engine.act(engine::actions::Action::SetExtension { target: uid.clone(), namespace: NAMESPACE.into(), fds: serde_json::to_value(extension).unwrap() }, None).await.unwrap();
            ids.push(uid);
        }
        for pair in ids.windows(2) {
            engine.act(engine::actions::Action::AssertRecord { subject: pair[0].clone(), predicate: "depends-on".into(), object: Some(pair[1].clone()), quantity: None, unit: None }, None).await.unwrap();
        }
        (engine, ids)
    });
    lince_sandbox::app::interface_app()
        .add_plugins(control_review::ReviewPlugin)
        .add_plugins(bevy::log::LogPlugin::default())
        .add_plugins(bevy::render::diagnostic::RenderDiagnosticsPlugin)
        .insert_resource(lince_sandbox::app::CellHandle(cell::CellRuntime {
            commands: default(),
            store: engine.store.clone(),
            engine,
            lanes: Arc::new(cell::LaneHub::new()),
            wire: default(),
            fiote: None,
            speech: None,
            information: None,
        }))
        .add_plugins(lince_sandbox::cell_bridge::CellBridgePlugin)
        .insert_resource(WinitSettings::continuous())
        .insert_resource(Demo {
            ids,
            owner: None,
            alternate_owner: None,
            since: Instant::now(),
            stage: 0,
            capturing: false,
            numeric,
            catalog,
            mesh,
            generated,
            volume,
            lighting,
            review_controls,
            frame_times: vec![],
            reference_blue: None,
        })
        .add_systems(
            Startup,
            |mut commands: Commands,
             mut windows: Query<&mut Window>,
             adapter: Option<Res<bevy::render::renderer::RenderAdapterInfo>>| {
                if let Some(adapter) = adapter {
                    eprintln!(
                        "Shader demo adapter: {} ({:?})",
                        adapter.name, adapter.backend
                    );
                }
                let mut window = windows.single_mut().unwrap();
                window.title = "Relation Castle · WGSL · 2D → 3D sphere → quantity update".into();
                window.resolution.set(1400.0, 850.0);
                if std::env::args().any(|argument| argument.starts_with("--lighting")) {
                    window.present_mode = bevy::window::PresentMode::AutoNoVsync;
                }
                commands.spawn((BoxRoot, CanvasView::default()));
            },
        )
        .add_systems(
            Update,
            exercise.after(lince_sandbox::physics::SimulateWorkspaces),
        )
        .run()
}
