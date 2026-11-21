//! Ordered, shared editor snapshots prepared for a Record binding update.

use super::{Count, Documents, Key, Stage, TextBinding, key_ref, performance};
use crate::native::laboratory::blocked;
use bevy::prelude::{Entity, QueryState, World};

#[derive(Default)]
pub(super) struct Views {
    ordered: Vec<(Entity, TextBinding)>,
    groups: Documents<Vec<(Entity, TextBinding)>>,
}

impl Views {
    pub(super) fn clear(&mut self) {
        self.ordered.clear();
        self.ordered.shrink_to_fit();
        self.groups.clear();
        self.groups.shrink_to_fit();
    }

    pub(super) fn collect(
        &mut self,
        world: &World,
        query: &mut QueryState<(Entity, &'static TextBinding)>,
        focus: Option<Entity>,
    ) {
        let _preparation = performance::Span::new(Stage::RecordBindingPreparation);
        self.ordered.clear();
        if let Some((entity, binding)) = focus
            .and_then(|entity| query.get(world, entity).ok())
            .filter(|(entity, _)| !blocked(world, *entity))
        {
            self.ordered.push((entity, binding.clone()));
        }
        self.ordered.extend(
            query
                .iter(world)
                .filter(|(entity, _)| Some(*entity) != focus && !blocked(world, *entity))
                .map(|(entity, binding)| (entity, binding.clone())),
        );
        performance::add(
            Count::RecordBindingSharedSnapshots,
            self.ordered.len() as u64,
        );
    }

    pub(super) fn ordered(&self) -> &[(Entity, TextBinding)] {
        &self.ordered
    }

    pub(super) fn group(&mut self) {
        let _grouping = performance::Span::new(Stage::RecordBindingGrouping);
        for (entity, binding) in self.ordered.drain(..) {
            self.groups
                .entry_ref(&key_ref(&binding.record))
                .or_insert_with(|| {
                    performance::count(Count::RecordBindingGroupsCreated);
                    Vec::new()
                })
                .push((entity, binding));
        }
        self.groups.retain(|_, members| !members.is_empty());
        if self.groups.is_empty() {
            self.ordered.shrink_to_fit();
            self.groups.shrink_to_fit();
        }
        performance::add(Count::RecordBindingPreparedGroups, self.groups.len() as u64);
    }

    pub(super) fn members(&self, key: &Key) -> &[(Entity, TextBinding)] {
        self.groups.get(key).map_or(&[], Vec::as_slice)
    }

    pub(super) fn has_groups(&self) -> bool {
        !self.groups.is_empty()
    }

    pub(super) fn release(&mut self) {
        for members in self.groups.values_mut() {
            members.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::{RecordBinding, Source},
        *,
    };
    use crate::native::laboratory::LaboratoryOwner;
    use std::sync::Arc;

    fn binding(source: Source, property: &str) -> TextBinding {
        TextBinding::new(
            RecordBinding {
                area: Entity::PLACEHOLDER,
                uid: "shared".into(),
                source,
            },
            property,
            None,
            Arc::from("observed"),
        )
    }

    #[test]
    fn focus_only_moves_one_admitted_editor_to_the_front() {
        let mut world = World::new();
        for _ in 0..4 {
            world.spawn(binding(Source::Local, "head"));
        }
        let blocked = world
            .spawn((binding(Source::Local, "head"), LaboratoryOwner))
            .id();
        let missing = world.spawn_empty().id();
        let mut query = world.query();
        let query_order = query
            .iter(&world)
            .filter_map(|(entity, _): (Entity, &TextBinding)| (entity != blocked).then_some(entity))
            .collect::<Vec<_>>();
        let mut views = Views::default();
        for focus in
            query_order
                .iter()
                .copied()
                .map(Some)
                .chain([None, Some(blocked), Some(missing)])
        {
            let mut expected = query_order.clone();
            if let Some(index) = expected.iter().position(|&entity| Some(entity) == focus) {
                expected[..=index].rotate_right(1);
            }
            views.collect(&world, &mut query, focus);
            assert_eq!(
                views
                    .ordered()
                    .iter()
                    .map(|&(entity, _)| entity)
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }

    #[test]
    fn grouping_keeps_shared_snapshots_and_releases_them_after_each_pass() {
        let mut world = World::new();
        let first = world.spawn(binding(Source::Local, "head")).id();
        let remote_source = Source::Organ("office".into());
        let remote = world.spawn(binding(remote_source.clone(), "head")).id();
        let focused = world.spawn(binding(Source::Local, "body")).id();
        let mut query = world.query();
        let mut views = Views::default();
        let original = Arc::clone(&world.get::<TextBinding>(focused).expect("binding").field);
        views.collect(&world, &mut query, Some(focused));
        {
            let mut current = world.get_mut::<TextBinding>(focused).expect("binding");
            current.property = "new body".into();
            current.observed = Arc::from("new observation");
            current.confirmed = Arc::from("new confirmation");
        }
        views.group();
        let local = (Source::Local, "shared".into());
        let other = (remote_source, "shared".into());
        let local_members = views.members(&local);
        let (entity, snapshot) = &local_members[0];
        assert_eq!(*entity, focused);
        assert_eq!(snapshot.property, "body");
        assert_eq!(snapshot.observed.as_ref(), "observed");
        assert_eq!(snapshot.confirmed.as_ref(), "observed");
        assert_eq!(local_members[1].0, first);
        assert_eq!(views.members(&other)[0].0, remote);
        assert!(views.has_groups());
        views.release();
        assert!(views.members(&local).is_empty());
        assert!(views.members(&other).is_empty());
        assert_eq!(Arc::strong_count(&original), 1);
        for entity in [first, remote, focused] {
            assert!(world.despawn(entity));
        }
        views.collect(&world, &mut query, Some(focused));
        views.group();
        assert!(!views.has_groups());
        assert_eq!(views.groups.capacity(), 0);
        assert_eq!(views.ordered.capacity(), 0);
        views.clear();
    }
}
