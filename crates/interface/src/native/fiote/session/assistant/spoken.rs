use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use bevy::ecs::{entity_disabling::Disabled, query::Allow};
use cell::fiote_communication::speech::voice::{Request as VoiceRequest, Status as VoiceStatus};
use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "best-tts and Kokoro-82M English voice",
        author: "Studio Ransom and hexgrad",
        license: concat!(
            include_str!("../../../../../../fiote/assets/kokoro/NOTICE"),
            "\n",
            include_str!("../../../../../../fiote/assets/kokoro/LICENSE-Apache-2.0.txt")
        ),
    },
    crate::native::credits::Attribution {
        name: "misaki pronunciation lexicon",
        author: "hexgrad",
        license: include_str!("../../../../../../fiote/assets/kokoro/LICENSE-misaki-MIT.txt"),
    },
];

#[derive(Component)]
struct Spoken {
    label: Entity,
    reply_label: Entity,
    import: Entity,
    outputs: Entity,
    output: Option<String>,
    automatic: bool,
    reply: Option<(String, String)>,
    pending: Option<String>,
    job: Option<String>,
    chunks: VecDeque<String>,
    playing: bool,
    ready: bool,
    next: Instant,
    resume_at: Instant,
    cancellation: Option<crate::native::cell_bridge::SpeechCancellation>,
}

impl Drop for Spoken {
    fn drop(&mut self) {
        if let (Some(job), Some(cancellation)) = (&self.job, &self.cancellation) {
            let _ = cancellation.send(cell::speech::Request::Voice {
                request: VoiceRequest::Cancel { job: job.clone() },
            });
        }
    }
}

#[derive(Component)]
struct PlaybackGuard {
    cancelled: Arc<AtomicBool>,
}

