pub(crate) mod device;
pub mod dsp;
pub mod library;
#[cfg(test)]
mod tests;

use bevy::prelude::*;
use dsp::Effects;
use library::{Clip, Library};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

pub(crate) enum Command {
    Refresh,
    Record(Entity, String),
    Finish(Entity),
    Cancel(Entity),
    Play {
        owner: Entity,
        path: String,
        effects: Option<Effects>,
        volume: f32,
    },
    Stop(Entity),
    Apply(Entity, String, Effects),
}

pub(crate) enum Event {
    Library(Vec<String>),
    Status(Entity, String),
    Recording(Entity, bool),
    Saved(Entity, String),
    Failure(String),
    Playback(Entity, Result<(), String>),
    Stopped(Entity),
}

#[derive(Component)]
pub(crate) struct PlaybackResult(pub Result<(), String>);

#[derive(Component)]
pub(crate) struct PlaybackPending;

#[derive(Component)]
pub(crate) struct StopPending;

#[derive(Component)]
pub(crate) struct StoppedPlayback;

#[derive(Resource)]
pub struct Audio {
    sender: mpsc::SyncSender<Command>,
    events: Mutex<mpsc::Receiver<Event>>,
    pub paths: Vec<String>,
    pub error: Option<String>,
    pub revision: u64,
    stopped: Arc<std::sync::atomic::AtomicBool>,
}

impl Audio {
    pub fn open(directory: PathBuf, wake: Option<crate::wake::WakeSignal>) -> Self {
        let (sender, receiver) = mpsc::sync_channel(64);
        let (events, incoming) = mpsc::channel();
        let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ended = stopped.clone();
        std::thread::spawn(move || {
            let notify = |event| {
                let _ = events.send(event);
                if let Some(wake) = &wake {
                    wake.ring();
                }
            };
            match Library::open(&directory) {
                Ok(library) => worker(library, receiver, &notify),
                Err(error) => notify(Event::Failure(error)),
            }
            ended.store(true, std::sync::atomic::Ordering::Release);
        });
        Self {
            sender,
            events: Mutex::new(incoming),
            paths: Vec::new(),
            error: None,
            revision: 0,
            stopped,
        }
    }

    pub(crate) fn send(&self, command: Command) -> Result<(), String> {
        self.sender
            .try_send(command)
            .map_err(|e| format!("Audio is unavailable or busy: {e}"))
    }

    pub(crate) fn shutdown(self) -> Arc<std::sync::atomic::AtomicBool> {
        self.stopped.clone()
    }
}

pub(crate) fn send(world: &World, command: Command) -> Result<(), String> {
    if crate::laboratory::active(world) {
        return Err("Audio is disabled in the Laboratory".into());
    }
    let owner = match &command {
        Command::Record(owner, _)
        | Command::Finish(owner)
        | Command::Cancel(owner)
        | Command::Stop(owner)
        | Command::Apply(owner, _, _)
        | Command::Play { owner, .. } => Some(*owner),
        Command::Refresh => None,
    };
    match owner {
        Some(owner) => send_to(world, owner, command),
        None => world
            .get_resource::<Audio>()
            .ok_or("No Lince recording directory is configured")?
            .send(command),
    }
}

pub(crate) fn audio_for(world: &World, owner: Entity) -> Option<&Audio> {
    if let Some(source) = crate::practice_cells::source(world, owner) {
        return world
            .get_resource::<crate::practice_cells::PracticeCells>()?
            .audio
            .get(&source);
    }
    world.get_resource::<Audio>()
}

pub(crate) fn send_to(world: &World, owner: Entity, command: Command) -> Result<(), String> {
    if crate::laboratory::active(world) {
        return Err("Audio is disabled in the Laboratory".into());
    }
    audio_for(world, owner)
        .ok_or("No Lince recording directory is configured")?
        .send(command)
}

pub struct SoundPlugin;
impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, receive);
    }
}

