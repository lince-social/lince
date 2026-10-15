use super::*;
use lince_sandbox::{
    area::{InfluenceArea, RecordProperties},
    canvas_host::Identity,
    protein_area::RecordBinding,
};
use nucleus::shader_effect::{
    Effect, Extension, NAMESPACE, NamedEffect,
    bindings::{NumericBinding, NumericField, NumericSource},
    spatial::{Deformation, MeshSettings, Mode},
};
use std::sync::{Arc, Mutex, mpsc};

#[derive(Resource)]
pub(super) struct Replay {
    engine: Arc<engine::Engine>,
    directory: tempfile::TempDir,
    records: Vec<String>,
    task: String,
    board: Option<String>,
    producer: Option<String>,
    anchors: Vec<String>,
    aligned: bool,
    completed: bool,
    detached: bool,
    next: u32,
    stage: usize,
    waiting: bool,
    images: Vec<Vec<u8>>,
    samples: Vec<f64>,
    timings: Vec<serde_json::Value>,
    pending: Option<Mutex<mpsc::Receiver<Result<(), String>>>>,
    saved: Option<serde_json::Value>,
    reloading: bool,
    occlusion: Option<Vec2>,
    mesh_pixels: Vec<(String, Vec2)>,
    presenting: bool,
    present_after: u32,
}

fn effect() -> Effect {
    let mut effect = Effect::default();
    effect.mode = Mode::Mesh(MeshSettings {
        deformation: Some(Deformation { source: "fn deform_vertex(vertex: MeshVertex, inputs: ShaderInputs) -> MeshVertex { let amount = select(0.0, 0.08, inputs.quantity_present != 0u && inputs.quantity < 0.0); return MeshVertex(vertex.position + vertex.normal * amount, vertex.normal, vertex.uv); }".into(), max_displacement: 0.1 }),
        ..default()
    });
    effect.programs.insert("negative".into(), "fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> vec4<f32> { let protein = clamp(inputs.numeric0.x / 100.0, 0.0, 1.0) * f32(inputs.present0.x); let light = 0.3 + 0.7 * max(dot(normalize(point.normal), normalize(vec3<f32>(-0.5, 1.0, 0.7))), 0.0); return vec4<f32>(vec3<f32>(0.08, 0.4, 1.0) * light * (0.3 + protein * 0.7), 1.0); }".into());
    effect.programs.insert("nonnegative".into(), "fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> vec4<f32> { let light = 0.35 + 0.65 * max(dot(normalize(point.normal), normalize(vec3<f32>(-0.5, 1.0, 0.7))), 0.0); return vec4<f32>(vec3<f32>(0.5) * light, 1.0); }".into());
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
    effect.validate().unwrap();
    effect
}