impl Drop for PlaybackGuard {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

pub(super) fn attach(world: &mut World, owner: Entity) {
    crate::native::edit_mode::label(world, owner, "Spoken replies (optional)", 16.0);
    crate::native::edit_mode::label(
        world,
        owner,
        "Local English Kokoro voice · 116 MB download/import · Rust CPU inference. Written replies stay in the conversation. Microphone activation pauses while speaking; Stop speaking does not stop the agent.",
        13.0,
    );
    let label = crate::native::edit_mode::label(world, owner, "Spoken replies: off", 13.0);
    let import = ui::field(
        world,
        owner,
        "Compatible Kokoro voice directory (kokoro.btm, voices.btv, lexicon.btl)",
        "",
    );
    let controls = ui::row(world, owner);
    for (caption, command) in [
        ("Download English voice", Command::Download),
        ("Import English voice", Command::Import),
        ("Check English voice", Command::Inspect),
        ("Toggle automatic spoken replies", Command::Automatic),
        ("Read last reply aloud", Command::Read),
        ("Stop speaking / cancel voice setup", Command::Stop),
        ("Mute spoken replies", Command::Mute),
        ("Choose speaker", Command::Outputs),
        ("Use default speaker", Command::Output(None)),
    ] {
        ui::button(world, controls, owner, caption, command);
    }
    ui::credits(world, controls, owner, CREDITS);
    let outputs = ui::column(world, owner);
    let reply_label = crate::native::edit_mode::label(world, owner, "", 14.0);
    let cancellation = crate::native::cell_bridge::SpeechCancellation::for_owner(world, owner);
    world.entity_mut(owner).insert((
        crate::native::sand_store::SandCredits(CREDITS),
        Spoken {
            label,
            reply_label,
            import,
            outputs,
            output: None,
            automatic: false,
            reply: None,
            pending: None,
            job: None,
            chunks: VecDeque::new(),
            playing: false,
            ready: false,
            next: Instant::now(),
            resume_at: Instant::now(),
            cancellation,
        },
    ));
}

pub(super) fn busy(world: &World, owner: Entity) -> bool {
    world
        .get::<Spoken>(owner)
        .is_some_and(|spoken| spoken.playing || !spoken.chunks.is_empty() || spoken.job.is_some())
}

fn send(world: &mut World, owner: Entity, request: VoiceRequest) -> Result<(), String> {
    let cancellation = crate::native::cell_bridge::SpeechCancellation::for_owner(world, owner);
    let starting = matches!(
        &request,
        VoiceRequest::Install { .. } | VoiceRequest::Speak { .. }
    );
    let id = nucleus::new_uid("voice-control");
    ui::send(
        world,
        owner,
        cell::ClientMessage::Speech {
            id: id.clone(),
            request: cell::speech::Request::Voice { request },
        },
    )?;
    let mut spoken = world
        .get_mut::<Spoken>(owner)
        .ok_or("Spoken reply controls closed.")?;
    spoken.pending = Some(id);
    spoken.cancellation = cancellation;
    if starting {
        spoken.ready = false;
    }
    Ok(())
}

#[derive(Clone)]
enum Command {
    Download,
    Import,
    Inspect,
    Automatic,
    Read,
    Stop,
    Mute,
    Outputs,
    Output(Option<String>),
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(spoken) = world.get::<Spoken>(owner) else {
            return;
        };
        let label = spoken.label;
        let pending = spoken.pending.is_some();
        let outputs = spoken.outputs;
        let import = spoken.import;
        let result = (|| {
            if matches!(self, Self::Stop | Self::Mute) {
                if matches!(self, Self::Mute) {
                    world.get_mut::<Spoken>(owner).unwrap().automatic = false;
                }
                stop(world, owner);
                return Ok("Speaking stopped. The Fiote run remains independent.".into());
            }
            if pending {
                return Err("Wait for the current voice control response.".into());
            }
            match self {
                Self::Automatic => {
                    let mut spoken = world.get_mut::<Spoken>(owner).unwrap();
                    spoken.automatic = !spoken.automatic;
                    Ok(format!(
                        "Automatic spoken replies: {}",
                        if spoken.automatic { "on" } else { "off" }
                    ))
                }
                Self::Read => {
                    begin(world, owner)?;
                    Ok("Preparing the completed reply for speech.".into())
                }
                Self::Inspect => {
                    send(world, owner, VoiceRequest::Inspect)?;
                    Ok("Checking the local English voice.".into())
                }
                Self::Outputs => {
                    if busy(world, owner) {
                        return Err("Stop speaking before changing its output.".into());
                    }
                    ui::clear(world, outputs);
                    for output in crate::native::sound::device::Output::devices()? {
                        ui::button(
                            world,
                            outputs,
                            owner,
                            &output,
                            Self::Output(Some(output.clone())),
                        );
                    }
                    Ok("Choose the speaker for spoken replies.".into())
                }
                Self::Output(output) => {
                    if busy(world, owner) {
                        return Err("Stop speaking before changing its output.".into());
                    }
                    world.get_mut::<Spoken>(owner).unwrap().output = output.clone();
                    Ok(format!(
                        "Spoken reply output: {}",
                        output.as_deref().unwrap_or("system default")
                    ))
                }
                Self::Download | Self::Import => {
                    if busy(world, owner) {
                        return Err("Stop speaking before installing its voice.".into());
                    }
                    let source = if matches!(self, Self::Import) {
                        let value = world
                            .get::<EditableText>(import)
                            .ok_or("Voice import field closed.")?
                            .value()
                            .to_string()
                            .trim()
                            .to_owned();
                        if value.is_empty() {
                            return Err("Choose a compatible English voice directory.".into());
                        }
                        Some(PathBuf::from(value))
                    } else {
                        None
                    };
                    let job = nucleus::new_uid("speech");
                    send(
                        world,
                        owner,
                        VoiceRequest::Install {
                            job: job.clone(),
                            source,
                        },
                    )?;
                    world.get_mut::<Spoken>(owner).unwrap().job = Some(job);
                    Ok("Installing the verified English voice. Microphone activation is unchanged.".into())
                }
                Self::Stop | Self::Mute => unreachable!(),
            }
        })();
        if let Some(mut text) = world.get_mut::<Text>(label) {
            text.0 = result.unwrap_or_else(|error| error);
        }
    }
}

