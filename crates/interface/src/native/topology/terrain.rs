use avian3d::prelude::*;
use bevy::{
    math::{DQuat, DVec2, DVec3},
    prelude::*,
};
use nucleus::terrain::{Anchor, Formula, SpatialBinding, TopologyEffect};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

mod drawing;
pub use drawing::install;
pub mod editor;

pub const TILE_SIZE: f64 = 512.0;
const GRID: usize = 32;
const MAX_TILES: usize = 4096;

#[derive(Component, Clone, Debug)]
pub struct Effect(pub TopologyEffect);
#[derive(Component, Clone, Debug)]
pub struct Bound(pub SpatialBinding);
#[derive(Component)]
pub struct FollowSlope;
#[derive(Component)]
pub struct Unsupported;
#[derive(Component)]
pub struct Ground {
    pub root: Entity,
    pub workspace: u64,
}
#[derive(Component, Default)]
pub struct Fault(pub String, pub TopologyEffect);
#[derive(Component, Clone)]
pub struct Preview(pub TopologyEffect);

#[derive(Clone)]
struct Input {
    id: String,
    entity: Entity,
    definition: TopologyEffect,
    center: DVec2,
    heading: f64,
}

impl PartialEq for Input {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.entity == other.entity
            && self.center == other.center
            && self.heading == other.heading
            && self.definition.same_surface(&other.definition)
    }
}

#[derive(Clone)]
struct Field {
    input: Input,
    formula: Formula,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key(Entity, u64, i64, i64);

impl Key {
    fn minimum(self) -> DVec2 {
        DVec2::new(self.2 as f64, self.3 as f64) * TILE_SIZE
    }
}

struct Tile {
    inputs: Vec<Input>,
    heights: Vec<f64>,
    collider: Option<Entity>,
    visual: Option<Entity>,
    used: u64,
    clipping: Option<Vec<Vec<DVec2>>>,
    depth: usize,
}

#[derive(Resource, Default)]
pub struct Terrain {
    fields: HashMap<(Entity, u64), Vec<Field>>,
    tiles: HashMap<Key, Tile>,
    render_tiles: HashMap<Key, Tile>,
    flats: HashMap<(Entity, u64), Entity>,
    epoch: u64,
    pub evaluations: u64,
    pub rebuilds: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Saved {
    pub workspace: u64,
    pub position: [f64; 2],
    pub effect: TopologyEffect,
    pub placement: crate::native::sand_placement::Placement,
}

impl Saved {
    pub fn valid(&self) -> bool {
        self.effect.compile().is_ok()
            && self.placement.valid()
            && self
                .position
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 1_000_000.0)
    }

