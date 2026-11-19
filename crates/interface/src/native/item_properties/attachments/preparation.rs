use super::*;
use crate::host::performance::{self, Count, Stage};
use std::collections::HashSet;

pub(super) struct Preparation {
    items: QueryState<
        (Entity, &'static Attachment),
        (
            With<crate::native::canvas::CanvasItem>,
            bevy::ecs::query::Allow<bevy::ecs::entity_disabling::Disabled>,
        ),
    >,
    kept: HashSet<Entity>,
    eligible: usize,
    pub pending: Vec<(Entity, String)>,
    pub stale: Vec<Entity>,
    pub messages: Vec<(Entity, Arc<cell::ServerMessage>)>,
}

impl FromWorld for Preparation {
    fn from_world(world: &mut World) -> Self {
        Self {
            items: world.query_filtered(),
            kept: HashSet::new(),
            eligible: 0,
            pending: Vec::new(),
            stale: Vec::new(),
            messages: Vec::new(),
        }
    }
}

impl Preparation {
    pub(super) fn prepare(
        &mut self,
        world: &World,
        streams: &Streams,
        sender: Option<&crate::native::cell_bridge::Sender>,
    ) {
        let _prepare = performance::Span::new(Stage::AttachmentPreparation);
        self.kept.clear();
        self.pending.clear();
        self.stale.clear();
        self.eligible = 0;
        if let Some(sender) = sender {
            for (item, attachment) in self.items.iter(world) {
                if !attachment.valid() || !source_allowed(world, item) {
                    continue;
                }
                self.eligible += 1;
                if streams.active.get(&item).is_some_and(|stream| {
                    stream.uid == attachment.uid && sender.same_channel(&stream.sender)
                }) {
                    self.kept.insert(item);
                } else {
                    self.pending.push((item, attachment.uid.clone()));
                }
            }
        }
        self.stale.extend(
            streams
                .active
                .keys()
                .filter(|item| !self.kept.contains(*item))
                .copied(),
        );
        performance::add(Count::AttachmentEligibleItems, self.eligible as u64);
        performance::add(
            Count::AttachmentCandidatesPrepared,
            self.pending.len() as u64,
        );
    }

    pub(super) fn select_messages(&mut self, world: &World, streams: &mut Streams) {
        self.messages.clear();
        if let Some(messages) =
            world.get_resource::<Messages<crate::native::cell_bridge::CellMessage>>()
        {
            for message in streams.cursor.read(messages) {
                let id = match message.0.as_ref() {
                    cell::ServerMessage::Snapshot { id, .. }
                    | cell::ServerMessage::Update { id, .. }
                    | cell::ServerMessage::Error { id, .. } => id,
                    _ => continue,
                };
                if let Some(&item) = streams.by_id.get(id.as_str()) {
                    self.messages.push((item, message.0.clone()));
                }
            }
        }
        performance::add(
            Count::AttachmentMessagesSelected,
            self.messages.len() as u64,
        );
    }

    pub(super) fn release_if_unused(&mut self, streams: &mut Streams) {
        if self.eligible == 0 && streams.active.is_empty() && streams.retired.is_empty() {
            self.kept.clear();
            self.kept.shrink_to_fit();
            self.pending.shrink_to_fit();
            self.stale.shrink_to_fit();
            self.messages.shrink_to_fit();
            streams.active.shrink_to_fit();
            streams.by_id.shrink_to_fit();
            streams.retired.shrink_to_fit();
        }
    }
}
