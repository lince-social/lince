//! Change-tick tracking for retained Protein row presentations.

use super::{State, fields::Original};
use crate::native::{
    area::InfluenceArea,
    protein_area::{RecordBinding, rows::Row},
    sand_settings::Declaration,
};
use bevy::{
    ecs::change_detection::Tick,
    prelude::{Children, Component, DetectChanges, Entity, EntityRef, World},
};

#[derive(Component)]
struct Observed {
    at: Tick,
    revision: Revision,
}

#[derive(Eq, PartialEq)]
struct Revision {
    row: Tick,
    binding: Tick,
    state: Option<Tick>,
    source: Tick,
    declaration: Option<Tick>,
    children: Option<Tick>,
    original: Option<Tick>,
}

fn changed_at<T: Component>(entity: &EntityRef<'_>) -> Option<Tick> {
    entity
        .get_ref::<T>()
        .map(|component| component.last_changed())
}

fn revision(world: &World, entity: EntityRef<'_>) -> Option<Revision> {
    let binding = entity.get_ref::<RecordBinding>()?;
    Some(Revision {
        row: changed_at::<Row>(&entity)?,
        binding: binding.last_changed(),
        state: changed_at::<State>(&entity),
        source: changed_at::<InfluenceArea>(&world.get_entity(binding.area).ok()?)?,
        declaration: changed_at::<Declaration>(&entity),
        children: changed_at::<Children>(&entity),
        original: changed_at::<Original>(&entity),
    })
}

pub(super) fn unchanged(world: &World, row: Entity) -> bool {
    let Ok(entity) = world.get_entity(row) else {
        return false;
    };
    entity.get::<Observed>().is_some_and(|observed| {
        // Another edit in the same tick can share the recorded change ticks.
        observed.at != world.read_change_tick()
            && revision(world, entity).as_ref() == Some(&observed.revision)
    })
}

pub(super) fn remember(world: &mut World, row: Entity) {
    let at = world.read_change_tick();
    if let Ok(entity) = world.get_entity(row)
        && let Some(revision) = revision(world, entity)
        && let Ok(mut entity) = world.get_entity_mut(row)
    {
        let next = Observed { at, revision };
        if let Some(mut observed) = entity.get_mut::<Observed>() {
            *observed = next;
        } else {
            entity.insert(next);
        }
    }
}

pub(super) fn forget(world: &mut World, row: Entity) {
    if let Ok(mut entity) = world.get_entity_mut(row)
        && entity.contains::<Observed>()
    {
        entity.remove::<Observed>();
    }
}
