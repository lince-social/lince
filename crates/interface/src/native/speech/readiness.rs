use super::*;
use crate::native::sound::{device::Capture, library::Clip};
use cell::fiote_communication::speech::input::Startup;

pub(crate) fn prepare(
    device: &Capture,
    mut cancelled: impl FnMut() -> bool,
) -> Result<Clip, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut startup = Startup::default();
    loop {
        if cancelled() {
            return Err("Microphone startup cancelled. Nothing was transcribed or sent.".into());
        }
        if let Some(error) = device.error.lock().unwrap().clone() {
            return Err(error);
        }
        if Instant::now() >= deadline {
            return Err("Microphone input did not settle within five seconds. Check its input level or select another microphone; nothing was transcribed or sent.".into());
        }
        let clip = device.drain();
        if let Some(samples) = startup.push(clip.rate, &clip.samples)? {
            if cancelled() {
                return Err(
                    "Microphone startup cancelled. Nothing was transcribed or sent.".into(),
                );
            }
            return Ok(Clip {
                rate: clip.rate,
                samples,
            });
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
