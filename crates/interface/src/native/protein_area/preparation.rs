use super::*;
use crate::host::performance::{self, Count, Stage};
use std::{collections::HashSet, sync::Arc};

pub(super) struct Preparation {
    draft_editors: QueryState<(Entity, &'static QueryEditor), With<ProteinCastle>>,
    editors: QueryState<(Entity, &'static QueryEditor)>,
    areas: QueryState<(Entity, &'static InfluenceArea)>,
    subscriptions: QueryState<Entity, With<filter::Subscription>>,
    editor_updates: Vec<(Entity, Entity)>,
    editor_removals: Vec<Entity>,
    configured: HashSet<Entity>,
    gone: Vec<Entity>,
    cursor: bevy::ecs::message::MessageCursor<CellMessage>,
    local: Vec<Arc<ServerMessage>>,
    pub(super) configs: Vec<(Entity, Arc<Config>, Arc<Config>)>,
    pub(super) owners: Vec<Entity>,
}

impl FromWorld for Preparation {
    fn from_world(world: &mut World) -> Self {
        Self {
            draft_editors: world.query_filtered(),
            editors: world.query(),
            areas: world.query(),
            subscriptions: world.query_filtered(),
            editor_updates: Vec::new(),
            editor_removals: Vec::new(),
            configured: HashSet::new(),
            gone: Vec::new(),
            cursor: Default::default(),
            local: Vec::new(),
            configs: Vec::new(),
            owners: Vec::new(),
        }
    }
}

impl Preparation {
    pub(super) fn configure(&mut self, world: &mut World) {
        let _preparation = performance::Span::new(Stage::RecordProjectionPrepare);
        self.editor_updates.extend(
            self.draft_editors
                .iter(world)
                .map(|(entity, link)| (entity, link.0)),
        );
        performance::add(
            Count::RecordProjectionEditorsPrepared,
            self.editor_updates.len() as u64,
        );
        for (editor, owner) in self.editor_updates.drain(..) {
            if crate::native::laboratory::blocked(world, owner) {
                continue;
            }
            let draft = &world.get::<ProteinCastle>(editor).unwrap().draft;
            if let Some(config) = configuration_ref(world, owner)
                && &config.draft != draft
            {
                let mut config = config.clone();
                config.draft = draft.clone();
                config.enabled = false;
                set_configuration(world, owner, Some(config));
            }
        }

        self.owners.extend(
            self.areas
                .iter(world)
                .filter(|(_, area)| area.protein.is_some())
                .map(|(entity, _)| entity),
        );
        self.owners.extend(self.subscriptions.iter(world));
        self.configs.extend(
            self.owners
                .drain(..)
                .filter(|owner| !crate::native::laboratory::blocked(world, *owner))
                .filter_map(|owner| configuration(world, owner)),
        );
        performance::add(
            Count::RecordProjectionConfigurationsPrepared,
            self.configs.len() as u64,
        );
        self.configured
            .extend(self.configs.iter().map(|(owner, _, _)| *owner));
        self.gone.extend(
            world
                .resource::<Runtime>()
                .areas
                .keys()
                .filter(|entity| {
                    world.get::<LaboratoryFeed>(**entity).is_none()
                        && !self.configured.contains(*entity)
                })
                .copied(),
        );
        self.configured.clear();
        for entity in self.gone.drain(..) {
            stop(world, entity);
            self.editor_removals.extend(
                self.editors
                    .iter(world)
                    .filter(|(_, editor)| editor.0 == entity)
                    .map(|(editor, _)| editor),
            );
            for editor in self.editor_removals.drain(..) {
                world.despawn(editor);
            }
        }
    }

    pub(super) fn receive(&mut self, world: &mut World) {
        let _messages = performance::Span::new(Stage::RecordProjectionMessages);
        self.local.extend(
            self.cursor
                .read(world.resource::<Messages<CellMessage>>())
                .map(|message| message.0.clone()),
        );
        performance::add(
            Count::RecordProjectionMessagesPrepared,
            self.local.len() as u64,
        );
        if self.local.is_empty() {
            self.local.shrink_to_fit();
        }
        for message in &self.local {
            observe(world, &Source::Local, message);
        }
        sessions::update(world);
        self.owners.extend(
            world
                .resource::<Runtime>()
                .areas
                .keys()
                .copied()
                .filter(|owner| world.get::<LaboratoryFeed>(*owner).is_none()),
        );
        routing::deliver(world, &self.owners, &self.local);
        self.local.clear();
    }

    pub(super) fn finish(&mut self) {
        let idle = self.owners.is_empty();
        self.owners.clear();
        if idle {
            self.owners.shrink_to_fit();
            self.configs.shrink_to_fit();
            self.editor_updates.shrink_to_fit();
            self.editor_removals.shrink_to_fit();
            self.configured.shrink_to_fit();
            self.gone.shrink_to_fit();
            self.local.shrink_to_fit();
        }
    }
}

fn configuration(world: &World, entity: Entity) -> Option<(Entity, Arc<Config>, Arc<Config>)> {
    let config = configuration_ref(world, entity)?;
    let owner = world
        .get::<filter::Subscription>(entity)
        .map_or(entity, |subscription| subscription.0);
    let enabled = config.enabled
        && !world
            .get::<InfluenceArea>(owner)
            .is_some_and(|area| !area.enabled);
    let state = world.resource::<Runtime>().areas.get(&entity);
    let configured = state
        .and_then(|state| state.configured.as_ref().or(state.applied.as_ref()))
        .filter(|previous| previous.as_ref() == config)
        .cloned()
        .unwrap_or_else(|| Arc::new(config.clone()));
    let applied = if configured.enabled == enabled {
        configured.clone()
    } else {
        state
            .filter(|state| {
                state
                    .configured
                    .as_ref()
                    .is_some_and(|previous| Arc::ptr_eq(previous, &configured))
            })
            .and_then(|state| state.applied.as_ref())
            .filter(|applied| applied.enabled == enabled)
            .cloned()
            .unwrap_or_else(|| {
                let mut config = configured.as_ref().clone();
                config.enabled = enabled;
                Arc::new(config)
            })
    };
    Some((entity, configured, applied))
}
