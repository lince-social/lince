use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use cell::{ClientMessage, ServerMessage};
use engine::calls::{Context, Identity, Operation, Signal, Snapshot, Tracks};
use lince_media::native::{
    capture::AudioInput,
    peer,
    preview::{Command as Capture, Devices, Preview},
    session::{Command as MediaCommand, Event, Session},
};

use crate::{actions::Action, protein_area::RecordBinding, sand_panel as panel};

#[cfg(test)]
mod tests;

#[derive(Component)]
struct Bar {
    binding: RecordBinding,
    thread: String,
    status: Entity,
    summary: Entity,
    last: Instant,
    snapshot: Snapshot,
}

#[derive(Resource, Default)]
struct Runtime {
    sequence: u64,
    pending: HashMap<String, (Instant, Pending)>,
    controller: Option<Controller>,
    waking: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

enum Pending {
    Inspect(Entity),
    InspectController,
    Context,
    Join(Vec<Capture>),
    Poll,
    Signal,
    Tracks(Option<Capture>),
    Leave,
    Group,
}

struct Controller {
    intent: Option<u8>,
    binding: RecordBinding,
    thread: String,
    root: Entity,
    body: Entity,
    header: Entity,
    collapsed: bool,
    status: Entity,
    people: Entity,
    participants: Entity,
    choices: Entity,
    group_choices: Entity,
    title: Entity,
    videos: Entity,
    images: BTreeMap<String, (Entity, Handle<Image>)>,
    person: Option<String>,
    context: Option<Context>,
    organs: Vec<String>,
    snapshot: Snapshot,
    device: String,
    media: Option<Session>,
    remote: Option<crate::protein_area::Remote>,
    preview: Option<Preview>,
    devices: Devices,
    tracks: Tracks,
    poll: Instant,
    heard: Instant,
    connections: BTreeMap<String, String>,
}

pub struct CallsPlugin;
impl Plugin for CallsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Runtime>()
            .add_message::<crate::cell_bridge::CellMessage>()
            .add_systems(Update, update.after(crate::cell_bridge::ReceiveCell));
    }
}

pub fn populate(world: &mut World, parent: Entity, binding: &RecordBinding, thread: &str) {
    let bar = panel::column(world, parent);
    let controls = panel::row(world, bar);
    for (label, command) in [
        ("Audio call", Ui::Open(Some(0))),
        ("Video call", Ui::Open(Some(1))),
        ("Share screen", Ui::Open(Some(2))),
        ("Join call", Ui::Open(Some(3))),
        ("Group…", Ui::Open(None)),
    ] {
        panel::button(world, controls, bar, label, command);
    }
    let status = crate::edit_mode::label(world, bar, "", 13.0);
    let summary = crate::edit_mode::label(world, bar, "", 12.0);
    world.entity_mut(bar).insert(Bar {
        binding: binding.clone(),
        thread: thread.into(),
        status,
        summary,
        last: Instant::now() - Duration::from_secs(5),
        snapshot: Snapshot::default(),
    });
}

