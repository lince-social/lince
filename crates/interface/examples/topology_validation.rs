use avian3d::{
    character_controller::move_and_slide::{
        MoveAndSlide, MoveAndSlideConfig, MoveAndSlideHitResponse,
    },
    prelude::*,
};
use bevy::{
    diagnostic::FrameCount,
    math::{DQuat, DVec2, DVec3},
    prelude::*,
    winit::WinitSettings,
};
use lince_interface::{
    actions::Action,
    app::interface_app,
    container::BoxRoot,
    topology::{
        self, Spatial,
        assets::{self, ImportedAsset, Ready},
        physics::Body,
    },
};
use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Resource)]
struct Trial {
    asset: Entity,
    bad: Entity,
    barrier: Option<Entity>,
    ready_at: Option<Duration>,
    copies: Vec<Entity>,
    source: tempfile::TempDir,
    started: Instant,
    last: Instant,
    samples: [Vec<f64>; 4],
    cpu_samples: [Vec<f64>; 4],
    frame_started: Instant,
    idle: Option<(u32, usize, Instant)>,
    done: Arc<AtomicBool>,
    wakes: Arc<AtomicUsize>,
}

fn fixture(directory: &std::path::Path, lines: bool) -> std::path::PathBuf {
    let mut vertices = Vec::<[f32; 3]>::new();
    let mut indices = Vec::<u32>::new();
    for (left, right) in [(-100.0, -20.0), (20.0, 100.0)] {
        let base = vertices.len() as u32;
        for z in [-5.0, 5.0] {
            for y in [-100.0, 100.0] {
                for x in [left, right] {
                    vertices.push([x, y, z]);
                }
            }
        }
        for triangle in [
            [0, 2, 1],
            [1, 2, 3],
            [4, 5, 6],
            [5, 7, 6],
            [0, 1, 4],
            [1, 5, 4],
            [2, 6, 3],
            [3, 6, 7],
            [0, 4, 2],
            [2, 4, 6],
            [1, 3, 5],
            [3, 7, 5],
        ] {
            indices.extend(triangle.map(|index| base + index));
        }
    }
    let subdivisions: u32 =
        std::env::var("LINCE_TOPOLOGY_SUBDIVISIONS").map_or(1, |value| value.parse().unwrap());
    assert!((1..=128).contains(&subdivisions));
    if subdivisions > 1 {
        let mut refined = Vec::new();
        for triangle in indices.chunks_exact(3) {
            let a = Vec3::from_array(vertices[triangle[0] as usize]);
            let ab = (Vec3::from_array(vertices[triangle[1] as usize]) - a) / subdivisions as f32;
            let ac = (Vec3::from_array(vertices[triangle[2] as usize]) - a) / subdivisions as f32;
            for i in 0..subdivisions {
                for j in 0..subdivisions - i {
                    let p = a + ab * i as f32 + ac * j as f32;
                    refined.extend([p.to_array(), (p + ab).to_array(), (p + ac).to_array()]);
                    if i + j + 1 < subdivisions {
                        refined.extend([
                            (p + ab).to_array(),
                            (p + ab + ac).to_array(),
                            (p + ac).to_array(),
                        ]);
                    }
                }
            }
        }
        vertices = refined;
        indices = (0..vertices.len() as u32).collect();
    }
    let texture_size: u32 =
        std::env::var("LINCE_TOPOLOGY_TEXTURE_SIZE").map_or(2, |value| value.parse().unwrap());
    assert!((1..=4096).contains(&texture_size));
    println!(
        "Fixture: {} triangles, {texture_size}×{texture_size} texture",
        indices.len() / 3
    );
    let mut bytes = Vec::new();
    for vertex in &vertices {
        for value in vertex {
            bytes.extend(value.to_le_bytes());
        }
    }
    let vertex_bytes = bytes.len();
    for index in &indices {
        bytes.extend(index.to_le_bytes());
    }
    let index_bytes = indices.len() * 4;
    for _ in &vertices {
        bytes.extend(0.5_f32.to_le_bytes());
        bytes.extend(0.5_f32.to_le_bytes());
    }
    image::RgbaImage::from_pixel(
        texture_size,
        texture_size,
        image::Rgba([100, 180, 200, 255]),
    )
    .save(directory.join("color.png"))
    .unwrap();
    let document = serde_json::json!({
        "asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": [0]}], "nodes": [{"mesh": 0}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "TEXCOORD_0": 2}, "indices": 1, "material": 0, "mode": if lines {1} else {4}}]}],
        "images": [{"uri": "color.png"}], "textures": [{"source": 0}], "materials": [{"pbrMetallicRoughness": {"baseColorTexture": {"index": 0}}}],
        "buffers": [{"uri": "opening.bin", "byteLength": bytes.len()}],
        "bufferViews": [{"buffer": 0, "byteLength": vertex_bytes}, {"buffer": 0, "byteOffset": vertex_bytes, "byteLength": index_bytes}, {"buffer": 0, "byteOffset": vertex_bytes + index_bytes, "byteLength": vertices.len() * 8}],
        "accessors": [{"bufferView": 0, "componentType": 5126, "count": vertices.len(), "type": "VEC3", "min": [-100,-100,-5], "max": [100,100,5]}, {"bufferView": 1, "componentType": 5125, "count": indices.len(), "type": "SCALAR"}, {"bufferView": 2, "componentType": 5126, "count": vertices.len(), "type": "VEC2"}]
    });
    std::fs::write(directory.join("opening.bin"), bytes).unwrap();
    let path = directory.join(if lines {
        "unsupported.gltf"
    } else {
        "opening.gltf"
    });
    std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    path
}

