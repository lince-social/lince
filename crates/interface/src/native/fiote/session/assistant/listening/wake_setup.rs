use super::*;
use cell::fiote_communication::speech::wake::{self, Profile};

const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "Google speech feature backbone and openWakeWord mel frontend",
        author: "Google Research; David Scripka and contributors",
        license: wake::MODEL_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "Wake feature model sources and Lince changes",
        author: "Google Research; openWakeWord; Lince contributors",
        license: wake::MODEL_NOTICE,
    },
    crate::native::credits::Attribution {
        name: "tract Rust ONNX inference",
        author: "Sonos and contributors",
        license: wake::TRACT_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "Rustpotter reference comparison",
        author: "Miguel Álvarez Díez and contributors; Lince adaptations",
        license: wake::LICENSE,
    },
    crate::native::credits::Attribution {
        name: "Rustpotter provenance and changes",
        author: "Rustpotter and Lince contributors",
        license: wake::NOTICE,
    },
    crate::native::credits::Attribution {
        name: "RustFFT",
        author: "RustFFT contributors",
        license: wake::FFT_LICENSE,
    },
];

#[derive(Component)]
struct Setup {
    label: Entity,
    import: Entity,
    samples: Vec<Vec<f32>>,
    worker: Option<Worker>,
    pending: Option<String>,
    profile: Option<Profile>,
    saving: bool,
}

struct Worker {
    stop: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
    announced: bool,
    result: Mutex<mpsc::Receiver<Result<Product, String>>>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

enum Product {
    Recording(Vec<f32>),
    Profile(Profile),
}

pub(super) fn attach(world: &mut World, owner: Entity) {
    let panel = ui::column(world, owner);
    world
        .entity_mut(panel)
        .insert(crate::native::sand_store::SandCredits(CREDITS));
    crate::native::edit_mode::label(world, panel, "Local Hey Fiote setup", 16.0);
    crate::native::edit_mode::label(
        world,
        panel,
        "Record Hey Fiote three times, one example per click, or import a compatible Lince wake reference. Raw recordings stay local and are discarded after saving. Voice-specific examples may not recognize everyone; this is not speaker verification. Installing or saving a reference does not start listening.",
        13.0,
    );
    let label = crate::native::edit_mode::label(world, panel, "Wake reference: not checked", 13.0);
    let import = ui::field(world, panel, "Lince wake reference JSON path", "");
    let row = ui::row(world, panel);
    for (caption, command) in [
        ("Record wake example (4 seconds)", Command::Record),
        ("Save wake examples", Command::Save),
        ("Import wake reference", Command::Import),
        ("Check wake reference", Command::Inspect),
        ("Cancel wake setup", Command::Cancel),
        ("Clear wake reference", Command::Clear),
    ] {
        ui::button(world, row, owner, caption, command);
    }
    ui::credits(world, row, panel, CREDITS);
    world.entity_mut(owner).insert(Setup {
        label,
        import,
        samples: Vec::new(),
        worker: None,
        pending: None,
        profile: None,
        saving: false,
    });
}

pub(super) fn busy(world: &World, owner: Entity) -> bool {
    world
        .get::<Setup>(owner)
        .is_some_and(|setup| setup.worker.is_some() || setup.pending.is_some())
}

pub(super) fn cancel(world: &mut World, owner: Entity) {
    if let Some(mut setup) = world.get_mut::<Setup>(owner) {
        setup.worker = None;
        setup.samples.clear();
    }
}

fn send(world: &mut World, owner: Entity, request: Request) -> Result<(), String> {
    let saving = matches!(request, Request::ConfigureWake { .. });
    let id = nucleus::new_uid("wake-setup");
    ui::send(
        world,
        owner,
        cell::ClientMessage::Speech {
            id: id.clone(),
            request,
        },
    )?;
    let mut setup = world.get_mut::<Setup>(owner).ok_or("Wake setup closed.")?;
    setup.pending = Some(id);
    setup.saving = saving;
    Ok(())
}

#[derive(Clone)]
enum Command {
    Record,
    Save,
    Import,
    Inspect,
    Cancel,
    Clear,
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(setup) = world.get::<Setup>(owner) else {
            return;
        };
        let label = setup.label;
        let field = setup.import;
        let count = setup.samples.len();
        let result = (|| {
            if matches!(self, Self::Cancel) {
                cancel(world, owner);
                world.get_mut::<Setup>(owner).unwrap().samples.clear();
                return Ok(
                    "Wake recording/setup cancelled. Already saved references remain available."
                        .to_string(),
                );
            }
            if busy(world, owner) {
                return Err("Wait for wake setup, or cancel its recording.".into());
            }
            if matches!(self, Self::Inspect) {
                send(world, owner, Request::Inspect)?;
                return Ok("Checking the saved local wake reference.".into());
            }
            if !quiet(world)
                || world.get::<Resume>(owner).is_some()
                || crate::native::sound::speaking(world)
            {
                return Err("Stop listening and speaking before changing wake setup.".into());
            }
            match self {
                Self::Clear => {
                    world.get_mut::<Setup>(owner).unwrap().samples.clear();
                    send(world, owner, Request::ConfigureWake { profile: None })?;
                    Ok("Removing the saved wake reference. Listening stays off.".into())
                }
                Self::Record => {
                    if crate::native::speech::call_microphone_active(world) {
                        return Err(
                            "Stop the call microphone before recording wake examples.".into()
                        );
                    }
                    if world.get::<super::input_check::Checking>(owner).is_some() {
                        return Err(
                            "Finish the microphone check before recording wake examples.".into(),
                        );
                    }
                    if count >= 8 {
                        return Err(
                            "Eight examples are ready. Save them or cancel and record again."
                                .into(),
                        );
                    }
                    if !view_active(world, owner) {
                        return Err("Open this assistant in the active workspace first.".into());
                    }
                    let microphone = world
                        .get::<Listening>(owner)
                        .and_then(|listening| listening.microphone.clone());
                    start_worker(world, owner, move |stop, ready| {
                        record(microphone.as_deref(), stop, ready).map(Product::Recording)
                    });
                    Ok(format!(
                        "Preparing microphone for example {}. Wait for the ready message, then say Hey Fiote once. Nothing is sent.",
                        count + 1
                    ))
                }
                Self::Save => {
                    if count < 3 {
                        return Err(format!(
                            "Record {} more wake examples before saving.",
                            3 - count
                        ));
                    }
                    let samples = world.get::<Setup>(owner).unwrap().samples.clone();
                    start_worker(world, owner, move |stop, _| {
                        if stop.load(Ordering::Acquire) {
                            return Err("Wake setup cancelled.".into());
                        }
                        Profile::enroll(&samples).map(Product::Profile)
                    });
                    Ok("Preparing local acoustic references. Listening stays off.".into())
                }
                Self::Import => {
                    let path = world
                        .get::<EditableText>(field)
                        .ok_or("Wake reference field closed.")?
                        .value()
                        .to_string();
                    let path = path.trim().to_string();
                    if path.is_empty() {
                        return Err("Choose a compatible wake reference JSON file.".into());
                    }
                    start_worker(world, owner, move |stop, _| {
                        use std::io::Read;
                        if stop.load(Ordering::Acquire) {
                            return Err("Wake setup cancelled.".into());
                        }
                        let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
                        if !file
                            .metadata()
                            .map_err(|error| error.to_string())?
                            .is_file()
                        {
                            return Err("Choose a regular wake reference file.".into());
                        }
                        let mut bytes = Vec::new();
                        file.take(524_289)
                            .read_to_end(&mut bytes)
                            .map_err(|error| error.to_string())?;
                        Profile::from_bytes(&bytes).map(Product::Profile)
                    });
                    Ok(
                        "Validating the local reference file. No model or microphone starts."
                            .into(),
                    )
                }
                Self::Inspect | Self::Cancel => unreachable!(),
            }
        })();
        if let Some(mut text) = world.get_mut::<Text>(label) {
            text.0 = result.unwrap_or_else(|error| error);
        }
    }
}

