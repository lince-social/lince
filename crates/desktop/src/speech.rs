use crate::{actions::Action, sand_panel as panel};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use bevy::{prelude::*, text::EditableText};
use cell::speech::{Request, Settings, Status};
use std::{
    sync::{Mutex, mpsc},
    time::{Duration, Instant},
};

const CREDITS: &[crate::credits::Attribution] = &[
    crate::credits::Attribution {
        name: "cpal",
        author: "The CPAL contributors",
        license: include_str!("../licenses/cpal-Apache-2.0.txt"),
    },
    crate::credits::Attribution {
        name: "hound",
        author: "Ruud van Asseldonk",
        license: include_str!("../licenses/hound-Apache-2.0.txt"),
    },
    crate::credits::Attribution {
        name: "base64",
        author: "The base64 contributors",
        license: include_str!("../licenses/base64-MIT.txt"),
    },
];

#[derive(Component)]
struct Composer {
    microphone: Option<String>,
    microphone_label: Entity,
    microphones: Entity,
    input: Entity,
    status: Entity,
    settings_panel: Entity,
    catalog: Entity,
    fields: [Entity; 5],
    cloud_label: Entity,
    cloud: bool,
    provider: String,
    saved: Option<Status>,
    pending: Option<String>,
    job: Option<String>,
    next: Instant,
    transcript: Option<String>,
    sender: Option<tokio::sync::mpsc::Sender<cell::ClientMessage>>,
}

impl Drop for Composer {
    fn drop(&mut self) {
        if let (Some(job), Some(sender)) = (&self.job, &self.sender) {
            let _ = sender.try_send(cell::ClientMessage::Speech {
                id: nucleus::new_uid("speech-cancel"),
                request: Request::Cancel { job: job.clone() },
            });
        }
    }
}

#[derive(Component)]
struct Recording {
    control: mpsc::SyncSender<bool>,
    result: Mutex<mpsc::Receiver<Result<String, String>>>,
    started: Instant,
}
impl Drop for Recording {
    fn drop(&mut self) {
        let _ = self.control.try_send(false);
    }
}

pub(crate) fn composer(world: &mut World, owner: Entity, parent: Entity, input: Entity) {
    let status = crate::edit_mode::label(
        world,
        parent,
        "Dictation inserts editable text and never sends it",
        12.0,
    );
    let row = panel::row(world, parent);
    for (title, action) in [
        ("Dictate", Command::Record),
        ("Stop and transcribe", Command::Stop),
        ("Cancel dictation", Command::Cancel),
        ("Speech settings", Command::Settings),
        ("Insert recognized text", Command::Insert),
    ] {
        panel::button(world, row, owner, title, action);
    }
    let settings_panel = panel::column(world, parent);
    world.get_mut::<Node>(settings_panel).unwrap().display = Display::None;
    world
        .entity_mut(settings_panel)
        .insert(crate::sand_store::SandCredits(CREDITS));
    crate::edit_mode::label(
        world,
        settings_panel,
        "Speech settings apply to all message drafts, including threads without Fiote. Stopping sends temporary audio to the selected provider; cloud transcription may cost money. Audio is not attached to the message.",
        13.0,
    );
    let settings = Settings::default();
    let fields = [
        panel::field(
            world,
            settings_panel,
            "Speech agent executable",
            &settings.command.to_string_lossy(),
        ),
        panel::field(world, settings_panel, "Arguments (JSON list)", "[\"acp\"]"),
        panel::field(
            world,
            settings_panel,
            "Agent working directory",
            &settings.directory.to_string_lossy(),
        ),
        panel::field(
            world,
            settings_panel,
            "Speech model · choose below or use agent default",
            "",
        ),
        crate::fiote::session::field(
            world,
            settings_panel,
            "Speech API key · optional, stored by the agent",
            true,
        ),
    ];
    let cloud_label =
        crate::edit_mode::label(world, settings_panel, "Cloud transcription: off", 13.0);
    panel::button(
        world,
        settings_panel,
        owner,
        "Toggle permission to send audio to cloud",
        Command::Cloud,
    );
    let controls = panel::row(world, settings_panel);
    panel::button(
        world,
        controls,
        owner,
        "Check available speech providers",
        Command::Check,
    );
    panel::button(
        world,
        controls,
        owner,
        "Save speech settings",
        Command::Save,
    );
    let catalog = panel::column(world, settings_panel);
    let microphone_label =
        crate::edit_mode::label(world, settings_panel, "Microphone: system default", 13.0);
    panel::button(
        world,
        settings_panel,
        owner,
        "Choose microphone",
        Command::Microphones,
    );
    let microphones = panel::column(world, settings_panel);
    let sender = world
        .get_non_send::<crate::cell_bridge::CellBridge>()
        .map(|bridge| bridge.outgoing.clone());
    world.entity_mut(owner).insert(Composer {
        microphone: None,
        microphone_label,
        microphones,
        input,
        status,
        settings_panel,
        catalog,
        fields,
        cloud_label,
        cloud: false,
        provider: String::new(),
        saved: None,
        pending: None,
        job: None,
        next: Instant::now(),
        transcript: None,
        sender,
    });
}