pub fn summaries(world: &mut World, parent: Entity, value: &serde_json::Value) {
    let bars: Vec<_> = world
        .query::<(Entity, &Bar, &ChildOf)>()
        .iter(world)
        .filter(|(_, _, child)| child.parent() == parent)
        .map(|(_, bar, _)| bar.summary)
        .collect();
    let text = value["calls"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|call| {
            format!(
                "Call · {} · {} · {}m {}s",
                call["started_by"].as_str().unwrap_or("Participant"),
                call["started_at"].as_str().unwrap_or(""),
                call["duration_seconds"].as_u64().unwrap_or(0) / 60,
                call["duration_seconds"].as_u64().unwrap_or(0) % 60
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    for bar in bars {
        panel::status(world, bar, &text);
    }
}

#[derive(Clone)]
enum Ui {
    Open(Option<u8>),
    Person(String),
    Start(u8),
    Join,
    Leave,
    End,
    Close,
    Collapse,
    Capture(Capture),
    Devices,
    Organ(String),
    Invite,
    Admit(String, bool),
    Remove(String),
    Refresh,
}

impl Action for Ui {
    fn apply(&self, world: &mut World, owner: Entity) {
        if let Ui::Open(intent) = self {
            if world.resource::<Runtime>().controller.is_some() {
                return;
            }
            let Some(bar) = world.get::<Bar>(owner) else {
                return;
            };
            let binding = bar.binding.clone();
            let thread = bar.thread.clone();
            let snapshot = bar.snapshot.clone();
            open(world, binding, thread, snapshot, *intent);
            return;
        }
        let Some(mut controller) = world.resource_mut::<Runtime>().controller.take() else {
            return;
        };
        let result = apply(world, &mut controller, self);
        let close = matches!(self, Ui::Close) && result.is_ok();
        if let Err(message) = result {
            panel::status(world, controller.status, message);
        }
        if close {
            world
                .resource_mut::<Runtime>()
                .pending
                .retain(|_, (_, pending)| matches!(pending, Pending::Inspect(_)));
            world.despawn(controller.root);
        } else {
            world.resource_mut::<Runtime>().controller = Some(controller);
        }
    }
}

fn request(
    world: &mut World,
    controller: &Controller,
    operation: Operation,
    kind: Pending,
) -> Result<(), String> {
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.sequence += 1;
    let id = format!("call:{}", runtime.sequence);
    drop(runtime);
    let sender = sender(world, controller).ok_or("The Cell is disconnected")?;
    sender
        .try_send(ClientMessage::Call {
            id: id.clone(),
            thread: controller.thread.clone(),
            person: controller.person.clone(),
            operation,
        })
        .map_err(|_| "The Cell is busy; try again")?;
    world
        .resource_mut::<Runtime>()
        .pending
        .insert(id, (Instant::now(), kind));
    Ok(())
}

fn sender(
    world: &World,
    controller: &Controller,
) -> Option<tokio::sync::mpsc::Sender<ClientMessage>> {
    match &controller.binding.source {
        crate::protein_area::Source::Local => {
            crate::protein_area::editor_sender(world, &controller.binding)
        }
        crate::protein_area::Source::Organ(_) => controller
            .remote
            .as_ref()
            .map(|remote| remote.outgoing.clone()),
    }
}

fn context(world: &mut World, controller: &Controller) -> Result<(), String> {
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.sequence += 1;
    let id = format!("call:{}", runtime.sequence);
    drop(runtime);
    sender(world, controller)
        .ok_or("The Cell is disconnected")?
        .try_send(ClientMessage::CallContext {
            id: id.clone(),
            thread: controller.thread.clone(),
        })
        .map_err(|_| "The Cell is busy")?;
    world
        .resource_mut::<Runtime>()
        .pending
        .insert(id, (Instant::now(), Pending::Context));
    Ok(())
}

fn open(
    world: &mut World,
    binding: RecordBinding,
    thread: String,
    snapshot: Snapshot,
    intent: Option<u8>,
) {
    let root = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: px(12),
                top: px(12),
                width: px(500),
                max_height: percent(92),
                overflow: Overflow::scroll_y(),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12)),
                row_gap: px(8),
                ..default()
            },
            GlobalZIndex(900),
            crate::token_style::background(crate::tokens::Token::Surface),
            ScrollPosition::default(),
            crate::sand_store::SandCredits(super::CREDITS),
        ))
        .id();
    let heading = panel::row(world, root);
    let header = crate::edit_mode::label(world, heading, "Thread call", 16.0);
    panel::button(world, heading, root, "Show / hide controls", Ui::Collapse);
    let body = panel::column(world, root);
    let status = crate::edit_mode::label(
        world,
        body,
        "Choose your person, then start or join. Sources start only when selected.",
        13.0,
    );
    let people = panel::row(world, body);
    let buttons = panel::row(world, body);
    for (name, command) in [
        ("Audio call", Ui::Start(0)),
        ("Video call", Ui::Start(1)),
        ("Share screen", Ui::Start(2)),
        ("Join call", Ui::Join),
        ("Leave", Ui::Leave),
        ("End for everyone", Ui::End),
        ("Close", Ui::Close),
    ] {
        panel::button(world, buttons, root, name, command);
    }
    let participants = crate::edit_mode::label(world, body, "", 13.0);
    let sources = panel::row(world, body);
    for (name, command) in [
        ("Microphone on", Capture::Microphone(None)),
        ("Mute", Capture::StopMicrophone),
        ("Camera on", Capture::Camera("0".into())),
        ("Camera off", Capture::StopCamera),
        ("Choose screen", Capture::Screen(None)),
        ("Stop screen", Capture::StopScreen),
        (
            "Share system audio",
            Capture::SharedAudio(AudioInput::System(None)),
        ),
        ("Stop shared audio", Capture::StopSharedAudio),
        ("Volume 0%", Capture::Volume(0.0)),
        ("Volume 50%", Capture::Volume(0.5)),
        ("Volume 100%", Capture::Volume(1.0)),
    ] {
        panel::button(world, sources, root, name, Ui::Capture(command));
    }
    panel::button(world, sources, root, "Select devices", Ui::Devices);
    let choices = panel::column(world, body);
    let videos = panel::row(world, body);
    crate::edit_mode::label(
        world,
        body,
        "Choose Organs for a new group. Current Organs are selected. The current thread stays private; everyone accepts separately.",
        13.0,
    );
    let title = panel::field(world, body, "New group name", "Group conversation");
    let group_choices = panel::column(world, body);
    let actions = panel::row(world, body);
    panel::button(world, actions, root, "Create group invitation", Ui::Invite);
    panel::button(world, actions, root, "Refresh membership", Ui::Refresh);
    panel::credits(world, actions, root, super::CREDITS);
    let remote = match &binding.source {
        crate::protein_area::Source::Organ(organ) => {
            match crate::protein_area::connect_organ(world, organ) {
                Ok(remote) => Some(remote),
                Err(error) => {
                    panel::status(world, status, error);
                    None
                }
            }
        }
        crate::protein_area::Source::Local => None,
    };
    let controller = Controller {
        intent,
        binding,
        thread,
        root,
        body,
        header,
        collapsed: false,
        status,
        people,
        participants,
        choices,
        group_choices,
        title,
        videos,
        images: BTreeMap::new(),
        person: None,
        context: None,
        organs: Vec::new(),
        snapshot,
        device: String::new(),
        media: None,
        remote,
        preview: None,
        devices: Devices::default(),
        tracks: Tracks::default(),
        poll: Instant::now() - Duration::from_secs(5),
        heard: Instant::now(),
        connections: BTreeMap::new(),
    };
    if let Err(error) = context(world, &controller) {
        panel::status(world, status, error);
    }
    world.resource_mut::<Runtime>().controller = Some(controller);
}

