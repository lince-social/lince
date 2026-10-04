use bevy::prelude::*;
use lince_interface::sound::{Cue, Mode, Output, Queue};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, mpsc},
    thread,
};

#[derive(Clone, Debug)]
pub struct Voice {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct Playback {
    pub key: String,
    pub at_ms: i64,
    pub started_ms: i64,
    pub projected: bool,
}

impl Playback {
    fn started(cue: &Cue) -> Self {
        Self {
            key: cue.key.clone(),
            at_ms: cue.at_ms,
            started_ms: chrono::Utc::now().timestamp_millis(),
            projected: cue.projected,
        }
    }
}

enum Command {
    Play(Cue),
    Preview(Cue),
    Schedule(u64, i64, Vec<Cue>),
    Stop,
    Cancel(u64),
    Retain(u64, Vec<(String, i64)>),
    Catalog,
    Shutdown,
}

enum Event {
    Voices(Vec<Voice>),
    Failure(String),
    Ready(Playback),
    Completed,
    Scheduled(Option<i64>),
}

struct Queued {
    cue: Cue,
    preview: bool,
}

struct Speaking {
    scope: u64,
    key: (String, i64),
    id: Option<String>,
    started: std::time::Instant,
    preview: bool,
}

#[derive(Resource)]
pub struct Native {
    sender: mpsc::Sender<Command>,
    events: Mutex<mpsc::Receiver<Event>>,
    task: Option<thread::JoinHandle<()>>,
    pub voices: Vec<Voice>,
    pub error: Option<String>,
    pub started: u64,
    pub completed: u64,
    pub next_ms: Option<i64>,
    pub recent: VecDeque<Playback>,
}

impl FromWorld for Native {
    fn from_world(world: &mut World) -> Self {
        let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
        let (sender, receiver) = mpsc::channel();
        let (events, incoming) = mpsc::channel();
        let task = thread::Builder::new()
            .name("interface-sound".into())
            .spawn(move || {
                let notify = |event| {
                    let _ = events.send(event);
                    if let Some(wake) = &wake {
                        wake.ring();
                    }
                };
                worker(receiver, notify);
            })
            .expect("start interface sound worker");
        Self {
            sender,
            events: Mutex::new(incoming),
            task: Some(task),
            voices: Vec::new(),
            error: None,
            started: 0,
            completed: 0,
            next_ms: None,
            recent: VecDeque::new(),
        }
    }
}

impl Native {
    pub fn schedule(&self, scope: u64, now: i64, cues: Vec<Cue>) -> Result<(), String> {
        self.send(Command::Schedule(scope, now, cues))
    }

    pub fn preview(&self, cue: Cue) -> Result<(), String> {
        self.send(Command::Preview(cue))
    }
    pub fn catalog(&self) -> Result<(), String> {
        self.send(Command::Catalog)
    }

    pub fn poll(&mut self) {
        for event in self.events.lock().unwrap().try_iter() {
            match event {
                Event::Voices(voices) => {
                    self.voices = voices;
                    self.error = None;
                }
                Event::Failure(error) => self.error = Some(error),
                Event::Ready(playback) => {
                    self.error = None;
                    self.started += 1;
                    if self.recent.len() == 64 {
                        self.recent.pop_front();
                    }
                    self.recent.push_back(playback);
                }
                Event::Completed => self.completed += 1,
                Event::Scheduled(next) => self.next_ms = next,
            }
        }
    }

    fn send(&self, command: Command) -> Result<(), String> {
        self.sender
            .send(command)
            .map_err(|error| format!("Interface sound is unavailable: {error}"))
    }
}

impl Output for Native {
    fn play(&self, cue: Cue) -> Result<(), String> {
        self.send(Command::Play(cue))
    }
    fn stop(&self) -> Result<(), String> {
        self.send(Command::Stop)
    }
    fn cancel(&self, scope: u64) -> Result<(), String> {
        self.send(Command::Cancel(scope))
    }
    fn retain(&self, scope: u64, keys: Vec<(String, i64)>) -> Result<(), String> {
        self.send(Command::Retain(scope, keys))
    }
}

impl Drop for Native {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Shutdown);
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}

