use super::*;
use bevy::ecs::change_detection::Tick;

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

fn revision(world: &World, row: Entity) -> Option<Revision> {
    let entity = world.get_entity(row).ok()?;
    let binding = entity.get_ref::<RecordBinding>()?;
    Some(Revision {
        row: entity.get_ref::<super::super::rows::Row>()?.last_changed(),
        binding: binding.last_changed(),
        state: entity.get_ref::<State>().map(|state| state.last_changed()),
        source: world
            .get_entity(binding.area)
            .ok()?
            .get_ref::<InfluenceArea>()?
            .last_changed(),
        declaration: entity
            .get_ref::<crate::native::sand_settings::Declaration>()
            .map(|declaration| declaration.last_changed()),
        children: entity
            .get_ref::<Children>()
            .map(|children| children.last_changed()),
        original: entity
            .get_ref::<super::fields::Original>()
            .map(|original| original.last_changed()),
    })
}

pub(super) fn unchanged(world: &World, row: Entity) -> bool {
    world.get::<Observed>(row).is_some_and(|observed| {
        observed.at != world.read_change_tick()
            && revision(world, row).as_ref() == Some(&observed.revision)
    })
}

pub(super) fn remember(world: &mut World, row: Entity) {
    if let Some(revision) = revision(world, row) {
        let at = world.read_change_tick();
        let next = Observed { at, revision };
        if let Some(mut observed) = world.get_mut::<Observed>(row) {
            *observed = next;
        } else {
            world.entity_mut(row).insert(next);
        }
    }
}

pub(super) fn forget(world: &mut World, row: Entity) {
    if world.get::<Observed>(row).is_some() {
        world.entity_mut(row).remove::<Observed>();
    }
}