fn media(controller: &Controller, command: Capture) -> Result<(), String> {
    controller
        .media
        .as_ref()
        .ok_or("Join the call first")?
        .send(MediaCommand::Capture(command))
        .map_err(|error| error.to_string())
}

fn apply(world: &mut World, controller: &mut Controller, command: &Ui) -> Result<(), String> {
    match command {
        Ui::Person(person) => {
            if world
                .resource::<Runtime>()
                .pending
                .values()
                .any(|(_, pending)| matches!(pending, Pending::Join(_)))
            {
                return Err("Wait for call admission before changing your person".into());
            }
            if controller.media.is_some() {
                return Err("Leave the call before choosing another person".into());
            }
            controller.person = Some(person.clone());
            render_context(world, controller);
            if let Some(intent) = controller.intent.take() {
                apply(
                    world,
                    controller,
                    &if intent == 3 {
                        Ui::Join
                    } else {
                        Ui::Start(intent)
                    },
                )?;
            }
        }
        Ui::Start(_) | Ui::Join => {
            if world
                .resource::<Runtime>()
                .pending
                .values()
                .any(|(_, pending)| matches!(pending, Pending::Join(_)))
            {
                return Err("Call admission is already in progress".into());
            }
            if controller.media.is_some() {
                return Err("You are already in this call".into());
            }
            if controller.person.is_none() {
                return Err("Choose your person first".into());
            }
            let capture = match command {
                Ui::Start(0) => vec![Capture::Microphone(None)],
                Ui::Start(1) => vec![Capture::Microphone(None), Capture::Camera("0".into())],
                Ui::Start(2) => vec![Capture::Screen(None)],
                _ => Vec::new(),
            };
            let operation = if matches!(command, Ui::Join) {
                Operation::Join {
                    call: controller
                        .snapshot
                        .call
                        .clone()
                        .ok_or("There is no active call")?,
                }
            } else {
                Operation::Start
            };
            request(world, controller, operation, Pending::Join(capture))?;
            panel::status(world, controller.status, "Joining…");
        }
        Ui::Capture(capture) => {
            if matches!(capture, Capture::Screen(_) | Capture::StopScreen) {
                world
                    .resource_mut::<Runtime>()
                    .pending
                    .retain(|_, (_, pending)| !matches!(pending, Pending::Tracks(Some(_))));
            }
            if matches!(capture, Capture::Screen(None))
                && !lince_media::native::preview::screen_uses_picker()
            {
                apply(world, controller, &Ui::Devices)?;
                panel::status(
                    world,
                    controller.status,
                    "Select a screen from the device list",
                );
                return Ok(());
            }
            if matches!(capture, Capture::Screen(_)) {
                let mut tracks = controller.tracks.clone();
                tracks.screen = true;
                request(
                    world,
                    controller,
                    Operation::Tracks {
                        call: controller.snapshot.call.clone().ok_or("Join first")?,
                        tracks,
                    },
                    Pending::Tracks(Some(capture.clone())),
                )?;
            } else {
                media(controller, capture.clone())?;
            }
        }
        Ui::Leave | Ui::End => {
            let call = controller
                .snapshot
                .call
                .clone()
                .ok_or("There is no active call")?;
            controller.media = None;
            world
                .resource_mut::<Runtime>()
                .pending
                .retain(|_, (_, pending)| {
                    matches!(
                        pending,
                        Pending::Inspect(_) | Pending::Context | Pending::Group
                    )
                });
            controller.tracks = Tracks::default();
            panel::clear(world, controller.videos);
            controller.images.clear();
            request(
                world,
                controller,
                if matches!(command, Ui::End) {
                    Operation::End { call }
                } else {
                    Operation::Leave { call }
                },
                Pending::Leave,
            )?;
        }
        Ui::Close => {
            if world
                .resource::<Runtime>()
                .pending
                .values()
                .any(|(_, pending)| matches!(pending, Pending::Join(_)))
            {
                return Err("Wait for admission, then leave or close the call".into());
            }
            if controller.media.is_some() {
                return Err("Leave the call before closing its controls".into());
            }
        }
        Ui::Collapse => {
            controller.collapsed = !controller.collapsed;
            world
                .get_mut::<Node>(controller.body)
                .ok_or("Call controls are unavailable")?
                .display = if controller.collapsed {
                Display::None
            } else {
                Display::Flex
            };
            world
                .get_mut::<Node>(controller.root)
                .ok_or("Call controls are unavailable")?
                .width = px(if controller.collapsed { 300.0 } else { 500.0 });
        }
        Ui::Devices => {
            if controller.preview.is_none() {
                controller.preview = Some(Preview::spawn().map_err(|error| error.to_string())?);
            }
            controller
                .preview
                .as_ref()
                .unwrap()
                .send(Capture::Devices)
                .map_err(|error| error.to_string())?;
        }
        Ui::Organ(organ) => {
            if controller.organs.contains(organ) {
                controller.organs.retain(|uid| uid != organ);
            } else if controller.organs.len() < 5 {
                controller.organs.push(organ.clone());
            }
            render_context(world, controller);
        }
        Ui::Invite => {
            let action = engine::actions::Action::ProposeGroup {
                thread: controller.thread.clone(),
                title: panel::value(world, controller.title)?,
                organs: controller.organs.clone(),
            };
            group_action(world, controller, action)?;
        }
        Ui::Admit(person, allowed) => group_action(
            world,
            controller,
            engine::actions::Action::SetGroupPerson {
                root: controller
                    .context
                    .as_ref()
                    .ok_or("Membership is loading")?
                    .root
                    .clone(),
                person: person.clone(),
                allowed: *allowed,
            },
        )?,
        Ui::Remove(organ) => group_action(
            world,
            controller,
            engine::actions::Action::RemoveGroupOrgan {
                root: controller
                    .context
                    .as_ref()
                    .ok_or("Membership is loading")?
                    .root
                    .clone(),
                organ: organ.clone(),
            },
        )?,
        Ui::Refresh => context(world, controller)?,
        Ui::Open(_) => {}
    }
    Ok(())
}

