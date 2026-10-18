use super::*;
use bevy::ecs::{entity_disabling::Disabled, query::Allow};
use cell::speech::{ListeningMode, ListeningStatus, Request, VoiceRequest};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

mod acoustic;
mod input_check;
mod wake_setup;

#[derive(Component)]
struct Listening {
    owner: String,
    label: Entity,
    microphones: Entity,
    microphone: Option<String>,
    saved: ListeningStatus,
    pending: Option<String>,
    polling: bool,
    capture: Option<Capture>,
    routing: bool,
    mode: ListeningMode,
    wanted: bool,
    stopping: bool,
    failure: Option<String>,
    next: Instant,
    cancellation: Option<crate::native::cell_bridge::SpeechCancellation>,
    recovery: Option<VoiceRequest>,
    recovery_label: Entity,
    recovered: Option<String>,
    conversation_label: Entity,
    restart: bool,
}

impl Drop for Listening {
    fn drop(&mut self) {
        self.capture = None;
        if !self.stopping
            && (self.wanted || self.saved.enabled || self.saved.paused || self.pending.is_some())
            && let Some(cancellation) = &self.cancellation
        {
            let _ = cancellation.send(Request::StopListening {
                owner: self.owner.clone(),
                generation: if self.saved.owner.as_deref() == Some(&self.owner) {
                    self.saved.generation
                } else {
                    0
                },
            });
        }
    }
}

struct Capture {
    stop: Arc<AtomicBool>,
    events: Mutex<mpsc::Receiver<Event>>,
    feedback: Arc<Mutex<Feedback>>,
    conversation: Arc<AtomicBool>,
}

#[derive(Default)]
struct Feedback {
    failure: Option<String>,
    started: bool,
    notice: Option<String>,
    window: Option<Instant>,
}