fn chunks(markdown: &str) -> Result<VecDeque<String>, String> {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};
    if markdown.len() > 262_144 {
        return Err(
            "This reply is too long to read aloud. Ask for a shorter spoken summary.".into(),
        );
    }
    let mut plain = String::new();
    let mut skip = 0usize;
    for event in Parser::new(markdown) {
        match event {
            Event::Start(Tag::CodeBlock(_) | Tag::HtmlBlock | Tag::Image { .. }) => skip += 1,
            Event::End(TagEnd::CodeBlock | TagEnd::HtmlBlock | TagEnd::Image) => {
                skip = skip.saturating_sub(1);
                plain.push(' ');
            }
            Event::Text(text) if skip == 0 => {
                plain.push_str(&text);
                plain.push(' ');
            }
            Event::SoftBreak
            | Event::HardBreak
            | Event::End(TagEnd::Paragraph | TagEnd::Heading(_))
                if skip == 0 =>
            {
                plain.push(' ')
            }
            _ => {}
        }
    }
    if plain.chars().count() > 12_000 {
        return Err(
            "This reply is too long to read aloud. Ask for a shorter spoken summary.".into(),
        );
    }
    let mut chunks = VecDeque::new();
    let mut chunk = String::new();
    for word in plain.split_whitespace() {
        if word.chars().count() > 300 {
            return Err(
                "The reply contains a word too long for speech. Ask for a spoken summary.".into(),
            );
        }
        if chunk.chars().count() + word.chars().count() + 1 > 300 {
            chunks.push_back(std::mem::take(&mut chunk));
        }
        if !chunk.is_empty() {
            chunk.push(' ');
        }
        chunk.push_str(word);
    }
    if !chunk.is_empty() {
        chunks.push_back(chunk);
    }
    if chunks.is_empty() {
        return Err("This reply contains no user-facing text to speak.".into());
    }
    Ok(chunks)
}

fn begin(world: &mut World, owner: Entity) -> Result<(), String> {
    if busy(world, owner) {
        return Err("Stop the current voice operation before reading another reply.".into());
    }
    if crate::native::sound::speaking(world) {
        return Err("Another spoken reply is already active.".into());
    }
    if crate::native::speech::any_recording(world) {
        return Err("Finish dictation before speaking a reply.".into());
    }
    if !super::listening::view_active(world, owner) {
        return Err("Open the assistant in the active workspace before speaking.".into());
    }
    let (_, text) = world
        .get::<Spoken>(owner)
        .and_then(|spoken| spoken.reply.clone())
        .ok_or("No completed assistant reply is available yet.")?;
    let chunks = chunks(&text)?;
    crate::native::sound::audio_for(world, owner)
        .ok_or("The native audio service is unavailable.")?;
    super::listening::suspend_all(world);
    let mut spoken = world.get_mut::<Spoken>(owner).unwrap();
    spoken.chunks = chunks;
    spoken.ready = false;
    world.entity_mut(owner).insert((
        PlaybackGuard {
            cancelled: Arc::new(AtomicBool::new(false)),
        },
        crate::native::sound::VoiceOutputActive,
    ));
    Ok(())
}

fn stop(world: &mut World, owner: Entity) {
    let Some(mut spoken) = world.get_mut::<Spoken>(owner) else {
        return;
    };
    if let (Some(job), Some(cancellation)) = (&spoken.job, &spoken.cancellation) {
        let _ = cancellation.send(cell::speech::Request::Voice {
            request: VoiceRequest::Cancel { job: job.clone() },
        });
    }
    spoken.job = None;
    spoken.pending = None;
    spoken.chunks.clear();
    spoken.playing = false;
    spoken.ready = false;
    spoken.resume_at = Instant::now() + Duration::from_millis(350);
    world.entity_mut(owner).remove::<PlaybackGuard>();
    world
        .entity_mut(owner)
        .remove::<crate::native::sound::VoiceOutputActive>();
    world
        .entity_mut(owner)
        .remove::<crate::native::sound::SpokenPlayback>();
}

