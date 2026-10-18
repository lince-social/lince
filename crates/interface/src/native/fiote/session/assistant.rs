use super::*;
use crate::native::sand_panel as ui;
use cell::fiote_assistant::{Directory, EntryReceipt, EntryRequest};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Component)]
struct Entry {
    panel: Entity,
    input: Entity,
    status: Entity,
    destination: Entity,
    choices: Entity,
    target: Option<String>,
    directory: Option<Directory>,
    pending: Option<String>,
    receipt: Option<EntryReceipt>,
    saved: Option<Saved>,
    path: Option<PathBuf>,
    next: Instant,
    restore: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    request_id: String,
    fingerprint: String,
    target: Option<String>,
    #[serde(default)]
    resolved: bool,
    #[serde(default)]
    destination: Option<String>,
}

#[derive(Clone)]
pub(super) struct Open;

impl Action for Open {
    fn apply(&self, world: &mut World, owner: Entity) {
        super::show(world, owner, Step::Assistant);
    }
}

pub(super) fn show(world: &mut World, panel: Entity, content: Entity, saved: Option<&FioteStatus>) {
    let Some(saved) = saved else { return };
    let (binding, view) = {
        let panel = world.get::<Panel>(panel).unwrap();
        (panel.binding.clone(), panel.view_uid.clone())
    };
    let owner = ui::column(world, content);
    crate::native::edit_mode::label(world, owner, "Ask Fiote", 20.0);
    let destination = crate::native::edit_mode::label(
        world,
        owner,
        "Automatic assistant: checking hierarchy",
        13.0,
    );
    let controls = ui::row(world, owner);
    for (caption, command) in [
        ("Use automatic assistant", Command::Automatic),
        ("Choose a Fiote", Command::Directory(0)),
        ("Configure recipient", Command::Configure),
    ] {
        ui::button(world, controls, owner, caption, command);
    }
    let choices = ui::column(world, owner);
    let input = world
        .spawn((
            crate::native::sand::text_editor(
                "",
                world.resource::<crate::native::theme::Typography>(),
                0,
            ),
            ChildOf(owner),
        ))
        .id();
    world.get_mut::<EditableText>(input).unwrap().max_characters = Some(65_536);
    crate::native::accessibility::input(world, input, "Assistant request", true);
    let status = crate::native::edit_mode::label(
        world,
        owner,
        "Write a request or dictate, then Send. Fiote may use its authorized tools.",
        13.0,
    );
    crate::native::message_content::draft(
        world,
        owner,
        input,
        status,
        &format!("assistant-{view}"),
        &binding,
        false,
    );
    models::attach(world, owner);
    listening::attach(world, owner);
    spoken::attach(world, owner);
    let controls = ui::row(world, owner);
    for (caption, command) in [
        ("Send assistant request", Command::Send),
        ("Inspect last request", Command::Inspect),
        ("Open request conversation", Command::Conversation),
        ("Stop this Fiote run", Command::Stop),
    ] {
        ui::button(world, controls, owner, caption, command);
    }
    let path = world
        .get_resource::<crate::native::workspace::WorkspaceFile>()
        .map(|file| {
            file.directory().join("assistant-requests").join(format!(
                "{}.json",
                nucleus::message::digest(view.as_bytes())
            ))
        });
    let previous = path.as_ref().and_then(|path| {
        let bytes = std::fs::read(path).ok()?;
        if bytes.len() > 4096 {
            return None;
        }
        serde_json::from_slice::<Saved>(&bytes).ok()
    });
    let restore = previous.is_some();
    world.entity_mut(owner).insert(Entry {
        panel,
        input,
        status,
        destination,
        choices,
        target: previous.as_ref().and_then(|saved| saved.target.clone()),
        directory: Some(Directory {
            fiotes: Vec::new(),
            next_offset: None,
            intake: saved.intake.clone(),
        }),
        pending: None,
        receipt: None,
        saved: previous,
        path,
        next: Instant::now(),
        restore,
    });
    destination_label(world, owner);
    Command::Directory(0).apply(world, owner);
}

