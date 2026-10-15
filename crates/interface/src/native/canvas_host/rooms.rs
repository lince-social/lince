use super::*;
use crate::native::topology::rooms::Member;
use std::collections::HashSet;

pub(super) fn validate(
    world: &World,
    root: Entity,
    before: &api::Snapshot,
    after: &api::Snapshot,
) -> Result<(), String> {
    let mut counts = BTreeMap::<u64, usize>::new();
    let mut inherited = HashSet::new();
    let mut known = HashSet::new();
    for placement in &after.placements {
        known.insert(placement.id.as_str());
        if placement.metadata.room_member.is_some() {
            *counts.entry(placement.workspace).or_default() +=
                1 + super::kanban::room_members(&placement.component)?;
            inherited.insert(placement.id.as_str());
        }
    }
    for placement in &before.placements {
        known.insert(placement.id.as_str());
        if placement.metadata.room_member.is_some() {
            inherited.insert(placement.id.as_str());
        }
    }
    for entity in world.get::<Children>(root).into_iter().flatten() {
        if world.get::<Member>(*entity).is_none()
            || world
                .get::<Identity>(*entity)
                .is_some_and(|id| known.contains(id.0.as_str()))
        {
            continue;
        }
        let owner = authoring::owner(world, *entity);
        let owner_id = world.get::<Identity>(owner).map(|id| id.0.as_str());
        if owner != *entity
            && (world.get::<crate::native::kanban::Kanban>(owner).is_some()
                && world
                    .get::<crate::native::area::InfluenceArea>(*entity)
                    .is_some()
                && owner_id.is_some_and(|id| inherited.contains(id))
                || owner_id.is_some_and(|id| {
                    before.placements.iter().any(|placement| placement.id == id)
                        && !after.placements.iter().any(|placement| placement.id == id)
                }))
        {
            continue;
        }
        if let Some(workspace) = world.get::<WorkspaceMember>(*entity) {
            *counts.entry(workspace.0).or_default() += 1;
        }
    }
    if counts.values().any(|count| *count > api::room::MAX_MEMBERS) {
        return Err("A workspace supports at most 256 room members, including owned Kanban regions and Record overrides.".into());
    }
    Ok(())
}