fn group_action(
    world: &mut World,
    controller: &Controller,
    action: engine::actions::Action,
) -> Result<(), String> {
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.sequence += 1;
    let id = format!("call:{}", runtime.sequence);
    drop(runtime);
    sender(world, controller)
        .ok_or("The Cell is disconnected")?
        .try_send(ClientMessage::Act {
            id: id.clone(),
            action,
        })
        .map_err(|_| "The Cell is busy")?;
    world
        .resource_mut::<Runtime>()
        .pending
        .insert(id, (Instant::now(), Pending::Group));
    Ok(())
}

fn render_context(world: &mut World, controller: &Controller) {
    let Some(context) = &controller.context else {
        return;
    };
    panel::clear(world, controller.people);
    panel::clear(world, controller.group_choices);
    if context.people.is_empty() {
        crate::edit_mode::label(
            world,
            controller.people,
            "Create a Person in this Organ to join calls.",
            13.0,
        );
    }
    for (person, name) in &context.people {
        let selected = if controller.person.as_ref() == Some(person) {
            "✓ "
        } else {
            ""
        };
        panel::button(
            world,
            controller.people,
            controller.root,
            &format!("{selected}{name}"),
            Ui::Person(person.clone()),
        );
    }
    if let Some(group) = &context.group {
        for member in &group.membership.members {
            let status = if member.removed {
                "removed"
            } else if member.accepted {
                "accepted"
            } else {
                "invited"
            };
            let row = panel::row(world, controller.group_choices);
            crate::edit_mode::label(world, row, &format!("{} · {status}", member.name), 13.0);
            if member.organ != group.membership.owner && !member.removed {
                panel::button(
                    world,
                    row,
                    controller.root,
                    "Remove from group",
                    Ui::Remove(member.organ.clone()),
                );
            }
        }
        for (person, name) in &context.people {
            let admitted = context.admitted.contains(person);
            panel::button(
                world,
                controller.group_choices,
                controller.root,
                &format!(
                    "{} {name}",
                    if admitted {
                        "Revoke admission for"
                    } else {
                        "Admit"
                    }
                ),
                Ui::Admit(person.clone(), !admitted),
            );
        }
    }
    let row = panel::row(world, controller.group_choices);
    for (organ, name) in &context.organs {
        let selected = if controller.organs.contains(organ) {
            "✓ "
        } else {
            ""
        };
        panel::button(
            world,
            row,
            controller.root,
            &format!("{selected}{name}"),
            Ui::Organ(organ.clone()),
        );
    }
}

