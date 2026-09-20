use super::*;
use cell::{ClientMessage, ServerMessage};
use serde_json::json;

#[derive(Component)]
pub(super) struct Transcript {
    binding: RecordBinding,
    message: String,
    thread: String,
    preview: Entity,
    content: Entity,
    text: Entity,
    subscription: Option<String>,
}

#[derive(Resource, Default)]
struct Subscriptions(HashMap<String, tokio::sync::mpsc::Sender<ClientMessage>>);

pub(super) fn populate(world: &mut World, owner: Entity, binding: &RecordBinding, data: &Value) {
    let Some(thread) = data["tool_call"]["thread"].as_str() else {
        return;
    };
    let preview = crate::edit_mode::label(world, owner, "", 13.0);
    control(world, owner, owner, "Show / hide tool transcript", Toggle);
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
    crate::scroll_sand::attach(world, content);
    let text = crate::edit_mode::label(world, content, "Loading transcript…", 13.0);
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
    let preview = transcript.preview;
    let tool = &data["tool_call"];
    world.get_mut::<Text>(preview).unwrap().0 = format!(
        "{}\n{}",
        tool["status"].as_str().unwrap_or("working"),
        tool["preview"].as_str().unwrap_or("")
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
        let sender = crate::protein_area::editor_sender(world, &transcript.binding);
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
    let (id, rows, error) = match event.clone() {
        ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows } => {
            (id, rows, None)
        }
        ServerMessage::Error { id, message, .. } => (id, Vec::new(), Some(message)),
        _ => return,
    };
    let targets: Vec<_> = world
        .query::<&Transcript>()
        .iter(world)
        .filter(|item| item.subscription.as_deref() == Some(&id))
        .map(|item| (item.text, item.thread.clone()))
        .collect();
    for (text, thread) in targets {
        let content = error.clone().unwrap_or_else(|| {
                let transcript = rows
                    .first()
                    .and_then(|row| row["threads"].as_array())
                    .and_then(|threads| threads.iter().find(|item| item["uid"] == thread));
                match transcript {
                    None => "Transcript deleted or unavailable.".into(),
                    Some(transcript) => {
                        let parts: Vec<_> = transcript["messages"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|message| message["body"].as_str())
                            .collect();
                        if parts.is_empty() {
                            "No output received yet.".into()
                        } else {
                            let mut text = parts.join("\n\n");
                            if transcript["messages_has_more"] == true {
                                text.insert_str(0, "Showing the latest 256 transcript entries. Older entries remain in the thread.\n\n");
                            }
                            text
                        }
                    }
                }
            });
        world.get_mut::<Text>(text).unwrap().0 = content;
    }
}

pub(super) fn receive(world: &mut World) {
    let active: HashSet<String> = world
        .query::<&Transcript>()
        .iter(world)
        .filter_map(|item| item.subscription.clone())
        .collect();
    if let Some(mut subscriptions) = world.get_resource_mut::<Subscriptions>() {
        subscriptions.0.retain(|id, sender| {
            if active.contains(id) {
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
        world.init_resource::<crate::theme::Typography>();
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