fn start_worker(
    world: &mut World,
    owner: Entity,
    work: impl FnOnce(&AtomicBool, &AtomicBool) -> Result<Product, String> + Send + 'static,
) {
    let (send, receive) = mpsc::sync_channel(1);
    let stop = Arc::new(AtomicBool::new(false));
    let control = stop.clone();
    let ready = Arc::new(AtomicBool::new(false));
    let ready_signal = ready.clone();
    let wake = world
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned();
    std::thread::spawn(move || {
        let result = work(&control, &ready_signal);
        if !control.load(Ordering::Acquire) {
            let _ = send.send(result);
        }
        if let Some(wake) = wake {
            wake.ring();
        }
    });
    world.get_mut::<Setup>(owner).unwrap().worker = Some(Worker {
        stop,
        ready,
        announced: false,
        result: Mutex::new(receive),
    });
}

fn record(
    microphone: Option<&str>,
    stop: &AtomicBool,
    ready: &AtomicBool,
) -> Result<Vec<f32>, String> {
    let device = crate::native::sound::device::Capture::start_when_available(microphone, stop)?;
    crate::native::speech::readiness::prepare(&device, || stop.load(Ordering::Acquire))?;
    ready.store(true, Ordering::Release);
    let end = Instant::now() + Duration::from_secs(4);
    let mut samples = Vec::new();
    let mut resampler = None;
    let mut quality = crate::native::speech::CaptureQuality::default();
    while Instant::now() < end {
        if stop.load(Ordering::Acquire) {
            return Err("Wake recording cancelled.".into());
        }
        if let Some(error) = device.error.lock().unwrap().clone() {
            return Err(error);
        }
        let clip = device.drain();
        quality.push(&clip)?;
        if resampler.is_none() {
            resampler = Some(crate::native::speech::Resampler::new(clip.rate)?);
        }
        samples.extend(resampler.as_mut().unwrap().push(&clip)?);
        std::thread::sleep(Duration::from_millis(25));
    }
    samples.truncate(64_000);
    if samples.len() < 8000 || !samples.iter().any(|sample| sample.abs() > 0.005) {
        return Err(
            "No clear wake phrase was recorded. Check the selected microphone and try again."
                .into(),
        );
    }
    Ok(samples)
}