pub(super) fn invalidate(world: &mut World, owner: Entity) {
    stop(world, owner);
    if let Some(mut spoken) = world.get_mut::<Spoken>(owner) {
        spoken.reply = None;
        let label = spoken.reply_label;
        if let Some(mut text) = world.get_mut::<Text>(label) {
            text.0.clear();
        }
    }
}

pub(super) fn observe(
    world: &mut World,
    owner: Entity,
    receipt: &EntryReceipt,
    text: Option<&str>,
) {
    let Some(text) = text.filter(|_| receipt.state == "completed") else {
        return;
    };
    let Some(mut spoken) = world.get_mut::<Spoken>(owner) else {
        return;
    };
    let label = spoken.reply_label;
    let changed = spoken
        .reply
        .as_ref()
        .is_none_or(|(id, _)| id != &receipt.request_id);
    let automatic = changed && spoken.automatic;
    spoken.reply = Some((receipt.request_id.clone(), text.to_string()));
    if let Some(mut display) = world.get_mut::<Text>(label) {
        display.0 = text.to_string();
    }
    if changed {
        stop(world, owner);
    }
    if automatic && let Err(error) = begin(world, owner) {
        let label = world.get::<Spoken>(owner).unwrap().label;
        world.get_mut::<Text>(label).unwrap().0 = error;
    }
}

fn clip(audio: &cell::FioteSecret) -> Result<crate::native::sound::library::Clip, String> {
    if audio.0.len() > 3_000_000 {
        return Err("Spoken audio exceeds its limit.".into());
    }
    let bytes = BASE64.decode(&audio.0).map_err(|error| error.to_string())?;
    let reader =
        hound::WavReader::new(std::io::Cursor::new(bytes)).map_err(|error| error.to_string())?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.sample_rate != 24_000
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
        || reader.duration() == 0
        || reader.duration() > 24_000 * 45
    {
        return Err("The English voice returned an unsupported audio format.".into());
    }
    let samples = reader
        .into_samples::<i16>()
        .map(|sample| sample.map(|sample| f32::from(sample) / 32768.0))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(crate::native::sound::library::Clip {
        samples,
        rate: 24_000,
    })
}

fn result(world: &mut World, owner: Entity, status: &VoiceStatus) -> Result<(), String> {
    let mut spoken = world
        .get_mut::<Spoken>(owner)
        .ok_or("Spoken reply controls closed.")?;
    spoken.pending = None;
    spoken.next = Instant::now() + Duration::from_millis(150);
    spoken.ready = status.present && !status.busy;
    if !status.present && !status.busy && !spoken.chunks.is_empty() {
        return Err("Download or import the supported English voice before reading replies aloud. The written reply is available below.".into());
    }
    let label = spoken.label;
    let expected = spoken.job.clone();
    if let Some(job) = status
        .job
        .as_ref()
        .filter(|job| expected.as_deref() == Some(&job.id))
    {
        if let Some(error) = &job.error {
            return Err(error.clone());
        }
        if !job.pending {
            spoken.job = None;
            if let Some(audio) = &job.audio {
                let clip = Arc::new(clip(audio)?);
                let output = spoken.output.clone();
                let cancelled = world
                    .get::<PlaybackGuard>(owner)
                    .ok_or("This spoken reply was stopped before playback.")?
                    .cancelled
                    .clone();
                crate::native::sound::send(
                    world,
                    crate::native::sound::Command::Speak {
                        owner,
                        clip,
                        output,
                        cancelled,
                    },
                )?;
                world.get_mut::<Spoken>(owner).unwrap().playing = true;
            }
        }
    }
    let speaking = world
        .get::<Spoken>(owner)
        .is_some_and(|spoken| spoken.playing);
    world.get_mut::<Text>(label).unwrap().0 = format!(
        "English voice: {} · {} / {} MB acquired{}",
        if status.present {
            "installed"
        } else {
            "not installed"
        },
        status.acquired_bytes / 1_000_000,
        status.required_bytes / 1_000_000,
        if speaking {
            " · speaking"
        } else if status.busy {
            " · working"
        } else {
            ""
        }
    );
    Ok(())
}

