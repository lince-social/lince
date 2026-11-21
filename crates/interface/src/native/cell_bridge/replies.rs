use super::{CONNECTION, CellMessage};
use bevy::{
    ecs::message::{MessageCursor, Messages},
    prelude::{Component, Entity, FromWorld, QueryState, World},
};
use cell::ServerMessage;
use nucleus::performance::{Span, add};
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
pub(crate) struct Wave {
    sequences: Vec<usize>,
}

impl Wave {
    pub(crate) fn capture(
        &mut self,
        cursor: &mut MessageCursor<CellMessage>,
        messages: &Messages<CellMessage>,
        mut accepts: impl FnMut(&ServerMessage) -> bool,
    ) {
        let _capture = Span::new("cell-reply-wave-capture");
        add("cell-reply-wave-messages", cursor.len(messages) as u64);
        self.sequences.clear();
        self.sequences.extend(
            cursor
                .read_with_id(messages)
                .filter_map(|(message, sequence)| accepts(&message.0).then_some(sequence.id)),
        );
        add("cell-reply-wave-sequences", self.len() as u64);
        if self.is_empty() {
            self.sequences = Vec::new();
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.sequences.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.sequences.is_empty()
    }

    pub(crate) fn drain(&mut self) -> impl Iterator<Item = usize> + '_ {
        self.sequences.drain(..)
    }
}

/// Resolves a captured sequence before the receiver's message buffer advances.
pub(crate) fn message(messages: &Messages<CellMessage>, sequence: usize) -> &Arc<ServerMessage> {
    let (message, _) = messages
        .get_message(sequence)
        .expect("reply from the current immutable CellMessage wave");
    &message.0
}

pub(crate) trait Pending: Component {
    fn pending_id(&self) -> Option<&str>;
}

pub(crate) struct Owners<T: Pending> {
    query: QueryState<(Entity, &'static T)>,
}

impl<T: Pending> FromWorld for Owners<T> {
    fn from_world(world: &mut World) -> Self {
        Self {
            query: world.query(),
        }
    }
}

impl<T: Pending> Owners<T> {
    pub(crate) fn first(&mut self, world: &World, id: &str) -> Option<Entity> {
        let mut examined = 0;
        let owner = self.query.iter(world).find_map(|(owner, view)| {
            examined += 1;
            (view.pending_id() == Some(id)).then_some(owner)
        });
        add("cell-reply-owners-examined", examined);
        owner
    }

    pub(crate) fn collect(
        &mut self,
        world: &World,
        id: &str,
        subscription: Option<Entity>,
        all: bool,
        owners: &mut Vec<Entity>,
    ) {
        owners.clear();
        let mut examined = 0;
        owners.extend(self.query.iter(world).filter_map(|(owner, view)| {
            examined += 1;
            (all || subscription == Some(owner) || view.pending_id() == Some(id)).then_some(owner)
        }));
        add("cell-reply-owners-examined", examined);
    }
}

pub(crate) enum Target<'a> {
    All,
    Id(&'a str),
}

pub(crate) fn action_id(message: &ServerMessage) -> Option<&str> {
    match message {
        ServerMessage::ActionOk { id, .. } | ServerMessage::Error { id, .. } => Some(id),
        _ => None,
    }
}

pub(crate) fn ordinary(message: &ServerMessage) -> Option<Target<'_>> {
    Some(match message {
        ServerMessage::Error { id, .. } if id == CONNECTION => Target::All,
        ServerMessage::Snapshot { id, .. }
        | ServerMessage::Update { id, .. }
        | ServerMessage::ActionOk { id, .. }
        | ServerMessage::Error { id, .. } => Target::Id(id),
        _ => return None,
    })
}

pub(crate) fn capture(
    cursor: &mut MessageCursor<CellMessage>,
    messages: &Messages<CellMessage>,
    scratch: &mut Vec<Arc<ServerMessage>>,
    has_consumers: bool,
) {
    scratch.clear();
    if has_consumers {
        scratch.extend(
            cursor
                .read(messages)
                .filter(|message| ordinary(&message.0).is_some())
                .map(|message| message.0.clone()),
        );
    } else {
        cursor.clear(messages);
    }
    if scratch.is_empty() {
        *scratch = Vec::new();
    }
}

pub(crate) struct Batch<'a> {
    messages: &'a [Arc<ServerMessage>],
    index: Option<Index<'a>>,
}

impl<'a> Batch<'a> {
    pub(crate) fn new(messages: &'a [Arc<ServerMessage>], multiple_consumers: bool) -> Self {
        Self {
            messages,
            index: multiple_consumers.then(|| Index::new(messages, ordinary)),
        }
    }

    pub(crate) fn next<'i>(
        &self,
        after: usize,
        ids: impl Iterator<Item = &'i str> + Clone,
    ) -> Option<usize> {
        if let Some(index) = &self.index {
            return index.next(after, ids);
        }
        let mut examined = 0;
        let next = self
            .messages
            .iter()
            .enumerate()
            .skip(after)
            .find_map(|(index, message)| {
                examined += 1;
                match ordinary(message) {
                    Some(Target::All) => Some(index),
                    Some(Target::Id(id)) if ids.clone().any(|current| current == id) => Some(index),
                    _ => None,
                }
            });
        add("cell-reply-linear-messages", examined);
        next
    }

    /// Resolves an index into the same immutable slice used to select replies.
    pub(crate) fn message(&self, index: usize) -> &'a ServerMessage {
        self.messages[index].as_ref()
    }
}

pub(crate) struct Index<'a> {
    messages: &'a [Arc<ServerMessage>],
    ids: HashMap<&'a str, Vec<usize>>,
    all: Vec<usize>,
}

impl<'a> Index<'a> {
    pub(crate) fn new(
        messages: &'a [Arc<ServerMessage>],
        mut target: impl FnMut(&'a ServerMessage) -> Option<Target<'a>>,
    ) -> Self {
        let _prepare = Span::new("cell-reply-index-preparation");
        let mut batch = Self {
            messages,
            ids: HashMap::new(),
            all: Vec::new(),
        };
        let mut entries = 0;
        for (index, message) in messages.iter().enumerate() {
            match target(message) {
                Some(Target::All) => batch.all.push(index),
                Some(Target::Id(id)) => batch.ids.entry(id).or_default().push(index),
                None => continue,
            }
            entries += 1;
        }
        add("cell-reply-indexed-entries", entries);
        add("cell-reply-indexed-ids", batch.ids.len() as u64);
        batch
    }

    pub(crate) fn id_count(&self) -> usize {
        self.ids.len()
    }

    pub(crate) fn next<'i>(
        &self,
        after: usize,
        ids: impl IntoIterator<Item = &'i str>,
    ) -> Option<usize> {
        let mut lookups = 0;
        let matches = ids.into_iter().filter_map(|id| {
            lookups += 1;
            first_after(self.ids.get(id)?, after)
        });
        let next = matches.chain(first_after(&self.all, after)).min();
        add("cell-reply-id-lookups", lookups);
        next
    }

    /// Resolves an index into the same immutable slice used to build this index.
    pub(crate) fn message(&self, index: usize) -> &'a ServerMessage {
        self.messages[index].as_ref()
    }
}

fn first_after(indices: &[usize], after: usize) -> Option<usize> {
    indices
        .get(indices.partition_point(|index| *index < after))
        .copied()
}
