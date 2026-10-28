use bevy::{
    core_pipeline::core_3d::Transparent3d,
    prelude::*,
    render::{
        Render, RenderSystems,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        render_phase::{ViewSortedRenderPhases, sort_phase_system},
        sync_world::MainEntity,
        view::ExtractedView,
    },
};
use std::collections::HashMap;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, ExtractComponent)]
pub(crate) struct Order {
    floating: bool,
    active: (bool, u8),
    index: i32,
    sibling: usize,
    part: i8,
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, ExtractComponent)]
pub(super) struct Plane([u64; 4]);

impl Plane {
    pub(super) fn new(normal: bevy::math::DVec3, point: bevy::math::DVec3) -> Self {
        let normal = normal.normalize();
        Self(
            [normal.x, normal.y, normal.z, normal.dot(point)]
                .map(|value| if value == 0.0 { 0 } else { value.to_bits() }),
        )
    }
}

#[derive(Component, Clone, Copy, ExtractComponent)]
pub(super) struct OrderedCamera;

#[derive(Component, Clone, Copy, PartialEq, Eq, ExtractComponent)]
pub(super) struct DecorationAnchor(pub Entity);

pub(super) fn key(world: &World, entity: Entity) -> Order {
    Order {
        floating: world
            .get::<crate::native::layout::desktop::Managed>(entity)
            .is_none_or(|managed| managed.floating),
        active: active_family(world, entity).unwrap_or_default(),
        index: world.get::<ZIndex>(entity).map_or(0, |index| index.0),
        sibling: world
            .get::<ChildOf>(entity)
            .and_then(|parent| world.get::<Children>(parent.parent()))
            .and_then(|children| children.iter().position(|child| child == entity))
            .unwrap_or(0),
        part: 0,
    }
}

fn active_family(world: &World, entity: Entity) -> Option<(bool, u8)> {
    use crate::native::{layout::desktop, workspace};
    world.get::<desktop::ApplicationWindow>(entity)?;
    if !world.get::<desktop::Managed>(entity)?.floating {
        return None;
    }
    let root = world.get::<ChildOf>(entity)?.parent();
    let workspace = world.get::<workspace::WorkspaceMember>(entity)?.0;
    let focused = world
        .get::<workspace::Workspaces>(root)?
        .entries
        .iter()
        .find(|entry| entry.id == workspace)?
        .desktop
        .focused()?;
    let target = world.get::<Children>(root)?.iter().find(|entity| {
        world
            .get::<desktop::Managed>(*entity)
            .is_some_and(|managed| managed.id == focused)
            && world.get::<desktop::ApplicationWindow>(*entity).is_some()
    })?;
    let family = |mut entity| {
        for depth in 0..32 {
            if world.get::<ChildOf>(entity)?.parent() != root
                || world.get::<workspace::WorkspaceMember>(entity)?.0 != workspace
            {
                return None;
            }
            let Some(parent) = world.get::<desktop::TransientParent>(entity) else {
                return Some((entity, depth));
            };
            entity = parent.0;
        }
        None
    };
    let (ancestor, depth) = family(entity)?;
    (ancestor == family(target)?.0).then_some((true, depth))
}

pub(super) fn synchronize(world: &mut World, owner: Entity, visual: Entity, plane: Plane) {
    let key = key(world, owner);
    if world.get::<Order>(visual) != Some(&key) {
        world.entity_mut(visual).insert(key);
    }
    if world.get::<Plane>(visual) != Some(&plane) {
        world.entity_mut(visual).insert(plane);
    }
}

pub(super) fn decoration(world: &mut World, face: Entity, decoration: Entity, part: i8) {
    if world.get::<DecorationAnchor>(decoration) != Some(&DecorationAnchor(face)) {
        world.entity_mut(decoration).insert(DecorationAnchor(face));
    }
    if let Some(mut order) = world.get::<Order>(face).copied() {
        order.part = part;
        if world.get::<Order>(decoration) != Some(&order) { world.entity_mut(decoration).insert(order); }
    }
    if let Some(plane) = world.get::<Plane>(face).copied()
        && world.get::<Plane>(decoration) != Some(&plane) { world.entity_mut(decoration).insert(plane); }
}