pub(super) async fn install(app: &mut App) {
    let engine = cleaning::engine(app.world());
    let mut records = Vec::new();
    engine
        .act(
            engine::actions::Action::CreateConcept {
                lingua: "g_local".into(),
                name: "depends-on".into(),
                parents: vec![],
            },
            None,
        )
        .await
        .unwrap();
    engine
        .act(
            engine::actions::Action::CreateConcept {
                lingua: "g_local".into(),
                name: "task".into(),
                parents: vec![],
            },
            None,
        )
        .await
        .unwrap();
    for title in ["Study WGSL", "Protein goal"] {
        let uid = engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: title.into(),
                    body: "Connected room Record".into(),
                    quantity: -3.0,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let mut extension = Extension::single(1, effect());
        let mut amber = effect();
        amber.programs = [("amber".into(), "fn shade_mesh(point: MeshPoint, inputs: ShaderInputs) -> vec4<f32> { let light = 0.3 + 0.7 * max(dot(normalize(point.normal), normalize(vec3<f32>(-0.5, 1.0, 0.7))), 0.0); return vec4<f32>(vec3<f32>(1.0, 0.35, 0.04) * light, 1.0); }".into())].into();
        amber.rules.clear();
        amber.fallback = "amber".into();
        extension.effects.insert(
            "amber".into(),
            NamedEffect {
                label: "Amber".into(),
                effect: amber,
            },
        );
        engine
            .act(
                engine::actions::Action::SetExtension {
                    target: uid.clone(),
                    namespace: NAMESPACE.into(),
                    fds: serde_json::to_value(extension).unwrap(),
                },
                None,
            )
            .await
            .unwrap();
        engine
            .act(
                engine::actions::Action::SetExtension {
                    target: uid.clone(),
                    namespace: "nutrition".into(),
                    fds: serde_json::json!({"daily":{"protein":20}}),
                },
                None,
            )
            .await
            .unwrap();
        records.push(uid);
    }
    engine
        .act(
            engine::actions::Action::AssertRecord {
                subject: records[0].clone(),
                predicate: "depends-on".into(),
                object: Some(records[1].clone()),
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    let task = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Review room shaders".into(),
                body: "Real Kanban task".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine
        .act(
            engine::actions::Action::AssertRecord {
                subject: task.clone(),
                predicate: "task".into(),
                object: None,
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    app.insert_resource(
        lince_sandbox::workspace::WorkspaceFile::with_settings(
            directory.path().join("room.json"),
            cell::InterfaceStorage {
                snapshot_seconds: 1,
                ..default()
            },
        )
        .unwrap(),
    )
    .insert_resource(Replay {
        engine,
        directory,
        records,
        task,
        board: None,
        producer: None,
        anchors: Vec::new(),
        aligned: false,
        completed: false,
        detached: false,
        next: 0,
        stage: 0,
        waiting: false,
        images: Vec::new(),
        samples: Vec::new(),
        timings: Vec::new(),
        pending: None,
        saved: None,
        reloading: false,
        occlusion: None,
        mesh_pixels: Vec::new(),
        presenting: false,
        present_after: 0,
    });
}

fn find(world: &mut World, id: &str) -> Option<Entity> {
    world
        .query::<(Entity, &Identity)>()
        .iter(world)
        .find(|(_, identity)| identity.0 == id)
        .map(|(entity, _)| entity)
}

fn identify(world: &mut World, entity: Entity) -> String {
    if world.get::<Identity>(entity).is_none() {
        world
            .entity_mut(entity)
            .insert(Identity(nucleus::new_uid("placement")));
    }
    world.get::<Identity>(entity).unwrap().0.clone()
}

fn regions(world: &mut World, board: Entity) -> Vec<Entity> {
    let board = world
        .get::<lince_sandbox::kanban::Kanban>(board)
        .unwrap()
        .clone();
    let ids: Vec<_> = std::iter::once(board.source)
        .chain(board.columns.into_iter().map(|column| column.area))
        .collect();
    world
        .query::<(Entity, &InfluenceArea)>()
        .iter(world)
        .filter(|(_, area)| ids.contains(&area.id))
        .map(|(entity, _)| entity)
        .collect()
}

pub(super) fn prepare(world: &mut World, root: Entity) -> bool {
    if !world.contains_resource::<Replay>() {
        return true;
    }
    if world.resource::<Replay>().board.is_none() {
        let room = world.resource::<Demo>().room.unwrap();
        let desk = rooms::create_proxy(world, room, model::Role::Desk).unwrap();
        let computer = rooms::create_proxy(world, room, model::Role::Computer).unwrap();
        let anchors = [desk, computer]
            .map(|entity| identify(world, entity))
            .to_vec();
        let producer = lince_sandbox::relation_castle::spawn(world, root).unwrap();
        let ids = world.resource::<Replay>().records.clone();
        {
            let mut area = world.get_mut::<InfluenceArea>(producer).unwrap();
            area.opacity = 0.0;
            area.size = [340.0, 340.0];
            let config = area.protein.as_mut().unwrap();
            config.shader_effects = true;
            config.motion = None;
            config.shader_placement.sphere = true;
            config.shader_placement.size = 70.0;
            config.shader_placement.elevation = 75.0;
            config.shader_placement.motion = false;
            config.shader_placement.distance = lince_sandbox::shader_effects::DistancePolicy::Live;
            config.width = 180.0;
            config.draft.query["where"] = serde_json::json!([{"any":ids.iter().map(|uid| serde_json::json!({"uid_eq":uid})).collect::<Vec<_>>()}]);
            area.records
                .entry(ids[1].clone())
                .or_default()
                .shader_placement = Some(lince_sandbox::shader_effects::FloatingSettings {
                effect_id: Some("amber".into()),
                sphere: true,
                size: 70.0,
                elevation: 75.0,
                motion: false,
                distance: lince_sandbox::shader_effects::DistancePolicy::Live,
                ..default()
            });
            area.records
                .get_mut(&ids[1])
                .unwrap()
                .presentation
                .hide_filled = true;
        }
        world.get_mut::<CanvasItem>(producer).unwrap().size = Vec2::splat(340.0);
        rooms::mount(world, producer, desk).unwrap();
        let board =
            lince_sandbox::kanban::spawn(world, root, 1, DVec2::new(-2000.0, -2000.0)).unwrap();
        for entity in regions(world, board) {
            if let Some(config) = world
                .get_mut::<InfluenceArea>(entity)
                .unwrap()
                .protein
                .as_mut()
            {
                config.hide_filled = true;
                config.width = 100.0;
            }
            if world.get::<InfluenceArea>(entity).unwrap().id
                == world
                    .get::<lince_sandbox::kanban::Kanban>(board)
                    .unwrap()
                    .source
            {
                continue;
            }
            world.get_mut::<CanvasItem>(entity).unwrap().size = Vec2::new(120.0, 160.0);
            world.get_mut::<InfluenceArea>(entity).unwrap().size = [120.0, 160.0];
            world
                .entity_mut(entity)
                .remove::<lince_sandbox::layout::LayoutBox>();
        }
        let board_id = identify(world, board);
        let producer_id = identify(world, producer);
        let next = world.resource::<FrameCount>().0 + 8;
        let mut state = world.resource_mut::<Replay>();
        state.board = Some(board_id);
        state.producer = Some(producer_id);
        state.anchors = anchors;
        state.next = next;
        return false;
    }
    let frame = world.resource::<FrameCount>().0;
    if frame < world.resource::<Replay>().next {
        return false;
    }
    let ids = world.resource::<Replay>().records.clone();
    let rows: Vec<_> = world
        .query::<(Entity, &RecordBinding, &CanvasItem)>()
        .iter(world)
        .filter(|(_, binding, _)| ids.contains(&binding.uid))
        .map(|(entity, binding, _)| (entity, binding.uid.clone(), binding.area))
        .collect();
    if rows.len() != 2 {
        return false;
    }
    if !world.resource::<Replay>().aligned {
        let computer_id = world.resource::<Replay>().anchors[1].clone();
        let computer = find(world, &computer_id).unwrap();
        let board_id = world.resource::<Replay>().board.clone().unwrap();
        let board = find(world, &board_id).unwrap();
        let wall = world.resource::<Demo>().proxies[1];
        rooms::mount(world, board, wall).unwrap();
        let mut member = world.get_mut::<rooms::Member>(board).unwrap();
        member.0.local.position = [-360.0, 12.0, -130.0];
        rooms::synchronize(world);
        for (index, (row, uid, area)) in rows.iter().enumerate() {
            let mut spatial = topology::spatial(world, *row);
            spatial.depth = Some(12.0);
            world.entity_mut(*row).insert(spatial);
            rooms::mount(world, *row, computer).unwrap();
            let mut member = world.get::<rooms::Member>(*row).unwrap().0.clone();
            member.local.position = [if index == 0 { -110.0 } else { 110.0 }, 40.0, -35.0];
            world.entity_mut(*row).insert(rooms::Member(member.clone()));
            world
                .get_mut::<InfluenceArea>(*area)
                .unwrap()
                .records
                .entry(uid.clone())
                .or_default()
                .room_member = Some(member);
        }
        rooms::synchronize(world);
        let desk_id = world.resource::<Replay>().anchors[0].clone();
        let desk = find(world, &desk_id).unwrap();
        let camera = world.resource::<topology::presentation::SceneCamera>().0;
        let point = topology::position(world, desk).unwrap() + DVec3::new(-90.0, 3.0, 0.0)
            - topology::presentation::origin(world, root);
        let projected = world
            .get::<Camera>(camera)
            .unwrap()
            .world_to_viewport(
                world.get::<GlobalTransform>(camera).unwrap(),
                point.as_vec3(),
            )
            .ok();
        let mut state = world.resource_mut::<Replay>();
        state.aligned = true;
        state.next = frame + 12;
        state.occlusion = projected;
        return false;
    }
    let meshes = world
        .query::<&lince_sandbox::shader_effects::RecordMesh>()
        .iter(world)
        .count();
    lince_sandbox::workspace::validate_scene(world, root).unwrap();
    let arrows = world
        .query::<&lince_sandbox::arrow_sand::ArrowSand>()
        .iter(world)
        .count();
    let task = world
        .query::<&RecordProperties>()
        .iter(world)
        .any(|properties| properties.0["uid"] == world.resource::<Replay>().task);
    if frame.is_multiple_of(120) {
        let errors: Vec<_> = world
            .query::<&Text>()
            .iter(world)
            .filter(|text| {
                text.0.contains("WGSL") || text.0.contains("invalid") || text.0.contains("error")
            })
            .map(|text| text.0.clone())
            .collect();
        eprintln!(
            "Connected readiness frame {frame}: meshes={meshes} arrows={arrows} task={task} texts={errors:?}"
        );
        let path = world
            .resource::<Demo>()
            .output
            .join("connected-readiness.png");
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
    meshes == 2 && arrows == 1 && task
}

pub(super) fn active(world: &World) -> bool {
    world
        .get_resource::<Replay>()
        .is_some_and(|state| !state.completed)
}

pub(super) fn presenting(world: &World) -> bool {
    world
        .get_resource::<Replay>()
        .is_some_and(|state| state.presenting)
}

pub(super) fn present(world: &mut World, root: Entity) -> bool {
    if !active(world) {
        return true;
    }
    let frame = world.resource::<FrameCount>().0;
    if !presenting(world) {
        world
            .get_mut::<lince_sandbox::edit_mode::EditMode>(root)
            .unwrap()
            .enabled = false;
        world
            .get_mut::<lince_sandbox::inspection::Inspection>(root)
            .unwrap()
            .selected = None;
        if let Some(mut selection) =
            world.get_mut::<lince_sandbox::canvas_selection::SandSelection>(root)
        {
            selection.0.clear();
        }
        let mut state = world.resource_mut::<Replay>();
        state.presenting = true;
        state.present_after = frame + 3;
        return false;
    }
    frame >= world.resource::<Replay>().present_after
}

pub(super) fn after_capture(world: &mut World, root: Entity) {
    if !active(world) {
        return;
    }
    world
        .get_mut::<lince_sandbox::edit_mode::EditMode>(root)
        .unwrap()
        .enabled = true;
    if let Some(floor) = world.resource::<Demo>().proxies.first().copied()
        && world.get_entity(floor).is_ok()
    {
        world
            .get_mut::<lince_sandbox::inspection::Inspection>(root)
            .unwrap()
            .selected = Some(floor);
    }
    world.resource_mut::<Replay>().presenting = false;
}

fn project(world: &World, root: Entity, point: DVec3) -> Option<Vec2> {
    let camera = world.resource::<topology::presentation::SceneCamera>().0;
    let camera_view = world.get::<Camera>(camera)?;
    let pixels =
        camera_view.physical_viewport_size()?.as_vec2() / camera_view.logical_viewport_size()?;
    camera_view
        .world_to_viewport(
            world.get::<GlobalTransform>(camera)?,
            (point - topology::presentation::origin(world, root)).as_vec3(),
        )
        .ok()
        .map(|point| point * pixels)
}

pub(super) fn before_capture(world: &mut World, root: Entity, stage: usize) {
    if stage != 13 || !active(world) {
        return;
    }
    let id = world.resource::<Replay>().anchors[0].clone();
    let desk = find(world, &id).unwrap();
    world.resource_mut::<Replay>().occlusion = project(
        world,
        root,
        topology::position(world, desk).unwrap() + DVec3::new(-90.0, 3.0, 0.0),
    );
}

fn actions(world: &mut World, actions: Vec<engine::actions::Action>) {
    let engine = world.resource::<Replay>().engine.clone();
    let (tx, rx) = mpsc::channel();
    tokio::spawn(async move {
        let result = async {
            for action in actions {
                engine
                    .act(action, None)
                    .await
                    .map_err(|error| error.to_string())?;
            }
            Ok(())
        }
        .await;
        tx.send(result).unwrap();
    });
    world.resource_mut::<Replay>().pending = Some(Mutex::new(rx));
}

fn properties(world: &mut World, uid: &str, quantity: &str) -> bool {
    world
        .query::<&RecordProperties>()
        .iter(world)
        .any(|record| {
            record.0["uid"] == uid
                && (record.0["quantity"].as_str() == Some(quantity)
                    || record.0["quantity"]
                        .as_f64()
                        .is_some_and(|value| value.to_string() == quantity))
        })
}

fn task_column(world: &mut World, uid: &str, completed: bool) -> bool {
    let id = world.resource::<Replay>().board.clone().unwrap();
    let Some(board) = find(world, &id) else {
        return false;
    };
    let id = world
        .get::<lince_sandbox::kanban::Kanban>(board)
        .unwrap()
        .columns[if completed { 5 } else { 0 }]
    .area
    .clone();
    let target = world
        .query::<(Entity, &InfluenceArea)>()
        .iter(world)
        .find(|(_, area)| area.id == id)
        .map(|(entity, _)| entity);
    target.is_some()
        && world
            .query::<(
                &RecordBinding,
                &CanvasItem,
                &lince_sandbox::layout::LayoutRuntime,
            )>()
            .iter(world)
            .any(|(binding, _, layout)| binding.uid == uid && layout.parent == target)
}

fn valid_poses(world: &mut World) {
    let members: Vec<_> = world
        .query::<(Entity, &rooms::Member)>()
        .iter(world)
        .map(|(entity, member)| (entity, member.0.clone()))
        .collect();
    for (entity, member) in members {
        let parent = find(world, member.parent.as_ref().unwrap_or(&member.room)).unwrap();
        let expected = model::compose(
            &Pose {
                position: topology::position(world, parent).unwrap().to_array(),
                rotation: topology::spatial(world, parent).rotation,
            },
            &member.local,
        );
        assert!(
            (topology::position(world, entity).unwrap() - DVec3::from_array(expected.position))
                .length()
                < 1e-5
        );
        assert!(
            DQuat::from_array(topology::spatial(world, entity).rotation)
                .dot(DQuat::from_array(expected.rotation))
                .abs()
                > 1.0 - 1e-8
        );
    }
}

fn button(world: &mut World, owner: Entity, caption: &str) -> bool {
    let action = world
        .query::<(
            &lince_sandbox::actions::ActionButton,
            &lince_sandbox::icons::Tooltip,
        )>()
        .iter(world)
        .find(|(action, tooltip)| action.target == owner && tooltip.0 == caption)
        .map(|(action, _)| action.clone());
    if let Some(action) = action {
        action.actions.run(world, owner);
        true
    } else {
        false
    }
}

fn reload(world: &mut World, root: Entity) -> bool {
    if world.resource::<Replay>().saved.is_none() {
        let directory = world
            .resource::<Replay>()
            .directory
            .path()
            .join("room.snapshots");
        let path = std::fs::read_dir(directory)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .max();
        let Some(path) = path else {
            return false;
        };
        let Ok(bytes) = std::fs::read(path) else {
            return false;
        };
        let Ok(document) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return false;
        };
        let room = world.resource::<Demo>().room.unwrap();
        let id = world.get::<Identity>(room).unwrap().0.clone();
        let current = lince_sandbox::canvas_host::capture(world, root).unwrap();
        let saved = document["canvas_state"]["snapshot"]["placements"].as_array();
        let saved_room =
            saved.and_then(|placements| placements.iter().find(|item| item["id"] == id));
        let expected = current
            .placements
            .iter()
            .find(|item| item.id == id)
            .unwrap();
        if saved_room.is_none_or(|room| {
            room["geometry"]["position"] != serde_json::json!(expected.geometry.position)
                || room["metadata"] != serde_json::to_value(&expected.metadata).unwrap()
        }) {
            return false;
        }
        let text = String::from_utf8(bytes.clone()).unwrap();
        if !world
            .resource::<Replay>()
            .records
            .iter()
            .all(|uid| text.contains(uid))
            || !text.contains("amber")
            || !text.contains("shader_surface")
        {
            return false;
        }
        std::fs::write(
            world
                .resource::<Demo>()
                .output
                .join("connected-room-saved.json"),
            bytes,
        )
        .unwrap();
        cleaning::clear_readback(world);
        world.resource_mut::<Replay>().saved = Some(document);
        world.resource_mut::<Replay>().reloading = true;
        world.despawn(root);
        world.spawn((BoxRoot, CanvasView::default()));
        let frame = world.resource::<FrameCount>().0;
        world.resource_mut::<Replay>().next = frame + 16;
        return false;
    }
    if world.resource::<Replay>().reloading {
        let Some(room) = world
            .query_filtered::<Entity, With<rooms::Room>>()
            .iter(world)
            .next()
        else {
            return false;
        };
        let proxies: Vec<_> = [model::Role::Floor, model::Role::Wall]
            .into_iter()
            .filter_map(|role| {
                world
                    .query::<(Entity, &rooms::Member)>()
                    .iter(world)
                    .find(|(_, member)| member.0.role == role)
                    .map(|(entity, _)| entity)
            })
            .collect();
        let Some(cloud) = world
            .query_filtered::<Entity, (With<assets::ImportedAsset>, With<assets::Ready>)>()
            .iter(world)
            .next()
        else {
            return false;
        };
        if proxies.len() != 2 {
            return false;
        }
        {
            let mut demo = world.resource_mut::<Demo>();
            demo.room = Some(room);
            demo.proxies = proxies;
            demo.cloud = Some(cloud);
        }
        world
            .get_mut::<lince_sandbox::edit_mode::EditMode>(root)
            .unwrap()
            .enabled = true;
        let ids = world.resource::<Replay>().records.clone();
        let rows = world
            .query::<(&RecordBinding, &CanvasItem)>()
            .iter(world)
            .filter(|(binding, _)| ids.contains(&binding.uid))
            .count();
        if rows != 2
            || world
                .query::<&lince_sandbox::shader_effects::RecordMesh>()
                .iter(world)
                .count()
                != 2
        {
            return false;
        }
        let rows: Vec<_> = world
            .query::<(Entity, &RecordBinding, &CanvasItem)>()
            .iter(world)
            .filter(|(_, binding, _)| ids.contains(&binding.uid))
            .map(|(entity, _, _)| entity)
            .collect();
        for row in rows {
            let mut spatial = topology::spatial(world, row);
            spatial.depth = Some(12.0);
            world.entity_mut(row).insert(spatial);
        }
        assert!(
            world
                .get::<lince_sandbox::workspace::Workspaces>(root)
                .unwrap()
                .error
                .is_none()
        );
        world.resource_mut::<Replay>().reloading = false;
    }
    true
}

fn change(world: &mut World, root: Entity, stage: usize) {
    let cloud = world.resource::<Demo>().cloud.unwrap();
    match stage {
        0 => {
            let visual = world.get::<assets::ImportedScene>(cloud).unwrap().0;
            world.entity_mut(visual).insert(Visibility::Hidden);
        }
        1 => {
            let visual = world.get::<assets::ImportedScene>(cloud).unwrap().0;
            world.entity_mut(visual).insert(Visibility::Inherited);
            let room = world.resource::<Demo>().room.unwrap();
            topology::set_position(world, room, DVec3::new(1e9, 140.0, -1e9));
            let mut spatial = topology::spatial(world, room);
            spatial.rotation = DQuat::from_rotation_y(0.55).to_array();
            world.entity_mut(room).insert(spatial);
            rooms::synchronize(world);
            assets::frame(world, cloud);
        }
        2 => {
            let computer_id = world.resource::<Replay>().anchors[1].clone();
            let computer = find(world, &computer_id).unwrap();
            world
                .get_mut::<rooms::Member>(computer)
                .unwrap()
                .0
                .local
                .position[0] += 80.0;
            let wall = world.resource::<Demo>().proxies[1];
            let mut member = world.get_mut::<rooms::Member>(wall).unwrap();
            member.0.local.position[0] += 30.0;
            member.0.local.rotation = (DQuat::from_rotation_y(0.08)
                * DQuat::from_rotation_x(std::f64::consts::FRAC_PI_2))
            .to_array();
            rooms::synchronize(world);
        }
        3 => {
            let target = world.resource::<Replay>().task.clone();
            actions(
                world,
                vec![engine::actions::Action::SetQuantity { target, value: 1.0 }],
            );
        }
        4 => {
            let target = world.resource::<Replay>().records[0].clone();
            actions(
                world,
                vec![engine::actions::Action::SetExtension {
                    target,
                    namespace: "nutrition".into(),
                    fds: serde_json::json!({"daily":{"protein":80}}),
                }],
            );
        }
        5 => {
            let target = world.resource::<Replay>().records[0].clone();
            actions(
                world,
                vec![engine::actions::Action::SetQuantity { target, value: 0.0 }],
            );
        }
        8 => {
            let producer_id = world.resource::<Replay>().producer.clone().unwrap();
            let producer = find(world, &producer_id).unwrap();
            world
                .get_mut::<InfluenceArea>(producer)
                .unwrap()
                .protein
                .as_mut()
                .unwrap()
                .shader_effects = false;
            let floor = world.resource::<Demo>().proxies[0];
            world
                .get_mut::<lince_sandbox::shader_effects::Surface>(floor)
                .unwrap()
                .disabled = true;
            world.get_mut::<View>(root).unwrap().spatial = false;
        }
        9 => {
            cleaning::clear_readback(world);
            assets::discard(world, cloud);
            let children: Vec<_> = world
                .get::<Children>(root)
                .unwrap()
                .iter()
                .filter(|entity| world.get::<CanvasItem>(*entity).is_some())
                .collect();
            for entity in children {
                if let Ok(entity) = world.get_entity_mut(entity) {
                    entity.despawn();
                }
            }
        }
        _ => {}
    }
}

fn strength(pixels: &[u8], size: UVec2, center: Vec2, amber: bool) -> u64 {
    let mut sum = 0;
    for y in center.y as i32 - 10..center.y as i32 + 10 {
        for x in center.x as i32 - 10..center.x as i32 + 10 {
            if x < 0 || y < 0 || x >= size.x as i32 || y >= size.y as i32 {
                continue;
            }
            let offset = ((y as usize * size.x as usize) + x as usize) * 4;
            let a = pixels[offset + if amber { 0 } else { 2 }];
            let b = pixels[offset + if amber { 2 } else { 0 }];
            sum += u64::from(a.saturating_sub(b));
        }
    }
    sum
}

fn verify(world: &mut World) {
    let state = world.resource::<Replay>();
    let changed = state.images[0]
        .chunks_exact(4)
        .zip(state.images[1].chunks_exact(4))
        .filter(|(a, b)| {
            a.iter()
                .zip(b.iter())
                .take(3)
                .any(|(a, b)| a.abs_diff(*b) > 12)
        })
        .count();
    assert!(
        changed > 400,
        "Combined Gaussian scene contributed only {changed} pixels"
    );
    let (cleaning_images, size) = cleaning::images(world);
    let center = state
        .occlusion
        .expect("Opaque desk projects into the cleaning view");
    let mut changes = 0;
    for y in center.y as i32 - 5..center.y as i32 + 5 {
        for x in center.x as i32 - 5..center.x as i32 + 5 {
            assert!(x >= 0 && y >= 0 && x < size.x as i32 && y < size.y as i32);
            let offset = ((y as usize * size.x as usize) + x as usize) * 4;
            changes += usize::from(
                cleaning_images[13][offset..offset + 3]
                    .iter()
                    .zip(&cleaning_images[14][offset..offset + 3])
                    .any(|(a, b)| a.abs_diff(*b) > 8),
            );
        }
    }
    assert!(
        changes < 10,
        "Cleaning dirt leaked through the opaque desk: {changes}/100 pixels"
    );
    for (uid, pixel) in &state.mesh_pixels {
        let amber = uid == &state.records[1];
        let low = strength(&state.images[4], size, *pixel, amber);
        let high = strength(&state.images[5], size, *pixel, amber);
        let calm = strength(&state.images[6], size, *pixel, amber);
        eprintln!("Sphere pixels {uid} at {pixel:?}: {low}/{high}/{calm}");
        if amber {
            assert!(
                high > 1000 && high.abs_diff(calm) < high / 5,
                "Independent amber effect changed at {pixel:?}: {low}/{high}/{calm}"
            );
        } else {
            assert!(
                high > low * 11 / 10 && calm < high / 2,
                "Live protein/quantity did not change sphere pixels: {low}/{high}/{calm}"
            );
        }
    }
    assert_eq!(state.mesh_pixels.len(), 2);
    std::fs::write(
        world
            .resource::<Demo>()
            .output
            .join("connected-room-timings.json"),
        serde_json::to_vec_pretty(&state.timings).unwrap(),
    )
    .unwrap();
    assert_eq!(world.query::<&rooms::Member>().iter(world).count(), 0);
    assert_eq!(
        world.query::<&assets::ImportedAsset>().iter(world).count(),
        0
    );
    assert_eq!(world.query::<&RecordBinding>().iter(world).count(), 0);
    assert_eq!(
        world
            .query::<&lince_sandbox::shader_effects::RecordMesh>()
            .iter(world)
            .count(),
        0
    );
    assert_eq!(
        world
            .query::<&lince_sandbox::arrow_sand::ArrowSand>()
            .iter(world)
            .count(),
        0
    );
    println!(
        "Connected Gaussian/cleaning/Record WGSL/Kanban replay passed: {changed} Gaussian pixels, {changes}/100 occlusion changes, actual workspace reload, anchor movement, task completion, numeric/sphere pixels, detach, disable and cleanup"
    );
    world.resource_mut::<Replay>().completed = true;
}

pub(super) fn exercise(world: &mut World, root: Entity) {
    let frame = world.resource::<FrameCount>().0;
    if world.resource::<Replay>().waiting || frame < world.resource::<Replay>().next {
        return;
    }
    let pending = world
        .resource::<Replay>()
        .pending
        .as_ref()
        .map(|pending| pending.lock().unwrap().try_recv());
    if let Some(pending) = pending {
        match pending {
            Ok(result) => {
                result.unwrap();
                world.resource_mut::<Replay>().pending = None;
            }
            Err(mpsc::TryRecvError::Empty) => return,
            Err(error) => panic!("Connected backend action failed: {error}"),
        }
    }
    let stage = world.resource::<Replay>().stage;
    if stage == 7 && !reload(world, root) {
        return;
    }
    if stage < 10 {
        rooms::synchronize(world);
        valid_poses(world);
        let task = world.resource::<Replay>().task.clone();
        if !properties(world, &task, if stage < 4 { "0" } else { "1" })
            || !task_column(world, &task, stage >= 4)
        {
            return;
        }
        let uid = world.resource::<Replay>().records[0].clone();
        if !properties(world, &uid, if stage < 6 { "-3" } else { "0" }) {
            return;
        }
        if (5..9).contains(&stage)
            && !world
                .query::<&Text>()
                .iter(world)
                .any(|text| text.0.contains("Protein: 80"))
        {
            return;
        }
        if stage == 8 && !world.resource::<Replay>().detached {
            let row = world
                .query::<(Entity, &RecordBinding, &CanvasItem)>()
                .iter(world)
                .find(|(_, binding, _)| binding.uid == uid)
                .unwrap()
                .0;
            world
                .get_mut::<lince_sandbox::inspection::Inspection>(root)
                .unwrap()
                .selected = Some(row);
            if !button(world, row, "Detach from room") {
                return;
            }
            assert!(world.get::<rooms::Member>(row).is_none());
            assert!(world.get::<rooms::WorldPlacement>(row).is_some());
            let before = topology::position(world, row).unwrap();
            let computer_id = world.resource::<Replay>().anchors[1].clone();
            let computer = find(world, &computer_id).unwrap();
            world
                .get_mut::<rooms::Member>(computer)
                .unwrap()
                .0
                .local
                .position[0] += 80.0;
            rooms::synchronize(world);
            assert_eq!(topology::position(world, row), Some(before));
            world.resource_mut::<Replay>().detached = true;
            world.resource_mut::<Replay>().next = frame + 8;
            return;
        }
        if stage == 9
            && (world
                .query::<&lince_sandbox::shader_effects::RecordMesh>()
                .iter(world)
                .count()
                != 0
                || lince_sandbox::shader_effects::surface_reading(
                    world,
                    world.resource::<Demo>().proxies[0],
                )
                .is_none_or(|reading| reading.enabled))
        {
            return;
        }
    }
    lince_sandbox::canvas_host::capture(world, root)
        .unwrap()
        .validate()
        .unwrap();
    let target_count = world
        .resource::<Assets<Image>>()
        .iter()
        .filter(|(_, image)| {
            image.width() == 256
                && image.height() == 256
                && image
                    .texture_descriptor
                    .usage
                    .contains(bevy::render::render_resource::TextureUsages::RENDER_ATTACHMENT)
        })
        .count();
    assert!(
        target_count <= 64,
        "Shader render target budget exceeded: {target_count}"
    );
    if !present(world, root) {
        return;
    }
    let delta = world.resource::<Time<Real>>().delta_secs_f64() * 1000.0;
    world.resource_mut::<Replay>().samples.push(delta);
    if world.resource::<Replay>().samples.len() < 16 {
        return;
    }
    let mut samples = world.resource::<Replay>().samples.clone();
    samples.sort_by(f64::total_cmp);
    let timing = serde_json::json!({"stage":stage,"frame_median_ms":samples[samples.len()/2],"frame_p95_ms":samples[(samples.len()*95).div_ceil(100)-1],"shader_targets":target_count});
    eprintln!("Connected room stage {stage}: {timing}");
    world.resource_mut::<Replay>().timings.push(timing);
    if stage == 11 {
        verify(world);
        return;
    }
    if stage == 4 && world.resource::<Replay>().mesh_pixels.is_empty() {
        let points: Vec<_> = world
            .query::<(&lince_sandbox::shader_effects::RecordMesh, &GlobalTransform)>()
            .iter(world)
            .filter_map(|(mesh, transform)| {
                Some((
                    world.get::<RecordBinding>(mesh.0)?.uid.clone(),
                    transform.translation(),
                ))
            })
            .collect();
        let camera = world.resource::<topology::presentation::SceneCamera>().0;
        let camera_view = world.get::<Camera>(camera).unwrap();
        let scale = camera_view.physical_viewport_size().unwrap().as_vec2()
            / camera_view.logical_viewport_size().unwrap();
        let points: Vec<_> = points
            .into_iter()
            .filter_map(|(uid, point)| {
                Some((
                    uid,
                    camera_view
                        .world_to_viewport(world.get::<GlobalTransform>(camera)?, point)
                        .ok()?
                        * scale,
                ))
            })
            .collect();
        let values: Vec<_> = points
            .iter()
            .map(|(uid, pixel)| serde_json::json!({"uid":uid,"pixel":pixel.to_array()}))
            .collect();
        std::fs::write(
            world
                .resource::<Demo>()
                .output
                .join("connected-room-samples.json"),
            serde_json::to_vec_pretty(&values).unwrap(),
        )
        .unwrap();
        eprintln!("Settled mesh sample points: {points:?}; pixel scale {scale:?}");
        world.resource_mut::<Replay>().mesh_pixels = points;
    }
    let names = [
        "full-spatial",
        "without-cloud",
        "moved-room",
        "moved-anchors",
        "task-done",
        "protein-80",
        "calm-sphere",
        "reloaded",
        "detached-record",
        "disabled-flat",
        "removed",
    ];
    let path = world
        .resource::<Demo>()
        .output
        .join(format!("connected-{}.png", names[stage]));
    world.resource_mut::<Replay>().waiting = true;
    world
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path))
        .observe(
            move |event: On<ScreenshotCaptured>, mut commands: Commands| {
                let pixels = capture_pixels(&event.image);
                commands.queue(move |world: &mut World| {
                    world.resource_mut::<Replay>().images.push(pixels);
                    after_capture(world, root);
                    change(world, root, stage);
                    let next = world.resource::<FrameCount>().0 + 8;
                    let mut state = world.resource_mut::<Replay>();
                    state.stage += 1;
                    state.waiting = false;
                    state.samples.clear();
                    state.next = next;
                });
            },
        );
}