fn receive(world: &mut World) {
    let mut events: Vec<_> = world
        .get_resource::<Audio>()
        .map(|audio| audio.events.lock().unwrap().try_iter().collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .map(|event| (None, event))
        .collect();
    if let Some(cells) = world.get_resource::<crate::practice_cells::PracticeCells>() {
        for (source, audio) in &cells.audio {
            events.extend(
                audio
                    .events
                    .lock()
                    .unwrap()
                    .try_iter()
                    .map(|event| (Some(source.clone()), event)),
            );
        }
    }
    for (source, event) in events {
        match event {
            Event::Library(paths) => {
                if let Some(source) = source {
                    if let Some(audio) = world
                        .resource_mut::<crate::practice_cells::PracticeCells>()
                        .audio
                        .get_mut(&source)
                    {
                        audio.paths = paths;
                        audio.revision += 1;
                    }
                    continue;
                }
                let mut audio = world.resource_mut::<Audio>();
                audio.paths = paths;
                audio.revision += 1;
            }
            Event::Failure(error) => {
                if let Some(source) = source {
                    if let Some(audio) = world
                        .resource_mut::<crate::practice_cells::PracticeCells>()
                        .audio
                        .get_mut(&source)
                    {
                        audio.error = Some(error);
                        audio.revision += 1;
                    }
                    continue;
                }
                let mut audio = world.resource_mut::<Audio>();
                audio.error = Some(error);
                audio.revision += 1;
            }
            Event::Status(owner, message) => {
                crate::recorder_castle::status(world, owner, &message);
                if world.get::<crate::area::InfluenceArea>(owner).is_some() {
                    world
                        .entity_mut(owner)
                        .insert(crate::sound_area::SoundStatus(message));
                }
            }
            Event::Recording(owner, recording) => {
                crate::recorder_castle::recording(world, owner, recording)
            }
            Event::Saved(owner, path) => crate::recorder_castle::saved(world, owner, path),
            Event::Playback(owner, result) => {
                if world.get_entity(owner).is_ok() {
                    world.entity_mut(owner).insert(PlaybackResult(result));
                }
            }
            Event::Stopped(owner) => {
                if world.get_entity(owner).is_ok() {
                    world.entity_mut(owner).insert(StoppedPlayback);
                }
            }
        }
    }
}

fn worker(library: Library, receiver: mpsc::Receiver<Command>, notify: &impl Fn(Event)) {
    let mut output: Option<device::Output> = None;
    let mut capture: Option<(Entity, String, device::Capture)> = None;
    let mut cache: HashMap<String, Arc<Clip>> = HashMap::new();
    refresh(&library, notify);
    loop {
        let command = if output.is_none() && capture.is_none() {
            match receiver.recv() {
                Ok(command) => Some(command),
                Err(_) => break,
            }
        } else {
            match receiver.recv_timeout(Duration::from_millis(30)) {
                Ok(command) => Some(command),
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => None,
            }
        };
        if let Some((owner, _, input)) = &capture {
            let owner = *owner;
            let error = input.error.lock().unwrap().take();
            if let Some(error) = error {
                capture = None;
                notify(Event::Recording(owner, false));
                notify(Event::Status(owner, format!("Microphone stopped: {error}")));
            } else if input.full() {
                finish(&library, &mut capture, notify);
            }
        }
        if let Some(device) = &output {
            let error = device.error.lock().unwrap().take();
            if let Some(error) = error {
                output = None;
                notify(Event::Failure(format!("Sound output stopped: {error}")));
            }
        }
        let Some(command) = command else { continue };
        let (owner, result) = match command {
            Command::Refresh => {
                cache.clear();
                refresh(&library, notify);
                continue;
            }
            Command::Record(owner, path) => {
                let result = if capture.is_some() {
                    Err("Another Castle is already recording".into())
                } else {
                    device::Capture::start().map(|input| {
                        capture = Some((owner, path, input));
                        notify(Event::Recording(owner, true));
                        notify(Event::Status(
                            owner,
                            "Recording microphone · maximum 120 seconds".into(),
                        ));
                    })
                };
                (owner, result)
            }
            Command::Finish(owner) => {
                if capture.as_ref().is_some_and(|c| c.0 == owner) {
                    finish(&library, &mut capture, notify);
                }
                continue;
            }
            Command::Cancel(owner) => {
                if capture.as_ref().is_some_and(|c| c.0 == owner) {
                    capture = None;
                    notify(Event::Recording(owner, false));
                }
                if let Some(output) = &output {
                    output.stop(owner);
                }
                continue;
            }
            Command::Stop(owner) => {
                notify(Event::Status(owner, "Playback stopped".into()));
                if let Some(output) = &output {
                    output.stop(owner);
                }
                notify(Event::Stopped(owner));
                continue;
            }
            Command::Apply(owner, path, effects) => {
                let result = library.apply(&path, effects).map(|()| {
                    cache.remove(&path);
                    notify(Event::Status(
                        owner,
                        "Effects saved. Sound areas now use this version.".into(),
                    ));
                    refresh(&library, notify);
                });
                (owner, result)
            }
            Command::Play {
                owner,
                path,
                effects,
                volume,
            } => {
                let result = (|| {
                    let clip = if let Some(effects) = effects {
                        let mut clip = library.read(&path, true)?;
                        clip.samples = effects.process(&clip.samples, clip.rate);
                        Arc::new(clip)
                    } else if let Some(clip) = cache.get(&path) {
                        clip.clone()
                    } else {
                        let clip = Arc::new(library.read(&path, false)?);
                        if cache.values().map(|c| c.samples.len()).sum::<usize>()
                            + clip.samples.len()
                            > 24_000_000
                        {
                            cache.clear();
                        }
                        cache.insert(path, clip.clone());
                        clip
                    };
                    if output.is_none() {
                        output = Some(device::Output::open()?);
                    }
                    notify(Event::Status(owner, "Playing sound".into()));
                    output
                        .as_ref()
                        .unwrap()
                        .play(owner, clip, volume, effects.is_some());
                    Ok(())
                })();
                notify(Event::Playback(owner, result.clone()));
                (owner, result)
            }
        };
        if let Err(error) = result {
            notify(Event::Status(owner, error));
        }
    }
}

fn refresh(library: &Library, notify: &impl Fn(Event)) {
    match library.list() {
        Ok(paths) => notify(Event::Library(paths)),
        Err(error) => notify(Event::Failure(error)),
    }
}

fn finish(
    library: &Library,
    capture: &mut Option<(Entity, String, device::Capture)>,
    notify: &impl Fn(Event),
) {
    let Some((owner, path, input)) = capture.take() else {
        return;
    };
    let clip = input.finish();
    notify(Event::Recording(owner, false));
    match library.save_recording(&path, &clip) {
        Ok(()) => {
            notify(Event::Saved(owner, path));
            notify(Event::Status(owner, "Recording saved".into()));
            refresh(library, notify);
        }
        Err(error) => notify(Event::Status(owner, error)),
    }
}