fn destination_label(world: &mut World, owner: Entity) {
    let entry = world.get::<Entry>(owner).unwrap();
    let label = entry.destination;
    let text = match &entry.target {
        Some(target) => {
            let candidate = entry.directory.as_ref().and_then(|directory| {
                directory
                    .fiotes
                    .iter()
                    .find(|candidate| &candidate.record == target)
            });
            format!(
                "Recipient: {}{}",
                candidate
                    .map(|candidate| candidate.title.as_str())
                    .unwrap_or(target),
                if candidate.is_some_and(|candidate| !candidate.enabled) {
                    " · connection needs setup"
                } else {
                    ""
                }
            )
        }
        None => entry
            .directory
            .as_ref()
            .and_then(|directory| directory.intake.as_ref())
            .map(|intake| {
                format!(
                    "Automatic assistant: {} · {} direct children · oldest on ties{}",
                    intake.title,
                    intake.children,
                    if intake.enabled {
                        ""
                    } else {
                        " · connection needs setup"
                    }
                )
            })
            .unwrap_or_else(|| "Create and configure a Fiote to receive requests.".into()),
    };
    if let Some(mut label) = world.get_mut::<Text>(label) {
        label.0 = text;
    }
}

fn send(world: &mut World, owner: Entity, request: FioteRequest) -> Result<(), String> {
    let panel = world
        .get::<Entry>(owner)
        .ok_or("Assistant draft is closed.")?
        .panel;
    let id = super::send(world, panel, request)?;
    world
        .get_mut::<Entry>(owner)
        .ok_or("Assistant draft is closed.")?
        .pending = Some(id);
    Ok(())
}

fn preserve(world: &mut World, owner: Entity, saved: Saved) -> Result<(), String> {
    if let Some(path) = world
        .get::<Entry>(owner)
        .and_then(|entry| entry.path.clone())
    {
        let parent = path
            .parent()
            .ok_or("Assistant request storage is unavailable.")?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let mut file =
            tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
        serde_json::to_writer(file.as_file_mut(), &saved).map_err(|error| error.to_string())?;
        file.as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        file.persist(path).map_err(|error| error.to_string())?;
    }
    world.get_mut::<Entry>(owner).unwrap().saved = Some(saved);
    Ok(())
}

fn dispatch(world: &mut World, owner: Entity, request: EntryRequest) -> Result<(), String> {
    spoken::invalidate(world, owner);
    let mut saved = world
        .get::<Entry>(owner)
        .and_then(|entry| entry.saved.clone())
        .filter(|saved| saved.request_id == request.request_id)
        .ok_or("The assistant request identity changed before delivery.")?;
    saved.resolved = true;
    saved.destination = request.target.clone();
    preserve(world, owner, saved)?;
    send(world, owner, FioteRequest::AssistantSend { request })
}

fn remember_voice_receipt(
    world: &mut World,
    owner: Entity,
    receipt: &EntryReceipt,
) -> Result<(), String> {
    let entry = world
        .get::<Entry>(owner)
        .ok_or("Assistant controls closed.")?;
    if entry.pending.is_some() {
        return Ok(());
    }
    if entry.receipt.as_ref().is_some_and(|previous| {
        previous.request_id == receipt.request_id
            && previous.state != "running"
            && previous.reply.is_some()
            && receipt.state == "running"
    }) {
        return Ok(());
    }
    if entry
        .saved
        .as_ref()
        .is_none_or(|saved| saved.request_id != receipt.request_id)
    {
        preserve(
            world,
            owner,
            Saved {
                request_id: receipt.request_id.clone(),
                fingerprint: String::new(),
                target: Some(receipt.target.clone()),
                resolved: true,
                destination: Some(receipt.target.clone()),
            },
        )?;
    }
    world.get_mut::<Entry>(owner).unwrap().receipt = Some(receipt.clone());
    Ok(())
}

#[derive(Clone)]
enum Command {
    Automatic,
    Select(String),
    Directory(usize),
    Send,
    Inspect,
    Conversation,
    Configure,
    Stop,
}

