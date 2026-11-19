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

mod inputs;
pub(super) use inputs::Keys;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, ExtractComponent)]

#[extract_app(bevy::render::RenderApp)]
pub(crate) struct Order {
    floating: bool,
    active: (bool, u8),
    index: i32,
    sibling: usize,
    part: i8,
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, ExtractComponent)]

#[extract_app(bevy::render::RenderApp)]
pub(crate) struct Plane([u64; 4]);

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

#[extract_app(bevy::render::RenderApp)]
pub(super) struct OrderedCamera;

#[derive(Component, Clone, Copy, PartialEq, Eq, ExtractComponent)]

#[extract_app(bevy::render::RenderApp)]
pub(super) struct DecorationAnchor(pub Entity);

#[cfg(test)]
fn key(world: &World, entity: Entity) -> Order {
    Keys::new(world).key(entity)
}

pub(super) fn synchronize(world: &mut World, visual: Entity, key: Order, plane: Plane) {
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

mod ordering;

fn sort(
    views: Query<&ExtractedView, With<OrderedCamera>>,
    orders: Query<(&Order, &Plane, &MainEntity, Option<&DecorationAnchor>)>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    mut prepared: Local<ordering::Preparation>,
) {
    let _ordering = nucleus::performance::Span::new("native-transparent-order");
    if orders.is_empty()
        || !views.iter().any(|view| {
            phases
                .get(&view.retained_view_entity)
                .is_some_and(|phase| phase.items.len() > 1)
        })
    {
        *prepared = Default::default();
        return;
    }
    for (order, plane, entity, anchor) in &orders {
        prepared.orders.insert(entity.id(), (*order, *plane));
        if let Some(anchor) = anchor {
            prepared.anchors.insert(entity.id(), (anchor.0, order.part));
        }
    }
    nucleus::performance::add(
        "native-stacking-prepared-orders",
        prepared.orders.len() as u64,
    );
    nucleus::performance::add(
        "native-stacking-prepared-anchors",
        prepared.anchors.len() as u64,
    );
    for view in &views {
        if let Some(phase) = phases.get_mut(&view.retained_view_entity) {
            prepared.apply(phase);
        }
    }
    prepared.clear_frame();
}

#[cfg(test)]
fn group_decorations(
    phase: &mut bevy::render::render_phase::SortedRenderPhase<Transparent3d>,
    anchors: &HashMap<Entity, (Entity, i8)>,
) {
    ordering::Decorations::default().apply(phase, anchors);
}

#[cfg(test)]
mod tests;
