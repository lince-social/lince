use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::thread;

use cpal::traits::HostTrait;

use flexaudio_core::types::{Error, Result};

static KEEPER_READY: OnceLock<std::result::Result<(), String>> = OnceLock::new();

pub(super) fn ensure() -> Result<()> {
    match KEEPER_READY.get_or_init(start_keeper) {
        Ok(()) => Ok(()),
        Err(message) => Err(Error::Backend(message.clone())),
    }
}

fn start_keeper() -> std::result::Result<(), String> {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let handle = thread::Builder::new()
        .name("flexaudio-cpal-wasapi-keeper".into())
        .spawn(move || {
            let initialized = catch_unwind(AssertUnwindSafe(initialize_enumerator))
                .map_err(panic_message)
                .and_then(|result| result);
            let keep_alive = initialized.is_ok();
            let _ = ready_tx.send(initialized);

            if keep_alive {
                loop {
                    thread::park();
                }
            }
        })
        .map_err(|error| format!("spawn cpal WASAPI keeper thread: {error}"))?;

    drop(handle);

    ready_rx
        .recv()
        .map_err(|_| "cpal WASAPI keeper exited before initialization completed".to_owned())?
}

fn initialize_enumerator() -> std::result::Result<(), String> {
    let host = cpal::default_host();
    let _ = host.default_input_device();
    Ok(())
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        format!("cpal WASAPI keeper initialization panicked: {message}")
    } else if let Some(message) = payload.downcast_ref::<String>() {
        format!("cpal WASAPI keeper initialization panicked: {message}")
    } else {
        "cpal WASAPI keeper initialization panicked with a non-string payload".to_owned()
    }
}
