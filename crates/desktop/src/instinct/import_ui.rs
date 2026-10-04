use super::*;
use cell::{ClientMessage, ServerMessage};

#[derive(Component, Default)]
pub(super) struct Import {
    request: Option<String>,
    fingerprint: Option<String>,
    preview: Option<serde_json::Value>,
    message: String,
}

pub(super) fn active(imports: Query<(), With<Import>>) -> bool { !imports.is_empty() }

#[derive(Clone, Copy)]
enum ImportAction {
    Preview,
    Commit,
    Cancel,
}

impl Action for ImportAction {
    fn apply(&self, world: &mut World, owner: Entity) {
        if matches!(self, Self::Cancel) {
            world.entity_mut(owner).remove::<Import>();
            reader::render(world, owner);
            return;
        }
        if world
            .get::<Import>(owner)
            .is_some_and(|state| state.request.is_some())
        {
            return;
        }
        let action = match self {
            Self::Preview => engine::actions::Action::PreviewInstinct,
            Self::Commit => {
                let Some(fingerprint) = world
                    .get::<Import>(owner)
                    .and_then(|state| state.fingerprint.clone())
                else {
                    return;
                };
                engine::actions::Action::ImportInstinct { fingerprint }
            }
            Self::Cancel => return,
        };
        let id = format!("instinct-import-{}", nucleus::new_uid("request"));
        let result = world
            .get_non_send::<crate::cell_bridge::CellBridge>()
            .ok_or("The Cell is disconnected.")
            .and_then(|bridge| {
                bridge
                    .outgoing
                    .try_send(ClientMessage::Act {
                        id: id.clone(),
                        action,
                    })
                    .map_err(
                        |_| "The Cell is busy or disconnected. Preview again when it is available.",
                    )
            });
        if world.get::<Import>(owner).is_none() {
            world.entity_mut(owner).insert(Import::default());
        }
        let mut state = world.get_mut::<Import>(owner).unwrap();
        state.message = match result {
            Ok(()) => {
                state.request = Some(id);
                "Waiting for the Cell…".into()
            }
            Err(message) => message.into(),
        };
        reader::render(world, owner);
    }
}

pub(super) fn controls(world: &mut World, owner: Entity, parent: Entity) {
    if !cfg!(feature = "instinct") {
        return;
    }
    crate::description::button(
        world,
        parent,
        owner,
        "Preview handbook import",
        ImportAction::Preview,
    );
    let Some(state) = world.get::<Import>(owner) else {
        return;
    };
    let (message, preview, ready) = (
        state.message.clone(),
        state.preview.clone(),
        state.request.is_none() && state.fingerprint.is_some(),
    );
    crate::edit_mode::label(world, parent, &message, 14.0);
    crate::description::button(world, parent, owner, "Cancel import", ImportAction::Cancel);
    if let Some(preview) = preview {
        crate::edit_mode::label(
            world,
            parent,
            &format!(
                "{} new · {} reusable · {} conflicts",
                preview["created"],
                preview["reused"],
                preview["conflicts"].as_array().map_or(0, Vec::len)
            ),
            14.0,
        );
        let pane = world
            .spawn((
                Node {
                    max_height: px(180),
                    flex_direction: FlexDirection::Column,
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                ScrollPosition::default(),
                ChildOf(parent),
            ))
            .id();
        for conflict in preview["conflicts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
        {
            crate::edit_mode::label(world, pane, conflict, 14.0);
        }
        for record in preview["records"].as_array().into_iter().flatten() {
            let head = record["head"].as_str().unwrap_or("");
            let slug = record["slug"].as_str().unwrap_or("");
            let uid = record["projection"]["uid"].as_str().unwrap_or("");
            crate::edit_mode::label(
                world,
                pane,
                &format!(
                    "{head} · @{slug} · {uid}\nQuantity: {}\nAssertions and identity: {}",
                    record["projection"]["quantity"], record["projection"]["assertions"]
                ),
                13.0,
            );
        }
        crate::edit_mode::label(
            world,
            pane,
            &format!(
                "Vocabulary: {}\nConcepts and units: {}",
                preview["vocabulary"], preview["concepts"]
            ),
            13.0,
        );
        if ready && preview["conflicts"].as_array().is_some_and(Vec::is_empty) {
            crate::description::button(
                world,
                parent,
                owner,
                "Import these Records",
                ImportAction::Commit,
            );
        }
    }
}

pub(super) fn receive(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let Some(messages) = world.get_resource::<Messages<crate::cell_bridge::CellMessage>>() else {
        return;
    };
    let messages: Vec<_> = cursor
        .read(messages)
        .map(|message| message.0.clone())
        .collect();
    if messages.is_empty() {
        return;
    }
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<Import>>()
        .iter(world)
        .collect();
    for owner in owners {
        for message in &messages {
            let request = world
                .get::<Import>(owner)
                .and_then(|state| state.request.as_ref());
            match message {
                ServerMessage::ActionOk {
                    id,
                    data: Some(data),
                    ..
                } if request == Some(id) => {
                    let mut state = world.get_mut::<Import>(owner).unwrap();
                    state.request = None;
                    state.fingerprint = data["fingerprint"].as_str().map(str::to_owned);
                    state.preview = state.fingerprint.as_ref().map(|_| data.clone());
                    state.message = if state.fingerprint.is_some() {
                        "Review the exact Records below. Cancel makes no changes.".into()
                    } else {
                        format!(
                            "Imported {} Records; reused {}. Reading and learning progress are separate.",
                            data["created"], data["reused"]
                        )
                    };
                    reader::render(world, owner);
                }
                ServerMessage::Error { id, message, .. } if request == Some(id) => {
                    let mut state = world.get_mut::<Import>(owner).unwrap();
                    state.request = None;
                    state.fingerprint = None;
                    state.message = message.clone();
                    reader::render(world, owner);
                }
                _ => {}
            }
        }
    }
}