pub(crate) fn active(world: &World, owner: Entity) -> bool {
    world.get::<Recording>(owner).is_some()
        || world
            .get::<Composer>(owner)
            .is_some_and(|composer| composer.job.is_some())
}

fn settings(world: &World, owner: Entity) -> Result<Settings, String> {
    let composer = world
        .get::<Composer>(owner)
        .ok_or("The message draft is closed.")?;
    Ok(Settings {
        command: panel::value(world, composer.fields[0])?.into(),
        args: serde_json::from_str(&panel::value(world, composer.fields[1])?)
            .map_err(|_| "Agent arguments must be a JSON list.")?,
        directory: panel::value(world, composer.fields[2])?.into(),
        provider: composer.provider.clone(),
        model: panel::value(world, composer.fields[3])?,
        allow_cloud: composer.cloud,
    })
}

fn send(world: &mut World, owner: Entity, request: Request) -> Result<(), String> {
    let id = nucleus::new_uid("speech-control");
    panel::send(
        world,
        cell::ClientMessage::Speech {
            id: id.clone(),
            request,
        },
    )?;
    world
        .get_mut::<Composer>(owner)
        .ok_or("The message draft is closed.")?
        .pending = Some(id);
    Ok(())
}

#[derive(Clone)]
enum Command {
    Record,
    Stop,
    Cancel,
    Settings,
    Cloud,
    Check,
    Save,
    Insert,
    Microphones,
}
impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(status) = world.get::<Composer>(owner).map(|composer| composer.status) else {
            return;
        };
        let result = (|| {
            let composer = world.get::<Composer>(owner).unwrap();
            if matches!(self, Self::Cancel) {
                world.entity_mut(owner).remove::<Recording>();
                let job = world.get_mut::<Composer>(owner).unwrap().job.take();
                if let Some(job) = job {
                    world.get_mut::<Composer>(owner).unwrap().pending = None;
                    panel::send(
                        world,
                        cell::ClientMessage::Speech {
                            id: nucleus::new_uid("speech-release"),
                            request: Request::Cancel { job },
                        },
                    )?;
                }
                panel::status(
                    world,
                    status,
                    "Dictation cancelled. The draft is unchanged. A request already sent to a cloud provider may still be charged.",
                );
                return Ok(());
            }
            if matches!(self, Self::Stop) {
                if let Some(recording) = world.get::<Recording>(owner) {
                    recording
                        .control
                        .try_send(true)
                        .map_err(|_| "Dictation is already stopping.")?;
                    panel::status(world, status, "Finishing recording…");
                }
                return Ok(());
            }
            if matches!(self, Self::Settings) {
                let panel_entity = composer.settings_panel;
                let visible = world.get::<Node>(panel_entity).unwrap().display != Display::None;
                world.get_mut::<Node>(panel_entity).unwrap().display = if visible {
                    Display::None
                } else {
                    Display::Flex
                };
                if world.get::<Composer>(owner).unwrap().saved.is_none() {
                    send(world, owner, Request::Inspect { settings: None })?;
                    panel::status(
                        world,
                        status,
                        "Checking speech setup without recording or sending audio…",
                    );
                }
                return Ok(());
            }
            if matches!(self, Self::Insert) {
                return insert(world, owner);
            }
            if composer.pending.is_some() || active(world, owner) {
                return Err("Finish the current speech operation first.".into());
            }
            match self {
                Self::Microphones => {
                    let parent = composer.microphones;
                    let devices = crate::sound::device::Capture::devices()?;
                    world.entity_mut(parent).despawn_children();
                    panel::button(
                        world,
                        parent,
                        owner,
                        "System default microphone",
                        Microphone(None),
                    );
                    for device in devices {
                        panel::button(
                            world,
                            parent,
                            owner,
                            &device,
                            Microphone(Some(device.clone())),
                        );
                    }
                }
                Self::Cloud => {
                    let mut composer = world.get_mut::<Composer>(owner).unwrap();
                    composer.cloud = !composer.cloud;
                    let (label, enabled) = (composer.cloud_label, composer.cloud);
                    panel::status(
                        world,
                        label,
                        if enabled {
                            "Cloud transcription: allowed after saving"
                        } else {
                            "Cloud transcription: off"
                        },
                    );
                }
                Self::Check => {
                    let settings = settings(world, owner)?;
                    send(
                        world,
                        owner,
                        Request::Inspect {
                            settings: Some(settings),
                        },
                    )?;
                    panel::status(
                        world,
                        status,
                        "Checking speech setup without sending audio…",
                    );
                }
                Self::Save => {
                    let settings = settings(world, owner)?;
                    let input = world.get::<Composer>(owner).unwrap().fields[4];
                    let key = panel::value(world, input)?;
                    send(
                        world,
                        owner,
                        Request::Configure {
                            settings,
                            credential: (!key.is_empty()).then_some(cell::FioteSecret(key)),
                        },
                    )?;
                    world
                        .get_mut::<EditableText>(input)
                        .unwrap()
                        .editor
                        .set_text("");
                    panel::status(world, status, "Saving speech settings…");
                }
                Self::Record => {
                    #[cfg(feature = "native-media")]
                    if crate::communication::calls::microphone_active(world) {
                        return Err(
                            "Close the call preview or stop its microphone before dictating."
                                .into(),
                        );
                    }
                    if world
                        .get::<crate::message_content::Draft>(owner)
                        .is_some_and(|draft| draft.locked)
                    {
                        return Err("Wait for this message to finish sending.".into());
                    }
                    if world
                        .query_filtered::<Entity, With<Recording>>()
                        .iter(world)
                        .next()
                        .is_some()
                    {
                        return Err("Another message is using the microphone.".into());
                    }
                    let composer = world.get::<Composer>(owner).unwrap();
                    if composer.saved.as_ref().is_none_or(|saved| !saved.ready) {
                        return Err("Open Speech settings, choose a configured speech provider, then save and check it.".into());
                    }
                    if settings(world, owner)? != composer.saved.as_ref().unwrap().settings {
                        return Err("Save the changed speech settings before recording.".into());
                    }
                    let (control, commands) = mpsc::sync_channel(1);
                    let microphone = composer.microphone.clone();
                    let (finished, result) = mpsc::channel();
                    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
                    std::thread::spawn(move || {
                        let result = capture(commands, microphone);
                        let _ = finished.send(result);
                        if let Some(wake) = wake {
                            wake.ring();
                        }
                    });
                    world.entity_mut(owner).insert(Recording {
                        control,
                        result: Mutex::new(result),
                        started: Instant::now(),
                    });
                    panel::status(
                        world,
                        status,
                        "Recording · Stop adds one final editable text block · maximum 2 minutes",
                    );
                }
                _ => {}
            }
            Ok::<_, String>(())
        })();
        if let Err(error) = result {
            panel::status(world, status, error);
        }
    }
}