fn next_request(spoken: &mut Spoken, quiet: bool) -> Option<VoiceRequest> {
    if let Some(job) = &spoken.job {
        return Some(VoiceRequest::Poll { job: job.clone() });
    }
    if !quiet || spoken.chunks.is_empty() {
        return None;
    }
    if !spoken.ready {
        return Some(VoiceRequest::Inspect);
    }
    let text = spoken.chunks.pop_front().unwrap();
    let job = nucleus::new_uid("speech");
    spoken.job = Some(job.clone());
    Some(VoiceRequest::Speak { job, text })
}

pub(in crate::native::fiote::session) fn update(
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
        let (id, status) = match event {
            cell::ServerMessage::Speech { id, status } => (id, Ok(status.voice)),
            cell::ServerMessage::Error { id, message, .. } => (id, Err(message)),
            _ => continue,
        };
        let owners: Vec<_> = world
            .query_filtered::<(Entity, &Spoken), Allow<Disabled>>()
            .iter(world)
            .filter(|(_, spoken)| {
                spoken.pending.as_deref() == Some(&id)
                    || id == crate::native::cell_bridge::CONNECTION
            })
            .map(|(owner, _)| owner)
            .collect();
        for owner in owners {
            let outcome = match &status {
                Ok(status) => result(world, owner, status),
                Err(error) => Err(error.clone()),
            };
            if let Err(error) = outcome {
                stop(world, owner);
                let label = world.get::<Spoken>(owner).unwrap().label;
                world.get_mut::<Text>(label).unwrap().0 = error;
            }
        }
    }
    let owners: Vec<_> = world
        .query_filtered::<(Entity, &Spoken), Allow<Disabled>>()
        .iter(world)
        .map(|(owner, _)| owner)
        .collect();
    for owner in owners {
        if world.get::<Spoken>(owner).is_some_and(|spoken| {
            (spoken.pending.is_some()
                || spoken.job.is_some()
                || spoken.playing
                || !spoken.chunks.is_empty())
                && spoken
                    .cancellation
                    .as_ref()
                    .is_some_and(|cancellation| cancellation.is_closed())
        }) {
            stop(world, owner);
            let label = world.get::<Spoken>(owner).unwrap().label;
            world.get_mut::<Text>(label).unwrap().0 =
                "The voice connection closed. Reconnect this Cell to read the written reply aloud."
                    .into();
            continue;
        }
        if !super::listening::view_active(world, owner) {
            if busy(world, owner) {
                stop(world, owner);
            }
            continue;
        }
        if let Some(done) = world
            .entity_mut(owner)
            .take::<crate::native::sound::SpokenPlayback>()
        {
            if done.1.load(Ordering::Acquire)
                || world
                    .get::<PlaybackGuard>(owner)
                    .is_none_or(|guard| !Arc::ptr_eq(&guard.cancelled, &done.1))
            {
                continue;
            }
            world.get_mut::<Spoken>(owner).unwrap().playing = false;
            world.get_mut::<Spoken>(owner).unwrap().resume_at =
                Instant::now() + Duration::from_millis(350);
            if let Err(error) = done.0 {
                stop(world, owner);
                let label = world.get::<Spoken>(owner).unwrap().label;
                world.get_mut::<Text>(label).unwrap().0 = error;
            } else {
                let label = world.get::<Spoken>(owner).unwrap().label;
                world.get_mut::<Text>(label).unwrap().0 =
                    "Spoken reply finished. Restoring previously enabled listening after playback."
                        .into();
            }
        }
        let quiet = super::listening::quiet(world);
        let spoken = world.get::<Spoken>(owner).unwrap();
        if spoken.pending.is_none() && !spoken.playing && Instant::now() >= spoken.next {
            let request = next_request(&mut world.get_mut::<Spoken>(owner).unwrap(), quiet);
            if let Some(request) = request
                && let Err(error) = send(world, owner, request)
            {
                stop(world, owner);
                let label = world.get::<Spoken>(owner).unwrap().label;
                world.get_mut::<Text>(label).unwrap().0 = error;
            }
        }
        if !busy(world, owner) && Instant::now() >= world.get::<Spoken>(owner).unwrap().resume_at {
            world.entity_mut(owner).remove::<PlaybackGuard>();
            world
                .entity_mut(owner)
                .remove::<crate::native::sound::VoiceOutputActive>();
            if !crate::native::sound::speaking(world) {
                super::listening::resume_all(world);
            }
        }
        let waiting = busy(world, owner)
            || world
                .get::<Spoken>(owner)
                .is_some_and(|spoken| spoken.pending.is_some())
            || world.get::<super::listening::Resume>(owner).is_some();
        if waiting && let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
            wake.after(Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn until(app: &mut App, owner: Entity, ready: impl Fn(&Spoken) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            if let Ok(message) = app
                .world_mut()
                .non_send_mut::<crate::native::cell_bridge::CellBridge>()
                .incoming
                .try_recv()
            {
                app.world_mut()
                    .write_message(crate::native::cell_bridge::CellMessage(message));
            }
            app.update();
            let spoken = app.world().get::<Spoken>(owner).unwrap();
            if ready(spoken) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "Voice workflow timed out: {}",
                app.world().get::<Text>(spoken.label).unwrap().0
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "Requires pinned LINCE_VOICE_TEST_ASSETS and an actual default speaker"]
    async fn actual_native_controls_import_voice_and_speak_a_normally_persisted_fiote_reply() {
        use cell::speech::Service as _;
        let source =
            std::env::var("LINCE_VOICE_TEST_ASSETS").expect("Choose the pinned Kokoro directory");
        let (mut app, owner, host, script, directory, record) =
            super::super::tests::fixture().await;
        let speech = Arc::new(
            cell::speech::Host::open(directory.path().join("speech.json"))
                .unwrap()
                .with_fiote(Some(host.clone())),
        );
        let mut runtime = app
            .world()
            .resource::<crate::native::app::CellHandle>()
            .0
            .clone();
        runtime.speech = Some(speech.clone());
        app.world_mut()
            .insert_non_send(crate::native::cell_bridge::connect(
                runtime,
                crate::native::wake::WakeSignal::new(|| {}),
            ));
        app.insert_resource(crate::native::sound::Audio::open(
            directory.path().to_path_buf(),
            None,
        ));
        if !app.is_plugin_added::<crate::native::sound::SoundPlugin>() {
            app.add_plugins(crate::native::sound::SoundPlugin);
        }
        let field = app.world().get::<Spoken>(owner).unwrap().import;
        app.world_mut()
            .get_mut::<EditableText>(field)
            .unwrap()
            .editor
            .set_text(&source);
        Command::Import.apply(app.world_mut(), owner);
        println!("phase=voice_import_requested");
        until(&mut app, owner, |spoken| {
            spoken.job.is_none() && spoken.pending.is_none()
        })
        .await;
        assert!(
            speech
                .handle(cell::speech::Request::Voice {
                    request: VoiceRequest::Inspect
                })
                .await
                .unwrap()
                .voice
                .present
        );
        println!("phase=voice_assets_verified");
        Command::Automatic.apply(app.world_mut(), owner);
        super::super::Command::Select(record).apply(app.world_mut(), owner);
        let input = app.world().get::<Entry>(owner).unwrap().input;
        app.world_mut()
            .get_mut::<EditableText>(input)
            .unwrap()
            .editor
            .set_text("Hello through the normal native assistant and local spoken reply.");
        super::super::Command::Send.apply(app.world_mut(), owner);
        println!("phase=normal_configured_fiote_request_submitted");
        until(&mut app, owner, |spoken| {
            spoken.reply.is_some() && (spoken.job.is_some() || spoken.playing)
        })
        .await;
        until(&mut app, owner, |spoken| {
            spoken.reply.is_some()
                && spoken.job.is_none()
                && spoken.chunks.is_empty()
                && !spoken.playing
        })
        .await;
        let spoken = app.world().get::<Spoken>(owner).unwrap();
        assert_eq!(
            spoken.reply.as_ref().unwrap().1,
            "Hello from the native assistant workflow"
        );
        assert!(
            app.world()
                .get::<Text>(spoken.label)
                .unwrap()
                .0
                .contains("finished")
        );
        assert_eq!(script.requests.lock().unwrap().len(), 1);
        println!(
            "Native import, ordinary stored assistant reply and actual speaker playback completed; provider was a fixture."
        );
        Command::Mute.apply(app.world_mut(), owner);
        app.world_mut().entity_mut(owner).despawn();
        speech.shutdown().await;
        host.stop_all().await;
    }

    #[test]
    fn speech_keeps_visible_prose_but_excludes_code_html_and_image_payloads() {
        let markdown = "Hello **world**. [Read this](https://secret.example/token).\n\n```json\n{\"hidden_tool_payload\": true}\n```\n\n`private command` ![private image](file:///secret.png)\n\n<div>hidden html</div>\n\nDone.";
        let text = chunks(markdown)
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            text.contains("Hello")
                && text.contains("world")
                && text.contains("Read this")
                && text.contains("Done")
        );
        for hidden in [
            "hidden_tool_payload",
            "private command",
            "private image",
            "hidden html",
            "secret.example",
            "secret.png",
        ] {
            assert!(!text.contains(hidden), "{text}");
        }
        assert!(chunks("```\nonly code\n```").is_err());
    }

    #[test]
    fn speech_chunks_preserve_word_order_unicode_and_bound_large_replies() {
        let text = "A useful résumé for tomorrow. ".repeat(80);
        let parts = chunks(&text).unwrap();
        assert!(parts.len() > 1);
        assert!(parts.iter().all(|part| part.chars().count() <= 300));
        assert_eq!(parts.into_iter().collect::<Vec<_>>().join(" "), text.trim());
        assert!(chunks(&"x".repeat(301)).is_err());
        assert!(chunks(&"large reply ".repeat(1200)).is_err());
    }

    #[tokio::test]
    async fn old_playback_completion_cannot_end_a_new_reply_and_stop_cancels_without_a_queue() {
        let (mut app, owner, host, _script, _directory, _record) =
            super::super::tests::fixture().await;
        let cancelled = Arc::new(AtomicBool::new(false));
        app.world_mut().entity_mut(owner).insert((
            PlaybackGuard {
                cancelled: cancelled.clone(),
            },
            crate::native::sound::VoiceOutputActive,
            crate::native::sound::SpokenPlayback(Ok(()), Arc::new(AtomicBool::new(false))),
        ));
        app.world_mut().get_mut::<Spoken>(owner).unwrap().playing = true;
        app.update();
        assert!(app.world().get::<Spoken>(owner).unwrap().playing);
        assert!(!cancelled.load(Ordering::Acquire));
        Command::Stop.apply(app.world_mut(), owner);
        assert!(cancelled.load(Ordering::Acquire));
        assert!(!app.world().get::<Spoken>(owner).unwrap().playing);
        assert!(!crate::native::sound::speaking(app.world_mut()));
        host.stop_all().await;
    }

    #[tokio::test]
    async fn a_new_reply_waits_for_cancelled_synthesis_before_consuming_its_text() {
        let (mut app, owner, host, _script, _directory, _record) =
            super::super::tests::fixture().await;
        app.world_mut().get_mut::<Spoken>(owner).unwrap().chunks =
            VecDeque::from(["The new reply stays queued.".into()]);
        let request = next_request(&mut app.world_mut().get_mut::<Spoken>(owner).unwrap(), true);
        assert!(matches!(request, Some(VoiceRequest::Inspect)));
        result(
            app.world_mut(),
            owner,
            &VoiceStatus {
                present: true,
                busy: true,
                ..Default::default()
            },
        )
        .unwrap();
        let mut spoken = app.world_mut().get_mut::<Spoken>(owner).unwrap();
        assert!(matches!(
            next_request(&mut spoken, true),
            Some(VoiceRequest::Inspect)
        ));
        assert_eq!(spoken.chunks.len(), 1);
        assert!(spoken.job.is_none());
        result(
            app.world_mut(),
            owner,
            &VoiceStatus {
                present: true,
                busy: false,
                ..Default::default()
            },
        )
        .unwrap();
        let mut spoken = app.world_mut().get_mut::<Spoken>(owner).unwrap();
        assert!(next_request(&mut spoken, false).is_none());
        let request = next_request(&mut spoken, true).unwrap();
        let VoiceRequest::Speak { job, text } = request else {
            panic!("The ready voice must receive queued text")
        };
        assert_eq!(text, "The new reply stays queued.");
        assert_eq!(spoken.job.as_deref(), Some(job.as_str()));
        assert!(spoken.chunks.is_empty());
        assert!(
            matches!(next_request(&mut spoken, true), Some(VoiceRequest::Poll { job: current }) if current == job)
        );
        stop(app.world_mut(), owner);
        host.stop_all().await;
    }

    #[tokio::test]
    async fn unavailable_voice_preserves_the_written_reply_and_reports_the_setup_step() {
        let (mut app, owner, host, _script, _directory, _record) =
            super::super::tests::fixture().await;
        let label = app.world().get::<Spoken>(owner).unwrap().reply_label;
        app.world_mut().get_mut::<Text>(label).unwrap().0 = "Keep the written answer.".into();
        app.world_mut().get_mut::<Spoken>(owner).unwrap().chunks =
            VecDeque::from(["Keep the written answer.".into()]);
        let error = result(app.world_mut(), owner, &VoiceStatus::default()).unwrap_err();
        assert!(error.contains("Download or import"));
        stop(app.world_mut(), owner);
        assert_eq!(
            app.world().get::<Text>(label).unwrap().0,
            "Keep the written answer."
        );
        assert!(!busy(app.world(), owner));
        host.stop_all().await;
    }

    #[tokio::test]
    async fn completed_reply_is_visible_without_voice_assets_and_auto_speech_starts_off() {
        let (mut app, owner, host, _script, _directory, _record) =
            super::super::tests::fixture().await;
        assert!(!app.world().get::<Spoken>(owner).unwrap().automatic);
        let receipt = EntryReceipt {
            request_id: "voice-reply-fixture".into(),
            fingerprint: "fixture".into(),
            target: nucleus::new_uid("r"),
            thread: Some(nucleus::new_uid("r")),
            message: Some(nucleus::new_uid("r")),
            reply: Some(nucleus::new_uid("r")),
            state: "completed".into(),
            detail: String::new(),
        };
        observe(
            app.world_mut(),
            owner,
            &receipt,
            Some("A completed visible answer."),
        );
        let spoken = app.world().get::<Spoken>(owner).unwrap();
        assert!(spoken.job.is_none() && spoken.chunks.is_empty());
        assert_eq!(
            app.world().get::<Text>(spoken.reply_label).unwrap().0,
            "A completed visible answer."
        );
        Command::Automatic.apply(app.world_mut(), owner);
        assert!(app.world().get::<Spoken>(owner).unwrap().automatic);
        Command::Mute.apply(app.world_mut(), owner);
        assert!(!app.world().get::<Spoken>(owner).unwrap().automatic);
        assert!(!busy(app.world(), owner));
        assert!(!crate::native::sound::speaking(app.world_mut()));
        host.stop_all().await;
    }
}
