use bevy::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

pub(crate) struct Children<K> {
    entities: HashMap<K, Entity>,
}

impl<K> Default for Children<K> {
    fn default() -> Self {
        Self {
            entities: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash + Clone> Children<K> {
    pub(crate) fn reconcile<T>(
        &mut self,
        world: &mut World,
        parent: Entity,
        rows: impl IntoIterator<Item = (K, T)>,
        mut create: impl FnMut(&mut World, Entity, &T) -> Entity,
        mut update: impl FnMut(&mut World, Entity, &T),
    ) {
        let mut live = HashSet::new();
        let mut ordered = Vec::new();
        for (key, value) in rows {
            if !live.insert(key.clone()) {
                continue;
            }
            let entity = self.entities.get(&key).copied().filter(|entity| {
                world
                    .get::<ChildOf>(*entity)
                    .is_some_and(|child| child.parent() == parent)
            });
            let entity = if let Some(entity) = entity {
                update(world, entity, &value);
                entity
            } else {
                let entity = create(world, parent, &value);
                self.entities.insert(key, entity);
                entity
            };
            ordered.push(entity);
        }
        self.entities.retain(|key, entity| {
            if live.contains(key) {
                return true;
            }
            if world
                .get::<ChildOf>(*entity)
                .is_some_and(|child| child.parent() == parent)
            {
                world.despawn(*entity);
            }
            false
        });
        let owned: HashSet<_> = ordered.iter().copied().collect();
        if let Some(children) = world.get::<bevy::prelude::Children>(parent) {
            ordered.extend(children.iter().filter(|child| !owned.contains(child)));
            if children.iter().eq(ordered.iter().copied()) {
                return;
            }
        }
        world.entity_mut(parent).replace_children(&ordered);
    }
}

pub(crate) mod tests {
    use super::*;

    #[derive(Component, PartialEq)]
    struct Value(i32);

    #[cfg_attr(test, test)]
    fn retained_children_preserve_identity_order_and_independent_ownership() {
        let mut world = World::new();
        crate::native::laboratory::isolate(&mut world);
        let parent = world.spawn_empty().id();
        let other = world.spawn_empty().id();
        let extra = world.spawn(ChildOf(parent)).id();
        let mut children = Children::default();
        let update = |world: &mut World, entity, value: &i32| {
            world
                .get_mut::<Value>(entity)
                .unwrap()
                .set_if_neq(Value(*value));
        };
        let create = |world: &mut World, parent, value: &i32| {
            world.spawn((Value(*value), ChildOf(parent))).id()
        };
        children.reconcile(&mut world, parent, [(1, 10), (2, 20)], create, update);
        let first = children.entities[&1];
        let second = children.entities[&2];
        let tick = world
            .entity(first)
            .get_ref::<Value>()
            .unwrap()
            .last_changed();
        world.increment_change_tick();
        children.reconcile(
            &mut world,
            parent,
            [(2, 21), (1, 10), (1, 90)],
            create,
            update,
        );
        assert_eq!(children.entities[&1], first);
        assert_eq!(children.entities[&2], second);
        assert_eq!(
            world
                .entity(first)
                .get_ref::<Value>()
                .unwrap()
                .last_changed(),
            tick
        );
        assert_eq!(world.get::<Value>(second).unwrap().0, 21);
        assert_eq!(
            world
                .get::<bevy::prelude::Children>(parent)
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            [second, first, extra]
        );
        world.entity_mut(first).insert(ChildOf(other));
        world.despawn(second);
        children.reconcile(&mut world, parent, [(1, 12), (2, 22)], create, update);
        assert_ne!(children.entities[&1], first);
        assert_ne!(children.entities[&2], second);
        children.reconcile(&mut world, parent, [], create, update);
        assert_eq!(
            world
                .get::<bevy::prelude::Children>(parent)
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            [extra]
        );
        assert_eq!(world.get::<ChildOf>(first).unwrap().parent(), other);
    }

    crate::native::laboratory_cases! {
        retained_children_preserve_identity_order_and_independent_ownership,
    }
}