fn sweep(In((start, delta)): In<(DVec3, DVec3)>, movement: MoveAndSlide) -> DVec3 {
    movement
        .move_and_slide(
            &Collider::sphere(8.0),
            start,
            DQuat::IDENTITY,
            delta,
            Duration::from_secs(1),
            &MoveAndSlideConfig::default(),
            &SpatialQueryFilter::default(),
            |_| MoveAndSlideHitResponse::Accept,
        )
        .position
}

fn mesh_pick(In(ray): In<Ray3d>, mut picking: MeshRayCast) -> Option<f32> {
    picking
        .cast_ray(
            ray,
            &MeshRayCastSettings::default()
                .with_visibility(bevy::picking::mesh_picking::ray_cast::RayCastVisibility::Any),
        )
        .first()
        .map(|(_, hit)| hit.distance)
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    let root = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(world)
        .unwrap();
    let ready = world
        .get_resource::<Trial>()
        .is_some_and(|trial| world.get::<Ready>(trial.asset).is_some());
    if let Some(mut trial) = world.get_resource_mut::<Trial>() {
        if ready && trial.ready_at.is_none() {
            trial.ready_at = Some(trial.started.elapsed());
        }
        let elapsed = trial.last.elapsed().as_secs_f64() * 1000.0;
        trial.last = Instant::now();
        let sample = match frame {
            70..=99 => Some(0),
            150..=199 => Some(1),
            250..=299 => Some(2),
            350..=399 => Some(3),
            _ => None,
        };
        if let Some(sample) = sample {
            trial.samples[sample].push(elapsed);
        }
        if trial.done.load(Ordering::Acquire) {
            let (start_frame, start_wakes, start) = trial.idle.unwrap();
            println!(
                "Idle: {} frames, {} application wake requests in {:.2}s",
                frame - start_frame,
                trial.wakes.load(Ordering::Relaxed) - start_wakes,
                start.elapsed().as_secs_f64()
            );
            println!("Topology validation passed");
            world.write_message(AppExit::Success);
            return;
        }
    }
    match frame {
        12 => {
            let source = tempfile::tempdir().unwrap();
            let directory = world.resource::<assets::AssetDirectory>().0.clone();
            let path = fixture(source.path(), false);
            let started = Instant::now();
            let mut asset = assets::copy_import(&path, &directory).unwrap();
            println!(
                "Package copy: {:.3}ms",
                started.elapsed().as_secs_f64() * 1000.0
            );
            asset.scale = 1.0;
            let asset = assets::spawn(world, root, 1, DVec2::ZERO, asset);
            world.get_mut::<Spatial>(asset).unwrap().elevation = 1000.0;
            let bad = assets::copy_import(&fixture(source.path(), true), &directory).unwrap();
            let bad = assets::spawn(world, root, 1, DVec2::new(300.0, 0.0), bad);
            let original_wake = world
                .resource::<lince_interface::wake::WakeSignal>()
                .clone();
            let wakes = Arc::new(AtomicUsize::new(0));
            let counter = wakes.clone();
            world.insert_resource(lince_interface::wake::WakeSignal::new(move || {
                counter.fetch_add(1, Ordering::Relaxed);
                original_wake.ring();
            }));
            world.insert_resource(Trial {
                asset,
                bad,
                barrier: None,
                ready_at: None,
                copies: Vec::new(),
                source,
                started,
                last: Instant::now(),
                samples: std::array::from_fn(|_| Vec::new()),
                cpu_samples: std::array::from_fn(|_| Vec::new()),
                frame_started: Instant::now(),
                idle: None,
                done: Arc::new(AtomicBool::new(false)),
                wakes,
            });
        }
        60 => {
            let (asset, bad) = {
                let trial = world.resource::<Trial>();
                (trial.asset, trial.bad)
            };
            assert!(world.get::<Ready>(asset).is_some());
            assert!(
                world.get_entity(bad).is_err(),
                "Unsupported collision must terminate import"
            );
            println!(
                "Load + collider ready observed by {:.2}ms",
                world.resource::<Trial>().ready_at.unwrap().as_secs_f64() * 1000.0
            );
            let open = world
                .run_system_cached_with(
                    sweep,
                    (DVec3::new(0.0, 1000.0, 30.0), DVec3::new(0.0, 0.0, -60.0)),
                )
                .unwrap();
            let blocked = world
                .run_system_cached_with(
                    sweep,
                    (DVec3::new(60.0, 1000.0, 30.0), DVec3::new(0.0, 0.0, -60.0)),
                )
                .unwrap();
            assert!(
                open.z < -29.0 && blocked.z > 12.0,
                "Open {open:?}, blocked {blocked:?}"
            );
            let path = world.resource::<Trial>().source.path().join("opening.gltf");
            assets::import(world, root, path).unwrap();
            topology::ui::TopologyAction::CancelImports.apply(world, root);
        }
        62 => {
            let original = world.resource::<Trial>().asset;
            let loading = assets::duplicate(world, original).unwrap();
            topology::ui::TopologyAction::CancelImports.apply(world, root);
            assert!(world.get_entity(loading).is_err());
            assert!(world.get::<Ready>(original).is_some());
            let asset = world.get::<ImportedAsset>(original).unwrap();
            assert!(
                world
                    .resource::<assets::AssetDirectory>()
                    .0
                    .join(&asset.id)
                    .join(&asset.file)
                    .exists()
            );
        }
        100 => {
            println!(
                "Process memory before copies: {}",
                std::fs::read_to_string("/proc/self/status")
                    .unwrap()
                    .lines()
                    .find(|line| line.starts_with("VmRSS:"))
                    .unwrap()
            );
            assert_eq!(
                world.query::<&ImportedAsset>().iter(world).count(),
                1,
                "Cancelled import must not appear"
            );
            let asset = world.resource::<Trial>().asset;
            let copies: Vec<_> = (0..32)
                .map(|i| {
                    let copy = assets::duplicate(world, asset).unwrap();
                    topology::set_position(
                        world,
                        copy,
                        DVec3::new((i % 8) as f64 * 240.0 - 960.0, 0.0, (i / 8) as f64 * 240.0),
                    );
                    copy
                })
                .collect();
            world.resource_mut::<Trial>().copies = copies;
        }
        140 => {
            let start = Instant::now();
            for _ in 0..100 {
                let hit = world
                    .run_system_cached_with(
                        mesh_pick,
                        Ray3d::new(Vec3::new(60.0, 1000.0, 30.0), Dir3::NEG_Z),
                    )
                    .unwrap();
                assert!(hit.is_some_and(|distance| (distance - 25.0).abs() < 0.1));
            }
            println!(
                "100 render-mesh picks: {:.3}ms",
                start.elapsed().as_secs_f64() * 1000.0
            );
            let asset = world.resource::<Trial>().asset;
            let shape = world.get::<topology::physics::MeshCollider>(asset).unwrap();
            let start = Instant::now();
            for _ in 0..100 {
                assert_eq!(
                    shape.ray_distance(DVec3::new(60.0, 0.0, 30.0), -DVec3::Z),
                    Some(25.0)
                );
                assert_eq!(
                    shape.ray_distance(DVec3::new(0.0, 0.0, 30.0), -DVec3::Z),
                    None
                );
            }
            println!(
                "100 indexed mesh picks plus opening checks: {:.3}ms",
                start.elapsed().as_secs_f64() * 1000.0
            );
            let shapes: HashSet<_> = world
                .query::<&Collider>()
                .iter(world)
                .map(|shape| Arc::as_ptr(&shape.shape().0) as *const () as usize)
                .collect();
            assert_eq!(
                shapes.len(),
                1,
                "Loaded copies must share collision geometry"
            );
            let copies = world.resource::<Trial>().copies.clone();
            let mut handles = HashSet::new();
            let mut textures = HashSet::new();
            for entity in copies {
                assert!(world.get::<Ready>(entity).is_some());
                for (part, mesh, _) in assets::mesh_parts(world, entity).unwrap() {
                    let material = world.get::<MeshMaterial3d<StandardMaterial>>(part).unwrap();
                    let texture = world
                        .resource::<Assets<StandardMaterial>>()
                        .get(&material.0)
                        .unwrap()
                        .base_color_texture
                        .as_ref()
                        .unwrap();
                    textures.insert(texture.id());
                    handles.insert(mesh.id());
                }
            }
            assert_eq!(handles.len(), 1, "Copies must share loaded mesh data");
            assert_eq!(textures.len(), 1, "Copies must share loaded textures");
            println!(
                "Shared texture payload: {} bytes",
                world
                    .resource::<Assets<Image>>()
                    .get(*textures.iter().next().unwrap())
                    .unwrap()
                    .data
                    .as_ref()
                    .unwrap()
                    .len()
            );
            println!(
                "Process memory after copies: {}",
                std::fs::read_to_string("/proc/self/status")
                    .unwrap()
                    .lines()
                    .find(|line| line.starts_with("VmRSS:"))
                    .unwrap()
            );
            let mesh_bytes: usize = handles
                .iter()
                .map(|id| {
                    let mesh = world.resource::<Assets<Mesh>>().get(*id).unwrap();
                    mesh.get_vertex_buffer_size()
                        + mesh.get_index_buffer_bytes().map_or(0, |bytes| bytes.len())
                })
                .sum();
            println!(
                "32 copies: {} unique mesh, {} mesh buffer bytes",
                handles.len(),
                mesh_bytes
            );
            let asset = world.resource::<Trial>().asset;
            world
                .entity_mut(asset)
                .insert(lince_interface::area_effects::AreaScale(2.0));
            assets::update(world);
            assert_eq!(
                world.get::<Transform>(asset).unwrap().scale,
                Vec3::splat(2.0)
            );
            topology::physics::synchronize(world);
            let body = world
                .query::<(Entity, &Body)>()
                .iter(world)
                .find(|(_, body)| body.members.contains(&asset))
                .unwrap()
                .0;
            let collider = world
                .query::<(&Collider, &ColliderOf)>()
                .iter(world)
                .find(|(_, owner)| owner.body == body)
                .unwrap()
                .0;
            assert_eq!(
                collider.shape_scaled().compute_local_aabb().extents().x,
                400.0
            );
            world
                .entity_mut(asset)
                .remove::<lince_interface::area_effects::AreaScale>();
        }
        210 => {
            for index in 0..32 {
                let sand = lince_interface::sand_store::spawn_sand(
                    world,
                    root,
                    1,
                    lince_interface::sand_store::SandKind::Square,
                    "",
                    DVec2::new(
                        (index % 8) as f64 * 220.0 - 900.0,
                        (index / 8) as f64 * 220.0,
                    ),
                );
                world
                    .get_mut::<lince_interface::canvas::CanvasItem>(sand)
                    .unwrap()
                    .size = Vec2::splat(128.0);
                world.entity_mut(sand).insert(Spatial {
                    world_pinned: true,
                    ..default()
                });
            }
        }
        310 => {
            let bodies: HashSet<_> = world
                .query::<&topology::presentation::Surface>()
                .iter(world)
                .map(|surface| world.get::<Mesh3d>(surface.body).unwrap().0.id())
                .collect();
            assert_eq!(bodies.len(), 1, "Sand bodies must share their render mesh");
            let pixels: u64 = world
                .query::<&topology::presentation::Surface>()
                .iter(world)
                .map(|s| u64::from(s.pixels.x) * u64::from(s.pixels.y))
                .sum();
            println!(
                "32 Sand content textures: {} bytes (RGBA, excluding depth/MSAA)",
                pixels * 4
            );
            let asset = world.resource::<Trial>().asset;
            world
                .entity_mut(asset)
                .insert(lince_interface::area::RecordProperties(
                    serde_json::json!({"quantity": 999}),
                ));
            let barrier = lince_interface::sand_store::spawn_sand(
                world,
                root,
                1,
                lince_interface::sand_store::SandKind::Square,
                "",
                DVec2::new(0.0, -250.0),
            );
            world
                .get_mut::<lince_interface::canvas::CanvasItem>(barrier)
                .unwrap()
                .size = Vec2::new(250.0, 20.0);
            world.entity_mut(barrier).insert(Spatial {
                elevation: 1100.0,
                depth: Some(200.0),
                world_pinned: true,
                ..default()
            });
            world.resource_mut::<Trial>().barrier = Some(barrier);
            let mut area = lince_interface::area::InfluenceArea::new(
                lince_interface::area::AreaShape::Square,
                DVec2::new(0.0, -500.0),
                DVec2::splat(1000.0),
            );
            area.strength = 2000.0;
            area.reach.mode = lince_interface::area::ReachMode::Unlimited;
            area.rules.push(lince_interface::area::PropertyRule {
                property: lince_interface::area::Property::Quantity,
                value: "999".into(),
            });
            let area = lince_interface::area::spawn_area(world, root, 1, area).unwrap();
            world.get_mut::<Spatial>(area).unwrap().elevation = 1000.0;
            lince_interface::edit_mode::EditAction::Open.apply(world, root);
            topology::ui::TopologyAction::Pin.apply(world, asset);
            assert!(!world.get::<Spatial>(asset).unwrap().world_pinned);
            lince_interface::edit_mode::EditAction::Close.apply(world, root);
        }
        450 => {
            let asset = world.resource::<Trial>().asset;
            let position = topology::position(world, asset).unwrap();
            let barrier = world
                .get::<lince_interface::canvas::CanvasItem>(
                    world.resource::<Trial>().barrier.unwrap(),
                )
                .unwrap();
            let contact = barrier.position.y + f64::from(barrier.size.y) * 0.5 + 5.0;
            assert!(
                position.z < -20.0 && (position.z - contact).abs() < 3.0,
                "Moving triangle body must stop at obstacle: {position:?}"
            );
            assert!((position.y - 1000.0).abs() < 1.0 && position.x.abs() < 1.0);
            world.get_mut::<Spatial>(asset).unwrap().world_pinned = true;
            let mut trial = world.resource_mut::<Trial>();
            for (label, samples) in [
                "1 static import",
                "33 static imports",
                "33 imports + 32 Sand textures",
                "moving triangle collision + 32 Sand textures",
            ]
            .into_iter()
            .zip(trial.samples.iter_mut())
            {
                samples.sort_by(f64::total_cmp);
                println!(
                    "{label} frame interval: median {:.2}ms, p95 {:.2}ms",
                    samples[samples.len() / 2],
                    samples[samples.len() * 95 / 100]
                );
            }
            for (index, samples) in trial.cpu_samples.iter_mut().enumerate() {
                samples.sort_by(f64::total_cmp);
                println!(
                    "Scenario {} main schedule elapsed: median {:.2}ms, p95 {:.2}ms",
                    index + 1,
                    samples[samples.len() / 2],
                    samples[samples.len() * 95 / 100]
                );
            }
            let trees = world.resource::<avian3d::collider_tree::ColliderTrees>();
            for tree in trees.iter_trees() {
                for (_, proxy) in tree.proxies.iter() {
                    assert!(
                        world.get::<Collider>(proxy.collider).is_some(),
                        "Removed collider remains in collision tree"
                    );
                }
            }
            let stats = world.resource::<topology::physics::PreparationMeasurements>();
            assert_eq!(
                stats.calls, 2,
                "Copies and restoring a shared scale must reuse prepared geometry"
            );
            println!(
                "Triangle preparation: {} calls, {:.3}ms total",
                stats.calls,
                stats.elapsed.as_secs_f64() * 1000.0
            );
        }
        460 => {
            let barrier = world.resource::<Trial>().barrier.unwrap();
            world.despawn(barrier);
        }
        470 => {
            let asset = world.resource::<Trial>().asset;
            let position = topology::position(world, asset).unwrap();
            let open = world
                .run_system_cached_with(sweep, (position + DVec3::Z * 30.0, -DVec3::Z * 60.0))
                .unwrap();
            assert!(
                open.z < position.z - 29.0,
                "Moved mesh retains its opening: {open:?}"
            );
        }
        480 => {
            assert!(!topology::physics::awake(world));
            assert!(world.query::<&Body>().iter(world).all(|body| body.held));
            world.insert_resource(lince_interface::theme::idle_settings());
            let wake = world
                .resource::<lince_interface::wake::WakeSignal>()
                .clone();
            let mut trial = world.resource_mut::<Trial>();
            trial.idle = Some((frame, trial.wakes.load(Ordering::Relaxed), Instant::now()));
            let done = trial.done.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(2));
                done.store(true, Ordering::Release);
                wake.ring();
            });
        }
        1000 => panic!("Validation timed out"),
        _ => {}
    }
}

fn begin_frame(trial: Option<ResMut<Trial>>) {
    if let Some(mut trial) = trial {
        trial.frame_started = Instant::now();
    }
}

fn end_frame(trial: Option<ResMut<Trial>>, frame: Res<FrameCount>) {
    if let Some(mut trial) = trial {
        let sample = match frame.0 {
            70..=99 => Some(0),
            150..=199 => Some(1),
            250..=299 => Some(2),
            350..=399 => Some(3),
            _ => None,
        };
        if let Some(sample) = sample {
            let elapsed = trial.frame_started.elapsed().as_secs_f64() * 1000.0;
            trial.cpu_samples[sample].push(elapsed);
        }
    }
}

fn main() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _guard = runtime.enter();
    interface_app()
        .add_plugins(bevy::log::LogPlugin::default())
        .insert_resource(WinitSettings::continuous())
        .init_resource::<topology::physics::PreparationMeasurements>()
        .add_systems(First, begin_frame)
        .add_systems(Last, end_frame)
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(BoxRoot);
        })
        .add_systems(
            Update,
            exercise.after(lince_interface::physics::SimulateWorkspaces),
        )
        .run();
}
