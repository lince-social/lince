use super::*;
use crate::layout::desktop::{Solution, Workspace};

#[derive(Component, Clone, Copy)]
pub(super) struct Placement {
    pub parent: Entity,
    pub offset: bevy::math::DVec2,
}

mod inheritance;
mod placement;

pub(super) fn inherit(world: &mut World) {
    inheritance::inherit(world);
}

pub(super) fn descendant(world: &World, mut entity: Entity, ancestor: Entity) -> bool {
    for _ in 0..32 {
        let Some(parent) = world.get::<TransientParent>(entity) else {
            return false;
        };
        if parent.0 == ancestor {
            return true;
        }
        if parent.0 == entity {
            return false;
        }
        entity = parent.0;
    }
    false
}

pub(super) fn minimized(world: &World, mut entity: Entity, model: &Workspace) -> bool {
    for _ in 0..32 {
        if world.get::<RestoredTransient>(entity).is_some() {
            return true;
        }
        if world
            .get::<LayoutBox>(entity)
            .is_some_and(|layout| model.hidden(PlacementId(layout.id)))
        {
            return true;
        }
        let Some(parent) = world.get::<TransientParent>(entity) else {
            return false;
        };
        if parent.0 == entity {
            return true;
        }
        entity = parent.0;
    }
    true
}

pub(super) fn place(
    world: &World,
    entities: &BTreeMap<PlacementId, Entity>,
    model: &mut Workspace,
    solution: &mut Solution,
    usable: Rectangle,
) {
    place_with(world, entities, model, solution, usable, None);
}

pub(super) fn place_with(
    world: &World,
    entities: &BTreeMap<PlacementId, Entity>,
    model: &mut Workspace,
    solution: &mut Solution,
    usable: Rectangle,
    preview: Option<(Entity, Placement)>,
) {
    placement::place_with(world, entities, model, solution, usable, preview);
}

#[cfg(test)]
mod tests;
