use super::*;

#[derive(Component)]
pub(super) struct Checking {
    stop: Arc<AtomicBool>,
    result: Mutex<mpsc::Receiver<Result<String, String>>>,
}

impl Drop for Checking {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

#[derive(Clone)]
pub(super) struct Check;

impl Action for Check {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(listening) = world.get::<Listening>(owner) else {
            return;
        };
        let label = listening.label;
        let active = listening.wanted || listening.saved.enabled || listening.pending.is_some();
        let microphone = listening.microphone.clone();
        let result = (|| {
            if super::wake_setup::busy(world, owner) {
                return Err("Finish or cancel wake setup before checking the microphone.".into());
            }
            if crate::native::sound::speaking(world) {
                return Err("Stop speaking before checking the microphone.".into());
            }
            if crate::native::speech::call_microphone_active(world) {
                return Err("Stop the call microphone before checking assistant input.".into());
            }
            if active {
                return Err("Stop listening before checking the microphone.".to_string());
            }
            if world.get::<Checking>(owner).is_some() {
                return Err("The three-second microphone check is already running.".into());
            }
            if !view_active(world, owner) {
                return Err("Open this assistant in the active workspace first.".into());
            }
            let wake = world
                .get_resource::<crate::native::wake::WakeSignal>()
                .cloned();
            let stop = Arc::new(AtomicBool::new(false));
            let control = stop.clone();
            let (send, receive) = mpsc::sync_channel(1);
            std::thread::spawn(move || {
                let result = inspect(microphone.as_deref(), &control);
                let _ = send.send(result);
                if let Some(wake) = wake {
                    wake.ring();
                }
            });
            world.entity_mut(owner).insert(Checking {
                stop,
                result: Mutex::new(receive),
            });
            Ok(())
        })();
        if let Some(mut text) = world.get_mut::<Text>(label) {
            text.0 = result.err().unwrap_or_else(|| {
                "Preparing microphone (up to five seconds), then checking three seconds. Speak normally; nothing is saved or sent. Stop listening cancels this check.".into()
            });
        }
    }
}

fn inspect(microphone: Option<&str>, stop: &AtomicBool) -> Result<String, String> {
    let device = crate::native::sound::device::Capture::start_when_available(microphone, stop)?;
    crate::native::speech::readiness::prepare(&device, || stop.load(Ordering::Acquire))?;
    let end = Instant::now() + Duration::from_secs(3);
    let mut quality = crate::native::speech::CaptureQuality::default();
    let mut count = 0usize;
    let mut square = 0.0f64;
    let mut peak = 0.0f32;
    while Instant::now() < end {
        if stop.load(Ordering::Acquire) {
            return Err("Microphone check cancelled.".into());
        }
        if let Some(error) = device.error.lock().unwrap().clone() {
            return Err(error);
        }
        let clip = device.drain();
        quality.push(&clip)?;
        for sample in clip.samples {
            count += 1;
            square += f64::from(sample).powi(2);
            peak = peak.max(sample.abs());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    if count == 0 || peak < 0.001 {
        return Err("No usable microphone sound. Select another input or check its routing, then retry. Nothing was sent.".into());
    }
    Ok(format!(
        "Microphone check complete · peak {:.0}% · RMS {:.1}%. Input passed the distortion check. This does not establish speech accuracy; enable listening when ready.",
        peak * 100.0,
        (square / count as f64).sqrt() * 100.0,
    ))
}

pub(super) fn cancel(world: &mut World, owner: Entity) {
    world.entity_mut(owner).remove::<Checking>();
}

pub(super) fn update(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, (With<Checking>, Allow<Disabled>)>()
        .iter(world)
        .collect();
    for owner in owners {
        if crate::native::speech::call_microphone_active(world) {
            cancel(world, owner);
            if let Some(label) = world
                .get::<Listening>(owner)
                .map(|listening| listening.label)
                && let Some(mut text) = world.get_mut::<Text>(label)
            {
                text.0 =
                    "Input check cancelled because a call started using the microphone.".into();
            }
            continue;
        }
        if !view_active(world, owner) {
            cancel(world, owner);
            continue;
        }
        let result = world.get::<Checking>(owner).and_then(|check| {
            match check.result.lock().ok()?.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                    "Microphone check stopped unexpectedly. Retry when ready.".into(),
                )),
            }
        });
        if let Some(result) = result {
            cancel(world, owner);
            if let Some(label) = world
                .get::<Listening>(owner)
                .map(|listening| listening.label)
                && let Some(mut text) = world.get_mut::<Text>(label)
            {
                text.0 = result.unwrap_or_else(|error| error);
            }
        } else if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
            wake.after(Duration::from_millis(100));
        }
    }
}