pub(super) fn update(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, (With<Setup>, Allow<Disabled>)>()
        .iter(world)
        .collect();
    for owner in owners {
        if world
            .get::<Setup>(owner)
            .is_some_and(|setup| setup.pending.is_some())
            && crate::native::cell_bridge::SpeechCancellation::for_owner(world, owner).is_none()
        {
            cancel(world, owner);
            let mut setup = world.get_mut::<Setup>(owner).unwrap();
            setup.pending = None;
            setup.saving = false;
            let label = setup.label;
            world.get_mut::<Text>(label).unwrap().0 = "The wake setup connection closed. Reconnect this Cell and check its saved reference.".into();
        }
        if crate::native::speech::call_microphone_active(world)
            && world
                .get::<Setup>(owner)
                .is_some_and(|setup| setup.worker.is_some())
        {
            cancel(world, owner);
            let label = world.get::<Setup>(owner).unwrap().label;
            if let Some(mut text) = world.get_mut::<Text>(label) {
                text.0 =
                    "Wake recording cancelled because a call started using the microphone.".into();
            }
        }
        if !view_active(world, owner) {
            cancel(world, owner);
        }
        let announcing = world.get_mut::<Setup>(owner).and_then(|mut setup| {
            let label = setup.label;
            let worker = setup.worker.as_mut()?;
            if worker.announced || !worker.ready.load(Ordering::Acquire) {
                return None;
            }
            worker.announced = true;
            Some(label)
        });
        if let Some(label) = announcing {
            world.get_mut::<Text>(label).unwrap().0 =
                "Microphone ready. Say Hey Fiote once now, then pause; recording for four seconds."
                    .into();
        }
        let result = world
            .get::<Setup>(owner)
            .and_then(|setup| setup.worker.as_ref())
            .and_then(|worker| match worker.result.lock().ok()?.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                    "Wake setup stopped unexpectedly. Retry when ready.".into(),
                )),
            });
        if let Some(result) = result {
            world.get_mut::<Setup>(owner).unwrap().worker = None;
            let label = world.get::<Setup>(owner).unwrap().label;
            let detail = match result {
                Err(error) => error,
                Ok(Product::Recording(samples)) => {
                    let mut setup = world.get_mut::<Setup>(owner).unwrap();
                    setup.samples.push(samples);
                    format!(
                        "{} wake examples recorded locally. Save after at least three, or record more (maximum eight).",
                        setup.samples.len()
                    )
                }
                Ok(Product::Profile(profile)) => match send(
                    world,
                    owner,
                    Request::ConfigureWake {
                        profile: Some(profile),
                    },
                ) {
                    Ok(()) => {
                        "Saving the validated acoustic reference. Listening stays off.".into()
                    }
                    Err(error) => error,
                },
            };
            if let Some(mut text) = world.get_mut::<Text>(label) {
                text.0 = detail;
            }
        }
        if busy(world, owner)
            && let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>()
        {
            wake.after(Duration::from_millis(100));
        }
    }
}

pub(super) fn response(world: &mut World, id: &str, status: Result<&cell::speech::Status, &str>) {
    let owners: Vec<_> = world
        .query_filtered::<(Entity, &Setup), Allow<Disabled>>()
        .iter(world)
        .filter(|(_, setup)| {
            setup.pending.as_deref() == Some(id) || id == crate::native::cell_bridge::CONNECTION
        })
        .map(|(owner, _)| owner)
        .collect();
    for owner in owners {
        let mut setup = world.get_mut::<Setup>(owner).unwrap();
        setup.pending = None;
        let saving = setup.saving;
        setup.saving = false;
        let label = setup.label;
        let detail = match status {
            Err(error) => error.to_string(),
            Ok(status) => {
                if let Some(error) = &status.wake_error {
                    setup.profile = None;
                    if let Some(mut text) = world.get_mut::<Text>(label) {
                        text.0 = error.clone();
                    }
                    continue;
                }
                setup.profile = status.wake_profile.clone();
                if let Some(profile) = &setup.profile {
                    let count = profile.references.len();
                    if saving {
                        setup.samples.clear();
                    }
                    format!(
                        "Local wake reference ready · {count} examples. Start listening when ready."
                    )
                } else {
                    "No saved wake reference. Record three examples or import a compatible profile."
                        .into()
                }
            }
        };
        if let Some(mut text) = world.get_mut::<Text>(label) {
            text.0 = detail;
        }
    }
}

#[cfg(test)]
mod tests;