pub(super) fn camera(world: &mut World, entity: Entity) {
    if world.get::<OrderedCamera>(entity).is_none() {
        world.entity_mut(entity).insert(OrderedCamera);
    }
}

pub(super) fn install(app: &mut App) {
    app.add_plugins((
        ExtractComponentPlugin::<Order>::default(),
        ExtractComponentPlugin::<Plane>::default(),
        ExtractComponentPlugin::<OrderedCamera>::default(),
        ExtractComponentPlugin::<DecorationAnchor>::default(),
    ));
    let Some(render) = app.get_sub_app_mut(bevy::render::RenderApp) else {
        return;
    };
    render.add_systems(
        Render,
        sort.in_set(RenderSystems::PhaseSort)
            .after(sort_phase_system::<Transparent3d>),
    );
}

fn sort(
    views: Query<&ExtractedView, With<OrderedCamera>>,
    orders: Query<(&Order, &Plane, &MainEntity, Option<&DecorationAnchor>)>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
) {
    if orders.is_empty() {
        return;
    }
    let anchors: HashMap<_, _> = orders.iter().filter_map(|(order, _, entity, anchor)| {
        anchor.map(|anchor| (entity.id(), (anchor.0, order.part)))
    }).collect();
    let orders: HashMap<_, _> = orders
        .iter()
        .map(|(order, plane, entity, _)| (entity.id(), (*order, *plane)))
        .collect();
    for view in &views {
        let Some(phase) = phases.get_mut(&view.retained_view_entity) else {
            continue;
        };
        let mut groups = HashMap::<Plane, Vec<_>>::new();
        for (index, (entity, item)) in phase.items.iter().enumerate() {
            if item.distance.is_finite()
                && let Some((order, plane)) = orders.get(&entity.1.id())
            {
                groups
                    .entry(*plane)
                    .or_default()
                    .push((index, *entity, *order));
            }
        }
        if groups
            .values()
            .all(|group| group.windows(2).all(|pair| pair[0].2 <= pair[1].2))
        {
            group_decorations(phase, &anchors);
            continue;
        }
        let mut ranks: HashMap<_, _> = phase
            .items
            .keys()
            .enumerate()
            .map(|(index, entity)| (*entity, index))
            .collect();
        for mut group in groups
            .into_values()
            .filter(|group| group.windows(2).any(|pair| pair[0].2 > pair[1].2))
        {
            let slots: Vec<_> = group.iter().map(|entry| entry.0).collect();
            group.sort_by_key(|entry| entry.2);
            for (slot, (_, entity, _)) in slots.into_iter().zip(group) {
                ranks.insert(entity, slot);
            }
        }
        phase.items.sort_by_key(|entity, _| ranks[entity]);
        group_decorations(phase, &anchors);
    }
}

fn group_decorations(
    phase: &mut bevy::render::render_phase::SortedRenderPhase<Transparent3d>,
    anchors: &HashMap<Entity, (Entity, i8)>,
) {
    if anchors.is_empty() { return; }
    let original: HashMap<_, _> = phase.items.keys().enumerate().map(|(index, key)| (key.1.id(), index)).collect();
    let ranks: HashMap<_, _> = phase.items.keys().enumerate().map(|(index, key)| {
        let rank = anchors.get(&key.1.id()).and_then(|(face, part)| original.get(face).map(|index| (*index, *part)))
            .unwrap_or((index, 0));
        (*key, rank)
    }).collect();
    phase.items.sort_by_key(|entity, _| ranks[entity]);
}

#[cfg(test)]
mod tests;