impl Capture {
    fn event(&self) -> Option<Event> {
        if let Ok(mut feedback) = self.feedback.lock() {
            if let Some(error) = feedback.failure.take() {
                return Some(Event::Failed(error));
            }
            if std::mem::take(&mut feedback.started) {
                return Some(Event::Started);
            }
            if let Some(notice) = feedback.notice.take() {
                return Some(Event::Notice(notice));
            }
        }
        match self.events.lock().ok()?.try_recv() {
            Ok(event) => Some(event),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Event::Failed(
                "The microphone stopped; listening is ending.".into(),
            )),
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

enum Event {
    Started,
    Segment {
        generation: u64,
        id: u64,
        audio: String,
    },
    WokenSegment {
        generation: u64,
        id: u64,
        audio: String,
    },
    Notice(String),
    Failed(String),
}

fn emit(
    send: &mpsc::SyncSender<Event>,
    feedback: &Mutex<Feedback>,
    event: Event,
    wake: Option<&crate::native::wake::WakeSignal>,
) -> bool {
    match event {
        Event::Started => feedback.lock().unwrap().started = true,
        Event::Notice(notice) => {
            feedback.lock().unwrap().notice = Some(notice.chars().take(2048).collect())
        }
        Event::Failed(error) => {
            feedback.lock().unwrap().failure = Some(error.chars().take(2048).collect())
        }
        event => match send.try_send(event) {
            Ok(()) => {}
            Err(mpsc::TrySendError::Full(_)) => {
                feedback.lock().unwrap().notice =
                    Some("Listening is busy; this utterance was not queued.".into());
            }
            Err(mpsc::TrySendError::Disconnected(_)) => return false,
        },
    }
    if let Some(wake) = wake {
        wake.ring();
    }
    true
}

pub(super) fn attach(world: &mut World, owner: Entity) {
    let label = crate::native::edit_mode::label(
        world,
        owner,
        "Listening: off. Set up Hey Fiote, then Start. Follow-up speech needs no wake phrase: a tool-free check uses recent conversation context to accept continuations and end on closing, unrelated or uncertain speech. Without a validated local admission model, the native provider checks transcripts, using its normal quota. Ninety idle seconds or End voice conversation returns to wake mode. Listening pauses during spoken replies. Dictate and Send keeps an editable draft.",
        13.0,
    );
    let controls = ui::row(world, owner);
    ui::button(
        world,
        controls,
        owner,
        "Check listening microphone",
        input_check::Check,
    );
    for (caption, command) in [
        ("Hey Fiote wake mode", Command::Mode(ListeningMode::Wake)),
        (
            "Addressed requests",
            Command::Mode(ListeningMode::Addressed),
        ),
        ("Model-based admission", Command::Mode(ListeningMode::Model)),
        ("Start listening", Command::Start),
        ("Stop listening", Command::Stop),
        ("End voice conversation", Command::End),
        ("Choose listening microphone", Command::Microphones),
        (
            "Use default listening microphone",
            Command::Microphone(None),
        ),
        ("Toggle local destination routing", Command::Routing),
        ("Recover unsent voice request", Command::Recover),
    ] {
        ui::button(world, controls, owner, caption, command);
    }
    let microphones = ui::column(world, owner);
    let recovery_label = crate::native::edit_mode::label(world, owner, "", 13.0);
    let conversation_label =
        crate::native::edit_mode::label(world, owner, "Voice conversation: none", 13.0);
    let cancellation = crate::native::cell_bridge::SpeechCancellation::for_owner(world, owner);
    world.entity_mut(owner).insert(Listening {
        owner: nucleus::new_uid("voice"),
        label,
        microphones,
        microphone: None,
        saved: ListeningStatus::default(),
        pending: None,
        polling: false,
        capture: None,
        routing: false,
        mode: ListeningMode::Wake,
        wanted: false,
        stopping: false,
        failure: None,
        next: Instant::now(),
        cancellation,
        recovery: None,
        recovery_label,
        recovered: None,
        conversation_label,
        restart: false,
    });
    wake_setup::attach(world, owner);
}

fn send(world: &mut World, owner: Entity, request: Request) -> Result<(), String> {
    let cancellation = crate::native::cell_bridge::SpeechCancellation::for_owner(world, owner);
    if matches!(
        &request,
        Request::StopListening { .. } | Request::PauseListening { .. }
    ) {
        let cancellation = world
            .get::<Listening>(owner)
            .and_then(|listening| listening.cancellation.clone())
            .or(cancellation)
            .ok_or("The speech connection is unavailable.")?;
        let id = cancellation.send(request)?;
        let mut listening = world
            .get_mut::<Listening>(owner)
            .ok_or("Listening controls closed.")?;
        listening.pending = Some(id);
        listening.polling = false;
        listening.cancellation = Some(cancellation);
        return Ok(());
    }
    let polling = matches!(&request, Request::Inspect);
    let id = nucleus::new_uid("listening-control");
    ui::send(
        world,
        owner,
        cell::ClientMessage::Speech {
            id: id.clone(),
            request,
        },
    )?;
    let mut listening = world
        .get_mut::<Listening>(owner)
        .ok_or("Listening controls closed.")?;
    listening.pending = Some(id);
    listening.polling = polling;
    listening.cancellation = cancellation;
    Ok(())
}

#[derive(Clone)]
enum Command {
    Mode(ListeningMode),
    Start,
    Stop,
    End,
    Microphones,
    Microphone(Option<String>),
    Routing,
    Recover,
}

#[derive(Component)]
pub(super) struct Resume(String);

pub(super) fn suspend(world: &mut World, owner: Entity) {
    let previous = world
        .get::<Resume>(owner)
        .map(|resume| resume.0.clone())
        .or_else(|| {
            world
                .get::<Listening>(owner)
                .filter(|listening| listening.wanted || listening.saved.enabled)
                .map(|listening| listening.owner.clone())
        });
    let enabled = world
        .get::<Listening>(owner)
        .is_some_and(|listening| listening.saved.enabled && !listening.stopping);
    if enabled {
        let mut listening = world.get_mut::<Listening>(owner).unwrap();
        listening.capture = None;
        listening.wanted = false;
        listening.stopping = true;
        let request = Request::PauseListening {
            owner: listening.owner.clone(),
            generation: listening.saved.generation,
        };
        if let Err(error) = send(world, owner, request) {
            let mut listening = world.get_mut::<Listening>(owner).unwrap();
            listening.stopping = false;
            listening.failure = Some(error);
            Command::Stop.apply(world, owner);
        }
    } else if world
        .get::<Listening>(owner)
        .is_none_or(|listening| !listening.saved.paused)
    {
        Command::Stop.apply(world, owner);
    }
    if let Some(previous) = previous {
        world.entity_mut(owner).insert(Resume(previous));
    }
}

pub(super) fn suspend_all(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, (With<Listening>, Allow<Disabled>)>()
        .iter(world)
        .collect();
    for owner in owners {
        suspend(world, owner);
    }
}

pub(super) fn quiet(world: &mut World) -> bool {
    !world
        .query_filtered::<&Listening, Allow<Disabled>>()
        .iter(world)
        .any(|listening| {
            listening.wanted
                || listening.saved.enabled
                || listening.pending.is_some()
                || listening.stopping
                || listening.capture.is_some()
        })
}

pub(super) fn resume(world: &mut World, owner: Entity) {
    let Some(previous) = world.get::<Resume>(owner) else {
        return;
    };
    let Some(listening) = world.get::<Listening>(owner) else {
        return;
    };
    if listening.pending.is_some() || listening.stopping || listening.saved.finishing {
        return;
    }
    let restore = previous.0 == listening.owner && view_active(world, owner);
    world.entity_mut(owner).remove::<Resume>();
    if restore {
        let listening = world.get::<Listening>(owner).unwrap();
        if listening.saved.paused {
            let request = Request::ResumeListening {
                owner: listening.owner.clone(),
                generation: listening.saved.generation,
            };
            world.get_mut::<Listening>(owner).unwrap().wanted = true;
            if let Err(error) = send(world, owner, request) {
                let mut listening = world.get_mut::<Listening>(owner).unwrap();
                listening.wanted = false;
                let label = listening.label;
                world.get_mut::<Text>(label).unwrap().0 = error;
            }
        } else {
            Command::Start.apply(world, owner);
        }
    }
}

pub(super) fn resume_all(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, (With<Resume>, Allow<Disabled>)>()
        .iter(world)
        .collect();
    for owner in owners {
        resume(world, owner);
    }
}

pub(super) fn routing(world: &World, owner: Entity) -> bool {
    world
        .get::<Listening>(owner)
        .is_some_and(|listening| listening.routing)
}

pub(super) fn invalidate(world: &mut World, owner: Entity) {
    if world.get::<Resume>(owner).is_some()
        || world.get::<Listening>(owner).is_some_and(|listening| {
            listening.wanted
                || listening.saved.enabled
                || listening.saved.paused
                || listening.capture.is_some()
        })
    {
        Command::Stop.apply(world, owner);
    }
}

pub(super) fn view_active(world: &World, mut owner: Entity) -> bool {
    if crate::native::laboratory::suspended(world, owner) {
        return false;
    }
    let mut workspace = None;
    loop {
        if world
            .get::<Node>(owner)
            .is_some_and(|node| node.display == Display::None)
            || world.get::<Visibility>(owner) == Some(&Visibility::Hidden)
            || world.get::<Disabled>(owner).is_some()
        {
            return false;
        }
        if let Some(member) = world.get::<crate::native::workspace::WorkspaceMember>(owner) {
            workspace.get_or_insert(member.0);
        }
        if let (Some(workspace), Some(spaces)) = (
            workspace,
            world.get::<crate::native::workspace::Workspaces>(owner),
        ) && workspace != spaces.active
        {
            return false;
        }
        let Some(parent) = world.get::<ChildOf>(owner) else {
            return true;
        };
        owner = parent.parent();
    }
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if matches!(self, Self::End) {
            let Some(listening) = world.get::<Listening>(owner) else {
                return;
            };
            let restart = listening.wanted || listening.saved.enabled || listening.saved.paused;
            super::spoken::invalidate(world, owner);
            Self::Stop.apply(world, owner);
            world.get_mut::<Listening>(owner).unwrap().restart = restart;
            return;
        }
        let Some(listening) = world.get::<Listening>(owner) else {
            return;
        };
        let label = listening.label;
        let paused = world.get::<Resume>(owner).is_some();
        let (session, generation, enabled, pending, routing, microphones, mode) = (
            listening.owner.clone(),
            listening.saved.generation,
            listening.saved.enabled,
            listening.pending.is_some(),
            listening.routing,
            listening.microphones,
            listening.mode,
        );
        let result = (|| {
            if matches!(self, Self::Stop) {
                world.entity_mut(owner).remove::<Resume>();
                input_check::cancel(world, owner);
                wake_setup::cancel(world, owner);
                let mut listening = world.get_mut::<Listening>(owner).unwrap();
                listening.capture = None;
                listening.wanted = false;
                listening.restart = false;
                let backend_paused = listening.saved.paused;
                listening.saved.conversation = None;
                let conversation_label = listening.conversation_label;
                world.get_mut::<Text>(conversation_label).unwrap().0 =
                    "Voice conversation: none".into();
                let mut listening = world.get_mut::<Listening>(owner).unwrap();
                if listening.stopping {
                    return Ok(());
                }
                listening.stopping = enabled || pending || backend_paused;
                if enabled || backend_paused {
                    return send(
                        world,
                        owner,
                        Request::StopListening {
                            owner: session,
                            generation,
                        },
                    );
                }
                if pending {
                    return send(
                        world,
                        owner,
                        Request::StopListening {
                            owner: session,
                            generation: 0,
                        },
                    );
                }
                return Ok(());
            }
            if pending {
                return Err("Wait for the current listening control response.".into());
            }
            match self {
                Self::Mode(mode) => {
                    if enabled
                        || paused
                        || world
                            .get::<Listening>(owner)
                            .is_some_and(|listening| listening.wanted || listening.stopping)
                    {
                        return Err("Stop listening before changing its mode.".into());
                    }
                    world.get_mut::<Listening>(owner).unwrap().mode = *mode;
                }
                Self::Start => {
                    if crate::native::speech::call_microphone_active(world) {
                        return Err(
                            "Stop the call microphone before enabling assistant listening.".into(),
                        );
                    }
                    if wake_setup::busy(world, owner) {
                        return Err("Finish or cancel wake setup before starting listening.".into());
                    }
                    if crate::native::sound::speaking(world) {
                        return Err("Stop speaking before enabling microphone activation.".into());
                    }
                    if world.get::<input_check::Checking>(owner).is_some() {
                        return Err("Finish or stop the microphone check before listening.".into());
                    }
                    if !view_active(world, owner) {
                        return Err(
                            "Open this assistant in the active workspace before listening.".into(),
                        );
                    }
                    if enabled {
                        return Ok(());
                    }
                    if crate::native::speech::active(world, owner) {
                        return Err(
                            "Finish or cancel dictation before starting general listening.".into(),
                        );
                    }
                    let target = world
                        .get::<Entry>(owner)
                        .ok_or("Assistant controls closed.")?
                        .target
                        .clone();
                    let session = nucleus::new_uid("voice");
                    let mut listening = world.get_mut::<Listening>(owner).unwrap();
                    listening.owner = session.clone();
                    listening.wanted = true;
                    listening.failure = None;
                    send(
                        world,
                        owner,
                        Request::StartListening {
                            mode,
                            owner: session,
                            target,
                            routing,
                        },
                    )?;
                }
                Self::Microphones => {
                    if enabled || paused {
                        return Err("Stop listening before changing its microphone.".into());
                    }
                    ui::clear(world, microphones);
                    for name in crate::native::sound::device::Capture::devices()? {
                        ui::button(
                            world,
                            microphones,
                            owner,
                            &name,
                            Command::Microphone(Some(name.clone())),
                        );
                    }
                }
                Self::Microphone(microphone) => {
                    if enabled || paused {
                        return Err("Stop listening before changing its microphone.".into());
                    }
                    world.get_mut::<Listening>(owner).unwrap().microphone = microphone.clone();
                }
                Self::Routing => {
                    if enabled || paused {
                        return Err("Stop listening before changing destination routing.".into());
                    }
                    world.get_mut::<Listening>(owner).unwrap().routing = !routing;
                }
                Self::Recover => {
                    let recovery = world
                        .get::<Listening>(owner)
                        .unwrap()
                        .recovery
                        .clone()
                        .ok_or("There is no admitted unsent voice request to recover.")?;
                    let entry = world.get::<Entry>(owner).ok_or("Assistant draft closed.")?;
                    if entry.pending.is_some() {
                        return Err(
                            "Wait for the current request response before recovering voice text."
                                .into(),
                        );
                    }
                    let receipt = entry
                        .receipt
                        .as_ref()
                        .filter(|receipt| receipt.request_id == recovery.request.request_id);
                    if receipt.is_some_and(|receipt| receipt.message.is_some()) {
                        return Err("This voice request was already delivered. Open its conversation instead of sending it again.".into());
                    }
                    if recovery.uncertain
                        && !receipt.is_some_and(|receipt| {
                            receipt.state == "failed" && receipt.thread.is_none()
                        })
                    {
                        return Err("Delivery is uncertain. Inspect the original request before recovering or sending it again.".into());
                    }
                    let input = entry.input;
                    let current = world
                        .get::<EditableText>(input)
                        .ok_or("Assistant draft closed.")?
                        .value()
                        .to_string();
                    let text = crate::native::speech::append(&current, &recovery.request.text)?;
                    world
                        .get_mut::<EditableText>(input)
                        .unwrap()
                        .editor
                        .set_text(&text);
                    let mut listening = world.get_mut::<Listening>(owner).unwrap();
                    listening.recovered = Some(recovery.request.request_id);
                    listening.recovery = None;
                    let recovery_label = listening.recovery_label;
                    world.get_mut::<Text>(recovery_label).unwrap().0.clear();
                }
                Self::Stop | Self::End => unreachable!(),
            }
            Ok(())
        })();
        if result.is_err() {
            if matches!(self, Self::Stop) {
                world.get_mut::<Listening>(owner).unwrap().stopping = false;
            } else if matches!(self, Self::Start) {
                world.get_mut::<Listening>(owner).unwrap().wanted = false;
            }
        }
        let failure = world
            .get::<Listening>(owner)
            .and_then(|listening| listening.failure.clone());
        if let Some(mut text) = world.get_mut::<Text>(label) {
            text.0 = match result { Err(error) => error, Ok(()) => match self { Self::Mode(ListeningMode::Wake) => "Wake mode selected. Say Hey Fiote for the first request; follow-ups are checked in context; closing, unrelated or uncertain speech ends it.".into(), Self::Mode(ListeningMode::Addressed) => "Address the chosen/default Fiote for the first request; follow-ups are checked in context in its conversation.".into(), Self::Mode(ListeningMode::Model) => "Model-based admission needs a separately validated model before recording can start.".into(), Self::Start => "Checking local listening requirements before opening the microphone…".into(), Self::Stop => "Microphone stopped; finishing listening cancellation. Already delivered Fiote runs remain separate.".into(), Self::End => "Ending the voice conversation.".into(), Self::Routing => format!("Local destination routing: {}. The ranked default is used when unavailable or uncertain.", if routing { "off" } else { "on" }), Self::Microphone(name) => format!("Listening microphone: {}", name.as_deref().unwrap_or("system default")), Self::Microphones => "Choose the listening microphone.".into(), Self::Recover => "Voice request copied into your editable draft. Review it and Send when ready.".into() } };
            if matches!(self, Self::Stop)
                && let Some(failure) = failure
            {
                text.0 = failure;
            }
        }
    }
}

struct Segments {
    segmenter: cell::fiote_communication::speech::segment::Segmenter,
    resampler: Option<crate::native::speech::Resampler>,
    quality: crate::native::speech::CaptureQuality,
    acoustic: Option<acoustic::Gate>,
}

impl Segments {
    fn new(generation: u64) -> Self {
        Self {
            segmenter: cell::fiote_communication::speech::segment::Segmenter::new(generation),
            resampler: None,
            quality: Default::default(),
            acoustic: None,
        }
    }