    pub fn restore(self, world: &mut World, root: Entity) {
        if let Ok(entity) = spawn(
            world,
            root,
            self.workspace,
            DVec2::from_array(self.position),
            self.effect,
        ) {
            self.placement.restore(world, entity);
        }
    }
}

pub fn snapshot(world: &mut World, root: Entity) -> Vec<Saved> {
    let entities: Vec<_> = world
        .query::<(
            Entity,
            &Effect,
            &crate::native::canvas::CanvasItem,
            &ChildOf,
            &crate::native::workspace::WorkspaceMember,
        )>()
        .iter(world)
        .filter(|(_, _, _, parent, _)| parent.parent() == root)
        .map(|(e, effect, item, _, member)| {
            (e, effect.0.clone(), item.position.to_array(), member.0)
        })
        .collect();
    entities
        .into_iter()
        .map(|(e, effect, position, workspace)| Saved {
            workspace,
            position,
            effect,
            placement: crate::native::sand_placement::Placement::capture(world, e),
        })
        .collect()
}

pub fn component(effect: &TopologyEffect) -> nucleus::canvas::Component {
    nucleus::canvas::Component::Native {
        kind: "topology-effect".into(),
        settings: std::collections::BTreeMap::from([(
            "state".into(),
            serde_json::json!(serde_json::to_string(effect).unwrap()),
        )]),
        bindings: Vec::new(),
    }
}

pub fn is_effect(component: &nucleus::canvas::Component) -> bool {
    matches!(nucleus::canvas::authoring::base(component), nucleus::canvas::Component::Native { kind, .. } if kind == "topology-effect")
}

pub fn definition(component: &nucleus::canvas::Component) -> Result<TopologyEffect, String> {
    match nucleus::canvas::authoring::base(component) {
        nucleus::canvas::Component::Native { settings, .. } if is_effect(component) => {
            parse(settings)
        }
        _ => Err("Expected a topology effect.".into()),
    }
}

pub fn parse(
    settings: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<TopologyEffect, String> {
    let effect: TopologyEffect = settings
        .get("state")
        .and_then(serde_json::Value::as_str)
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    effect.compile()?;
    Ok(effect)
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    effect: TopologyEffect,
) -> Result<Entity, String> {
    effect.compile()?;
    let count = world
        .query::<(&Effect, &ChildOf)>()
        .iter(world)
        .filter(|(_, p)| p.parent() == root)
        .count();
    if count >= 256 {
        return Err("This canvas already has 256 topology effects.".into());
    }
    let entity = world
        .spawn((
            Effect(effect.clone()),
            crate::native::canvas::CanvasItem {
                position,
                size: Vec2::splat(32.0),
            },
            crate::native::workspace::WorkspaceMember(workspace),
            crate::native::canvas_host::Identity(nucleus::new_uid("placement")),
            crate::native::canvas_host::Content(component(&effect)),
            crate::native::sand::Square,
            Node {
                width: px(32),
                height: px(32),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            crate::native::token_style::background(crate::native::tokens::Token::Accent),
            ChildOf(root),
        ))
        .id();
    if world.contains_resource::<crate::native::theme::Typography>() {
        crate::native::edit_mode::label(world, entity, "⌁", 20.0);
    }
    Ok(entity)
}

pub fn height(world: &World, root: Entity, workspace: u64, point: DVec2) -> f64 {
    world
        .get_resource::<Terrain>()
        .map_or(0.0, |terrain| terrain.sample(root, workspace, point))
}

impl Terrain {
    fn sample(&self, root: Entity, workspace: u64, point: DVec2) -> f64 {
        let Some(fields) = self.fields.get(&(root, workspace)) else {
            return 0.0;
        };
        let key = Key(
            root,
            workspace,
            (point.x / TILE_SIZE).floor() as i64,
            (point.y / TILE_SIZE).floor() as i64,
        );
        let local = (point - key.minimum()) / TILE_SIZE * GRID as f64;
        let x = (local.x.floor() as usize).min(GRID - 1);
        let y = (local.y.floor() as usize).min(GRID - 1);
        let (u, v) = (local.x - x as f64, local.y - y as f64);
        let stride = GRID + 1;
        let cached = self
            .tiles
            .get(&key)
            .or_else(|| self.render_tiles.get(&key))
            .filter(|tile| {
                tile.inputs.iter().eq(fields
                    .iter()
                    .filter(|f| affects(&f.input, key))
                    .map(|f| &f.input))
            });
        let [a, b, c, d] = if let Some(tile) = cached {
            [
                tile.heights[y * stride + x],
                tile.heights[y * stride + x + 1],
                tile.heights[(y + 1) * stride + x],
                tile.heights[(y + 1) * stride + x + 1],
            ]
        } else {
            [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)].map(|(x, y)| {
                let point =
                    key.minimum() + DVec2::new(x as f64, y as f64) * (TILE_SIZE / GRID as f64);
                fields
                    .iter()
                    .filter(|f| affects(&f.input, key))
                    .map(|field| field_height(field, point).unwrap_or(0.0))
                    .sum::<f64>()
            })
        };
        if u + v <= 1.0 {
            a + (b - a) * u + (c - a) * v
        } else {
            d + (c - d) * (1.0 - u) + (b - d) * (1.0 - v)
        }
    }
}

fn field_height(field: &Field, point: DVec2) -> Result<f64, String> {
    let local = DQuat::from_rotation_y(field.input.heading).inverse()
        * DVec3::new(
            point.x - field.input.center.x,
            0.0,
            point.y - field.input.center.y,
        );
    field
        .input
        .definition
        .height(&field.formula, [local.x, local.z])
}

fn affects(input: &Input, key: Key) -> bool {
    let radius = DVec2::from_array(input.definition.extent).max_element();
    let minimum = key.minimum();
    input.center.x + radius >= minimum.x
        && input.center.x - radius <= minimum.x + TILE_SIZE
        && input.center.y + radius >= minimum.y
        && input.center.y - radius <= minimum.y + TILE_SIZE
}

pub fn ray_point(world: &World, root: Entity, workspace: u64, screen: Vec2) -> Option<DVec3> {
    let ray = super::input::ray(world, screen)?;
    let origin = ray.origin.as_dvec3() + super::presentation::origin(world, root);
    let direction = ray.direction.as_dvec3();
    if direction.y >= -1e-6 {
        return None;
    }
    let mut previous = 0.0;
    let mut previous_gap =
        origin.y - height(world, root, workspace, DVec2::new(origin.x, origin.z));
    if previous_gap < 0.0 {
        return None;
    }
    let plane = (-origin.y / direction.y).max(0.0);
    let limit = (plane + 200_000.0 / -direction.y).min(1_000_000.0);
    if world
        .get_resource::<Terrain>()
        .is_none_or(|t| !t.fields.contains_key(&(root, workspace)))
    {
        return (plane > 0.0).then_some(origin + direction * plane);
    }
    let mut distance = 0.0;
    for _ in 0..1024 {
        distance += (previous_gap.abs() * 0.5).clamp(4.0, 1024.0);
        if distance > limit {
            break;
        }
        let point = origin + direction * distance;
        let gap = point.y - height(world, root, workspace, DVec2::new(point.x, point.z));
        if gap <= 0.0 {
            let mut low = previous;
            let mut high = distance;
            for _ in 0..24 {
                let middle = (low + high) * 0.5;
                let point = origin + direction * middle;
                if point.y > height(world, root, workspace, DVec2::new(point.x, point.z)) {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            return Some(origin + direction * ((low + high) * 0.5));
        }
        previous = distance;
        previous_gap = gap;
    }
    None
}

fn cursor(world: &mut World, root: Entity, workspace: u64) -> Option<DVec3> {
    use bevy::picking::{hover::HoverMap, pointer::PointerId};
    let screen = world
        .query_filtered::<&Window, With<bevy::window::PrimaryWindow>>()
        .iter(world)
        .next()?
        .cursor_position()?;
    if let Some(hover) = world.get_resource::<HoverMap>()
        && let Some(hits) = hover.get(&PointerId::Mouse)
        && let Some((hit, _)) = hits
            .iter()
            .min_by(|(_, a), (_, b)| a.depth.total_cmp(&b.depth))
    {
        let mut current = Some(*hit);
        while let Some(entity) = current {
            if world
                .get::<crate::native::inspection::InspectionExcluded>(entity)
                .is_some()
                || world
                    .get::<crate::native::canvas_controls::CanvasToolbar>(entity)
                    .is_some()
            {
                return None;
            }
            current = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
    }
    if world
        .get::<super::presentation::SpatialRoot>(root)
        .is_some()
    {
        ray_point(world, root, workspace, screen)
    } else {
        crate::native::area_input::canvas_point(world, root, screen)
            .map(|p| DVec3::new(p.x, height(world, root, workspace, p), p.y))
    }
}

pub fn resolve_bindings(world: &mut World) {
    if world.query::<&Bound>().iter(world).next().is_none() {
        return;
    }
    let mut poses: HashMap<String, (Entity, Entity, u64, DVec3, DQuat)> = world
        .query::<(
            Entity,
            &crate::native::canvas_host::Identity,
            &crate::native::canvas::CanvasItem,
            &ChildOf,
            &crate::native::workspace::WorkspaceMember,
        )>()
        .iter(world)
        .map(|(e, id, item, parent, member)| {
            (
                id.0.clone(),
                (
                    e,
                    parent.parent(),
                    member.0,
                    super::spatial(world, e).position(item.position),
                    super::spatial(world, e).rotation(),
                ),
            )
        })
        .collect();
    for (body, position) in world
        .query::<(&super::physics::Body, &Position)>()
        .iter(world)
    {
        for member in &body.members {
            if let Some(id) = world.get::<crate::native::canvas_host::Identity>(*member)
                && let Some(pose) = poses.get_mut(&id.0)
            {
                pose.3 += position.0 - body.position;
            }
        }
    }
    let bindings: HashMap<_, _> = world
        .query::<(Entity, &Bound)>()
        .iter(world)
        .map(|(e, b)| (e, b.0.clone()))
        .collect();
    let mut pending: HashSet<_> = bindings.keys().copied().collect();
    let mut queue = VecDeque::new();
    let mut dependents = HashMap::<Entity, Vec<Entity>>::new();
    for (entity, binding) in &bindings {
        let dependency = match &binding.anchor {
            Anchor::Item(id) => poses
                .get(id)
                .filter(|p| {
                    bindings.contains_key(&p.0)
                        && world
                            .get::<ChildOf>(*entity)
                            .is_some_and(|parent| parent.parent() == p.1)
                        && world
                            .get::<crate::native::workspace::WorkspaceMember>(*entity)
                            .is_some_and(|member| member.0 == p.2)
                })
                .map(|p| p.0),
            _ => None,
        };
        if let Some(parent) = dependency {
            dependents.entry(parent).or_default().push(*entity);
        } else {
            queue.push_back(*entity);
        }
    }
    let mut resolved = HashMap::<Entity, (DVec3, DQuat, SpatialBinding)>::new();
    while let Some(entity) = queue.pop_front() {
        if let Some(children) = dependents.remove(&entity) {
            queue.extend(children);
        }
        let Some(parent) = world.get::<ChildOf>(entity) else {
            pending.remove(&entity);
            continue;
        };
        let root = parent.parent();
        let Some(workspace) = world
            .get::<crate::native::workspace::WorkspaceMember>(entity)
            .map(|m| m.0)
        else {
            pending.remove(&entity);
            continue;
        };
        if !super::holes::live(world, root, workspace)
            || world
                .get::<bevy::ecs::entity_disabling::Disabled>(root)
                .is_some()
            || crate::native::laboratory::suspended(world, root)
        {
            pending.remove(&entity);
            continue;
        }
        let mut binding = bindings[&entity].clone();
        let last = DVec3::from_array(binding.position);
        let rotation = DQuat::from_array(binding.rotation);
        let anchor = match &binding.anchor {
            Anchor::Fixed => Some((
                super::position(world, entity).unwrap_or(last),
                super::spatial(world, entity).rotation(),
            )),
            Anchor::Cursor => cursor(world, root, workspace).map(|p| (p, DQuat::IDENTITY)),
            Anchor::Camera => world.get::<super::view::View>(root).map(|v| {
                let mut point = DVec3::from_array(v.position);
                if !v.spatial {
                    if let Some(canvas) = world.get::<crate::native::canvas::CanvasView>(root) {
                        point.x = canvas.center.x;
                        point.z = canvas.center.y;
                    }
                }
                if binding.camera_floor {
                    point.y = height(world, root, workspace, DVec2::new(point.x, point.z));
                }
                (
                    point,
                    DQuat::from_euler(EulerRot::YXZ, f64::from(v.yaw), f64::from(v.pitch), 0.0),
                )
            }),
            Anchor::Item(id) => match poses.get(id).filter(|p| p.1 == root && p.2 == workspace) {
                Some(pose) if pending.contains(&pose.0) => continue,
                Some(pose) => Some((pose.3, pose.4)),
                None => {
                    binding.anchor = Anchor::Fixed;
                    binding.offset = [0.0; 3];
                    Some((last, rotation))
                }
            },
        };
        let (point, orientation) = if let Some((point, orientation)) = anchor {
            if binding.anchor == Anchor::Fixed {
                (point, rotation)
            } else {
                let turn = if binding.follow_orientation {
                    orientation
                } else {
                    DQuat::IDENTITY
                };
                let dragged = crate::native::canvas_pan::dragged(world) == Some(entity)
                    || world
                        .get_resource::<super::input::PointerState>()
                        .is_some_and(|p| p.drag.is_some_and(|(e, _)| e == entity));
                if dragged && let Some(current) = super::position(world, entity) {
                    binding.offset = (turn.inverse() * (current - point)).to_array();
                }
                (
                    point + turn * DVec3::from_array(binding.offset),
                    if binding.follow_orientation {
                        orientation
                    } else {
                        rotation
                    },
                )
            }
        } else {
            (last, rotation)
        };
        binding.position = point.to_array();
        binding.rotation = orientation.to_array();
        if let Some(id) = world.get::<crate::native::canvas_host::Identity>(entity)
            && let Some(pose) = poses.get_mut(&id.0)
        {
            pose.3 = point;
            pose.4 = orientation;
        }
        resolved.insert(entity, (point, orientation, binding));
        pending.remove(&entity);
    }
    for (entity, (point, rotation, binding)) in resolved {
        if !binding.valid() {
            continue;
        }
        if let Some(mut current) = world.get_mut::<Bound>(entity) {
            current.0 = binding;
        }
        if super::position(world, entity) != Some(point) {
            super::set_position(world, entity, point);
        }
        let mut spatial = super::spatial(world, entity);
        if spatial.rotation != rotation.to_array() {
            spatial.rotation = rotation.to_array();
            world.entity_mut(entity).insert(spatial);
        }
    }
}

pub fn prepare(world: &mut World) {
    if !world.contains_resource::<Terrain>() {
        return;
    }
    resolve_bindings(world);
    let inputs: Vec<_> = world
        .query::<(
            Entity,
            &Effect,
            Option<&Preview>,
            &crate::native::canvas::CanvasItem,
            &ChildOf,
            &crate::native::workspace::WorkspaceMember,
        )>()
        .iter(world)
        .filter(|(_, _, _, _, p, m)| super::holes::live(world, p.parent(), m.0))
        .filter(|(_, _, _, _, p, _)| {
            world
                .get::<bevy::ecs::entity_disabling::Disabled>(p.parent())
                .is_none()
                && !crate::native::laboratory::suspended(world, p.parent())
        })
        .map(|(entity, effect, preview, item, parent, member)| {
            let _ = preview;
            let definition = effect.0.clone();
            let heading = super::spatial(world, entity)
                .rotation()
                .to_euler(EulerRot::YXZ)
                .0;
            let id = world
                .get::<crate::native::canvas_host::Identity>(entity)
                .map_or_else(|| entity.to_bits().to_string(), |id| id.0.clone());
            (
                parent.parent(),
                member.0,
                Input {
                    id,
                    entity,
                    definition,
                    center: item.position,
                    heading,
                },
            )
        })
        .collect();
    let mut terrain = world.remove_resource::<Terrain>().unwrap();
    let mut previous = std::mem::take(&mut terrain.fields);
    for (root, workspace, input) in inputs {
        if !input.definition.enabled || input.definition.amplitude == 0.0 {
            continue;
        }
        if world
            .get::<Fault>(input.entity)
            .is_some_and(|fault| fault.1 == input.definition)
        {
            continue;
        }
        let existing = previous.get_mut(&(root, workspace)).and_then(|fields| {
            fields
                .iter()
                .position(|f| f.input.entity == input.entity)
                .map(|i| fields.swap_remove(i))
        });
        let formula = if let Some(existing) =
            existing.filter(|f| f.input.definition.same_surface(&input.definition))
        {
            existing.formula
        } else {
            world.entity_mut(input.entity).remove::<Fault>();
            match input.definition.compile() {
                Ok(formula) => formula,
                Err(error) => {
                    world
                        .entity_mut(input.entity)
                        .insert(Fault(error, input.definition.clone()));
                    continue;
                }
            }
        };
        terrain
            .fields
            .entry((root, workspace))
            .or_default()
            .push(Field { input, formula });
    }
    for fields in terrain.fields.values_mut() {
        fields.sort_by(|a, b| a.input.id.cmp(&b.input.id));
    }
    terrain.epoch += 1;
    world.insert_resource(terrain);
}

fn tile(terrain: &mut Terrain, world: &mut World, key: Key, collision: bool) {
    let inputs: Vec<_> = terrain
        .fields
        .get(&(key.0, key.1))
        .into_iter()
        .flatten()
        .filter(|f| affects(&f.input, key))
        .map(|f| f.input.clone())
        .collect();
    let changed = terrain.tiles.get(&key).is_none_or(|t| t.inputs != inputs);
    if changed {
        let fields: Vec<_> = terrain
            .fields
            .get(&(key.0, key.1))
            .into_iter()
            .flatten()
            .filter(|f| affects(&f.input, key))
            .collect();
        let mut heights = Vec::with_capacity((GRID + 1).pow(2));
        let mut errors = HashMap::new();
        for z in 0..=GRID {
            for x in 0..=GRID {
                let point =
                    key.minimum() + DVec2::new(x as f64, z as f64) * (TILE_SIZE / GRID as f64);
                let mut height = 0.0;
                for field in &fields {
                    terrain.evaluations += 1;
                    match field_height(field, point) {
                        Ok(value) => height += value,
                        Err(error) => {
                            errors.insert(
                                field.input.entity,
                                (error, field.input.definition.clone()),
                            );
                        }
                    }
                }
                heights.push(height);
            }
        }
        if !errors.is_empty() {
            for (entity, (error, definition)) in errors {
                world.entity_mut(entity).insert(Fault(error, definition));
            }
            return;
        }
        terrain.rebuilds += 1;
        let old = terrain.tiles.remove(&key);
        if collision {
            wake_changed(
                world,
                key,
                old.as_ref().map(|t| t.heights.as_slice()),
                &heights,
            );
        }
        if let Some(old) = old {
            destroy_tile(world, old);
        }
        terrain.tiles.insert(
            key,
            Tile {
                inputs,
                heights,
                collider: None,
                visual: None,
                used: terrain.epoch,
                clipping: None,
                depth: 0,
            },
        );
    }
    let Some(tile) = terrain.tiles.get_mut(&key) else {
        return;
    };
    tile.used = terrain.epoch;
    if collision && tile.collider.is_none() {
        let vertices = vertices(key, &tile.heights);
        let entity = world
            .spawn((
                Ground {
                    root: key.0,
                    workspace: key.1,
                },
                RigidBody::Static,
                Collider::trimesh(vertices, triangles()),
                Position(DVec3::ZERO),
                Rotation::IDENTITY,
                Friction::new(0.05),
                Restitution::new(0.0),
                ActiveCollisionHooks::FILTER_PAIRS,
                Transform::default(),
            ))
            .id();
        tile.collider = Some(entity);
        world.entity_mut(entity).insert(ColliderOf { body: entity });
    }
}

fn wake_changed(world: &mut World, key: Key, old: Option<&[f64]>, heights: &[f64]) {
    let mut minimum = DVec2::INFINITY;
    let mut maximum = DVec2::NEG_INFINITY;
    for (index, value) in heights.iter().enumerate() {
        if (value - old.map_or(0.0, |old| old[index])).abs() > 1e-6 {
            let point = key.minimum()
                + DVec2::new((index % (GRID + 1)) as f64, (index / (GRID + 1)) as f64)
                    * (TILE_SIZE / GRID as f64);
            minimum = minimum.min(point);
            maximum = maximum.max(point);
        }
    }
    if !minimum.is_finite() {
        return;
    }
    minimum -= DVec2::splat(TILE_SIZE / GRID as f64);
    maximum += DVec2::splat(TILE_SIZE / GRID as f64);
    let bodies: Vec<_> = world
        .query::<(Entity, &super::physics::Body, &Position)>()
        .iter(world)
        .filter(|(_, b, p)| {
            let radius = b
                .signature
                .iter()
                .map(|(member, size, _)| {
                    super::position(world, *member).map_or(0.0, |p| (p - b.position).length())
                        + f64::from(size.length()) * 0.5
                })
                .fold(0.0_f64, f64::max);
            b.root == key.0
                && b.workspace == key.1
                && !b.held
                && p.0.x + radius >= minimum.x
                && p.0.x - radius <= maximum.x
                && p.0.z + radius >= minimum.y
                && p.0.z - radius <= maximum.y
        })
        .map(|(e, _, _)| e)
        .collect();
    for body in bodies {
        world
            .entity_mut(body)
            .insert(SleepTimer(0.0))
            .remove::<Sleeping>();
    }
}

fn flat(terrain: &mut Terrain, world: &mut World, root: Entity, workspace: u64) {
    if terrain.flats.contains_key(&(root, workspace)) {
        return;
    }
    let ground = world
        .spawn((
            Ground { root, workspace },
            RigidBody::Static,
            Collider::half_space(DVec3::Y),
            Position(DVec3::ZERO),
            Rotation::IDENTITY,
            Transform::default(),
            Friction::new(0.05),
            Restitution::new(0.0),
            ActiveCollisionHooks::FILTER_PAIRS,
        ))
        .id();
    world.entity_mut(ground).insert(ColliderOf { body: ground });
    terrain.flats.insert((root, workspace), ground);
}

fn vertices(key: Key, heights: &[f64]) -> Vec<DVec3> {
    heights
        .iter()
        .enumerate()
        .map(|(i, height)| {
            let point = key.minimum()
                + DVec2::new((i % (GRID + 1)) as f64, (i / (GRID + 1)) as f64)
                    * (TILE_SIZE / GRID as f64);
            DVec3::new(point.x, *height, point.y)
        })
        .collect()
}

fn triangles() -> Vec<[u32; 3]> {
    let mut indices = Vec::with_capacity(GRID * GRID * 2);
    for z in 0..GRID {
        for x in 0..GRID {
            let a = (z * (GRID + 1) + x) as u32;
            let b = a + 1;
            let c = a + (GRID + 1) as u32;
            let d = c + 1;
            indices.extend([[a, c, b], [b, c, d]]);
        }
    }
    indices
}

fn destroy_tile(world: &mut World, tile: Tile) {
    if let Some(collider) = tile.collider {
        world.despawn(collider);
    }
    if let Some(visual) = tile.visual {
        drawing::destroy(world, visual);
    }
}

pub fn support(world: &mut World) -> bool {
    if !world.contains_resource::<Terrain>() {
        return false;
    }
    let mut bodies: Vec<_> = world
        .query::<(
            Entity,
            &super::physics::Body,
            &Position,
            Option<&LinearVelocity>,
        )>()
        .iter(world)
        .map(|(e, b, p, v)| {
            let radius = b
                .signature
                .iter()
                .map(|(member, size, _)| {
                    super::position(world, *member).map_or(0.0, |p| (p - b.position).length())
                        + size.length() as f64
                })
                .fold(32.0_f64, f64::max);
            (
                e,
                b.root,
                b.workspace,
                p.0,
                radius,
                v.map_or(DVec3::ZERO, |v| v.0),
            )
        })
        .collect();
    for (root, view, workspaces) in world
        .query::<(
            Entity,
            &super::view::View,
            &crate::native::workspace::Workspaces,
        )>()
        .iter(world)
    {
        if view.spatial {
            bodies.push((
                Entity::PLACEHOLDER,
                root,
                workspaces.active,
                DVec3::from_array(view.position),
                32.0,
                DVec3::ZERO,
            ));
        }
    }
    let mut terrain = world.remove_resource::<Terrain>().unwrap();
    let before = (terrain.rebuilds, terrain.flats.len(), terrain.tiles.len());
    let live: HashSet<_> = bodies.iter().map(|(_, r, w, _, _, _)| (*r, *w)).collect();
    let stale: Vec<_> = terrain
        .flats
        .iter()
        .filter(|(key, _)| !live.contains(key) || terrain.fields.contains_key(key))
        .map(|(k, e)| (*k, *e))
        .collect();
    for (key, e) in stale {
        world.despawn(e);
        terrain.flats.remove(&key);
    }
    for (root, workspace) in &live {
        if !terrain.fields.contains_key(&(*root, *workspace))
            && !terrain.flats.contains_key(&(*root, *workspace))
        {
            flat(&mut terrain, world, *root, *workspace);
        }
    }
    let mut requested = HashSet::new();
    for (body, root, workspace, point, radius, velocity) in &bodies {
        if !terrain.fields.contains_key(&(*root, *workspace)) {
            set_supported(world, *body, true);
            continue;
        }
        let center = DVec2::new(point.x, point.z);
        let future = center + DVec2::new(velocity.x, velocity.z) * 0.1;
        let minimum = center.min(future) - DVec2::splat(*radius);
        let maximum = center.max(future) + DVec2::splat(*radius);
        let first = (minimum / TILE_SIZE).floor().as_i64vec2();
        let last = (maximum / TILE_SIZE).floor().as_i64vec2();
        let count = (last.x - first.x + 1).saturating_mul(last.y - first.y + 1);
        if count > MAX_TILES as i64 {
            set_supported(world, *body, false);
            continue;
        }
        let keys: HashSet<_> = (first.y..=last.y)
            .flat_map(|z| (first.x..=last.x).map(move |x| Key(*root, *workspace, x, z)))
            .collect();
        if requested.len() + keys.difference(&requested).count() > MAX_TILES {
            set_supported(world, *body, false);
            continue;
        }
        set_supported(world, *body, true);
        requested.extend(keys);
    }
    for key in &requested {
        tile(&mut terrain, world, *key, true);
    }
    let mut failed = false;
    terrain.fields.retain(|_, fields| {
        fields.retain(|field| {
            let valid = !world
                .get::<Fault>(field.input.entity)
                .is_some_and(|f| f.1 == field.input.definition);
            failed |= !valid;
            valid
        });
        !fields.is_empty()
    });
    if failed {
        for key in &requested {
            if terrain.fields.contains_key(&(key.0, key.1)) {
                tile(&mut terrain, world, *key, true);
            }
        }
        for (root, workspace) in &live {
            if !terrain.fields.contains_key(&(*root, *workspace)) {
                flat(&mut terrain, world, *root, *workspace);
            }
        }
    }
    let obsolete: Vec<_> = terrain
        .tiles
        .keys()
        .filter(|k| !terrain.fields.contains_key(&(k.0, k.1)))
        .copied()
        .collect();
    for key in obsolete {
        if let Some(tile) = terrain.tiles.remove(&key) {
            wake_changed(
                world,
                key,
                Some(&tile.heights),
                &[0.0; (GRID + 1) * (GRID + 1)],
            );
            destroy_tile(world, tile);
        }
    }
    let unused: Vec<_> = terrain
        .tiles
        .keys()
        .filter(|key| !requested.contains(key))
        .copied()
        .collect();
    for key in unused {
        if let Some(tile) = terrain.tiles.remove(&key) {
            destroy_tile(world, tile);
        }
    }
    let changed = before != (terrain.rebuilds, terrain.flats.len(), terrain.tiles.len());
    world.insert_resource(terrain);
    correct_support(world, changed);
    changed
}

fn set_supported(world: &mut World, entity: Entity, supported: bool) {
    let Some(members) = world
        .get::<super::physics::Body>(entity)
        .map(|b| b.members.clone())
    else {
        return;
    };
    for member in members {
        if supported {
            world.entity_mut(member).remove::<Unsupported>();
        } else {
            world.entity_mut(member).insert(Unsupported);
        }
    }
    if !supported {
        world.entity_mut(entity).insert((
            RigidBody::Static,
            ConstantForce::default(),
            LinearVelocity::ZERO,
            Sleeping,
        ));
        if let Some(mut body) = world.get_mut::<super::physics::Body>(entity) {
            body.held = true;
        }
    }
}

fn correct_support(world: &mut World, terrain_changed: bool) {
    let bodies: Vec<_> = world
        .query::<(Entity, &super::physics::Body, &Position)>()
        .iter(world)
        .filter(|(_, body, _)| !body.held)
        .map(|(e, body, position)| {
            (
                e,
                body.root,
                body.workspace,
                body.members.clone(),
                position.0,
                body.position,
                body.rotation,
            )
        })
        .collect();
    for (entity, root, workspace, members, position, previous, rotation) in bodies {
        let mut lift = 0.0_f64;
        for member in &members {
            if world.get::<super::assets::ImportedAsset>(*member).is_some() {
                continue;
            }
            let Some(item) = world.get::<crate::native::canvas::CanvasItem>(*member) else {
                continue;
            };
            let spatial = super::spatial(world, *member);
            let scale = world
                .get::<crate::native::area_effects::AreaScale>(*member)
                .map_or(1.0, |s| f64::from(s.0));
            let center = spatial.position(item.position) + position - previous;
            for (x, z) in [
                (0.0, 0.0),
                (-0.5, -0.5),
                (-0.5, 0.5),
                (0.5, -0.5),
                (0.5, 0.5),
            ] {
                let foot = center
                    + spatial.rotation()
                        * DVec3::new(
                            f64::from(item.size.x) * x * scale,
                            -spatial.depth(item.size) * scale,
                            f64::from(item.size.y) * z * scale,
                        );
                lift =
                    lift.max(height(world, root, workspace, DVec2::new(foot.x, foot.z)) - foot.y);
            }
        }
        if lift > 2.0 || terrain_changed && lift > 0.05 {
            world
                .entity_mut(entity)
                .insert(Position(position + DVec3::Y * lift))
                .remove::<Sleeping>();
            if let Some(mut velocity) = world.get_mut::<LinearVelocity>(entity) {
                velocity.0.y = velocity.0.y.max(0.0);
            }
        }
        if members.len() == 1 && world.get::<FollowSlope>(members[0]).is_some() {
            let point = DVec2::new(position.x, position.z);
            let delta = TILE_SIZE / GRID as f64;
            let dx = (height(world, root, workspace, point + DVec2::X * delta)
                - height(world, root, workspace, point - DVec2::X * delta))
                / (2.0 * delta);
            let dz = (height(world, root, workspace, point + DVec2::Y * delta)
                - height(world, root, workspace, point - DVec2::Y * delta))
                / (2.0 * delta);
            let normal = DVec3::new(-dx, 1.0, -dz).normalize();
            let yaw = rotation.to_euler(EulerRot::YXZ).0;
            let next = DQuat::from_rotation_arc(DVec3::Y, normal) * DQuat::from_rotation_y(yaw);
            world.entity_mut(entity).insert(Rotation(next));
            let mut spatial = super::spatial(world, members[0]);
            spatial.rotation = next.to_array();
            world.entity_mut(members[0]).insert(spatial);
            if let Some(mut body) = world.get_mut::<super::physics::Body>(entity) {
                body.rotation = next;
            }
        }
    }
}

pub fn step(world: &mut World) {
    prepare(world);
    support(world);
}

pub fn draw(world: &mut World) {
    drawing::draw(world);
}

#[cfg(test)]
mod tests;
