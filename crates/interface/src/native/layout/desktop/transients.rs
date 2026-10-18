use super::*;
use crate::layout::desktop::{Solution, SolvedItem, Workspace};

pub(super) fn inherit(world: &mut World) {
    let transients: Vec<_> = world
        .query::<(Entity, &TransientParent)>()
        .iter(world)
        .map(|(entity, parent)| (entity, parent.0))
        .collect();
    for (entity, parent) in transients {
        if entity == parent || world.get_entity(parent).is_err() {
            world.entity_mut(entity).remove::<TransientParent>();
            continue;
        }
        if let Some(member) = world.get::<WorkspaceMember>(parent).copied()
            && world.get::<WorkspaceMember>(entity).map(|member| member.0) != Some(member.0)
        {
            world.entity_mut(entity).insert(member);
        }
    }
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
        if world.get::<LayoutBox>(entity).is_some_and(|layout| model.minimized(PlacementId(layout.id))) { return true; }
        let Some(parent) = world.get::<TransientParent>(entity) else { return false };
        if parent.0 == entity { return true; }
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
    solution.items.retain(|item| entities.get(&item.id).is_none_or(|entity| !minimized(world, *entity, model)));
    let transients: Vec<_> = entities
        .iter()
        .filter_map(|(id, entity)| {
            if minimized(world, *entity, model) { return None; }
            let parent = world.get::<TransientParent>(*entity)?.0;
            let parent_id = PlacementId(world.get::<LayoutBox>(parent)?.id);
            Some((*id, parent_id))
        })
        .collect();
    let mut remaining = transients;
    for _ in 0..32 {
        let previous = remaining.len();
        remaining.retain(|(id, parent)| {
            let Some(bounds) = solution
                .items
                .iter()
                .find(|item| item.id == *parent)
                .map(|item| item.rectangle)
            else {
                return true;
            };
            let Some(item) = model.items().iter().find(|item| item.id == *id) else {
                return false;
            };
            let size = std::array::from_fn(|axis| item.free.size[axis].min(usable.size()[axis]));
            let origin = std::array::from_fn(|axis| {
                (bounds.origin()[axis] + (bounds.size()[axis] - size[axis]) * 0.5).clamp(
                    usable.origin()[axis],
                    (usable.origin()[axis] + usable.size()[axis] - size[axis])
                        .max(usable.origin()[axis]),
                )
            });
            let Some(rectangle) = Rectangle::new(origin, size) else {
                return false;
            };
            let _ = model.set_free_geometry(*id, Geometry { origin, size });
            if let Some(item) = solution.items.iter_mut().find(|item| item.id == *id) {
                item.rectangle = rectangle;
                item.floating = true;
            } else {
                solution.items.push(SolvedItem {
                    id: *id,
                    rectangle,
                    floating: true,
                });
            }
            if model.focused() == Some(*id) {
                solution.focused = Some(*id);
            }
            false
        });
        if remaining.len() == previous || remaining.is_empty() {
            break;
        }
    }
}
