use bevy::{
    camera::{RenderTarget, visibility::{CascadesVisibleEntities, CubemapVisibleEntities, VisibleEntities, VisibleMeshEntities}},
    ecs::{entity_disabling::Disabled, query::Allow},
    prelude::*,
};
use std::collections::BTreeSet;

pub struct Cache {
    surfaces: crate::native::topology::surface_render::ExternalCaptureCache,
    appearance: crate::native::topology::compositor::CaptureCache,
}

#[derive(Resource)]
pub struct ExternalPass;

#[derive(Component, Clone, Copy)]
pub struct TargetSource(pub Entity);

mod targets;
pub use targets::{Targets, reusable_targets, target_ids};
pub(crate) mod aliases;

pub fn begin_cache(world: &mut World) -> Cache {
    Cache {
        surfaces: crate::native::topology::surface_render::begin_external_capture(world),
        appearance: crate::native::topology::compositor::begin_capture(world),
    }
}

pub fn end_cache(world: &mut World, cache: Cache) {
    crate::native::topology::surface_render::end_external_capture(world, cache.surfaces);
    crate::native::topology::compositor::end_capture(world, cache.appearance);
}

pub struct Filter {
    visibility: Vec<(Entity, Visibility)>,
    inherited: Vec<(Entity, InheritedVisibility)>,
    views: Vec<(Entity, ViewVisibility)>,
    cameras: Vec<(Entity, bool)>,
    visible: Vec<(Entity, VisibleEntities)>,
    meshes: Vec<(Entity, VisibleMeshEntities)>,
    cubemaps: Vec<(Entity, CubemapVisibleEntities)>,
    cascades: Vec<(Entity, CascadesVisibleEntities)>,
}

impl Filter {
    pub fn apply(world: &mut World, additional: &BTreeSet<Entity>) -> Result<Self, &'static str> {
        let entities: Vec<_> = world.query_filtered::<Entity, Allow<Disabled>>().iter(world).take(crate::host::capture::MAX_CONTRIBUTORS + 1).collect();
        if entities.len() > crate::host::capture::MAX_CONTRIBUTORS { return Err("The capture contributor limit was exceeded."); }
        let private: BTreeSet<_> = entities.into_iter().filter(|entity| super::excluded_with(world, *entity, additional)).collect();
        let mut filter = Self { visibility: Vec::new(), inherited: Vec::new(), views: Vec::new(), cameras: Vec::new(), visible: Vec::new(), meshes: Vec::new(), cubemaps: Vec::new(), cascades: Vec::new() };
        for entity in &private {
            if let Some(mut visibility) = world.get_mut::<Visibility>(*entity) {
                filter.visibility.push((*entity, *visibility));
                *visibility = Visibility::Hidden;
            }
            if let Some(mut visibility) = world.get_mut::<InheritedVisibility>(*entity) {
                filter.inherited.push((*entity, *visibility));
                *visibility = InheritedVisibility::HIDDEN;
            }
            if let Some(mut visibility) = world.get_mut::<ViewVisibility>(*entity) {
                filter.views.push((*entity, *visibility));
                *visibility = ViewVisibility::HIDDEN;
            }
            if let Some(mut camera) = world.get_mut::<Camera>(*entity) {
                filter.cameras.push((*entity, camera.is_active));
                camera.is_active = false;
            }
        }
        for (entity, mut visible) in world.query_filtered::<(Entity, &mut VisibleEntities), Allow<Disabled>>().iter_mut(world) {
            if visible.entities.values().any(|entities| entities.iter().any(|entity| private.contains(entity))) {
                filter.visible.push((entity, visible.clone()));
                for entities in visible.entities.values_mut() { entities.retain(|entity| !private.contains(entity)); }
            }
        }
        for (entity, mut visible) in world.query_filtered::<(Entity, &mut VisibleMeshEntities), Allow<Disabled>>().iter_mut(world) {
            if visible.entities.iter().any(|entity| private.contains(entity)) {
                filter.meshes.push((entity, visible.clone()));
                visible.entities.retain(|entity| !private.contains(entity));
            }
        }
        for (entity, mut visible) in world.query_filtered::<(Entity, &mut CubemapVisibleEntities), Allow<Disabled>>().iter_mut(world) {
            if visible.iter().any(|face| face.entities.iter().any(|entity| private.contains(entity))) {
                filter.cubemaps.push((entity, visible.clone()));
                for face in visible.iter_mut() { face.entities.retain(|entity| !private.contains(entity)); }
            }
        }
        for (entity, mut visible) in world.query_filtered::<(Entity, &mut CascadesVisibleEntities), Allow<Disabled>>().iter_mut(world) {
            if visible.entities.values().flatten().any(|cascade| cascade.entities.iter().any(|entity| private.contains(entity))) {
                filter.cascades.push((entity, visible.clone()));
                for cascade in visible.entities.values_mut().flatten() { cascade.entities.retain(|entity| !private.contains(entity)); }
            }
        }
        Ok(filter)
    }

    pub fn restore(self, world: &mut World) {
        for (entity, value) in self.visibility { if let Some(mut current) = world.get_mut::<Visibility>(entity) { *current = value; } }
        for (entity, value) in self.inherited { if let Some(mut current) = world.get_mut::<InheritedVisibility>(entity) { *current = value; } }
        for (entity, value) in self.views { if let Some(mut current) = world.get_mut::<ViewVisibility>(entity) { *current = value; } }
        for (entity, value) in self.cameras { if let Some(mut current) = world.get_mut::<Camera>(entity) { current.is_active = value; } }
        for (entity, value) in self.visible { if let Some(mut current) = world.get_mut::<VisibleEntities>(entity) { *current = value; } }
        for (entity, value) in self.meshes { if let Some(mut current) = world.get_mut::<VisibleMeshEntities>(entity) { *current = value; } }
        for (entity, value) in self.cubemaps { if let Some(mut current) = world.get_mut::<CubemapVisibleEntities>(entity) { *current = value; } }
        for (entity, value) in self.cascades { if let Some(mut current) = world.get_mut::<CascadesVisibleEntities>(entity) { *current = value; } }
    }
}

#[cfg(test)]
mod tests;
