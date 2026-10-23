use crate::native::{actions::Action, sand_panel as panel};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use bevy::{prelude::*, text::EditableText};
use cell::speech::{Request, Status};
use std::{
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

pub(crate) mod readiness;

const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "Earshot voice activity detection",
        author: "Carson M / pyke.io and Earshot contributors",
        license: concat!(
            include_str!("../../../fiote/assets/earshot/NOTICE"),
            "\n",
            include_str!("../../../fiote/assets/earshot/LICENSE-MIT.txt"),
        ),
    },
    crate::native::credits::Attribution {
        name: "phonon-rs and Candle",
        author: "The phonon-rs and Hugging Face Candle contributors",
        license: include_str!("../../../fiote/assets/phonon/LICENSE-CODE-Apache-2.0.txt"),
    },
    crate::native::credits::Attribution {
        name: "Phonon-2 model weights",
        author: "Fermion Research; derived from NVIDIA Parakeet",
        license: include_str!("../../../fiote/assets/phonon/LICENSE-WEIGHTS-CC-BY-4.0.txt"),
    },
    crate::native::credits::Attribution {
        name: "Phonon-2 attribution and changes",
        author: "Fermion Research",
        license: include_str!("../../../fiote/assets/phonon/NOTICE"),
    },
    crate::native::credits::Attribution {
        name: "Moonshine speech recognition",
        author: "Moonshine AI and Hugging Face contributors; Rust implementation by Lince",
        license: concat!(
            include_str!("../../../fiote/assets/moonshine/NOTICE"),
            "\n",
            include_str!("../../../fiote/assets/moonshine/LICENSE-CODE-Apache-2.0.txt"),
            "\n",
            include_str!("../../../fiote/assets/moonshine/LICENSE-WEIGHTS-MIT.txt"),
        ),
    },
    crate::native::credits::Attribution {
        name: "ruzstd",
        author: "The ruzstd contributors",
        license: include_str!("../../licenses/native/ruzstd-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "tar and Rayon",
        author: "The tar-rs and Rayon contributors",
        license: include_str!("../../../fiote/assets/phonon/LICENSE-CODE-Apache-2.0.txt"),
    },
    crate::native::credits::Attribution {
        name: "cpal",
        author: "The CPAL contributors",
        license: include_str!("../../licenses/native/cpal-Apache-2.0.txt"),
    },
    crate::native::credits::Attribution {
        name: "hound",
        author: "Ruud van Asseldonk",
        license: include_str!("../../licenses/native/hound-Apache-2.0.txt"),
    },
    crate::native::credits::Attribution {
        name: "base64",
        author: "The base64 contributors",
        license: include_str!("../../licenses/native/base64-MIT.txt"),
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
    endpoint_fields: [Entity; 4],
    local_status: Entity,
    local_import: Entity,
    requires_key: bool,
    key_label: Entity,
    cloud_label: Entity,
    cloud: bool,
    saved: Option<Status>,
    pending: Option<String>,
    job: Option<String>,
    next: Instant,
    transcript: Option<String>,
    cancellation: Option<crate::native::cell_bridge::SpeechCancellation>,
}

impl Drop for Composer {
    fn drop(&mut self) {
        if let (Some(job), Some(cancellation)) = (&self.job, &self.cancellation) {
            let _ = cancellation.send(Request::Cancel { job: job.clone() });
        }
    }
}

#[derive(Component)]
struct Recording {
    mode: cell::speech::Mode,
    endpoint: Option<cell::speech::endpoint::Settings>,
    control: mpsc::SyncSender<bool>,
    result: Mutex<mpsc::Receiver<Result<String, String>>>,
    started: Arc<Mutex<Option<Instant>>>,
}
impl Drop for Recording {
    fn drop(&mut self) {
        let _ = self.control.try_send(false);
    }
}

pub(crate) fn composer(world: &mut World, owner: Entity, parent: Entity, input: Entity) {
    let status = crate::native::edit_mode::label(
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
        .insert(crate::native::sand_store::SandCredits(CREDITS));
    crate::native::edit_mode::label(
        world,
        settings_panel,
        "The chosen local model serves dictation and general Fiote listening. Download a supported English model or import its compatible files. Moonshine Tiny uses about 355 MiB RAM, Small about 1.1 GiB, and Phonon-2 about 2.6 GiB. Installation keeps the microphone off. Transcription endpoints serve dictation and receive its recording. Dictation stays editable and unsent.",
        13.0,
    );
    let local_status = crate::native::edit_mode::label(
        world,
        settings_panel,
        "Local English speech: not checked",
        13.0,
    );
    panel::button(
        world,
        settings_panel,
        owner,
        "Use local English speech",
        Command::Local,
    );
    for model in cell::speech::ModelId::ALL {
        panel::button(
            world,
            settings_panel,
            owner,
            model.label(),
            Command::SelectLocal(model),
        );
    }
    panel::button(
        world,
        settings_panel,
        owner,
        "Download English speech model",
        Command::DownloadLocal,
    );
    let local_import = panel::field(
        world,
        settings_panel,
        "Phonon archive or Moonshine model directory",
        "",
    );
    panel::button(
        world,
        settings_panel,
        owner,
        "Import English speech model",
        Command::ImportLocal,
    );
    panel::button(
        world,
        settings_panel,
        owner,
        "Use saved transcription endpoint",
        Command::Endpoint,
    );
    crate::native::edit_mode::label(
        world,
        settings_panel,
        "Compatible transcription endpoint",
        18.0,
    );
    let endpoint_fields = [
        panel::field(world, settings_panel, "Full transcription URL", ""),
        panel::field(world, settings_panel, "Transcription model", ""),
        crate::native::fiote::session::field(
            world,
            settings_panel,
            "Transcription API key (vault)",
            true,
        ),
        crate::native::fiote::session::field(world, settings_panel, "Vault password", true),
    ];
    let key_label = crate::native::edit_mode::label(
        world,
        settings_panel,
        "Endpoint API key required: no",
        13.0,
    );
    panel::button(
        world,
        settings_panel,
        owner,
        "Toggle endpoint key requirement",
        Command::KeyRequired,
    );
    panel::button(
        world,
        settings_panel,
        owner,
        "Save transcription endpoint",
        Command::SaveEndpoint,
    );
    panel::button(
        world,
        settings_panel,
        owner,
        "Unlock transcription vault",
        Command::UnlockEndpoint,
    );
    let cloud_label =
        crate::native::edit_mode::label(world, settings_panel, "Cloud transcription: off", 13.0);
    panel::button(
        world,
        settings_panel,
        owner,
        "Toggle permission to send audio to cloud",
        Command::Cloud,
    );
    panel::button(
        world,
        settings_panel,
        owner,
        "Check saved endpoint",
        Command::Check,
    );
    let microphone_label =
        crate::native::edit_mode::label(world, settings_panel, "Microphone: system default", 13.0);
    panel::button(
        world,
        settings_panel,
        owner,
        "Choose microphone",
        Command::Microphones,
    );
    let microphones = panel::column(world, settings_panel);
    let cancellation = crate::native::cell_bridge::SpeechCancellation::for_owner(world, owner);
    world.entity_mut(owner).insert(Composer {
        microphone: None,
        microphone_label,
        microphones,
        input,
        status,
        settings_panel,
        endpoint_fields,
        local_status,
        local_import,
        requires_key: false,
        key_label,
        cloud_label,
        cloud: false,
        saved: None,
        pending: None,
        job: None,
        next: Instant::now(),
        transcript: None,
        cancellation,
    });
}

pub(crate) fn active(world: &World, owner: Entity) -> bool {
    world.get::<Recording>(owner).is_some()
        || world.get::<Composer>(owner).is_some_and(|composer| {
            composer.job.is_some()
                || composer.pending.is_some()
                || composer
                    .saved
                    .as_ref()
                    .and_then(|saved| saved.local.as_ref())
                    .is_some_and(|local| local.busy)
        })
}

pub(crate) fn any_recording(world: &mut World) -> bool {
    world
        .query_filtered::<Entity, With<Recording>>()
        .iter(world)
        .next()
        .is_some()
}

pub(crate) fn call_microphone_active(world: &World) -> bool {
    #[cfg(feature = "native-media")]
    {
        crate::native::communication::calls::microphone_active(world)
    }
    #[cfg(not(feature = "native-media"))]
    {
        let _ = world;
        false
    }
}

fn send(world: &mut World, owner: Entity, request: Request) -> Result<(), String> {
    let cancellation = crate::native::cell_bridge::SpeechCancellation::for_owner(world, owner);
    if matches!(&request, Request::Cancel { .. }) {
        let cancellation = world
            .get::<Composer>(owner)
            .and_then(|composer| composer.cancellation.clone())
            .or(cancellation)
            .ok_or("The speech connection is unavailable.")?;
        let id = cancellation.send(request)?;
        let mut composer = world
            .get_mut::<Composer>(owner)
            .ok_or("The message draft is closed.")?;
        composer.pending = Some(id);
        composer.cancellation = Some(cancellation);
        return Ok(());
    }
    let id = nucleus::new_uid("speech-control");
    panel::send(
        world,
        owner,
        cell::ClientMessage::Speech {
            id: id.clone(),
            request,
        },
    )?;
    let mut composer = world
        .get_mut::<Composer>(owner)
        .ok_or("The message draft is closed.")?;
    composer.pending = Some(id);
    composer.cancellation = cancellation;
    Ok(())
}

#[derive(Clone)]
enum Command {
    Local,
    SelectLocal(cell::speech::ModelId),
    Endpoint,
    DownloadLocal,
    ImportLocal,
    Record,
    Stop,
    Cancel,
    Settings,
    Cloud,
    Check,
    Insert,
    Microphones,
    SaveEndpoint,
    UnlockEndpoint,
    KeyRequired,
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
                    send(world, owner, Request::Cancel { job })?;
                }
                panel::status(
                    world,
                    status,
                    "Dictation cancelled. The draft is unchanged.",
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
                    send(world, owner, Request::Inspect)?;
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
                Self::SelectLocal(model) => {
                    send(world, owner, Request::SelectLocal { model: *model })?;
                }
                Self::Local | Self::Endpoint => {
                    send(
                        world,
                        owner,
                        Request::SelectMode {
                            mode: if matches!(self, Self::Local) {
                                cell::speech::Mode::Local
                            } else {
                                cell::speech::Mode::Endpoint
                            },
                        },
                    )?;
                }
                Self::DownloadLocal | Self::ImportLocal => {
                    let source = if matches!(self, Self::ImportLocal) {
                        let value = panel::value(world, composer.local_import)?;
                        if value.trim().is_empty() {
                            return Err(
                                "Choose a compatible Phonon archive or Moonshine model directory."
                                    .into(),
                            );
                        }
                        Some(std::path::PathBuf::from(value))
                    } else {
                        None
                    };
                    let job = nucleus::new_uid("speech");
                    send(
                        world,
                        owner,
                        Request::InstallLocal {
                            job: job.clone(),
                            source,
                        },
                    )?;
                    world.get_mut::<Composer>(owner).unwrap().job = Some(job);
                    panel::status(
                        world,
                        status,
                        "Preparing local English speech · your draft is unchanged",
                    );
                }
                Self::KeyRequired => {
                    let mut composer = world.get_mut::<Composer>(owner).unwrap();
                    composer.requires_key = !composer.requires_key;
                    let (label, enabled) = (composer.key_label, composer.requires_key);
                    panel::status(
                        world,
                        label,
                        if enabled {
                            "Endpoint API key required: yes"
                        } else {
                            "Endpoint API key required: no"
                        },
                    );
                }
                Self::SaveEndpoint => {
                    let fields = composer.endpoint_fields;
                    let mut settings = cell::speech::endpoint::Settings {
                        url: panel::value(world, fields[0])?,
                        model: panel::value(world, fields[1])?,
                        allow_cloud: composer.cloud,
                        requires_key: composer.requires_key,
                    };
                    settings.validate()?;
                    let credential = panel::value(world, fields[2])?;
                    let password = panel::value(world, fields[3])?;
                    send(
                        world,
                        owner,
                        Request::ConfigureEndpoint {
                            settings,
                            credential: (!credential.is_empty())
                                .then_some(cell::FioteSecret(credential)),
                            password: (!password.is_empty()).then_some(cell::FioteSecret(password)),
                        },
                    )?;
                    for field in &fields[2..] {
                        world
                            .get_mut::<EditableText>(*field)
                            .unwrap()
                            .editor
                            .set_text("");
                    }
                }
                Self::UnlockEndpoint => {
                    let field = composer.endpoint_fields[3];
                    let password = panel::value(world, field)?;
                    send(
                        world,
                        owner,
                        Request::UnlockEndpoint {
                            password: cell::FioteSecret(password),
                        },
                    )?;
                    world
                        .get_mut::<EditableText>(field)
                        .unwrap()
                        .editor
                        .set_text("");
                }
                Self::Microphones => {
                    let parent = composer.microphones;
                    let devices = crate::native::sound::device::Capture::devices()?;
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
                    send(world, owner, Request::Inspect)?;
                    panel::status(
                        world,
                        status,
                        "Checking speech setup without sending audio…",
                    );
                }
                Self::Record => {
                    if crate::native::sound::speaking(world) {
                        return Err("Stop speaking before starting dictation.".into());
                    }
                    #[cfg(feature = "native-media")]
                    if crate::native::communication::calls::microphone_active(world) {
                        return Err(
                            "Close the call preview or stop its microphone before dictating."
                                .into(),
                        );
                    }
                    if world
                        .get::<crate::native::message_content::Draft>(owner)
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
                        return Err("Open Speech settings and download/import the local English model, or configure an endpoint.".into());
                    }
                    let saved = composer.saved.as_ref().unwrap();
                    let mode = saved.mode;
                    let endpoint = saved.endpoint.clone();
                    if mode == cell::speech::Mode::Endpoint {
                        let endpoint = endpoint
                            .as_ref()
                            .ok_or("Save a transcription endpoint before recording.")?;
                        let fields = composer.endpoint_fields;
                        let mut draft = cell::speech::endpoint::Settings {
                            url: panel::value(world, fields[0])?,
                            model: panel::value(world, fields[1])?,
                            allow_cloud: composer.cloud,
                            requires_key: composer.requires_key,
                        };
                        draft.validate()?;
                        if &draft != endpoint {
                            return Err(
                                "Save the changed transcription endpoint before recording.".into(),
                            );
                        }
                    }
                    let (control, commands) = mpsc::sync_channel(1);
                    let microphone = composer.microphone.clone();
                    let (finished, result) = mpsc::channel();
                    let wake = world
                        .get_resource::<crate::native::wake::WakeSignal>()
                        .cloned();
                    let started = Arc::new(Mutex::new(None));
                    let ready = started.clone();
                    std::thread::spawn(move || {
                        let result = capture(commands, microphone, || {
                            *ready.lock().unwrap() = Some(Instant::now());
                            if let Some(wake) = &wake {
                                wake.ring();
                            }
                        });
                        let _ = finished.send(result);
                        if let Some(wake) = wake {
                            wake.ring();
                        }
                    });
                    world.entity_mut(owner).insert(Recording {
                        mode,
                        endpoint,
                        control,
                        result: Mutex::new(result),
                        started,
                    });
                    panel::status(
                        world,
                        status,
                        "Preparing microphone · wait for Recording before speaking · Stop or Cancel ends setup",
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

fn capture(
    commands: mpsc::Receiver<bool>,
    microphone: Option<String>,
    ready: impl FnOnce(),
) -> Result<String, String> {
    if !matches!(commands.try_recv(), Err(mpsc::TryRecvError::Empty)) {
        return Err("Dictation cancelled before microphone startup.".into());
    }
    let capture = crate::native::sound::device::Capture::start_on(microphone.as_deref())?;
    let mut clip = readiness::prepare(&capture, || {
        !matches!(commands.try_recv(), Err(mpsc::TryRecvError::Empty))
    })?;
    let limit = clip.rate as usize * 120;
    let deadline = Instant::now()
        + Duration::from_secs_f64(
            limit.saturating_sub(clip.samples.len()) as f64 / f64::from(clip.rate),
        );
    ready();
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
                if capture.full() || Instant::now() >= deadline {
                    break;
                }
            }
        }
    }
    clip.samples.extend(capture.finish().samples);
    clip.samples.truncate(limit);
    encode(clip)
}

pub(crate) fn encode(clip: crate::native::sound::library::Clip) -> Result<String, String> {
    if clip.samples.is_empty() || clip.samples.iter().all(|sample| sample.abs() < 0.000_01) {
        return Err("No microphone sound was captured. The draft is unchanged.".into());
    }
    let normalized = mono16k(&clip)?;
    CaptureQuality::default().push(&clip)?;
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
        for sample in normalized {
            writer
                .write_sample((sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16)
                .map_err(|error| error.to_string())?;
        }
        writer.finalize().map_err(|error| error.to_string())?;
    }
    Ok(BASE64.encode(bytes.into_inner()))
}

pub(crate) fn mono16k(clip: &crate::native::sound::library::Clip) -> Result<Vec<f32>, String> {
    Resampler::new(clip.rate)?.push(clip)
}

#[derive(Default)]
pub(crate) struct CaptureQuality(cell::fiote_communication::speech::input::Quality);

impl CaptureQuality {
    pub(crate) fn push(
        &mut self,
        clip: &crate::native::sound::library::Clip,
    ) -> Result<(), String> {
        self.0.push(clip.rate, &clip.samples)
    }
}

pub(crate) struct Resampler {
    rate: u32,
    ticks: u32,
    sum: f64,
}

impl Resampler {
    pub(crate) fn new(rate: u32) -> Result<Self, String> {
        if !(8000..=192_000).contains(&rate) {
            return Err("Capture contains an unsupported sample rate.".into());
        }
        Ok(Self {
            rate,
            ticks: 0,
            sum: 0.0,
        })
    }

    pub(crate) fn push(
        &mut self,
        clip: &crate::native::sound::library::Clip,
    ) -> Result<Vec<f32>, String> {
        if clip.rate != self.rate
            || clip.samples.len() > clip.rate as usize * 120
            || clip.samples.iter().any(|sample| !sample.is_finite())
        {
            return Err("Capture contains unsupported or invalid mono PCM.".into());
        }
        let mut output = Vec::with_capacity(
            (clip.samples.len() as u64 * 16_000 / u64::from(self.rate)) as usize + 1,
        );
        for sample in &clip.samples {
            let mut remaining = 16_000;
            while remaining > 0 {
                let duration = remaining.min(self.rate - self.ticks);
                self.sum += f64::from(*sample) * f64::from(duration);
                self.ticks += duration;
                remaining -= duration;
                if self.ticks == self.rate {
                    output.push((self.sum / f64::from(self.rate)).clamp(-1.0, 1.0) as f32);
                    self.ticks = 0;
                    self.sum = 0.0;
                }
            }
        }
        Ok(output)
    }
}

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
        .get::<crate::native::message_content::Draft>(owner)
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

pub(crate) fn append(existing: &str, transcript: &str) -> Result<String, String> {
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
        if let cell::ServerMessage::Error { id, message, .. } = &event {
            if id == crate::native::cell_bridge::CONNECTION {
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
                if let Some(local) = &saved.local {
                    let selected = if saved.mode == cell::speech::Mode::Local {
                        "selected"
                    } else {
                        "available"
                    };
                    let detail = if local.busy {
                        "working"
                    } else if local.loaded {
                        "loaded"
                    } else if local.present {
                        "installed; loads on first use"
                    } else {
                        "model not installed"
                    };
                    let label = composer.local_status;
                    drop(composer);
                    panel::status(
                        world,
                        label,
                        format!(
                            "{} · {selected} · {detail} · {} / {} MB acquired",
                            saved.local_selection.label(),
                            local.acquired_bytes / 1_000_000,
                            local.required_bytes / 1_000_000
                        ),
                    );
                    composer = world.get_mut::<Composer>(owner).unwrap();
                }
                if let Some(job) = saved
                    .job
                    .as_ref()
                    .filter(|job| composer.job.as_deref() == Some(&job.id))
                {
                    if !job.pending {
                        composer.job = None;
                        let setup = job.purpose != cell::speech::Purpose::Transcription;
                        if !setup {
                            composer.transcript = job.text.clone();
                        }
                        if let Some(error) = &job.error {
                            panel::status(world, status_entity, error);
                        } else if setup {
                            panel::status(
                                world,
                                status_entity,
                                job.text.as_deref().unwrap_or(&saved.detail),
                            );
                        } else if let Err(error) = insert(world, owner) {
                            panel::status(world, status_entity, error);
                        }
                        let _ = panel::send(
                            world,
                            owner,
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
                            if job.purpose == cell::speech::Purpose::ModelSetup {
                                "Preparing local English speech · your draft is unchanged"
                            } else if saved
                                .local
                                .as_ref()
                                .is_some_and(|local| local.busy && !local.loaded)
                            {
                                "Loading local English speech · your message remains unsent"
                            } else {
                                "Transcribing · your message remains unsent"
                            },
                        );
                    }
                    world.get_mut::<Composer>(owner).unwrap().saved = Some(saved);
                } else {
                    let cloud_label = composer.cloud_label;
                    composer.cloud = saved
                        .endpoint
                        .as_ref()
                        .is_some_and(|endpoint| endpoint.allow_cloud);
                    composer.saved = Some(saved.clone());
                    let endpoint_fields = composer.endpoint_fields;
                    let key_label = composer.key_label;
                    if let Some(endpoint) = &saved.endpoint {
                        composer.requires_key = endpoint.requires_key;
                        let enabled = endpoint.requires_key;
                        for (field, value) in endpoint_fields[..2]
                            .iter()
                            .zip([&endpoint.url, &endpoint.model])
                        {
                            world
                                .get_mut::<EditableText>(*field)
                                .unwrap()
                                .editor
                                .set_text(value);
                        }
                        panel::status(
                            world,
                            key_label,
                            if enabled {
                                "Endpoint API key required: yes"
                            } else {
                                "Endpoint API key required: no"
                            },
                        );
                    }
                    panel::status(
                        world,
                        cloud_label,
                        if saved
                            .endpoint
                            .as_ref()
                            .is_some_and(|endpoint| endpoint.allow_cloud)
                        {
                            "Cloud transcription: allowed"
                        } else {
                            "Cloud transcription: off"
                        },
                    );
                    panel::status(world, status_entity, &saved.detail);
                }
            }
        }
    }
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<Composer>>()
        .iter(world)
        .collect();
    for owner in owners {
        if world.get::<Composer>(owner).is_some_and(|composer| {
            (composer.pending.is_some() || composer.job.is_some())
                && composer
                    .cancellation
                    .as_ref()
                    .is_some_and(|cancellation| cancellation.is_closed())
        }) {
            Command::Cancel.apply(world, owner);
            let mut composer = world.get_mut::<Composer>(owner).unwrap();
            composer.pending = None;
            composer.job = None;
            let status = composer.status;
            panel::status(
                world,
                status,
                "The speech connection closed. Reconnect this Cell and retry; your draft is unchanged.",
            );
            continue;
        }
        #[cfg(feature = "native-media")]
        if world.get::<Recording>(owner).is_some()
            && crate::native::communication::calls::microphone_active(world)
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
            let recording = world.entity_mut(owner).take::<Recording>().unwrap();
            let frozen_endpoint = recording.endpoint.clone();
            let mode = recording.mode;
            match result {
                Ok(audio) => {
                    let job = nucleus::new_uid("speech");
                    match send(
                        world,
                        owner,
                        match mode {
                            cell::speech::Mode::Local => Request::StartLocal {
                                job: job.clone(),
                                audio: cell::FioteSecret(audio),
                            },
                            cell::speech::Mode::Endpoint => {
                                let Some(settings) = frozen_endpoint else {
                                    panel::status(
                                        world,
                                        status,
                                        "The recording has no saved endpoint. Your draft is unchanged.",
                                    );
                                    continue;
                                };
                                Request::StartEndpoint {
                                    job: job.clone(),
                                    settings,
                                    audio: cell::FioteSecret(audio),
                                }
                            }
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
            let detail = match *recording.started.lock().unwrap() {
                Some(started) => format!("Recording · {} seconds · Stop and transcribe adds one editable block", started.elapsed().as_secs()),
                None => "Preparing microphone · wait for Recording before speaking · Stop or Cancel ends setup".into(),
            };
            panel::status(world, status, detail);
        }
        let composer = world.get::<Composer>(owner).unwrap();
        if composer.pending.is_none()
            && composer.next <= Instant::now()
            && let Some(job) = composer.job.clone()
        {
            let _ = send(world, owner, Request::Poll { job });
        } else if composer.pending.is_none()
            && composer.next <= Instant::now()
            && composer
                .saved
                .as_ref()
                .and_then(|saved| saved.local.as_ref())
                .is_some_and(|local| local.busy)
        {
            world.get_mut::<Composer>(owner).unwrap().next =
                Instant::now() + Duration::from_millis(250);
            let _ = send(world, owner, Request::Inspect);
        }
        if active(world, owner)
            && let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>()
        {
            wake.after(Duration::from_millis(250));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Records two seconds from the actual default microphone"]
    fn actual_microphone_capture_encodes_audio_or_reports_silence_and_releases_the_device() {
        use cpal::traits::{DeviceTrait, HostTrait};
        let devices = crate::native::sound::device::Capture::devices().unwrap();
        assert!(!devices.is_empty(), "No input devices were enumerated");
        println!("Microphone candidates: {devices:?}");
        let selected = std::env::var("LINCE_SPEECH_TEST_MICROPHONE").ok();
        let host = cpal::default_host();
        let device = match &selected {
            Some(name) => host
                .input_devices()
                .unwrap()
                .find(|device| {
                    device
                        .description()
                        .is_ok_and(|description| description.name() == name.as_str())
                })
                .unwrap(),
            None => host.default_input_device().unwrap(),
        };
        println!(
            "Microphone configuration: {:?}",
            device.default_input_config().unwrap()
        );
        let recording =
            crate::native::sound::device::Capture::start_on(selected.as_deref()).unwrap();
        assert!(crate::native::sound::device::Capture::start().is_err());
        std::thread::sleep(Duration::from_millis(500));
        let mut clip = recording.drain();
        assert!(crate::native::sound::device::Capture::start().is_err());
        std::thread::sleep(Duration::from_millis(1500));
        assert!(recording.error.lock().unwrap().is_none());
        let remaining = recording.finish();
        assert_eq!(clip.rate, remaining.rate);
        clip.samples.extend(remaining.samples);
        assert!(!clip.samples.is_empty());
        let peak = clip
            .samples
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        let mean = clip
            .samples
            .iter()
            .map(|sample| f64::from(*sample))
            .sum::<f64>()
            / clip.samples.len() as f64;
        let rms = (clip
            .samples
            .iter()
            .map(|sample| f64::from(*sample).powi(2))
            .sum::<f64>()
            / clip.samples.len() as f64)
            .sqrt();
        let clipped = clip
            .samples
            .iter()
            .filter(|sample| sample.abs() >= 0.99)
            .count() as f64
            / clip.samples.len() as f64;
        println!(
            "Microphone: {} devices, {} samples at {} Hz, peak {peak}, mean {mean}, RMS {rms}, clipped fraction {clipped}",
            devices.len(),
            clip.samples.len(),
            clip.rate
        );
        match encode(clip) {
            Ok(audio) => {
                let bytes = BASE64.decode(audio).unwrap();
                let wav = hound::WavReader::new(std::io::Cursor::new(bytes)).unwrap();
                assert_eq!(wav.spec().sample_rate, 16000);
                assert_eq!(wav.spec().channels, 1);
                assert!(wav.duration() > 16000);
            }
            Err(error) => {
                if error.contains("badly distorted") {
                    assert!(error.contains("before transcription"));
                    assert!(error.contains("draft is unchanged"));
                    println!("Capture quality refusal before transcription: {error}");
                } else {
                    assert!(peak < 0.000_01);
                    assert!(error.contains("draft is unchanged"));
                }
            }
        }
        drop(crate::native::sound::device::Capture::start_on(selected.as_deref()).unwrap());
    }

    #[test]
    fn capture_quality_refuses_sustained_biased_clipping_before_encoding() {
        let samples = [vec![-1.0; 3200], vec![0.0; 4800]].concat();
        let mut quality = CaptureQuality::default();
        for chunk in samples[..7800].chunks(200) {
            quality
                .push(&crate::native::sound::library::Clip {
                    rate: 16_000,
                    samples: chunk.to_vec(),
                })
                .unwrap();
        }
        let error = quality
            .push(&crate::native::sound::library::Clip {
                rate: 16_000,
                samples: samples[7800..].to_vec(),
            })
            .unwrap_err();
        assert!(error.contains("badly distorted"));
        let error = encode(crate::native::sound::library::Clip {
            rate: 16_000,
            samples,
        })
        .unwrap_err();
        assert!(error.contains("before transcription"));
        assert!(error.contains("draft is unchanged"));
    }

    #[test]
    fn capture_quality_keeps_brief_peaks_and_balanced_audio() {
        for samples in [
            [vec![-1.0; 160], vec![0.01; 7840]].concat(),
            [-1.0, 1.0].into_iter().cycle().take(8000).collect(),
            vec![0.1; 8000],
        ] {
            let mut quality = CaptureQuality::default();
            for chunk in samples.chunks(320) {
                quality
                    .push(&crate::native::sound::library::Clip {
                        rate: 16_000,
                        samples: chunk.to_vec(),
                    })
                    .unwrap();
            }
            assert!(
                encode(crate::native::sound::library::Clip {
                    rate: 16_000,
                    samples,
                })
                .is_ok()
            );
        }
    }

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
                endpoint_fields: [input; 4],
                local_status: status,
                local_import: input,
                requires_key: false,
                key_label: status,
                cloud_label: status,
                cloud: false,
                saved: None,
                pending: Some(request.into()),
                job: Some(job.into()),
                next: Instant::now() + Duration::from_secs(3600),
                transcript: None,
                cancellation: None,
            })
            .id();
        (owner, input)
    }

    fn result(request: &str, job: &str) -> crate::native::cell_bridge::CellMessage {
        crate::native::cell_bridge::CellMessage(cell::ServerMessage::Speech {
            id: request.into(),
            status: Status {
                wake_profile: None,
                wake_error: None,
                voice: Default::default(),
                mode: cell::speech::Mode::Local,
                local_selection: Default::default(),
                decision_selection: Default::default(),
                local: None,
                decision_model: None,
                decision_capabilities: None,
                listening: Default::default(),
                endpoint: None,
                ready: true,
                detail: String::new(),
                job: Some(cell::speech::Job {
                    purpose: cell::speech::Purpose::Transcription,
                    id: job.into(),
                    pending: false,
                    text: Some("Recognized words".into()),
                    decision: None,
                    error: None,
                }),
            },
        })
    }

    #[test]
    fn late_transcripts_only_update_their_original_draft_once() {
        let mut app = App::new();
        app.add_message::<crate::native::cell_bridge::CellMessage>()
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
            .write_message(crate::native::cell_bridge::CellMessage(
                cell::ServerMessage::Error {
                    id: crate::native::cell_bridge::CONNECTION.into(),
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
    fn model_setup_results_never_insert_text_or_send_a_message() {
        let mut app = App::new();
        app.add_message::<crate::native::cell_bridge::CellMessage>()
            .add_systems(Update, update);
        let (owner, input) = waiting(
            app.world_mut(),
            "install-request",
            "install-job",
            "Keep this draft",
        );
        let mut event = result("install-request", "install-job");
        if let cell::ServerMessage::Speech { status, .. } = &mut event.0 {
            let job = status.job.as_mut().unwrap();
            job.purpose = cell::speech::Purpose::ModelSetup;
            job.text = Some("Local English speech model installed and verified.".into());
        }
        app.world_mut().write_message(event);
        app.update();
        assert_eq!(
            app.world()
                .get::<EditableText>(input)
                .unwrap()
                .value()
                .to_string(),
            "Keep this draft"
        );
        let composer = app.world().get::<Composer>(owner).unwrap();
        assert!(composer.job.is_none());
        assert!(composer.transcript.is_none());
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
        let audio = encode(crate::native::sound::library::Clip {
            samples: vec![0.25; 48_000],
            rate: 48_000,
        })
        .unwrap();
        let bytes = BASE64.decode(audio).unwrap();
        let reader = hound::WavReader::new(std::io::Cursor::new(bytes)).unwrap();
        assert_eq!(reader.duration(), 16_000);
        assert_eq!(reader.spec().channels, 1);
        assert!(
            encode(crate::native::sound::library::Clip {
                samples: vec![0.0; 10],
                rate: 16_000
            })
            .is_err()
        );
    }
}

#[cfg(test)]
mod endpoint_tests {
    use super::*;
    use cell::speech::Service;
    use std::sync::Arc;

    #[tokio::test]
    #[ignore = "Requires installed Phonon weights and public English WAV through LINCE_SPEECH_TEST_CACHE and LINCE_SPEECH_TEST_WAV"]
    async fn real_local_controls_import_transcribe_preserve_draft_cancel_and_create_no_messages() {
        let directory = tempfile::tempdir().unwrap();
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let speech =
            Arc::new(cell::speech::Host::open(directory.path().join("speech.json")).unwrap());
        let before: i64 = store::sqlx::query_scalar("SELECT count(*) FROM record")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
        let runtime = cell::CellRuntime {
            commands: Default::default(),
            store: engine.store.clone(),
            engine: engine.clone(),
            lanes: Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            fiote: None,
            speech: Some(speech.clone()),
            information: None,
        };
        let mut app = crate::native::sand_panel::tests::app();
        app.insert_resource(crate::native::wake::WakeSignal::new(|| {}))
            .insert_resource(crate::native::app::CellHandle(runtime))
            .add_plugins(crate::native::cell_bridge::CellBridgePlugin)
            .add_systems(
                Update,
                update.after(crate::native::cell_bridge::ReceiveCell),
            );
        app.update();
        let parent = app.world_mut().spawn(Node::default()).id();
        let owner = app.world_mut().spawn(Node::default()).id();
        let input = app
            .world_mut()
            .spawn(EditableText::new("Keep this unsent draft"))
            .id();
        composer(app.world_mut(), owner, parent, input);
        let selection = std::env::var("LINCE_SPEECH_TEST_SELECTION")
            .map(|value| serde_json::from_str::<cell::speech::ModelId>(&value).unwrap())
            .unwrap_or_default();
        Command::SelectLocal(selection).apply(app.world_mut(), owner);
        crate::native::sand_panel::tests::settle(&mut app, |world| {
            world
                .get::<Composer>(owner)
                .unwrap()
                .saved
                .as_ref()
                .is_some_and(|status| status.local_selection == selection)
        })
        .await;
        let model = speech
            .handle(Request::Inspect)
            .await
            .unwrap()
            .local
            .unwrap();
        let archive = std::env::var_os("LINCE_SPEECH_TEST_IMPORT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(
                    std::env::var_os("LINCE_SPEECH_TEST_CACHE")
                        .expect("Choose installed model cache"),
                )
                .join(format!("{}-{}", model.model, model.revision))
                .join("phonon-2.bps.tar.zst")
            });
        let import = app.world().get::<Composer>(owner).unwrap().local_import;
        app.world_mut()
            .get_mut::<EditableText>(import)
            .unwrap()
            .editor
            .set_text(&archive.to_string_lossy());
        Command::ImportLocal.apply(app.world_mut(), owner);
        async fn settle(app: &mut App, done: impl Fn(&World) -> bool) {
            let deadline = Instant::now() + Duration::from_secs(180);
            while Instant::now() < deadline {
                app.update();
                if done(app.world()) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            panic!("Real local speech controls did not settle");
        }
        settle(&mut app, |world| {
            let composer = world.get::<Composer>(owner).unwrap();
            composer.pending.is_none()
                && composer.job.is_none()
                && composer.saved.as_ref().is_some_and(|status| status.ready)
        })
        .await;
        assert_eq!(
            app.world()
                .get::<EditableText>(input)
                .unwrap()
                .value()
                .to_string(),
            "Keep this unsent draft"
        );
        let recorded = BASE64.encode(
            std::fs::read(
                std::env::var_os("LINCE_SPEECH_TEST_WAV").expect("Choose public English WAV"),
            )
            .unwrap(),
        );
        let samples = cell::fiote_communication::speech::decode_audio(&recorded).unwrap();
        let audio = encode(crate::native::sound::library::Clip {
            rate: 48_000,
            samples: samples
                .into_iter()
                .flat_map(|sample| std::iter::repeat_n(sample, 3))
                .collect(),
        })
        .unwrap();
        fn recording(
            app: &mut App,
            owner: Entity,
        ) -> (mpsc::Receiver<bool>, mpsc::Sender<Result<String, String>>) {
            let (control, commands) = mpsc::sync_channel(1);
            let (finished, result) = mpsc::channel();
            app.world_mut().entity_mut(owner).insert(Recording {
                mode: cell::speech::Mode::Local,
                endpoint: None,
                control,
                result: Mutex::new(result),
                started: Arc::new(Mutex::new(Some(Instant::now()))),
            });
            (commands, finished)
        }
        let (commands, finished) = recording(&mut app, owner);
        Command::Stop.apply(app.world_mut(), owner);
        assert!(commands.try_recv().unwrap());
        finished.send(Ok(audio.clone())).unwrap();
        app.update();
        app.world_mut()
            .get_mut::<EditableText>(input)
            .unwrap()
            .editor
            .set_text("Edited while transcribing");
        settle(&mut app, |world| {
            let composer = world.get::<Composer>(owner).unwrap();
            composer.pending.is_none()
                && composer.job.is_none()
                && world
                    .get::<EditableText>(input)
                    .unwrap()
                    .value()
                    .to_string()
                    .to_ascii_lowercase()
                    .contains("ask not what your country can do for you")
        })
        .await;
        let expected = app
            .world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string();
        assert!(expected.starts_with("Edited while transcribing"));
        let (commands, finished) = recording(&mut app, owner);
        Command::Stop.apply(app.world_mut(), owner);
        assert!(commands.try_recv().unwrap());
        finished.send(Ok(audio)).unwrap();
        app.update();
        Command::Cancel.apply(app.world_mut(), owner);
        settle(&mut app, |world| {
            let composer = world.get::<Composer>(owner).unwrap();
            composer.pending.is_none()
                && composer.job.is_none()
                && composer
                    .saved
                    .as_ref()
                    .and_then(|saved| saved.local.as_ref())
                    .is_some_and(|local| !local.busy)
        })
        .await;
        assert_eq!(
            app.world()
                .get::<EditableText>(input)
                .unwrap()
                .value()
                .to_string(),
            expected
        );
        let after: i64 = store::sqlx::query_scalar("SELECT count(*) FROM record")
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn endpoint_controls_save_without_harness_and_changed_recording_destination_preserves_draft()
     {
        let directory = tempfile::tempdir().unwrap();
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let speech =
            Arc::new(cell::speech::Host::open(directory.path().join("speech.json")).unwrap());
        let runtime = cell::CellRuntime {
            commands: Default::default(),
            store: engine.store.clone(),
            engine,
            lanes: Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            fiote: None,
            speech: Some(speech.clone()),
            information: None,
        };
        let service = runtime.speech.clone().unwrap();
        let mut app = crate::native::sand_panel::tests::app();
        app.insert_resource(crate::native::wake::WakeSignal::new(|| {}))
            .insert_resource(crate::native::app::CellHandle(runtime))
            .add_plugins(crate::native::cell_bridge::CellBridgePlugin)
            .add_systems(
                Update,
                update.after(crate::native::cell_bridge::ReceiveCell),
            );
        app.update();
        let parent = app.world_mut().spawn(Node::default()).id();
        let owner = app.world_mut().spawn(Node::default()).id();
        let input = app
            .world_mut()
            .spawn(EditableText::new("My unsent draft"))
            .id();
        composer(app.world_mut(), owner, parent, input);
        let fields = app.world().get::<Composer>(owner).unwrap().endpoint_fields;
        let selected = app
            .world_mut()
            .query::<(
                &crate::native::actions::ActionButton,
                &bevy::a11y::AccessibilityNode,
            )>()
            .iter(app.world())
            .find(|(_, node)| node.label() == Some("Moonshine Tiny"))
            .map(|(button, _)| button.clone())
            .unwrap();
        selected.actions.run(app.world_mut(), selected.target);
        crate::native::sand_panel::tests::settle(&mut app, |world| {
            world
                .get::<Composer>(owner)
                .unwrap()
                .saved
                .as_ref()
                .is_some_and(|status| {
                    status.local_selection == cell::speech::ModelId::MoonshineTiny
                })
        })
        .await;
        let selected = service.handle(Request::Inspect).await.unwrap();
        assert_eq!(
            selected.local_selection,
            cell::speech::ModelId::MoonshineTiny
        );
        assert!(!selected.listening.enabled);
        assert!(!selected.local.unwrap().present);
        assert_eq!(
            app.world().get::<EditableText>(input).unwrap().value(),
            "My unsent draft"
        );
        app.world_mut()
            .get_mut::<EditableText>(fields[0])
            .unwrap()
            .editor
            .set_text("http://127.0.0.1:8123/v1/audio/transcriptions");
        app.world_mut()
            .get_mut::<EditableText>(fields[1])
            .unwrap()
            .editor
            .set_text("local-transcriber");
        Command::SaveEndpoint.apply(app.world_mut(), owner);
        crate::native::sand_panel::tests::settle(&mut app, |world| {
            world
                .get::<Composer>(owner)
                .unwrap()
                .saved
                .as_ref()
                .is_some_and(|saved| saved.endpoint.is_some() && saved.ready)
        })
        .await;
        let frozen = app
            .world()
            .get::<Composer>(owner)
            .unwrap()
            .saved
            .as_ref()
            .unwrap()
            .endpoint
            .clone()
            .unwrap();
        assert!(
            app.world()
                .get::<EditableText>(fields[2])
                .unwrap()
                .value()
                .to_string()
                .is_empty()
        );
        let mut changed = frozen.clone();
        changed.url = "http://127.0.0.1:8124/v1/audio/transcriptions".into();
        service
            .handle(Request::ConfigureEndpoint {
                settings: changed,
                credential: None,
                password: None,
            })
            .await
            .unwrap();
        let (control, _commands) = mpsc::sync_channel(1);
        let (finished, result) = mpsc::channel();
        finished
            .send(Ok(BASE64.encode(include_bytes!(
                "../../tests/fixtures/fiote/audio.wav"
            ))))
            .unwrap();
        app.world_mut().entity_mut(owner).insert(Recording {
            mode: cell::speech::Mode::Endpoint,
            endpoint: Some(frozen),
            control,
            result: Mutex::new(result),
            started: Arc::new(Mutex::new(Some(Instant::now()))),
        });
        app.update();
        crate::native::sand_panel::tests::settle(&mut app, |world| {
            let composer = world.get::<Composer>(owner).unwrap();
            composer.pending.is_none()
                && composer.job.is_none()
                && world.get::<Recording>(owner).is_none()
        })
        .await;
        assert_eq!(
            app.world()
                .get::<EditableText>(input)
                .unwrap()
                .value()
                .to_string(),
            "My unsent draft"
        );
        let status = app.world().get::<Composer>(owner).unwrap().status;
        assert!(
            app.world()
                .get::<Text>(status)
                .unwrap()
                .0
                .contains("changed")
        );
        assert!(
            service
                .handle(Request::Inspect)
                .await
                .unwrap()
                .job
                .is_none()
        );
    }
}
