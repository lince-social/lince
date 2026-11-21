//! Retain native presentation state and reconcile owned children in display order.

use crate::native::theme::Typography;
use bevy::prelude::{
    ChildOf, Children as ChildEntities, Component, Entity, Font, Handle, RelationshipTarget, World,
};
use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    hash::Hash,
    mem::take,
};

#[derive(Component)]
pub(crate) struct Presentation<T: Send + Sync + 'static> {
    state: T,
    children: Vec<Entity>,
    font: Handle<Font>,
}

impl<T: Send + Sync + 'static> Presentation<T> {
    pub(crate) fn state(world: &World, entity: Entity) -> Option<&T> {
        world.get::<Self>(entity).map(|presented| &presented.state)
    }

    pub(crate) fn update_state(world: &mut World, entity: Entity, update: impl FnOnce(&mut T)) {
        if let Some(mut presented) = world.get_mut::<Self>(entity) {
            update(&mut presented.state);
        }
    }

    pub(crate) fn matches(world: &World, entity: Entity, matches: impl FnOnce(&T) -> bool) -> bool {
        world.get::<Self>(entity).is_some_and(|old| {
            matches(&old.state)
                && world
                    .get_resource::<Typography>()
                    .is_some_and(|theme| old.font == theme.0)
                && world.get::<ChildEntities>(entity).map_or_else(
                    || old.children.is_empty(),
                    |children| children.iter().eq(old.children.iter().copied()),
                )
        })
    }

    pub(crate) fn remember(world: &mut World, entity: Entity, state: T) {
        let Some(font) = world
            .get_resource::<Typography>()
            .map(|theme| theme.0.clone())
        else {
            return;
        };
        let Ok(mut entity) = world.get_entity_mut(entity) else {
            return;
        };
        let children = entity
            .get::<ChildEntities>()
            .into_iter()
            .flat_map(|children| children.iter())
            .collect();
        entity.insert(Self {
            state,
            children,
            font,
        });
    }
}

pub(crate) struct Children<K> {
    entities: HashMap<K, Child>,
    reconciling: bool,
}

struct Child {
    entity: Entity,
    seen: bool,
}

impl<K> Default for Children<K> {
    fn default() -> Self {
        Self {
            entities: HashMap::new(),
            reconciling: false,
        }
    }
}

impl<K: Eq + Hash> Children<K> {
    pub(crate) fn reconcile<T>(
        &mut self,
        world: &mut World,
        parent: Entity,
        rows: impl IntoIterator<Item = (K, T)>,
        mut create: impl FnMut(&mut World, Entity, &T) -> Entity,
        mut update: impl FnMut(&mut World, Entity, &T),
    ) {
        if self.reconciling {
            // A callback may have unwound before the previous pass reset its marks.
            for child in self.entities.values_mut() {
                child.seen = false;
            }
        }
        self.reconciling = true;
        let mut ordered = Vec::new();
        for (key, value) in rows {
            let child = match self.entities.entry(key) {
                Entry::Occupied(entry) => {
                    let child = entry.into_mut();
                    if child.seen {
                        continue;
                    }
                    if owned(world, parent, child.entity) {
                        update(world, child.entity, &value);
                    } else {
                        child.entity = create(world, parent, &value);
                    }
                    child
                }
                Entry::Vacant(entry) => entry.insert(Child {
                    entity: create(world, parent, &value),
                    seen: false,
                }),
            };
            child.seen = true;
            ordered.push(child.entity);
        }
        self.entities.retain(|_, child| {
            if take(&mut child.seen) {
                return true;
            }
            if owned(world, parent, child.entity) {
                world.despawn(child.entity);
            }
            false
        });
        self.reconciling = false;
        order(world, parent, ordered);
    }
}

fn owned(world: &World, parent: Entity, entity: Entity) -> bool {
    world
        .get::<ChildOf>(entity)
        .is_some_and(|child| child.parent() == parent)
}

