use super::*;
use crate::host::performance::{self, Count, Stage};

pub(in crate::native::protein_area) struct Preparation {
    subscriptions: QueryState<(Entity, &'static Subscription)>,
    owners: QueryState<(Entity, &'static InfluenceArea)>,
    editors: QueryState<(Entity, &'static QueryEditor)>,
    snapshot: Vec<(Entity, Entity, bool)>,
    configured: Vec<(Entity, bool, bool)>,
    removals: Vec<Entity>,
    present: HashSet<(Entity, bool)>,
}

impl FromWorld for Preparation {
    fn from_world(world: &mut World) -> Self {
        Self {
            subscriptions: world.query(),
            owners: world.query(),
            editors: world.query(),
            snapshot: Vec::new(),
            configured: Vec::new(),
            removals: Vec::new(),
            present: HashSet::new(),
        }
    }
}

impl Preparation {
    fn capture(&mut self, world: &World) {
        self.snapshot.clear();
        self.snapshot.extend(
            self.subscriptions
                .iter(world)
                .map(|(entity, subscription)| (entity, subscription.0, subscription.1)),
        );
        performance::add(
            Count::AreaFilterSubscriptionsPrepared,
            self.snapshot.len() as u64,
        );
        if self.snapshot.is_empty() {
            self.snapshot.shrink_to_fit();
        }
    }

    pub(in crate::native::protein_area) fn maintain(&mut self, world: &mut World) {
        let _timing = performance::Span::new(Stage::AreaFilterMaintenance);
        self.capture(world);
        for (entity, owner, changes) in self.snapshot.drain(..) {
            let valid = world.get::<InfluenceArea>(owner).is_some_and(|area| {
                if changes {
                    area.change_filter.is_some()
                } else {
                    area.filter.is_some()
                }
            });
            if !valid {
                stop(world, entity);
                self.removals.extend(
                    self.editors
                        .iter(world)
                        .filter(|(_, editor)| editor.0 == entity)
                        .map(|(entity, _)| entity),
                );
                for editor in self.removals.drain(..) {
                    world.despawn(editor);
                }
                world.despawn(entity);
                if world.get_entity(owner).is_ok() {
                    if changes {
                        world.entity_mut(owner).remove::<ChangeMatches>();
                    } else {
                        world.entity_mut(owner).remove::<Matches>();
                    }
                }
            } else if let (Some(root), Some(member)) = (
                world.get::<ChildOf>(owner).map(ChildOf::parent),
                world.get::<WorkspaceMember>(owner).copied(),
            ) {
                if world.get::<ChildOf>(entity).map(ChildOf::parent) != Some(root)
                    || world.get::<WorkspaceMember>(entity) != Some(&member)
                {
                    world.entity_mut(entity).insert((ChildOf(root), member));
                }
            }
        }

        self.present.clear();
        self.present.extend(
            self.subscriptions
                .iter(world)
                .map(|(_, subscription)| (subscription.0, subscription.1)),
        );
        self.configured.extend(
            self.owners
                .iter(world)
                .filter(|(_, area)| area.filter.is_some() || area.change_filter.is_some())
                .map(|(entity, area)| {
                    (entity, area.filter.is_some(), area.change_filter.is_some())
                }),
        );
        let idle = self.present.is_empty() && self.configured.is_empty();
        for (owner, effects, changes) in self.configured.drain(..) {
            for (enabled, changes) in [(effects, false), (changes, true)] {
                if !enabled {
                    continue;
                }
                performance::count(Count::AreaFilterPresenceLookups);
                if !self.present.contains(&(owner, changes))
                    && spawn(world, owner, changes).is_some()
                {
                    self.present.insert((owner, changes));
                }
            }
        }
        self.present.clear();
        if idle {
            self.present.shrink_to_fit();
            self.snapshot.shrink_to_fit();
            self.configured.shrink_to_fit();
            self.removals.shrink_to_fit();
        }
    }

    pub(in crate::native::protein_area) fn publish(&mut self, world: &mut World) {
        let _timing = performance::Span::new(Stage::AreaFilterPublication);
        self.capture(world);
        super::publish(world, self.snapshot.drain(..));
    }
}
