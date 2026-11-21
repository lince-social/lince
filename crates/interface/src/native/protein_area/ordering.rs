use super::{RecordBinding, Runtime, Source, filter};
use crate::native::area::InfluenceArea;
use bevy::{ecs::system::InRef, prelude::*};
use nucleus::performance::{Span, add};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

pub(crate) type Subscriptions = QueryState<(Entity, &'static filter::Subscription)>;
pub(crate) type Snapshot = Arc<(Source, Arc<Vec<String>>)>;

pub(crate) struct Order<'w> {
    pub(crate) source: &'w Source,
    pub(crate) ids: &'w Arc<Vec<String>>,
}

impl Order<'_> {
    pub(crate) fn capture(&self, previous: Option<&Snapshot>) -> Snapshot {
        if let Some(previous) = previous
            && &previous.0 == self.source
            && Arc::ptr_eq(&previous.1, self.ids)
        {
            return previous.clone();
        }
        add("protein-order-snapshots-prepared", 1);
        Arc::new((self.source.clone(), self.ids.clone()))
    }
}

static EMPTY: LazyLock<Arc<Vec<String>>> = LazyLock::new(Arc::default);

enum SubscriptionIndex {
    Empty,
    Single(Entity, Option<Entity>),
    Indexed(HashMap<Entity, Entity>),
}

pub(crate) struct Lookup<'w, 'q> {
    world: &'w World,
    subscriptions: &'q mut Subscriptions,
    index: SubscriptionIndex,
}

impl<'w, 'q> Lookup<'w, 'q> {
    pub(crate) fn new(world: &'w World, subscriptions: &'q mut Subscriptions) -> Self {
        Self {
            world,
            subscriptions,
            index: SubscriptionIndex::Empty,
        }
    }

    fn subscription(&mut self, owner: Entity) -> Option<Entity> {
        use SubscriptionIndex::*;
        match &self.index {
            Indexed(index) => index.get(&owner).copied(),
            Single(first, entity) if *first == owner => *entity,
            Single(..) => {
                let mut index = HashMap::new();
                let mut examined = 0;
                for (entity, subscription) in self.subscriptions.iter(self.world) {
                    examined += 1;
                    if !subscription.1 {
                        index.entry(subscription.0).or_insert(entity);
                    }
                }
                add("protein-order-subscriptions-examined", examined);
                add("protein-order-indexes-prepared", 1);
                let entity = index.get(&owner).copied();
                self.index = Indexed(index);
                entity
            }
            Empty => {
                let mut examined = 0;
                let entity =
                    self.subscriptions
                        .iter(self.world)
                        .find_map(|(entity, subscription)| {
                            examined += 1;
                            (subscription.0 == owner && !subscription.1).then_some(entity)
                        });
                add("protein-order-subscriptions-examined", examined);
                self.index = Single(owner, entity);
                entity
            }
        }
    }

    pub(crate) fn get(&mut self, owner: Entity) -> Option<Order<'w>> {
        let _read = Span::new("protein-order-read");
        let world = self.world;
        let area = world.get::<InfluenceArea>(owner)?;
        let (config, filtered) = if let Some(config) = &area.filter {
            (config, true)
        } else {
            (area.protein.as_ref()?, false)
        };
        let entity = if filtered {
            self.subscription(owner)
        } else {
            Some(owner)
        };
        let ids = entity
            .and_then(|entity| world.get_resource::<Runtime>()?.areas.get(&entity))
            .filter(|state| {
                config.enabled && state.ready && state.applied.as_deref() == Some(config)
            })
            .map(|state| &state.order)
            .unwrap_or_else(|| &EMPTY);
        Some(Order {
            source: &config.source,
            ids,
        })
    }
}

pub(crate) fn columns(
    world: &mut World,
    owners: &[Option<Entity>],
) -> Vec<Option<Arc<Vec<String>>>> {
    // Both cached readers inspect current data without invoking other systems.
    world
        .run_system_cached_with(read_columns, owners)
        .expect("read Protein column orders")
}

fn read_columns(
    InRef(owners): InRef<'_, [Option<Entity>]>,
    world: &mut World,
    subscriptions: &mut Subscriptions,
) -> Vec<Option<Arc<Vec<String>>>> {
    let mut lookup = Lookup::new(world, subscriptions);
    owners
        .iter()
        .map(|owner| {
            owner
                .and_then(|owner| lookup.get(owner))
                .map(|order| order.ids.clone())
        })
        .collect()
}

pub(super) fn position(world: &mut World, owner: Entity, record: Entity, reverse: bool) -> usize {
    world
        .run_system_cached_with(read_position, (owner, record, reverse))
        .expect("read Protein position")
}

fn read_position(
    In((owner, record, reverse)): In<(Entity, Entity, bool)>,
    world: &mut World,
    subscriptions: &mut Subscriptions,
) -> usize {
    let mut lookup = Lookup::new(world, subscriptions);
    lookup
        .get(owner)
        .and_then(|order| {
            let binding = world.get::<RecordBinding>(record)?;
            order
                .ids
                .iter()
                .position(|id| id == &binding.uid)
                .map(|index| {
                    if reverse {
                        order.ids.len() - 1 - index
                    } else {
                        index
                    }
                })
        })
        .unwrap_or(0)
}
