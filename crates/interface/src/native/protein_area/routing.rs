use super::*;
use crate::host::performance::{self, Count, Stage};
use crate::native::cell_bridge::replies::{Index, Target as IndexedTarget};
use std::sync::Arc;

enum Target<'a> {
    All,
    Rows(&'a str),
    Action(&'a str),
    Error(&'a str),
}

impl<'a> Target<'a> {
    fn of(message: &'a ServerMessage) -> Option<Self> {
        Some(match message {
            ServerMessage::SessionAuthenticated { .. }
            | ServerMessage::LiveHello {
                login_required: true,
            } => Self::All,
            ServerMessage::Snapshot { id, .. } | ServerMessage::Update { id, .. } => Self::Rows(id),
            ServerMessage::ActionOk { id, .. } => Self::Action(id),
            ServerMessage::Error { id, .. }
                if id == "connection" || id == crate::native::cell_bridge::CONNECTION =>
            {
                Self::All
            }
            ServerMessage::Error { id, .. } => Self::Error(id),
            _ => return None,
        })
    }

    fn accepts(&self, state: &State) -> bool {
        match self {
            Self::All => true,
            Self::Rows(id) => state.subscription.as_deref() == Some(*id) && !state.login,
            Self::Action(id) => state.actions.contains_key(*id),
            Self::Error(id) => {
                state.subscription.as_deref() == Some(*id) || state.actions.contains_key(*id)
            }
        }
    }
}

struct Batch<'a> {
    index: Index<'a>,
    first_presentation: Option<usize>,
}

impl<'a> Batch<'a> {
    fn new(messages: &'a [Arc<ServerMessage>]) -> Self {
        let _prepare = performance::Span::new(Stage::RecordProjectionRoutingPrepare);
        let index = Index::new(messages, |message| match Target::of(message)? {
            Target::All => Some(IndexedTarget::All),
            Target::Rows(id) | Target::Action(id) | Target::Error(id) => {
                Some(IndexedTarget::Id(id))
            }
        });
        let first_presentation = messages
            .iter()
            .position(|message| !matches!(message.as_ref(), ServerMessage::Error { .. }));
        performance::add(Count::RecordProjectionRoutingIds, index.id_count() as u64);
        Self {
            index,
            first_presentation,
        }
    }

    fn next(&self, state: &State, after: usize) -> Option<usize> {
        let mut lookups = 0;
        let ids = state
            .subscription
            .as_deref()
            .into_iter()
            .chain(state.actions.keys().map(String::as_str))
            .inspect(|_| lookups += 1);
        let mut next = self.index.next(after, ids);
        if let Some(index) = self.first_presentation.filter(|index| *index >= after) {
            next = Some(next.map_or(index, |next| next.min(index)));
        }
        performance::add(Count::RecordProjectionRoutingLookups, lookups);
        next
    }

    fn deliver(&self, world: &mut World, owner: Entity) {
        let mut after = 0;
        loop {
            let Some(state) = world.resource::<Runtime>().areas.get(&owner) else {
                break;
            };
            let Some(index) = self.next(state, after) else {
                break;
            };
            let message = self.index.message(index);
            if Target::of(message).is_some_and(|target| target.accepts(state)) {
                performance::count(Count::RecordProjectionLocalDeliveries);
                super::receive(world, owner, message);
            } else if Some(index) == self.first_presentation {
                performance::count(Count::RecordProjectionInitialMirrors);
                mirror_editor(world, owner);
            }
            after = index + 1;
        }
    }
}

pub(super) fn deliver(world: &mut World, owners: &[Entity], messages: &[Arc<ServerMessage>]) {
    if messages.is_empty() {
        return;
    }
    let _routing = performance::Span::new(Stage::RecordProjectionRouting);
    let mut batch = None;
    for &owner in owners {
        if world.resource::<Runtime>().areas[&owner]
            .applied
            .as_ref()
            .is_some_and(|config| matches!(config.source, Source::Local))
        {
            batch
                .get_or_insert_with(|| Batch::new(messages))
                .deliver(world, owner);
        }
    }
}
