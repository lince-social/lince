use bevy::prelude::Entity;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn sources_first(mut dependencies: BTreeMap<Entity, BTreeSet<Entity>>) -> Vec<Entity> {
    if dependencies.len() > crate::host::capture::MAX_CONTRIBUTORS
        || dependencies.values().map(BTreeSet::len).sum::<usize>() > crate::host::capture::MAX_CONTRIBUTORS
    { return Vec::new(); }
    let mut consumers: BTreeMap<Entity, BTreeSet<Entity>> = BTreeMap::new();
    for (entity, inputs) in &dependencies {
        for input in inputs { consumers.entry(*input).or_default().insert(*entity); }
    }
    let mut ready: BTreeSet<_> = dependencies.iter().filter_map(|(entity, inputs)| inputs.is_empty().then_some(*entity)).collect();
    let mut ordered = Vec::with_capacity(dependencies.len());
    while let Some(entity) = ready.pop_first() {
        ordered.push(entity);
        if let Some(users) = consumers.remove(&entity) {
            for user in users {
                let Some(inputs) = dependencies.get_mut(&user) else { continue; };
                inputs.remove(&entity);
                if inputs.is_empty() { ready.insert(user); }
            }
        }
    }
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::World;

    #[test]
    fn filtered_preview_dependencies_precede_their_consumers_and_cycles_cannot_gain_a_completion() {
        let mut world = World::new();
        let outer = world.spawn_empty().id();
        let source = world.spawn_empty().id();
        let cycle = world.spawn_empty().id();
        let blocked = world.spawn_empty().id();
        let ordered = sources_first(BTreeMap::from([
            (outer, BTreeSet::from([source])), (source, BTreeSet::new()),
            (cycle, BTreeSet::from([cycle])), (blocked, BTreeSet::from([cycle])),
        ]));
        assert_eq!(ordered, [source, outer]);
        assert!(sources_first(BTreeMap::from([(outer, BTreeSet::from([source])), (source, BTreeSet::from([outer]))])).is_empty());
    }
}
