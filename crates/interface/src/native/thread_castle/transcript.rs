// non-lean-yet
// Scope: struct Transcript, struct Subscriptions, fn populate, fn refresh.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 48, 87, 89, 96,
//   107, 111, 136, 249, 251, 286.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (151).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   164.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (49, 86, 93, 108, 118, 121, 254, 261).
// - [rustc probe: elided_lifetimes_in_paths] Check API clarity and framework allowances before
//   making lifetimes explicit. Sites 155.

use super::*;
use cell::{ClientMessage, ServerMessage};
use serde_json::json;

#[derive(Component)]
pub(super) struct Transcript {
    binding: RecordBinding,
    message: String,
    thread: String,
    toggle: Entity,
    preview: Entity,
    content: Entity,
    text: Entity,
    subscription: Option<String>,
}

#[derive(Resource, Default)]
pub(super) struct Subscriptions(HashMap<String, crate::native::cell_bridge::Sender>);

pub(super) fn populate(world: &mut World, owner: Entity, binding: &RecordBinding, data: &Value) {
    let Some(thread) = data["tool_call"]["thread"].as_str() else {
        return;
    };
    let preview = crate::native::edit_mode::label(world, owner, "", 13.0);
    let toggle = control(world, owner, owner, "Show / hide tool transcript", Toggle);
    let content = world
        .spawn((
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                max_height: px(260),
                flex_shrink: 0.0,
                width: percent(100),
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    crate::native::scroll_sand::attach(world, content);
    let text = crate::native::edit_mode::label(world, content, "Loading transcript…", 13.0);
    control(
        world,
        content,
        owner,
        "Delete transcript thread",
        DeleteTranscript,
    );
    world.entity_mut(owner).insert(Transcript {
        binding: binding.clone(),
        message: data["uid"].as_str().unwrap().into(),
        thread: thread.into(),
        toggle,
        preview,
        content,
        text,
        subscription: None,
    });
    refresh(world, owner, data);
}

pub(super) fn refresh(world: &mut World, owner: Entity, data: &Value) {
    let Some(transcript) = world.get::<Transcript>(owner) else {
        return;
    };
    let _presentation = nucleus::performance::Span::new("message-transcript-presentation");
    let preview = transcript.preview;
    let toggle = transcript.toggle;
    let tool = &data["tool_call"];
    if tool["has_diff"] == true {
        let caption = "Show / hide tool output and diff";
        if let Some(mut access) = world.get_mut::<AccessibilityNode>(toggle) {
            if access.label() != Some(caption) {
                access.set_label(caption);
            }
        }
        let label = world.get::<Children>(toggle).and_then(|children| children.first()).copied();
        if let Some(mut text) = label.and_then(|label| world.get_mut::<Text>(label)) {
            text.set_if_neq(Text::new(caption));
        }
    }
    publish(
        world,
        preview,
        [
            tool["status"].as_str().unwrap_or("working"),
            "\n",
            tool["preview"].as_str().unwrap_or(""),
        ]
        .into_iter(),
    );
}

#[derive(Clone)]
struct Toggle;
impl Action for Toggle {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(transcript) = world.get::<Transcript>(owner) else {
            return;
        };
        let content = transcript.content;
        let opening = world.get::<Node>(content).unwrap().display == Display::None;
        let old = transcript.subscription.clone();
        let sender = crate::native::protein_area::editor_sender(world, &transcript.binding);
        let message = transcript.message.clone();
        if let Some(sender) = sender {
            if opening {
                let id = nucleus::new_uid("transcript");
                let protein = serde_json::from_value(json!({"source":"record","where":[{"uid_eq":message}],"fields":["uid","threads"],"include":{"threads":{"messages_limit":256}},"limit":1})).unwrap();
                if sender
                    .try_send(ClientMessage::Subscribe {
                        id: id.clone(),
                        protein,
                    })
                    .is_err()
                {
                    return;
                }
                world.init_resource::<Subscriptions>();
                world
                    .resource_mut::<Subscriptions>()
                    .0
                    .insert(id.clone(), sender);
                world.get_mut::<Transcript>(owner).unwrap().subscription = Some(id);
            } else if let Some(id) = old {
                if sender
                    .try_send(ClientMessage::Unsubscribe { id: id.clone() })
                    .is_ok()
                {
                    if let Some(mut subscriptions) = world.get_resource_mut::<Subscriptions>() {
                        subscriptions.0.remove(&id);
                    }
                }
                world.get_mut::<Transcript>(owner).unwrap().subscription = None;
            }
        }
        world.get_mut::<Node>(content).unwrap().display = if opening {
            Display::Flex
        } else {
            Display::None
        };
    }
}

#[derive(Component)]
struct SavedHistory {
    binding: RecordBinding,
    thread: String,
    message: String,
    group: Entity,
    text: Entity,
    pending: Option<String>,
    loaded: bool,
}

pub(super) fn saved_history(
    world: &mut World,
    owner: Entity,
    binding: &RecordBinding,
    thread: &str,
    data: &Value,
    has_live_transcript: bool,
) {
    let eligible = !has_live_transcript
        && data["author"].as_str() == Some(&binding.uid)
        && data["tool_call"].is_null()
        && data["message_state"] != "writing";
    if !eligible {
        if let Some(history) = world.entity_mut(owner).take::<SavedHistory>() {
            world.despawn(history.group);
        }
        return;
    }
    if world.get::<SavedHistory>(owner).is_some() {
        return;
    }
    let Some(message) = data["uid"].as_str() else {
        return;
    };
    let group = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Start,
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    control(world, group, owner, "Saved tool history", OpenSavedHistory);
    let text = crate::native::edit_mode::label(world, group, "", 13.0);
    world.get_mut::<Node>(text).unwrap().display = Display::None;
    world.entity_mut(owner).insert(SavedHistory {
        binding: binding.clone(),
        thread: thread.into(),
        message: message.into(),
        group,
        text,
        pending: None,
        loaded: false,
    });
}

#[derive(Clone)]
struct OpenSavedHistory;

impl Action for OpenSavedHistory {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(history) = world.get::<SavedHistory>(owner) else {
            return;
        };
        let entity = history.text;
        let opening = world
            .get::<Node>(entity)
            .is_some_and(|node| node.display == Display::None);
        let fetch = opening && history.pending.is_none() && !history.loaded;
        if fetch {
            let id = nucleus::new_uid("fiote-tool-history");
            let request = cell::FioteRequest::InspectToolHistory {
                thread: history.thread.clone(),
                message: history.message.clone(),
            };
            let sent = crate::native::protein_area::editor_sender(world, &history.binding)
                .is_some_and(|sender| {
                    sender
                        .try_send(ClientMessage::Fiote {
                            id: id.clone(),
                            request,
                        })
                        .is_ok()
                });
            if sent {
                world.get_mut::<SavedHistory>(owner).unwrap().pending = Some(id);
            }
            world.get_mut::<Text>(entity).unwrap().0 = if sent {
                "Loading tool history…"
            } else {
                "The local Cell is unavailable."
            }
            .into();
        }
        world.get_mut::<Node>(entity).unwrap().display = if opening {
            Display::Flex
        } else {
            Display::None
        };
    }
}