pub(crate) fn order(world: &mut World, parent: Entity, mut ordered: Vec<Entity>) {
    let Ok(mut parent) = world.get_entity_mut(parent) else {
        return;
    };
    if let Some(children) = parent.get::<ChildEntities>() {
        if children.iter().eq(ordered.iter().copied()) {
            return;
        }
        let owned: HashSet<_> = ordered.iter().copied().collect();
        ordered.extend(children.iter().filter(|child| !owned.contains(child)));
        if children.iter().eq(ordered.iter().copied()) {
            return;
        }
    } else if ordered.is_empty() {
        return;
    }
    parent.replace_children(&ordered);
}

pub(crate) mod tests {
    use super::*;
    use bevy::prelude::{DetectChanges, DetectChangesMut};

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
        let first = children.entities[&1].entity;
        let second = children.entities[&2].entity;
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
        assert_eq!(children.entities[&1].entity, first);
        assert_eq!(children.entities[&2].entity, second);
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
                .get::<ChildEntities>(parent)
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            [second, first, extra]
        );
        world.entity_mut(first).insert(ChildOf(other));
        world.despawn(second);
        children.reconcile(&mut world, parent, [(1, 12), (2, 22)], create, update);
        assert_ne!(children.entities[&1].entity, first);
        assert_ne!(children.entities[&2].entity, second);
        children.reconcile(&mut world, parent, [], create, update);
        assert_eq!(
            world
                .get::<ChildEntities>(parent)
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

    #[cfg(test)]
    #[test]
    fn children_accept_owned_keys_and_can_retry_after_an_interrupted_update() {
        #[derive(PartialEq, Eq, Hash)]
        struct Key(u8);

        let mut world = World::new();
        let parent = world.spawn_empty().id();
        let mut children = Children::default();
        let create = |world: &mut World, parent, &value: &i32| {
            world.spawn((Value(value), ChildOf(parent))).id()
        };
        let update = |world: &mut World, entity, &value: &i32| {
            world.get_mut::<Value>(entity).expect("retained test row").0 = value;
        };
        children.reconcile(
            &mut world,
            parent,
            [(Key(1), 10), (Key(2), 20)],
            create,
            update,
        );
        let first = children.entities[&Key(1)].entity;
        let second = children.entities[&Key(2)].entity;
        let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            children.reconcile(
                &mut world,
                parent,
                [(Key(1), 11), (Key(2), 21)],
                create,
                |world, entity, value| {
                    assert_ne!(*value, 21, "interrupt the second update");
                    update(world, entity, value);
                },
            );
        }));
        assert!(interrupted.is_err());

        children.reconcile(
            &mut world,
            parent,
            [(Key(2), 22), (Key(1), 12), (Key(1), 99)],
            create,
            update,
        );
        assert_eq!(children.entities[&Key(1)].entity, first);
        assert_eq!(children.entities[&Key(2)].entity, second);
        assert_eq!(world.get::<Value>(first).expect("first").0, 12);
        assert_eq!(world.get::<Value>(second).expect("second").0, 22);
        assert_eq!(
            world
                .get::<ChildEntities>(parent)
                .expect("children")
                .iter()
                .collect::<Vec<_>>(),
            [second, first]
        );
    }

    #[cfg(test)]
    #[test]
    fn remembered_presentations_require_a_current_theme_and_child_order() {
        let mut world = World::new();
        let parent = world.spawn_empty().id();
        Presentation::remember(&mut world, parent, 1_u8);
        assert_eq!(Presentation::<u8>::state(&world, parent), None);
        world.insert_resource(Typography(Handle::default()));
        let first = world.spawn(ChildOf(parent)).id();
        let second = world.spawn(ChildOf(parent)).id();
        Presentation::remember(&mut world, parent, 1_u8);
        Presentation::update_state(&mut world, parent, |state: &mut u8| *state += 1);
        assert_eq!(Presentation::<u8>::state(&world, parent), Some(&2));
        assert!(Presentation::<u8>::matches(&world, parent, |&state| state == 2));
        order(&mut world, parent, vec![second, first]);
        assert!(!Presentation::<u8>::matches(&world, parent, |_| true));
        Presentation::remember(&mut world, parent, 2_u8);
        world.remove_resource::<Typography>();
        assert!(!Presentation::<u8>::matches(&world, parent, |_| true));
        assert!(world.despawn(parent));
        order(&mut world, parent, vec![second, first]);
        world.insert_resource(Typography(Handle::default()));
        Presentation::remember(&mut world, parent, 3_u8);
    }
}
