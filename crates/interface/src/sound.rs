mod device;
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
}

#[derive(Resource)]
pub struct Audio {
    sender: mpsc::SyncSender<Command>,
    events: Mutex<mpsc::Receiver<Event>>,
    pub paths: Vec<String>,
    pub error: Option<String>,
    pub revision: u64,
}

impl Audio {
    pub fn open(directory: PathBuf, wake: Option<crate::wake::WakeSignal>) -> Self {
        let (sender, receiver) = mpsc::sync_channel(64);
        let (events, incoming) = mpsc::channel();
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
        });
        Self {
            sender,
            events: Mutex::new(incoming),
            paths: Vec::new(),
            error: None,
            revision: 0,
        }
    }

    pub(crate) fn send(&self, command: Command) -> Result<(), String> {
        self.sender
            .try_send(command)
            .map_err(|e| format!("Audio is unavailable or busy: {e}"))
    }
}

pub(crate) fn send(world: &World, command: Command) -> Result<(), String> {
    if crate::laboratory::active(world) {
        return Err("Audio is disabled in the Laboratory".into());
    }
    world
        .get_resource::<Audio>()
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
    let events: Vec<_> = world
        .get_resource::<Audio>()
        .map(|audio| audio.events.lock().unwrap().try_iter().collect())
        .unwrap_or_default();
    for event in events {
        match event {
            Event::Library(paths) => {
                let mut audio = world.resource_mut::<Audio>();
                audio.paths = paths;
                audio.revision += 1;
            }
            Event::Failure(error) => {
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
        }
    }
}

fn worker(library: Library, receiver: mpsc::Receiver<Command>, notify: &impl Fn(Event)) {
    let mut output: Option<device::Output> = None;
    let mut capture: Option<(Entity, String, device::Capture)> = None;
    let mut cache: HashMap<String, Arc<Clip>> = HashMap::new();
    refresh(&library, notify);
    loop {
        let command = match receiver.recv_timeout(Duration::from_millis(30)) {
            Ok(command) => Some(command),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => None,
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