impl Command {
    fn execute(&self, world: &mut World, owner: Entity) -> Result<(), String> {
        let entry = world
            .get::<Entry>(owner)
            .ok_or("Assistant draft is closed.")?;
        if entry.pending.is_some() {
            return Err("Wait for the current assistant request response.".into());
        }
        match self {
            Self::Automatic | Self::Select(_) => {
                let target = match self {
                    Self::Select(target) => Some(target.clone()),
                    _ => None,
                };
                if entry.target != target {
                    listening::invalidate(world, owner);
                    spoken::invalidate(world, owner);
                }
                models::cancel_routing(world, owner);
                world.get_mut::<Entry>(owner).unwrap().target = target;
                destination_label(world, owner);
            }
            Self::Directory(offset) => {
                let record = world
                    .get::<Panel>(entry.panel)
                    .ok_or("Fiote controls are closed.")?
                    .binding
                    .uid
                    .clone();
                send(
                    world,
                    owner,
                    FioteRequest::AssistantDirectory {
                        record,
                        offset: *offset,
                        limit: 30,
                    },
                )?;
            }
            Self::Send => {
                if models::routing_busy(world, owner) {
                    return Err("This request is being routed. Wait or cancel its local model operation before sending again.".into());
                }
                let text = world
                    .get::<EditableText>(entry.input)
                    .ok_or("Assistant draft is closed.")?
                    .value()
                    .to_string();
                let target = entry.target.clone();
                let content = crate::native::message_content::contents(world, owner)?;
                if text.trim().is_empty() && content.is_empty() {
                    return Err("Write or attach an assistant request.".into());
                }
                let fingerprint = nucleus::message::digest(
                    &serde_json::to_vec(&(&target, &text, &content))
                        .map_err(|error| error.to_string())?,
                );
                let previous = entry
                    .saved
                    .as_ref()
                    .filter(|saved| saved.fingerprint == fingerprint)
                    .cloned();
                let request_id = previous
                    .as_ref()
                    .map(|saved| saved.request_id.clone())
                    .unwrap_or_else(|| nucleus::new_uid("assistant-request"));
                let resolved = previous.as_ref().is_some_and(|saved| saved.resolved);
                let destination = previous
                    .as_ref()
                    .and_then(|saved| saved.destination.clone());
                preserve(
                    world,
                    owner,
                    Saved {
                        request_id: request_id.clone(),
                        fingerprint,
                        target: target.clone(),
                        resolved,
                        destination: destination.clone(),
                    },
                )?;
                let request = EntryRequest {
                    request_id,
                    target: if resolved { destination } else { target },
                    text,
                    content,
                };
                if resolved || !models::route(world, owner, &request) {
                    dispatch(world, owner, request)?;
                }
            }
            Self::Inspect => {
                let request_id = entry
                    .saved
                    .as_ref()
                    .ok_or("No assistant request has been submitted from this view.")?
                    .request_id
                    .clone();
                send(world, owner, FioteRequest::AssistantInspect { request_id })?;
            }
            Self::Conversation => {
                let receipt = entry
                    .receipt
                    .as_ref()
                    .ok_or("Inspect a sent request before opening its conversation.")?;
                let thread = receipt
                    .thread
                    .clone()
                    .ok_or("The request has no conversation yet.")?;
                let panel = entry.panel;
                super::management::OpenConversation {
                    target: receipt.target.clone(),
                    thread,
                }
                .apply(world, panel);
            }
            Self::Configure => {
                let target = entry
                    .target
                    .clone()
                    .or_else(|| {
                        entry
                            .directory
                            .as_ref()
                            .and_then(|directory| directory.intake.as_ref())
                            .map(|intake| intake.record.clone())
                    })
                    .ok_or("Create a Fiote first.")?;
                let panel = entry.panel;
                super::show(world, panel, Step::Manage);
                super::management::Select(target).apply(world, panel);
            }
            Self::Stop => {
                let thread = entry
                    .receipt
                    .as_ref()
                    .and_then(|receipt| receipt.thread.clone())
                    .ok_or("Inspect a sent request before stopping its Fiote.")?;
                send(world, owner, FioteRequest::Stop { thread })?;
            }
        }
        Ok(())
    }
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        let result = self.execute(world, owner);
        if let Some(label) = world.get::<Entry>(owner).map(|entry| entry.status)
            && let Some(mut text) = world.get_mut::<Text>(label)
        {
            text.0 = match result {
                Ok(()) => match self {
                    Self::Automatic | Self::Select(_) => {
                        "Recipient selected. Your draft remains unsent.".into()
                    }
                    Self::Directory(_) => "Checking available Fiotes.".into(),
                    Self::Stop => {
                        "Stop requested; waiting for the Fiote to finish stopping.".into()
                    }
                    _ => "Request submitted; checking the result.".into(),
                },
                Err(error) => error,
            };
        }
    }
}

