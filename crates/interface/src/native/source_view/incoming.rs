use super::*;
use bevy::ecs::query::QueryState;

pub(super) fn disconnected(message: &ServerMessage) -> bool {
    matches!(message, ServerMessage::Error { id, code, .. }
        if id == crate::native::cell_bridge::CONNECTION
            || id == "connection"
            || code.as_deref() == Some("session_expired"))
}

pub(super) fn target(world: &World, source: &Source, id: &str) -> Option<(Entity, bool)> {
    world
        .get_resource::<Pending>()?
        .0
        .get(id)
        .filter(|(_, origin, _)| origin == source)
        .map(|(owner, _, card)| (*owner, *card))
}

pub(super) struct Maintenance {
    cards: QueryState<(Entity, &'static Card)>,
    candidates: Vec<Entity>,
    retired: Vec<String>,
}

impl FromWorld for Maintenance {
    fn from_world(world: &mut World) -> Self {
        Self {
            cards: world.query(),
            candidates: Vec::new(),
            retired: Vec::new(),
        }
    }
}

impl Maintenance {
    pub(super) fn has_cards(&mut self, world: &World) -> bool {
        self.cards.iter(world).next().is_some()
    }

    pub(super) fn update(&mut self, world: &mut World) {
        let _maintenance = nucleus::performance::Span::new("source-view-maintenance");
        self.candidates.clear();
        self.candidates.extend(
            self.cards
                .iter(world)
                .filter(|(_, card)| !card.requested)
                .map(|(entity, _)| entity),
        );
        if self.candidates.is_empty() {
            self.candidates = Vec::new();
        }
        for entity in self.candidates.drain(..) {
            let Some(card) = world.get::<Card>(entity) else {
                continue;
            };
            if crate::native::description::source_sender(world, &card.source).is_none() {
                continue;
            }
            let (reference, source) = (card.reference.clone(), card.source.clone());
            let Some(mut card) = world.get_mut::<Card>(entity) else {
                continue;
            };
            card.requested = true;
            request(
                world,
                entity,
                source,
                Command::Inspect { record: reference },
                true,
            );
        }

        self.retired.clear();
        self.retired.extend(
            world
                .resource::<Pending>()
                .0
                .iter()
                .filter(|(_, (owner, _, _))| !world.entities().contains(*owner))
                .map(|(id, _)| id.clone()),
        );
        if self.retired.is_empty() {
            self.retired = Vec::new();
        } else {
            let mut pending = world.resource_mut::<Pending>();
            for id in self.retired.drain(..) {
                pending.0.remove(&id);
            }
        }
    }
}