fn engine<'a>(
    engine: &'a mut Option<tts::Tts>,
    completed: &mpsc::Sender<String>,
) -> Result<&'a mut tts::Tts, String> {
    if engine.is_none() {
        let speech =
            tts::Tts::default().map_err(|error| format!("Title speech is unavailable: {error}"))?;
        if speech.supported_features().utterance_callbacks {
            let ended = completed.clone();
            speech
                .on_utterance_end(Some(Box::new(move |id| {
                    let _ = ended.send(format!("{id:?}"));
                })))
                .map_err(|error| error.to_string())?;
            let stopped = completed.clone();
            speech
                .on_utterance_stop(Some(Box::new(move |id| {
                    let _ = stopped.send(format!("{id:?}"));
                })))
                .map_err(|error| error.to_string())?;
        }
        *engine = Some(speech);
    }
    Ok(engine.as_mut().unwrap())
}

fn worker(receiver: mpsc::Receiver<Command>, notify: impl Fn(Event)) {
    let mut schedule = Queue::default();
    let mut output: Option<crate::sound::device::Output> = None;
    let mut speech: Option<tts::Tts> = None;
    let mut last_voice = None;
    let mut pending = VecDeque::<Queued>::new();
    let mut speaking: Option<Speaking> = None;
    let (completed, finished) = mpsc::channel();
    let clip = Arc::new(crate::sound::library::Clip {
        samples: lince_interface::sound::blip(48_000),
        rate: 48_000,
    });
    loop {
        let now = chrono::Utc::now().timestamp_millis();
        let delay = schedule
            .next_ms(now)
            .map(|at| at.saturating_sub(now).clamp(1, 60_000) as u64);
        let delay = if speaking.is_some() || !pending.is_empty() {
            Some(delay.unwrap_or(30).min(30))
        } else {
            delay
        };
        let command = delay.map_or_else(
            || {
                receiver
                    .recv()
                    .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
            },
            |delay| receiver.recv_timeout(std::time::Duration::from_millis(delay)),
        );
        match command {
            Ok(Command::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                if let Some(speech) = &mut speech {
                    let _ = speech.stop();
                }
                break;
            }
            Ok(Command::Stop) => {
                schedule.retain(|_| false);
                notify(Event::Scheduled(None));
                pending.clear();
                if let Some(speech) = &mut speech {
                    let _ = speech.stop();
                }
                speaking = None;
                if let Some(output) = &output {
                    output.stop_all();
                }
            }
            Ok(Command::Cancel(scope)) => {
                schedule.retain(|id| id != scope);
                notify(Event::Scheduled(schedule.next_ms(now)));
                pending.retain(|queued| queued.cue.scope != scope);
                if speaking
                    .as_ref()
                    .is_some_and(|current| current.scope == scope)
                {
                    if let Some(speech) = &mut speech {
                        let _ = speech.stop();
                    }
                    speaking = None;
                }
                if let Some(output) = &output {
                    output.stop(Entity::from_bits(scope));
                }
            }
            Ok(Command::Schedule(scope, loaded_at, cues)) => {
                let prepare_speech = speech.is_none()
                    && cues
                        .iter()
                        .any(|cue| cue.settings.mode == Mode::Title && cue.at_ms > loaded_at);
                let keys = cues
                    .iter()
                    .map(|cue| (cue.key.clone(), cue.at_ms))
                    .collect::<Vec<_>>();
                schedule.refresh(scope, loaded_at, cues);
                retain_pending(scope, &keys, &mut pending, &mut speaking, &mut speech);
                notify(Event::Scheduled(schedule.next_ms(now)));
                if prepare_speech && let Err(error) = engine(&mut speech, &completed) {
                    notify(Event::Failure(error));
                }
            }
            Ok(Command::Retain(scope, keys)) => {
                retain_pending(scope, &keys, &mut pending, &mut speaking, &mut speech);
            }
            Ok(Command::Catalog) => {
                let result = engine(&mut speech, &completed)
                    .and_then(|speech| speech.voices().map_err(|error| error.to_string()))
                    .map(|voices| {
                        voices
                            .into_iter()
                            .map(|voice| Voice {
                                id: voice.id().to_string(),
                                name: voice.name().to_string(),
                            })
                            .collect()
                    });
                match result {
                    Ok(voices) => notify(Event::Voices(voices)),
                    Err(error) => notify(Event::Failure(error)),
                }
            }
            Ok(Command::Play(cue)) if cue.settings.mode != Mode::Off => pending.push_back(Queued {
                cue,
                preview: false,
            }),
            Ok(Command::Preview(cue)) if cue.settings.mode != Mode::Off => {
                pending.push_back(Queued { cue, preview: true })
            }
            Ok(Command::Play(_) | Command::Preview(_)) | Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        let ready = schedule.due(chrono::Utc::now().timestamp_millis());
        if !ready.is_empty() {
            pending.extend(ready.into_iter().map(|cue| Queued {
                cue,
                preview: false,
            }));
            notify(Event::Scheduled(schedule.next_ms(now)));
        }
        for id in finished.try_iter() {
            if speaking
                .as_ref()
                .is_some_and(|current| current.id.as_ref() == Some(&id))
            {
                speaking = None;
                notify(Event::Completed);
            }
        }
        if speaking.as_ref().is_some_and(|current| {
            current.id.is_none() && current.started.elapsed().as_millis() >= 250
        }) && !speech
            .as_ref()
            .is_some_and(|speech| speech.is_speaking().unwrap_or(false))
        {
            speaking = None;
            notify(Event::Completed);
        }
        if speaking.is_none()
            && let Some(Queued { cue, preview }) = pending.pop_front()
        {
            if cue.settings.mode == Mode::Blip {
                let result = (|| {
                    if output.is_none() {
                        output = Some(crate::sound::device::Output::open()?);
                    }
                    let output = output.as_ref().unwrap();
                    if let Some(error) = output.error.lock().unwrap().take() {
                        return Err(error);
                    }
                    output.play(
                        Entity::from_bits(cue.scope),
                        clip.clone(),
                        f32::from(cue.settings.volume) / 100.0,
                        true,
                    );
                    Ok(())
                })();
                match result {
                    Ok(()) => notify(Event::Ready(Playback::started(&cue))),
                    Err(error) => {
                        output = None;
                        notify(Event::Failure(error));
                    }
                }
                continue;
            }
            if last_voice != cue.settings.voice {
                if cue.settings.voice.is_none() {
                    speech = None;
                }
                last_voice = cue.settings.voice.clone();
            }
            let result = (|| {
                let speech = engine(&mut speech, &completed)?;
                if speech.supported_features().volume {
                    let volume = speech.min_volume()
                        + (speech.max_volume() - speech.min_volume())
                            * f32::from(cue.settings.volume)
                            / 100.0;
                    speech
                        .set_volume(volume)
                        .map_err(|error| error.to_string())?;
                }
                if let Some(id) = &cue.settings.voice {
                    let voice = speech
                        .voices()
                        .map_err(|error| error.to_string())?
                        .into_iter()
                        .find(|voice| voice.id() == *id)
                        .ok_or("The selected voice is unavailable")?;
                    speech
                        .set_voice(&voice)
                        .map_err(|error| error.to_string())?;
                }
                let title = if cue.projected {
                    format!("Projected. {}", cue.title)
                } else {
                    cue.title.clone()
                };
                let utterance = speech
                    .speak(title, false)
                    .map_err(|error| error.to_string())?;
                Ok(if speech.supported_features().utterance_callbacks {
                    utterance.map(|id| format!("{id:?}"))
                } else {
                    None
                })
            })();
            match result {
                Ok(id) => {
                    let playback = Playback::started(&cue);
                    speaking = Some(Speaking {
                        scope: cue.scope,
                        key: (cue.key, cue.at_ms),
                        id,
                        started: std::time::Instant::now(),
                        preview,
                    });
                    notify(Event::Ready(playback));
                }
                Err(error) => notify(Event::Failure(error)),
            }
        }
    }
}

fn retain_pending(
    scope: u64,
    keys: &[(String, i64)],
    pending: &mut VecDeque<Queued>,
    speaking: &mut Option<Speaking>,
    speech: &mut Option<tts::Tts>,
) {
    pending.retain(|queued| {
        queued.preview
            || queued.cue.scope != scope
            || keys.contains(&(queued.cue.key.clone(), queued.cue.at_ms))
    });
    if speaking.as_ref().is_some_and(|current| {
        !current.preview && current.scope == scope && !keys.contains(&current.key)
    }) {
        if let Some(speech) = speech {
            let _ = speech.stop();
        }
        *speaking = None;
    }
}