pub(super) fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::native::cell_bridge::CellMessage>>,
) {
    let events: Vec<_> = world
        .get_resource::<Messages<crate::native::cell_bridge::CellMessage>>()
        .map(|messages| {
            cursor
                .read(messages)
                .map(|message| message.0.clone())
                .collect()
        })
        .unwrap_or_default();
    for event in events {
        let (id, result) = match event {
            cell::ServerMessage::Fiote { id, status } => (id, Ok(status)),
            cell::ServerMessage::Error { id, message, .. } => (id, Err(message)),
            _ => continue,
        };
        let owners: Vec<_> = world
            .query::<(Entity, &Entry)>()
            .iter(world)
            .filter(|(_, entry)| {
                entry.pending.as_deref() == Some(&id)
                    || (id == crate::native::cell_bridge::CONNECTION && entry.pending.is_some())
            })
            .map(|(owner, _)| owner)
            .collect();
        for owner in owners {
            let mut entry = world.get_mut::<Entry>(owner).unwrap();
            entry.pending = None;
            entry.next = Instant::now() + Duration::from_millis(400);
            let label = entry.status;
            match &result {
                Err(error) => {
                    world.get_mut::<Text>(label).unwrap().0 = format!(
                        "Not sent or not confirmed: {error}. Inspect the last request before retrying."
                    );
                }
                Ok(status) => {
                    if let Some(directory) = &status.assistant_directory {
                        entry.directory = Some(directory.clone());
                        let choices = entry.choices;
                        ui::clear(world, choices);
                        for candidate in &directory.fiotes {
                            ui::button(
                                world,
                                choices,
                                owner,
                                &format!(
                                    "{}{}",
                                    candidate.title,
                                    if candidate.enabled {
                                        ""
                                    } else {
                                        " · setup needed"
                                    }
                                ),
                                Command::Select(candidate.record.clone()),
                            );
                        }
                        if let Some(offset) = directory.next_offset {
                            ui::button(
                                world,
                                choices,
                                owner,
                                "More Fiotes",
                                Command::Directory(offset),
                            );
                        }
                        destination_label(world, owner);
                    } else if let Some(receipt) = &status.assistant_request {
                        entry.receipt = Some(receipt.clone());
                        let input = entry.input;
                        let accepted = receipt.message.is_some();
                        let submitted = entry.saved.as_ref().map(|saved| saved.fingerprint.clone());
                        if accepted {
                            let current = world.get::<Entry>(owner).unwrap();
                            let text = world
                                .get::<EditableText>(input)
                                .unwrap()
                                .value()
                                .to_string();
                            let content = world
                                .get::<crate::native::message_content::Draft>(owner)
                                .map(|draft| draft.parts.clone())
                                .unwrap_or_default();
                            let fingerprint =
                                serde_json::to_vec(&(&current.target, &text, &content))
                                    .map(|bytes| nucleus::message::digest(&bytes))
                                    .ok();
                            if fingerprint == submitted {
                                world
                                    .get_mut::<EditableText>(input)
                                    .unwrap()
                                    .editor
                                    .set_text("");
                                crate::native::message_content::sent(world, owner, true);
                            }
                        }
                        world.get_mut::<Text>(label).unwrap().0 = format!(
                            "{} · {} · recipient {}\n{}",
                            receipt.request_id, receipt.state, receipt.target, receipt.detail
                        );
                        spoken::observe(world, owner, receipt, status.assistant_reply.as_deref());
                    }
                }
            }
            if world.get::<Entry>(owner).is_some_and(|entry| {
                entry
                    .receipt
                    .as_ref()
                    .is_some_and(|receipt| receipt.state == "running")
            }) && let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>()
            {
                wake.after(Duration::from_millis(400));
            }
        }
    }
    let restoring: Vec<_> = world
        .query::<(Entity, &Entry)>()
        .iter(world)
        .filter(|(_, entry)| entry.restore && entry.pending.is_none())
        .map(|(owner, _)| owner)
        .collect();
    for owner in restoring {
        world.get_mut::<Entry>(owner).unwrap().restore = false;
        Command::Inspect.apply(world, owner);
    }
    let owners: Vec<_> = world
        .query::<(Entity, &Entry)>()
        .iter(world)
        .filter(|(_, entry)| {
            entry.pending.is_none()
                && entry.next <= Instant::now()
                && entry
                    .receipt
                    .as_ref()
                    .is_some_and(|receipt| receipt.state == "running")
        })
        .map(|(owner, _)| owner)
        .collect();
    for owner in owners {
        Command::Inspect.apply(world, owner);
    }
}

mod models;
#[cfg(test)]
mod tests;
pub(super) use models::update as models_update;
mod listening;
pub(super) use listening::update as listening_update;
mod spoken;
pub(super) use spoken::update as spoken_update;