fn capture(commands: mpsc::Receiver<bool>, microphone: Option<String>) -> Result<String, String> {
    let capture = crate::sound::device::Capture::start_on(microphone.as_deref())?;
    loop {
        if let Some(error) = capture.error.lock().unwrap().clone() {
            return Err(error);
        }
        match commands.recv_timeout(Duration::from_millis(100)) {
            Ok(true) => break,
            Ok(false) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("Dictation cancelled".into());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if capture.full() {
                    break;
                }
            }
        }
    }
    encode(capture.finish())
}

fn encode(clip: crate::sound::library::Clip) -> Result<String, String> {
    if clip.samples.is_empty() || clip.samples.iter().all(|sample| sample.abs() < 0.000_01) {
        return Err("No microphone sound was captured. The draft is unchanged.".into());
    }
    let length = (clip.samples.len() as u64 * 16_000 / u64::from(clip.rate)) as usize;
    let mut bytes = std::io::Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(
            &mut bytes,
            hound::WavSpec {
                channels: 1,
                sample_rate: 16_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .map_err(|error| error.to_string())?;
        for index in 0..length {
            let start = index as f64 * f64::from(clip.rate) / 16_000.0;
            let end = ((index + 1) as f64 * f64::from(clip.rate) / 16_000.0).ceil() as usize;
            let samples = &clip.samples[start as usize..end.min(clip.samples.len())];
            let sample = samples.iter().copied().sum::<f32>() / samples.len().max(1) as f32;
            writer
                .write_sample((sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16)
                .map_err(|error| error.to_string())?;
        }
        writer.finalize().map_err(|error| error.to_string())?;
    }
    Ok(BASE64.encode(bytes.into_inner()))
}

#[derive(Clone)]
struct Provider(String);

#[derive(Clone)]
struct Microphone(Option<String>);
impl Action for Microphone {
    fn apply(&self, world: &mut World, owner: Entity) {
        if active(world, owner) {
            return;
        }
        let Some(mut composer) = world.get_mut::<Composer>(owner) else {
            return;
        };
        composer.microphone = self.0.clone();
        let label = composer.microphone_label;
        panel::status(
            world,
            label,
            format!(
                "Microphone: {}",
                self.0.as_deref().unwrap_or("system default")
            ),
        );
    }
}
impl Action for Provider {
    fn apply(&self, world: &mut World, owner: Entity) {
        if active(world, owner) {
            return;
        }
        let Some(mut composer) = world.get_mut::<Composer>(owner) else {
            return;
        };
        composer.provider = self.0.clone();
        let model = composer
            .saved
            .as_ref()
            .and_then(|saved| {
                saved
                    .providers
                    .iter()
                    .find(|provider| provider.id == self.0)
            })
            .map(|provider| provider.selected_model.clone())
            .unwrap_or_default();
        let field = composer.fields[3];
        world
            .get_mut::<EditableText>(field)
            .unwrap()
            .editor
            .set_text(&model);
        catalog(world, owner);
    }
}

#[derive(Clone)]
struct Model(String);
impl Action for Model {
    fn apply(&self, world: &mut World, owner: Entity) {
        if active(world, owner) {
            return;
        }
        if let Some(field) = world
            .get::<Composer>(owner)
            .map(|composer| composer.fields[3])
        {
            world
                .get_mut::<EditableText>(field)
                .unwrap()
                .editor
                .set_text(&self.0);
        }
    }
}

fn catalog(world: &mut World, owner: Entity) {
    let composer = world.get::<Composer>(owner).unwrap();
    let (parent, saved, selected) = (
        composer.catalog,
        composer.saved.clone(),
        composer.provider.clone(),
    );
    world.entity_mut(parent).despawn_children();
    let Some(saved) = saved else { return };
    if !saved
        .providers
        .iter()
        .any(|provider| provider.local && provider.configured)
    {
        crate::edit_mode::label(
            world,
            parent,
            "No ready local transcription backend is reported by this installation.",
            13.0,
        );
    }
    for provider in &saved.providers {
        panel::button(
            world,
            parent,
            owner,
            &format!(
                "{}{} · {} · {}",
                if provider.id == selected {
                    "Selected: "
                } else {
                    ""
                },
                provider.id,
                if provider.local { "local" } else { "cloud" },
                if provider.configured {
                    "configured"
                } else {
                    "needs setup"
                }
            ),
            Provider(provider.id.clone()),
        );
        if provider.id == selected {
            crate::edit_mode::label(world, parent, &provider.description, 12.0);
            for model in &provider.models {
                panel::button(
                    world,
                    parent,
                    owner,
                    &format!("Use {}", model.label),
                    Model(model.id.clone()),
                );
            }
        }
    }
}

fn insert(world: &mut World, owner: Entity) -> Result<(), String> {
    let composer = world.get::<Composer>(owner).ok_or("The draft is closed.")?;
    let Some(transcript) = composer.transcript.as_ref() else {
        return Ok(());
    };
    let (input, status) = (composer.input, composer.status);
    let existing = world
        .get::<EditableText>(input)
        .ok_or("The draft is closed.")?
        .value()
        .to_string();
    let combined = append(&existing, transcript)?;
    if world
        .get::<crate::message_content::Draft>(owner)
        .is_some_and(|draft| draft.locked)
    {
        return Err("The message is sending. Insert the recognized text afterward.".into());
    }
    world
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text(&combined);
    world.get_mut::<Composer>(owner).unwrap().transcript = None;
    panel::status(
        world,
        status,
        "Transcript added to the draft. Edit it, then send when ready.",
    );
    Ok(())
}

fn append(existing: &str, transcript: &str) -> Result<String, String> {
    let combined = format!(
        "{existing}{}{transcript}",
        if existing.is_empty() || existing.ends_with('\n') {
            ""
        } else {
            "\n\n"
        }
    );
    if combined.len() > 65_536 {
        return Err(
            "The draft is too long. Shorten it, then choose Insert recognized text.".into(),
        );
    }
    Ok(combined)
}

pub(crate) fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let events: Vec<_> = world
        .get_resource::<Messages<crate::cell_bridge::CellMessage>>()
        .map(|messages| {
            cursor
                .read(messages)
                .map(|message| message.0.clone())
                .collect()
        })
        .unwrap_or_default();
    for event in events {
        if let cell::ServerMessage::Error { id, message, .. } = &event {
            if id == crate::cell_bridge::CONNECTION {
                let owners: Vec<_> = world
                    .query_filtered::<Entity, With<Composer>>()
                    .iter(world)
                    .collect();
                for owner in owners {
                    world.entity_mut(owner).remove::<Recording>();
                    let mut composer = world.get_mut::<Composer>(owner).unwrap();
                    composer.pending = None;
                    composer.job = None;
                    if let Some(saved) = composer.saved.as_mut() {
                        saved.ready = false;
                    }
                    let status = composer.status;
                    panel::status(world, status, format!("Dictation stopped: {message}"));
                }
                continue;
            }
        }
        let (id, result) = match event {
            cell::ServerMessage::Speech { id, status } => (id, Ok(status)),
            cell::ServerMessage::Error { id, message, .. } => (id, Err(message)),
            _ => continue,
        };
        let owner = world
            .query::<(Entity, &Composer)>()
            .iter(world)
            .find(|(_, composer)| composer.pending.as_deref() == Some(&id))
            .map(|(entity, _)| entity);
        let Some(owner) = owner else { continue };
        let mut composer = world.get_mut::<Composer>(owner).unwrap();
        composer.pending = None;
        let status_entity = composer.status;
        match result {
            Err(error) => {
                composer.job = None;
                panel::status(world, status_entity, error);
            }
            Ok(saved) => {
                if let Some(job) = saved
                    .job
                    .as_ref()
                    .filter(|job| composer.job.as_deref() == Some(&job.id))
                {
                    if !job.pending {
                        composer.job = None;
                        composer.transcript = job.text.clone();
                        if let Some(error) = &job.error {
                            panel::status(world, status_entity, error);
                        } else if let Err(error) = insert(world, owner) {
                            panel::status(world, status_entity, error);
                        }
                        let _ = panel::send(
                            world,
                            cell::ClientMessage::Speech {
                                id: nucleus::new_uid("speech-release"),
                                request: Request::Cancel {
                                    job: job.id.clone(),
                                },
                            },
                        );
                    } else {
                        composer.next = Instant::now() + Duration::from_millis(250);
                        panel::status(
                            world,
                            status_entity,
                            "Transcribing · your message remains unsent",
                        );
                    }
                    world.get_mut::<Composer>(owner).unwrap().saved = Some(saved);
                } else {
                    let fields = composer.fields;
                    let cloud_label = composer.cloud_label;
                    composer.cloud = saved.settings.allow_cloud;
                    composer.provider = saved.settings.provider.clone();
                    composer.saved = Some(saved.clone());
                    for (field, value) in fields[..4].iter().zip([
                        saved.settings.command.to_string_lossy().to_string(),
                        serde_json::to_string(&saved.settings.args).unwrap(),
                        saved.settings.directory.to_string_lossy().to_string(),
                        saved.settings.model.clone(),
                    ]) {
                        world
                            .get_mut::<EditableText>(*field)
                            .unwrap()
                            .editor
                            .set_text(&value);
                    }
                    panel::status(
                        world,
                        cloud_label,
                        if saved.settings.allow_cloud {
                            "Cloud transcription: allowed"
                        } else {
                            "Cloud transcription: off"
                        },
                    );
                    panel::status(world, status_entity, &saved.detail);
                    catalog(world, owner);
                }
            }
        }
    }
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<Composer>>()
        .iter(world)
        .collect();
    for owner in owners {
        #[cfg(feature = "native-media")]
        if world.get::<Recording>(owner).is_some()
            && crate::communication::calls::microphone_active(world)
        {
            Command::Cancel.apply(world, owner);
            let status = world.get::<Composer>(owner).unwrap().status;
            panel::status(
                world,
                status,
                "Dictation cancelled because a call started using the microphone.",
            );
        }
        let result = world
            .get::<Recording>(owner)
            .and_then(|recording| recording.result.lock().unwrap().try_recv().ok());
        let status = world.get::<Composer>(owner).unwrap().status;
        if let Some(result) = result {
            world.entity_mut(owner).remove::<Recording>();
            match result {
                Ok(audio) => {
                    let job = nucleus::new_uid("speech");
                    match send(
                        world,
                        owner,
                        Request::Start {
                            job: job.clone(),
                            settings: world
                                .get::<Composer>(owner)
                                .unwrap()
                                .saved
                                .as_ref()
                                .unwrap()
                                .settings
                                .clone(),
                            audio: cell::FioteSecret(audio),
                        },
                    ) {
                        Ok(()) => {
                            world.get_mut::<Composer>(owner).unwrap().job = Some(job);
                            panel::status(
                                world,
                                status,
                                "Transcribing · your message remains unsent",
                            );
                        }
                        Err(error) => panel::status(world, status, error),
                    }
                }
                Err(error) => panel::status(world, status, error),
            }
        }
        if let Some(recording) = world.get::<Recording>(owner) {
            panel::status(
                world,
                status,
                format!(
                    "Recording · {} seconds · Stop and transcribe adds one editable block",
                    recording.started.elapsed().as_secs()
                ),
            );
        }
        let composer = world.get::<Composer>(owner).unwrap();
        if composer.pending.is_none()
            && composer.next <= Instant::now()
            && let Some(job) = composer.job.clone()
        {
            let _ = send(world, owner, Request::Poll { job });
        }
        if active(world, owner)
            && let Some(wake) = world.get_resource::<crate::wake::WakeSignal>()
        {
            wake.after(Duration::from_millis(250));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn waiting(world: &mut World, request: &str, job: &str, body: &str) -> (Entity, Entity) {
        let input = world.spawn(EditableText::new(body)).id();
        let status = world.spawn(Text::default()).id();
        let unused = world.spawn(Node::default()).id();
        let owner = world
            .spawn(Composer {
                microphone: None,
                microphone_label: status,
                microphones: unused,
                input,
                status,
                settings_panel: unused,
                catalog: unused,
                fields: [input; 5],
                cloud_label: status,
                cloud: false,
                provider: "fixture".into(),
                saved: None,
                pending: Some(request.into()),
                job: Some(job.into()),
                next: Instant::now() + Duration::from_secs(3600),
                transcript: None,
                sender: None,
            })
            .id();
        (owner, input)
    }

    fn result(request: &str, job: &str) -> crate::cell_bridge::CellMessage {
        crate::cell_bridge::CellMessage(cell::ServerMessage::Speech {
            id: request.into(),
            status: Status {
                settings: Default::default(),
                providers: Vec::new(),
                ready: true,
                detail: String::new(),
                job: Some(cell::speech::Job {
                    id: job.into(),
                    pending: false,
                    text: Some("Recognized words".into()),
                    error: None,
                }),
            },
        })
    }

    #[test]
    fn late_transcripts_only_update_their_original_draft_once() {
        let mut app = App::new();
        app.add_message::<crate::cell_bridge::CellMessage>()
            .add_systems(Update, update);
        let (_, first) = waiting(
            app.world_mut(),
            "request-a",
            "job-a",
            "Edited while transcribing",
        );
        let (second_owner, second) =
            waiting(app.world_mut(), "request-b", "job-b", "Another thread");
        app.world_mut().write_message(result("request-a", "job-a"));
        app.update();
        assert_eq!(
            app.world()
                .get::<EditableText>(first)
                .unwrap()
                .value()
                .to_string(),
            "Edited while transcribing\n\nRecognized words"
        );
        assert_eq!(
            app.world()
                .get::<EditableText>(second)
                .unwrap()
                .value()
                .to_string(),
            "Another thread"
        );
        app.world_mut().write_message(result("request-a", "job-a"));
        app.update();
        assert_eq!(
            app.world()
                .get::<EditableText>(first)
                .unwrap()
                .value()
                .to_string()
                .matches("Recognized words")
                .count(),
            1
        );
        app.world_mut().despawn(second_owner);
        let (third_owner, third) = waiting(app.world_mut(), "request-c", "job-c", "New draft");
        app.world_mut().write_message(result("request-b", "job-b"));
        app.update();
        assert_eq!(
            app.world()
                .get::<EditableText>(third)
                .unwrap()
                .value()
                .to_string(),
            "New draft"
        );
        app.world_mut()
            .write_message(crate::cell_bridge::CellMessage(
                cell::ServerMessage::Error {
                    id: crate::cell_bridge::CONNECTION.into(),
                    message: "Disconnected".into(),
                    code: None,
                },
            ));
        app.update();
        assert!(
            app.world()
                .get::<Composer>(third_owner)
                .unwrap()
                .job
                .is_none()
        );
        assert!(!active(app.world(), third_owner));
    }
    #[test]
    fn final_transcript_preserves_edits_and_respects_limit() {
        assert_eq!(
            append("Manually edited", "spoken words").unwrap(),
            "Manually edited\n\nspoken words"
        );
        assert_eq!(append("", "spoken words").unwrap(), "spoken words");
        assert!(append(&"a".repeat(65_536), "words").is_err());
    }
    #[test]
    fn capture_encoding_is_bounded_mono_pcm_and_silence_is_rejected() {
        let audio = encode(crate::sound::library::Clip {
            samples: vec![0.25; 48_000],
            rate: 48_000,
        })
        .unwrap();
        let bytes = BASE64.decode(audio).unwrap();
        let reader = hound::WavReader::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(reader.duration(), 16_000);
        assert_eq!(reader.spec().channels, 1);
        assert!(
            encode(crate::sound::library::Clip {
                samples: vec![0.0; 10],
                rate: 16_000
            })
            .is_err()
        );
    }
}