fn key(identity: &Identity) -> String {
    format!("{}:{}:{}", identity.organ, identity.person, identity.device)
}

fn incoming(signal: Signal) -> peer::Signal {
    match signal {
        Signal::Offer(sdp) => peer::Signal::Offer(sdp),
        Signal::Answer(sdp) => peer::Signal::Answer(sdp),
        Signal::Candidate {
            mid,
            line,
            candidate,
        } => peer::Signal::Candidate {
            mid,
            line,
            candidate,
        },
    }
}
fn outgoing(signal: peer::Signal) -> Signal {
    match signal {
        peer::Signal::Offer(sdp) => Signal::Offer(sdp),
        peer::Signal::Answer(sdp) => Signal::Answer(sdp),
        peer::Signal::Candidate {
            mid,
            line,
            candidate,
        } => Signal::Candidate {
            mid,
            line,
            candidate,
        },
    }
}

pub(crate) fn receive(world: &mut World, message: &ServerMessage) {
    let id = match message {
        ServerMessage::Call { id, .. }
        | ServerMessage::CallContext { id, .. }
        | ServerMessage::Error { id, .. }
        | ServerMessage::ActionOk { id, .. } => id,
        _ => return,
    };
    let Some((_, pending)) = world
        .get_resource_mut::<Runtime>()
        .and_then(|mut runtime| runtime.pending.remove(id))
    else {
        return;
    };
    if let Pending::Inspect(entity) = pending {
        let mut status = None;
        if let Some(mut bar) = world.get_mut::<Bar>(entity) {
            if let ServerMessage::Call { snapshot, .. } = message {
                bar.snapshot = snapshot.clone();
                status = Some((
                    bar.status,
                    if snapshot.call.is_some() {
                        format!(
                            "Call available · {}",
                            snapshot
                                .participants
                                .iter()
                                .map(|person| format!("{} ({})", person.name, person.organ_name))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    } else {
                        String::new()
                    },
                ));
            }
        }
        if let Some((entity, message)) = status {
            panel::status(world, entity, message);
        }
        return;
    }
    let Some(mut controller) = world.resource_mut::<Runtime>().controller.take() else {
        return;
    };
    let result = received(world, &mut controller, pending, message);
    if let Err(error) = result {
        panel::status(world, controller.status, error);
    }
    world.resource_mut::<Runtime>().controller = Some(controller);
}

fn received(
    world: &mut World,
    controller: &mut Controller,
    pending: Pending,
    message: &ServerMessage,
) -> Result<(), String> {
    match message {
        ServerMessage::Error { message, .. } => {
            if matches!(pending, Pending::Poll | Pending::Tracks(None)) {
                controller.media = None;
            }
            return Err(message.clone());
        }
        ServerMessage::CallContext { context, .. } => {
            if controller.context.is_none() {
                controller.organs = context
                    .current_organs
                    .iter()
                    .filter(|organ| context.organs.iter().any(|(uid, _)| uid == *organ))
                    .take(5)
                    .cloned()
                    .collect();
            }
            controller.context = Some(context.clone());
            render_context(world, controller);
        }
        ServerMessage::ActionOk { created, .. } => {
            if let Some(uid) = created {
                panel::status(
                    world,
                    controller.status,
                    "New group created. Open it after the other Organs accept; join it separately.",
                );
                let _ = crate::full_record::open(
                    world,
                    controller.binding.area,
                    uid,
                    controller.binding.source.clone(),
                );
            }
            context(world, controller)?;
        }
        ServerMessage::Call {
            device, snapshot, ..
        } => {
            match &pending {
                Pending::Signal | Pending::Tracks(None) => return Ok(()),
                Pending::InspectController | Pending::Leave => {
                    if controller.media.is_none() {
                        controller.snapshot = snapshot.clone();
                    }
                    return Ok(());
                }
                Pending::Tracks(Some(capture)) => {
                    if snapshot.call == controller.snapshot.call
                        && snapshot.participants.iter().any(|participant| {
                            participant.identity.device == *device
                                && controller.person.as_ref() == Some(&participant.identity.person)
                                && participant.tracks.screen
                        })
                        && let Some(media) = &controller.media
                    {
                        media
                            .send(MediaCommand::Capture(capture.clone()))
                            .map_err(|error| error.to_string())?;
                    }
                    return Ok(());
                }
                Pending::Poll if controller.media.is_none() => return Ok(()),
                Pending::Join(_) | Pending::Poll => {}
                _ => return Ok(()),
            }
            controller.device = device.clone();
            controller.heard = Instant::now();
            controller.snapshot = snapshot.clone();
            let own = snapshot.participants.iter().find(|participant| {
                participant.identity.device == *device
                    && controller.person.as_ref() == Some(&participant.identity.person)
            });
            if let Pending::Join(captures) = &pending {
                if own.is_none() {
                    return Err("The call did not admit this device".into());
                }
                controller.media = Some(
                    Session::spawn(
                        peer::Network::from_environment().map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?,
                );
                controller.connections.clear();
                for capture in captures {
                    apply(world, controller, &Ui::Capture(capture.clone()))?;
                }
            }
            if controller.media.is_some() && own.is_none() {
                controller.media = None;
                panel::status(
                    world,
                    controller.status,
                    "The call ended or this device lost admission",
                );
            }
            if let Some(media) = &controller.media {
                media
                    .send(MediaCommand::Renew)
                    .map_err(|error| error.to_string())?;
                let own = key(&own.unwrap().identity);
                let peers = snapshot
                    .participants
                    .iter()
                    .filter(|participant| participant.identity.device != *device)
                    .map(|participant| {
                        let peer = key(&participant.identity);
                        let initiator = own < peer;
                        (peer, initiator)
                    })
                    .collect();
                media
                    .send(MediaCommand::Attendance(peers))
                    .map_err(|error| error.to_string())?;
                for envelope in &snapshot.signals {
                    media
                        .send(MediaCommand::Signal {
                            peer: key(&envelope.from),
                            signal: incoming(envelope.signal.clone()),
                        })
                        .map_err(|error| error.to_string())?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let mut messages: Vec<_> = cursor
        .read(world.resource::<Messages<crate::cell_bridge::CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    let mut disconnected = None;
    if let Some(controller) = world.resource_mut::<Runtime>().controller.as_mut()
        && let Some(remote) = controller.remote.as_mut()
    {
        for _ in 0..64 {
            let Ok(message) = remote.incoming.try_recv() else {
                break;
            };
            if let ServerMessage::Error { id, message, .. } = &message
                && id == "connection"
            {
                controller.media = None;
                disconnected = Some((controller.status, message.clone()));
            }
            messages.push(message);
        }
    }
    if let Some((status, message)) = disconnected {
        panel::status(world, status, message);
    }
    for message in messages {
        receive(world, &message);
    }
    let due: Vec<_> = world
        .query::<(Entity, &Bar)>()
        .iter(world)
        .filter(|(_, bar)| bar.last.elapsed() > Duration::from_secs(3))
        .map(|(entity, bar)| (entity, bar.binding.clone(), bar.thread.clone()))
        .take(8)
        .collect();
    for (entity, binding, thread) in due {
        world.get_mut::<Bar>(entity).unwrap().last = Instant::now();
        let mut runtime = world.resource_mut::<Runtime>();
        runtime.sequence += 1;
        let id = format!("call:{}", runtime.sequence);
        drop(runtime);
        if let Some(sender) = crate::protein_area::editor_sender(world, &binding) {
            if sender
                .try_send(ClientMessage::Call {
                    id: id.clone(),
                    thread,
                    person: None,
                    operation: Operation::Inspect,
                })
                .is_ok()
            {
                world
                    .resource_mut::<Runtime>()
                    .pending
                    .insert(id, (Instant::now(), Pending::Inspect(entity)));
            }
        }
    }
    world
        .resource_mut::<Runtime>()
        .pending
        .retain(|_, (at, _)| at.elapsed() < Duration::from_secs(10));
    let controller = world.resource_mut::<Runtime>().controller.take();
    if let Some(mut controller) = controller {
        if let Err(error) = tick(world, &mut controller) {
            panel::status(world, controller.status, error);
        }
        world.resource_mut::<Runtime>().controller = Some(controller);
    }
    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>().cloned() {
        if world.resource::<Runtime>().controller.is_some()
            || world.query::<&Bar>().iter(world).next().is_some()
        {
            let active = world
                .resource::<Runtime>()
                .controller
                .as_ref()
                .is_some_and(|controller| controller.media.is_some());
            let waking = world.resource::<Runtime>().waking.clone();
            if waking.swap(true, std::sync::atomic::Ordering::AcqRel) {
                return;
            }
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(if active { 50 } else { 1000 })).await;
                waking.store(false, std::sync::atomic::Ordering::Release);
                wake.ring();
            });
        }
    }
}

fn tick(world: &mut World, controller: &mut Controller) -> Result<(), String> {
    panel::status(
        world,
        controller.header,
        if controller.media.is_some() {
            format!(
                "Call · {}{}{}{}",
                if controller.tracks.microphone {
                    "mic on"
                } else {
                    "muted"
                },
                if controller.tracks.camera {
                    " · camera"
                } else {
                    ""
                },
                if controller.tracks.screen {
                    " · screen"
                } else {
                    ""
                },
                if controller.tracks.shared_audio {
                    " · audio sharing"
                } else {
                    ""
                }
            )
        } else {
            "Thread call".into()
        },
    );
    if controller.media.is_some() && controller.heard.elapsed() > Duration::from_secs(30) {
        controller.media = None;
        return Err(
            "The coordinator has been unreachable for 30 seconds. The call stopped.".into(),
        );
    }
    if controller.poll.elapsed() > Duration::from_secs(1)
        && !world
            .resource::<Runtime>()
            .pending
            .values()
            .any(|(_, pending)| {
                matches!(
                    pending,
                    Pending::Poll | Pending::InspectController | Pending::Join(_)
                )
            })
    {
        controller.poll = Instant::now();
        let (operation, pending) = if controller.media.is_some() {
            (
                Operation::Poll {
                    call: controller.snapshot.call.clone().ok_or("The call ended")?,
                },
                Pending::Poll,
            )
        } else {
            (Operation::Inspect, Pending::InspectController)
        };
        request(world, controller, operation, pending)?;
    }
    let mut events = Vec::new();
    let mut frames = Vec::new();
    if let Some(media) = &controller.media {
        for _ in 0..32 {
            let Some(event) = media.event() else { break };
            events.push(event);
        }
        frames = media.frames();
    }
    for event in events {
        match event {
            Event::Signal { peer, signal } => {
                let Some(to) = controller
                    .snapshot
                    .participants
                    .iter()
                    .find(|participant| key(&participant.identity) == peer)
                    .map(|participant| participant.identity.clone())
                else {
                    continue;
                };
                request(
                    world,
                    controller,
                    Operation::Signal {
                        call: controller.snapshot.call.clone().ok_or("Call ended")?,
                        to,
                        signal: outgoing(signal),
                    },
                    Pending::Signal,
                )?;
            }
            Event::Tracks {
                microphone,
                camera,
                screen,
                shared_audio,
            } => {
                controller.tracks = Tracks {
                    microphone,
                    camera,
                    screen,
                    shared_audio,
                };
                request(
                    world,
                    controller,
                    Operation::Tracks {
                        call: controller.snapshot.call.clone().ok_or("Call ended")?,
                        tracks: controller.tracks.clone(),
                    },
                    Pending::Tracks(None),
                )?;
            }
            Event::Connection { peer, state } => {
                controller.connections.insert(peer, state);
            }
            Event::Error(error) => panel::status(world, controller.status, error),
            Event::Ended => {
                controller.media = None;
                panel::status(
                    world,
                    controller.status,
                    "Media stopped. Join again to reconnect.",
                );
            }
        }
    }
    let visible = |key: &str| {
        let (peer, source) = key.rsplit_once('/').unwrap_or(("", ""));
        let tracks = if peer == "local" {
            Some(&controller.tracks)
        } else {
            controller
                .snapshot
                .participants
                .iter()
                .find(|person| self::key(&person.identity) == peer)
                .map(|person| &person.tracks)
        };
        controller.media.is_some()
            && tracks.is_some_and(|tracks| match source {
                "camera" => tracks.camera,
                "screen" => tracks.screen,
                _ => false,
            })
    };
    let stale: Vec<_> = controller
        .images
        .keys()
        .filter(|key| !visible(key))
        .cloned()
        .collect();
    frames.retain(|(key, _)| visible(key));
    for key in stale {
        if let Some((entity, _)) = controller.images.remove(&key) {
            if let Some(parent) = world.get::<ChildOf>(entity).map(ChildOf::parent) {
                world.despawn(parent);
            }
        }
    }
    for (key, frame) in frames {
        let (entity, mut handle) = controller.images.remove(&key).unwrap_or_else(|| {
            let column = panel::column(world, controller.videos);
            world.get_mut::<Node>(column).unwrap().width = px(210);
            let caption = if key.starts_with("local/") {
                format!("You · {}", key.rsplit('/').next().unwrap_or("video"))
            } else {
                controller
                    .snapshot
                    .participants
                    .iter()
                    .find(|person| key.starts_with(&format!("{}/", self::key(&person.identity))))
                    .map_or_else(
                        || "Participant".into(),
                        |person| {
                            format!(
                                "{} · {}",
                                person.name,
                                key.rsplit('/').next().unwrap_or("video")
                            )
                        },
                    )
            };
            crate::edit_mode::label(world, column, &caption, 12.0);
            let entity = world
                .spawn((
                    ChildOf(column),
                    Node {
                        width: px(210),
                        height: px(120),
                        ..default()
                    },
                ))
                .id();
            (entity, Handle::default())
        });
        super::texture(world, entity, &mut handle, frame);
        controller.images.insert(key, (entity, handle));
    }
    let participants = controller
        .snapshot
        .participants
        .iter()
        .map(|person| {
            format!(
                "{} · {} · {}{}{}{} · {}",
                person.name,
                person.organ_name,
                if person.tracks.microphone {
                    "mic "
                } else {
                    "muted "
                },
                if person.tracks.camera { "camera " } else { "" },
                if person.tracks.screen { "screen " } else { "" },
                if person.tracks.shared_audio {
                    "shared audio"
                } else {
                    ""
                },
                controller
                    .connections
                    .get(&key(&person.identity))
                    .map(String::as_str)
                    .unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    panel::status(world, controller.participants, participants);
    if let Some(preview) = &controller.preview {
        let status = preview.status();
        if status.devices != controller.devices {
            controller.devices = status.devices;
            panel::clear(world, controller.choices);
            for (name, items) in [
                ("Microphone", &controller.devices.microphones),
                ("Speaker", &controller.devices.speakers),
                ("Camera", &controller.devices.cameras),
                ("Screen", &controller.devices.screens),
                ("Application audio", &controller.devices.applications),
            ] {
                for choice in items {
                    let capture = match name {
                        "Microphone" => Capture::Microphone(Some(choice.id.clone())),
                        "Speaker" => Capture::Speaker(Some(choice.id.clone())),
                        "Camera" => Capture::Camera(choice.id.clone()),
                        "Screen" => Capture::Screen(choice.id.parse().ok()),
                        _ => Capture::SharedAudio(AudioInput::Application(
                            choice.id.parse().map_err(|_| "Invalid application")?,
                        )),
                    };
                    panel::button(
                        world,
                        controller.choices,
                        controller.root,
                        &format!("{name}: {}", choice.label),
                        Ui::Capture(capture),
                    );
                }
            }
        }
    }
    Ok(())
}
