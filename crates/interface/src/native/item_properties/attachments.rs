// non-lean-yet
// Scope: mod preparation, mod properties, struct Attachment, struct Stream.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 48, 116, 157,
//   166.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (148).

use super::*;
use bevy::ecs::message::MessageCursor;
use std::collections::HashMap;
use std::sync::Arc;

mod preparation;
mod properties;
use properties::{apply, restore};

#[derive(Component, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attachment {
    pub uid: String,
}

impl Attachment {
    pub fn valid(&self) -> bool {
        nucleus::valid_uid(&self.uid, "r")
    }
}

struct Stream {
    id: Arc<str>,
    uid: String,
    sender: crate::native::cell_bridge::Sender,
    baseline: Option<crate::native::area::RecordProperties>,
    message: String,
}

#[derive(Resource, Default)]
struct Streams {
    active: HashMap<Entity, Stream>,
    by_id: HashMap<Arc<str>, Entity>,
    retired: Vec<Stream>,
    cursor: MessageCursor<crate::native::cell_bridge::CellMessage>,
}

impl Streams {
    fn retire(&mut self, world: &mut World, item: Entity) {
        if let Some(stream) = self.active.remove(&item) {
            self.by_id.remove(stream.id.as_ref());
            restore(world, item, &stream);
            self.retired.push(stream);
        }
    }

    fn admit(&mut self, item: Entity, stream: Stream) {
        self.by_id.insert(stream.id.clone(), item);
        self.active.insert(item, stream);
    }
}

pub(super) fn install(app: &mut App) {
    app.init_resource::<Streams>().add_systems(
        PostUpdate,
        synchronize
            .after(crate::native::actions::ApplyActions)
            .before(crate::native::area::ComputeAreaForces)
            .before(bevy::ui::UiSystems::Layout),
    );
}

pub fn attach(
    world: &mut World,
    root: Entity,
    item: Entity,
    uid: &str,
) -> Result<(), &'static str> {
    if !eligible(world, root, item) {
        return Err("The original canvas item is unavailable.");
    }
    let attachment = Attachment {
        uid: uid.trim().to_owned(),
    };
    if !attachment.valid() {
        return Err("Choose an existing Record UID.");
    }
    if world.get::<Attachment>(item).is_none() && world.get::<RecordBinding>(item).is_some() {
        return Err("This item already presents its own Record.");
    }
    world.entity_mut(item).insert(attachment);
    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
        wake.ring();
    }
    Ok(())
}

pub fn detach(world: &mut World, root: Entity, item: Entity) -> Result<(), &'static str> {
    if !eligible(world, root, item) {
        return Err("The original canvas item is unavailable.");
    }
    world.entity_mut(item).remove::<Attachment>();
    if let Some(mut streams) = world.remove_resource::<Streams>() {
        streams.retire(world, item);
        world.insert_resource(streams);
    }
    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
        wake.ring();
    }
    Ok(())
}

pub(super) fn status(world: &World, item: Entity) -> &str {
    world
        .get_resource::<Streams>()
        .and_then(|streams| streams.active.get(&item))
        .map_or("", |stream| stream.message.as_str())
}

fn synchronize(world: &mut World, mut prepared: Local<preparation::Preparation>) {
    use crate::host::performance::{self, Count, Stage};
    let _synchronize = performance::Span::new(Stage::AttachmentSynchronize);
    let mut streams = world.remove_resource::<Streams>().unwrap_or_default();
    let sender = world
        .get_non_send::<crate::native::cell_bridge::CellBridge>()
        .map(|bridge| bridge.outgoing.clone())
        .filter(|sender| !sender.is_closed());
    prepared.prepare(world, &streams, sender.as_ref());
    for item in prepared.stale.drain(..) {
        streams.retire(world, item);
    }
    let mut retry;
    {
        let _subscriptions = performance::Span::new(Stage::AttachmentSubscriptions);
        streams
            .retired
            .retain(|stream| match stream.sender.try_reserve() {
                Ok(permit) => {
                    permit.send(cell::ClientMessage::Unsubscribe {
                        id: stream.id.to_string(),
                    });
                    false
                }
                Err(tokio::sync::mpsc::error::TrySendError::Full(())) => true,
                Err(tokio::sync::mpsc::error::TrySendError::Closed(())) => false,
            });
        retry = !streams.retired.is_empty();
        if let Some(sender) = sender {
            for (item, uid) in prepared.pending.drain(..) {
                let Ok(permit) = sender.try_reserve() else {
                    performance::count(Count::AttachmentSubscriptionDeferrals);
                    retry = true;
                    continue;
                };
                let mut query = crate::native::full_record::config(&uid, Source::Local)
                    .draft
                    .compile()
                    .unwrap();
                query.fields = None;
                query.include.links = Some(protein::LinksInclude {
                    kinds: vec!["*".into()],
                    direction: protein::LinkDirection::Both,
                    depth: 1,
                });
                let id = format!("item-properties-{}", nucleus::new_uid("op"));
                permit.send(cell::ClientMessage::Subscribe {
                    id: id.clone(),
                    protein: query,
                });
                performance::count(Count::AttachmentSubscriptionsPrepared);
                streams.admit(
                    item,
                    Stream {
                        id: id.into(),
                        uid,
                        sender: sender.clone(),
                        baseline: world
                            .get::<crate::native::area::RecordProperties>(item)
                            .cloned(),
                        message: "Loading Record…".into(),
                    },
                );
            }
        }
    }
    {
        let _messages = performance::Span::new(Stage::AttachmentMessages);
        prepared.select_messages(world, &mut streams);
        for (item, message) in prepared.messages.drain(..) {
            let Some(stream) = streams.active.get_mut(&item) else {
                continue;
            };
            match message.as_ref() {
                cell::ServerMessage::Snapshot { rows, .. }
                | cell::ServerMessage::Update { rows, .. } => {
                    let row = rows.iter().find(|row| row["uid"] == stream.uid);
                    apply(world, item, stream, row);
                }
                cell::ServerMessage::Error { message, .. } => {
                    restore(world, item, stream);
                    let message = lince_interface::text::prefix(message, 1024);
                    if stream.message != message {
                        stream.message.clear();
                        stream.message.push_str(message);
                    }
                }
                _ => unreachable!("attachment message selection"),
            }
        }
    }
    prepared.release_if_unused(&mut streams);
    world.insert_resource(streams);
    if retry && let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
        wake.after(std::time::Duration::from_millis(50));
    }
}

pub(super) fn source_allowed(world: &World, mut item: Entity) -> bool {
    for _ in 0..crate::native::layout::MAX_DEPTH {
        if world
            .get::<crate::native::practice_cells::PracticeSource>(item)
            .is_some()
        {
            return false;
        }
        if world.get::<Workspaces>(item).is_some() {
            return !crate::native::laboratory::blocked(world, item);
        }
        let Some(parent) = world.get::<ChildOf>(item) else {
            return false;
        };
        item = parent.parent();
    }
    false
}