pub(crate) fn receive_saved_history(world: &mut World, id: &str, text: &str, loaded: bool) {
    let targets: Vec<_> = world
        .query::<(Entity, &SavedHistory)>()
        .iter(world)
        .filter(|(_, history)| history.pending.as_deref() == Some(id))
        .map(|(entity, history)| (entity, history.text))
        .collect();
    for (owner, label) in targets {
        if let Some(mut history) = world.get_mut::<SavedHistory>(owner) {
            history.loaded = loaded;
            history.pending = None;
        }
        if let Some(mut output) = world.get_mut::<Text>(label) {
            output.0 = text.into();
        }
    }
}

#[derive(Clone)]
struct DeleteTranscript;
impl Action for DeleteTranscript {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(transcript) = world.get::<Transcript>(owner) else {
            return;
        };
        let (binding, thread) = (transcript.binding.clone(), transcript.thread.clone());
        super::controls::request(world, owner, binding, thread);
    }
}

pub(crate) fn receive_message(world: &mut World, event: &ServerMessage) {
    if !matches!(
        event,
        ServerMessage::Snapshot { .. } | ServerMessage::Update { .. } | ServerMessage::Error { .. }
    ) {
        return;
    }
    let _presentation = nucleus::performance::Span::new("message-transcript-presentation");
    world
        .run_system_cached_with(apply_message, event)
        .expect("apply current transcript reply");
}