    fn push(&mut self, clip: &crate::native::sound::library::Clip) -> Result<Vec<Event>, String> {
        if self.resampler.is_none() {
            self.resampler = Some(crate::native::speech::Resampler::new(clip.rate)?);
        }
        let samples = self.resampler.as_mut().unwrap().push(clip)?;
        self.quality.push(clip)?;
        let mut events = Vec::new();
        if samples.is_empty()
            && let Some(acoustic) = &mut self.acoustic
        {
            return acoustic.push(&samples);
        }
        for chunk in samples.chunks(16_000) {
            if let Some(acoustic) = &mut self.acoustic {
                events.extend(acoustic.push(chunk)?);
                continue;
            }
            for event in self.segmenter.push(chunk)? {
                events.push(match event {
                    cell::fiote_communication::speech::segment::Event::Utterance {
                        id,
                        generation,
                        samples,
                    } => Event::Segment {
                        generation,
                        id,
                        audio: crate::native::speech::encode(
                            crate::native::sound::library::Clip {
                                samples,
                                rate: 16_000,
                            },
                        )?,
                    },
                    cell::fiote_communication::speech::segment::Event::Rejected { reason } => {
                        Event::Notice(reason.into())
                    }
                });
            }
        }
        Ok(events)
    }
}

fn capture(
    microphone: Option<String>,
    generation: u64,
    wake: Option<crate::native::wake::WakeSignal>,
    profile: Option<cell::fiote_communication::speech::wake::Profile>,
    continuing: bool,
) -> Capture {
    let (send, receive) = mpsc::sync_channel(1);
    let stop = Arc::new(AtomicBool::new(false));
    let control = stop.clone();
    let feedback = Arc::new(Mutex::new(Feedback::default()));
    let report = feedback.clone();
    let conversation = Arc::new(AtomicBool::new(continuing));
    let ongoing = conversation.clone();
    std::thread::spawn(move || {
        let result = (|| {
            let mut segments = Segments::new(generation);
            if let Some(profile) = profile {
                segments.acoustic = Some(acoustic::Gate::new(profile, generation)?);
            }
            let device = crate::native::sound::device::Capture::start_when_available(
                microphone.as_deref(),
                &control,
            )?;
            let initial = crate::native::speech::readiness::prepare(&device, || {
                control.load(Ordering::Acquire)
            })?;
            if let Some(gate) = &mut segments.acoustic {
                gate.set_conversation(ongoing.load(Ordering::Acquire));
            }
            if !emit(&send, &report, Event::Started, wake.as_ref()) {
                return Ok(());
            }
            for event in segments.push(&initial)? {
                if !emit(&send, &report, event, wake.as_ref()) {
                    return Ok(());
                }
            }
            let mut last_audio = Instant::now();
            while !control.load(Ordering::Acquire) {
                if let Some(gate) = &mut segments.acoustic {
                    gate.set_conversation(ongoing.load(Ordering::Acquire));
                }
                if let Some(error) = device.error.lock().unwrap().clone() {
                    return Err(error);
                }
                let clip = device.drain();
                if !clip.samples.is_empty() {
                    last_audio = Instant::now();
                } else if last_audio.elapsed() >= Duration::from_secs(3) {
                    return Err("The microphone stopped delivering audio. Listening stopped; choose an input and retry.".into());
                }
                for event in segments.push(&clip)? {
                    if !emit(&send, &report, event, wake.as_ref()) {
                        return Ok(());
                    }
                }
                report.lock().unwrap().window =
                    segments.acoustic.as_ref().and_then(|gate| gate.deadline());
                std::thread::sleep(Duration::from_millis(100));
            }
            Ok::<_, String>(())
        })();
        if let Err(error) = result {
            emit(&send, &report, Event::Failed(error), wake.as_ref());
        }
    });
    Capture {
        stop,
        events: Mutex::new(receive),
        feedback,
        conversation,
    }
}

pub(in crate::native::fiote::session) fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::native::cell_bridge::CellMessage>>,
) {
    input_check::update(world);
    wake_setup::update(world);
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
        match &event {
            cell::ServerMessage::Speech { id, status } => {
                wake_setup::response(world, id, Ok(status))
            }
            cell::ServerMessage::Error { id, message, .. } => {
                wake_setup::response(world, id, Err(message))
            }
            _ => {}
        }
        if let cell::ServerMessage::Error { id, message, .. } = &event
            && id == crate::native::cell_bridge::CONNECTION
        {
            let owners: Vec<_> = world
                .query_filtered::<Entity, (With<Listening>, Allow<Disabled>)>()
                .iter(world)
                .collect();
            for owner in owners {
                Command::Stop.apply(world, owner);
                let mut listening = world.get_mut::<Listening>(owner).unwrap();
                listening.capture = None;
                listening.wanted = false;
                listening.pending = None;
                listening.polling = false;
                listening.saved.enabled = false;
                listening.failure = Some(message.chars().take(2048).collect());
                let label = listening.label;
                world.get_mut::<Text>(label).unwrap().0 =
                    format!("General listening stopped: {message}");
            }
            continue;
        }
        let (id, result) = match event {
            cell::ServerMessage::Speech { id, status } => (id, Ok(status)),
            cell::ServerMessage::Error { id, message, .. } => (id, Err(message)),
            _ => continue,
        };
        let owners: Vec<_> = world
            .query_filtered::<(Entity, &Listening), Allow<Disabled>>()
            .iter(world)
            .filter(|(_, listening)| listening.pending.as_deref() == Some(&id))
            .map(|(owner, _)| owner)
            .collect();
        for owner in owners {
            let active_view = view_active(world, owner);
            let mut listening = world.get_mut::<Listening>(owner).unwrap();
            listening.pending = None;
            listening.polling = false;
            listening.stopping = false;
            listening.next = Instant::now() + Duration::from_millis(250);
            let label = listening.label;
            match &result {
                Err(error) => {
                    listening.capture = None;
                    listening.wanted = false;
                    let detail = listening
                        .failure
                        .as_ref()
                        .map_or_else(|| error.clone(), |failure| format!("{failure}\n{error}"));
                    world.get_mut::<Text>(label).unwrap().0 = detail;
                }
                Ok(status) => {
                    let saved = &status.listening;
                    if saved.enabled && saved.owner.as_deref() != Some(&listening.owner) {
                        listening.capture = None;
                        listening.wanted = false;
                        listening.saved.enabled = false;
                        world.get_mut::<Text>(label).unwrap().0 = "Another listening session owns the microphone. This view stopped recording.".into();
                        continue;
                    }
                    listening.saved = saved.clone();
                    let conversation_label = listening.conversation_label;
                    if let Some(capture) = &listening.capture {
                        capture
                            .conversation
                            .store(saved.conversation.is_some(), Ordering::Release);
                    }
                    let detail = saved.conversation.as_ref().map_or_else(|| "Voice conversation: none".into(), |conversation| {
                        format!("Voice conversation: Fiote {} · thread {}. Contextually accepted follow-ups go here; closing speech, idle timeout or End voice conversation chooses again.", conversation.target, conversation.thread)
                    });
                    world.get_mut::<Text>(conversation_label).unwrap().0 = detail;
                    if saved.paused && (world.get::<Resume>(owner).is_none() || !active_view) {
                        Command::Stop.apply(world, owner);
                        continue;
                    }
                    let mut listening = world.get_mut::<Listening>(owner).unwrap();
                    if saved.enabled
                        && (!listening.wanted || !active_view || saved.mode != listening.mode)
                    {
                        if saved.mode != listening.mode {
                            listening.failure = Some("The confirmed listening mode differs from this view. Listening was stopped; choose the mode and Start again.".into());
                        }
                        let session = listening.owner.clone();
                        let generation = saved.generation;
                        listening.capture = None;
                        listening.wanted = false;
                        listening.stopping = true;
                        let _ = send(
                            world,
                            owner,
                            Request::StopListening {
                                owner: session,
                                generation,
                            },
                        );
                        continue;
                    }
                    let generation = saved.generation;
                    let needs_capture = saved.enabled && listening.capture.is_none();
                    let microphone = listening.microphone.clone();
                    let profile = if listening.mode == ListeningMode::Wake {
                        status.wake_profile.clone()
                    } else {
                        None
                    };
                    if !saved.enabled {
                        listening.capture = None;
                    }
                    if needs_capture {
                        if saved.mode == ListeningMode::Wake && profile.is_none() {
                            world.get_mut::<Listening>(owner).unwrap().failure = Some("The wake reference is missing. Record three examples or import a compatible profile, then Start again.".into());
                            Command::Stop.apply(world, owner);
                            continue;
                        }
                        let wake = world
                            .get_resource::<crate::native::wake::WakeSignal>()
                            .cloned();
                        world.get_mut::<Listening>(owner).unwrap().capture = Some(capture(
                            microphone,
                            generation,
                            wake,
                            profile,
                            saved.conversation.is_some(),
                        ));
                    }
                    let mut issue = None;
                    if let Some(receipt) = &saved.receipt {
                        issue = remember_voice_receipt(world, owner, receipt).err();
                    }
                    if let Some(recovery) = &saved.unsent {
                        let listening = world.get::<Listening>(owner).unwrap();
                        if recovery.owner == listening.owner
                            && recovery.generation == saved.generation
                            && listening.recovered.as_deref() != Some(&recovery.request.request_id)
                        {
                            let label = listening.recovery_label;
                            world.get_mut::<Listening>(owner).unwrap().recovery =
                                Some(recovery.clone());
                            world.get_mut::<Text>(label).unwrap().0 = format!(
                                "Admitted request remains unsent{}:\n{}",
                                if recovery.uncertain {
                                    "; inspect delivery first"
                                } else {
                                    ""
                                },
                                recovery.request.text,
                            );
                            if saved.receipt.is_none()
                                && world
                                    .get::<Entry>(owner)
                                    .is_some_and(|entry| entry.pending.is_none())
                            {
                                let unchanged = world
                                    .get::<Entry>(owner)
                                    .unwrap()
                                    .saved
                                    .as_ref()
                                    .is_some_and(|previous| {
                                        previous.request_id == recovery.request.request_id
                                    });
                                if !unchanged {
                                    issue = preserve(
                                        world,
                                        owner,
                                        Saved {
                                            request_id: recovery.request.request_id.clone(),
                                            fingerprint: String::new(),
                                            target: recovery.request.target.clone(),
                                            resolved: true,
                                            destination: recovery.request.target.clone(),
                                        },
                                    )
                                    .err()
                                    .or(issue);
                                }
                            }
                        }
                    }
                    let failure = world.get::<Listening>(owner).unwrap().failure.clone();
                    world.get_mut::<Text>(label).unwrap().0 = format!(
                        "Listening: {:?}{}\n{}",
                        saved.stage,
                        if saved.pending {
                            " · one utterance pending"
                        } else {
                            ""
                        },
                        issue.as_deref().unwrap_or(&saved.detail)
                    );
                    if let Some(failure) = failure {
                        world
                            .get_mut::<Text>(label)
                            .unwrap()
                            .0
                            .push_str(&format!("\n{failure}"));
                    }
                }
            }
        }
    }
    let owners: Vec<_> = world
        .query_filtered::<Entity, (With<Listening>, Allow<Disabled>)>()
        .iter(world)
        .collect();
    for owner in owners {
        if world.get::<Listening>(owner).is_some_and(|listening| {
            (listening.wanted
                || listening.saved.enabled
                || listening.pending.is_some()
                || listening.capture.is_some())
                && listening
                    .cancellation
                    .as_ref()
                    .is_some_and(|cancellation| cancellation.is_closed())
        }) {
            Command::Stop.apply(world, owner);
            let mut listening = world.get_mut::<Listening>(owner).unwrap();
            listening.pending = None;
            listening.polling = false;
            listening.stopping = false;
            listening.saved.enabled = false;
            listening.saved.finishing = false;
            listening.saved.stage = cell::speech::ListeningStage::Degraded;
            let label = listening.label;
            world.get_mut::<Text>(label).unwrap().0 = "The speech connection closed. Reconnect this Cell and Start again; your draft is unchanged.".into();
            continue;
        }
        if crate::native::speech::call_microphone_active(world)
            && world.get::<Listening>(owner).is_some_and(|listening| {
                listening.wanted || listening.saved.enabled || listening.capture.is_some()
            })
        {
            world.get_mut::<Listening>(owner).unwrap().failure = Some(
                "Assistant listening stopped because a call started using the microphone.".into(),
            );
            Command::Stop.apply(world, owner);
            continue;
        }
        if !view_active(world, owner)
            && world.get::<Listening>(owner).is_some_and(|listening| {
                !listening.stopping
                    && (listening.wanted
                        || listening.saved.enabled
                        || listening.saved.paused
                        || listening.pending.is_some()
                        || listening.capture.is_some())
            })
        {
            Command::Stop.apply(world, owner);
            continue;
        }
        let event = world
            .get::<Listening>(owner)
            .and_then(|listening| listening.capture.as_ref())
            .and_then(Capture::event);
        if let Some(event) = event {
            let woken = matches!(&event, Event::WokenSegment { .. });
            let listening = world.get::<Listening>(owner).unwrap();
            let label = listening.label;
            match event {
                Event::Started => {
                    let detail = if listening.saved.conversation.is_some() {
                        "Microphone active · follow-ups need no wake phrase. Conversation context is checked before sending; closing or uncertain speech returns to wake detection."
                    } else {
                        match listening.mode {
                            ListeningMode::Wake => {
                                "Microphone active · acoustic detector waiting for Hey Fiote. Other speech is neither transcribed nor sent."
                            }
                            ListeningMode::Addressed => {
                                "Microphone active · clear addresses send a normal Fiote request; other speech stays local."
                            }
                            ListeningMode::Model => {
                                "Microphone active · speech stays local until the model admits a request."
                            }
                        }
                    };
                    world.get_mut::<Text>(label).unwrap().0 = detail.into();
                }
                Event::Notice(detail) => {
                    world.get_mut::<Text>(label).unwrap().0 = detail;
                }
                Event::Failed(error) => {
                    world.get_mut::<Listening>(owner).unwrap().failure = Some(error.clone());
                    Command::Stop.apply(world, owner);
                    world.get_mut::<Text>(label).unwrap().0 = error;
                }
                Event::Segment {
                    generation,
                    id,
                    audio,
                }
                | Event::WokenSegment {
                    generation,
                    id,
                    audio,
                } => {
                    if listening.saved.enabled && listening.saved.generation == generation {
                        if (listening.pending.is_some() && !listening.polling)
                            || listening.saved.pending
                        {
                            world.get_mut::<Text>(label).unwrap().0 =
                                "Listening is busy; this utterance was not queued.".into();
                        } else {
                            let session = listening.owner.clone();
                            let request = if woken {
                                Request::WokenSegment {
                                    owner: session,
                                    generation,
                                    segment: id,
                                    audio: cell::FioteSecret(audio),
                                }
                            } else {
                                Request::ListeningSegment {
                                    owner: session,
                                    generation,
                                    segment: id,
                                    audio: cell::FioteSecret(audio),
                                }
                            };
                            if let Err(error) = send(world, owner, request) {
                                world.get_mut::<Text>(label).unwrap().0 = error;
                            }
                        }
                    }
                }
            }
        }
        let window = world
            .get::<Listening>(owner)
            .and_then(|listening| listening.capture.as_ref())
            .and_then(|capture| capture.feedback.lock().ok()?.window)
            .filter(|deadline| *deadline > Instant::now());
        if let Some(deadline) = window {
            let mut listening = world.get_mut::<Listening>(owner).unwrap();
            listening.saved.stage = cell::speech::ListeningStage::RequestWindow;
            let label = listening.label;
            world.get_mut::<Text>(label).unwrap().0 = format!(
                "Heard Hey Fiote · start one request within {} seconds. Pause when finished; requests longer than twenty seconds are refused. Nothing is sent until the request is complete.",
                deadline.saturating_duration_since(Instant::now()).as_secs() + 1
            );
        }
        if world.get::<Listening>(owner).is_some_and(|listening| {
            listening.restart
                && listening.pending.is_none()
                && !listening.stopping
                && !listening.saved.finishing
        }) && view_active(world, owner)
            && !crate::native::sound::speaking(world)
        {
            world.get_mut::<Listening>(owner).unwrap().restart = false;
            Command::Start.apply(world, owner);
        }
        let listening = world.get::<Listening>(owner).unwrap();
        if (listening.saved.enabled || listening.saved.finishing)
            && listening.pending.is_none()
            && listening.next <= Instant::now()
        {
            if let Err(error) = send(world, owner, Request::Inspect) {
                let label = world.get::<Listening>(owner).unwrap().label;
                world.get_mut::<Text>(label).unwrap().0 = error;
            }
        }
        if world
            .get::<Listening>(owner)
            .is_some_and(|listening| listening.capture.is_some() || listening.pending.is_some())
            && let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>()
        {
            wake.after(Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod tests;