fn apply_message(
    bevy::ecs::system::InRef(event): bevy::ecs::system::InRef<ServerMessage>,
    world: &mut World,
    transcripts: &mut QueryState<(Entity, &'static Transcript)>,
    mut targets: Local<Vec<Entity>>,
) {
    let (id, rows, error) = match event {
        ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows } => {
            (id, rows.as_slice(), None)
        }
        ServerMessage::Error { id, message, .. } => (id, &[][..], Some(message)),
        _ => return,
    };
    targets.clear();
    targets.extend(
        transcripts
            .iter(world)
            .filter(|(_, item)| item.subscription.as_deref() == Some(id.as_str()))
            .map(|(owner, _)| owner),
    );
    nucleus::performance::add("transcript-reply-targets", targets.len() as u64);
    if targets.is_empty() {
        targets.shrink_to_fit();
    }
    for owner in targets.drain(..) {
        let Some(current) = world.get::<Transcript>(owner) else {
            continue;
        };
        let text = current.text;
        if let Some(error) = error {
            publish(world, text, std::iter::once(error.as_str()));
            continue;
        }
        let transcript = rows
            .first()
            .and_then(|row| row["threads"].as_array())
            .and_then(|threads| {
                threads
                    .iter()
                    .find(|item| item["uid"].as_str() == Some(current.thread.as_str()))
            });
        let Some(transcript) = transcript else {
            publish(
                world,
                text,
                std::iter::once("Transcript deleted or unavailable."),
            );
            continue;
        };
        let mut messages = transcript["messages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|message| message["body"].as_str())
            .peekable();
        if messages.peek().is_none() {
            publish(world, text, std::iter::once("No output received yet."));
            continue;
        }
        let heading = (transcript["messages_has_more"] == true).then_some(
            "Showing the latest 256 transcript entries. Older entries remain in the thread.\n\n",
        );
        let body = messages.enumerate().flat_map(|(index, text)| {
            [(index > 0).then_some("\n\n"), Some(text)]
                .into_iter()
                .flatten()
        });
        publish(world, text, heading.into_iter().chain(body));
    }
}

fn append_suffix<'a, I>(
    mut current: &str,
    mut chunks: I,
) -> Option<std::iter::Chain<std::iter::Once<&'a str>, I>>
where
    I: Iterator<Item = &'a str>,
{
    while let Some(chunk) = chunks.next() {
        if current.len() < chunk.len() {
            let suffix = chunk.strip_prefix(current)?;
            return Some(std::iter::once(suffix).chain(chunks));
        }
        current = current.strip_prefix(chunk)?;
    }
    let empty: &'a str = "";
    current
        .is_empty()
        .then(|| std::iter::once(empty).chain(chunks))
}

fn publish<'a>(world: &mut World, entity: Entity, chunks: impl Iterator<Item = &'a str> + Clone) {
    let Some(current) = world.get::<Text>(entity) else {
        return;
    };
    let suffix = append_suffix(&current.0, chunks.clone());
    let bytes = if let Some(suffix) = suffix {
        if !suffix.clone().any(|chunk| !chunk.is_empty()) {
            return;
        }
        let mut text = world.get_mut::<Text>(entity).unwrap();
        let previous = text.0.len();
        text.0.extend(suffix);
        text.0.len() - previous
    } else {
        let value: String = chunks.collect();
        let bytes = value.len();
        world.get_mut::<Text>(entity).unwrap().0 = value;
        bytes
    };
    nucleus::performance::add("message-transcript-text-updates", 1);
    nucleus::performance::add("message-transcript-text-bytes-prepared", bytes as u64);
}

pub(super) fn receive(
    subscriptions: Option<ResMut<Subscriptions>>,
    transcripts: Query<&Transcript>,
) {
    let Some(mut subscriptions) = subscriptions else {
        return;
    };
    if !subscriptions.0.is_empty() {
        let _maintenance = nucleus::performance::Span::new("transcript-subscription-maintenance");
        let active: HashSet<_> = transcripts
            .iter()
            .filter_map(|item| item.subscription.as_deref())
            .collect();
        subscriptions.0.retain(|id, sender| {
            if active.contains(id.as_str()) {
                true
            } else {
                sender
                    .try_send(ClientMessage::Unsubscribe { id: id.clone() })
                    .is_err()
                    && !sender.is_closed()
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fiote_tool_transcript_starts_collapsed_and_updates_without_closing() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::native::theme::Typography>();
        let owner = world.spawn(Node::default()).id();
        let binding = RecordBinding {
            area: owner,
            uid: nucleus::new_uid("r"),
            source: Source::Local,
        };
        let mut data = json!({"uid":nucleus::new_uid("r"),"tool_call":{"thread":nucleus::new_uid("r"),"preview":"one\n… +9 lines","status":"in_progress"}});
        populate(&mut world, owner, &binding, &data);
        let content = world.get::<Transcript>(owner).unwrap().content;
        assert_eq!(world.get::<Node>(content).unwrap().display, Display::None);
        assert_eq!(world.get::<Node>(content).unwrap().max_height, px(260));
        Toggle.apply(&mut world, owner);
        data["tool_call"]["status"] = "completed".into();
        refresh(&mut world, owner, &data);
        assert_eq!(world.get::<Node>(content).unwrap().display, Display::Flex);
        let preview = world.get::<Transcript>(owner).unwrap().preview;
        assert!(world.get::<Text>(preview).unwrap().0.contains("completed"));
        Toggle.apply(&mut world, owner);
        assert_eq!(world.get::<Node>(content).unwrap().display, Display::None);
    }
}
