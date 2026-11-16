use bevy::prelude::*;
use std::{sync::mpsc, thread, time::Duration};

#[derive(Resource, Default)]
pub(super) struct Pacing {
    timer: Option<Timer>,
    active: bool,
}

struct Timer {
    sender: Option<mpsc::Sender<bool>>,
    task: Option<thread::JoinHandle<()>>,
}

impl Pacing {
    pub fn set(&mut self, active: bool, wake: Option<crate::native::wake::WakeSignal>) {
        if active
            && self.timer.is_none()
            && let Some(wake) = wake
        {
            let (sender, receiver) = mpsc::channel();
            let task = thread::Builder::new()
                .name("kamalie-clock".into())
                .spawn(move || {
                    let mut active = false;
                    loop {
                        let event = if active {
                            receiver.recv_timeout(Duration::from_millis(16))
                        } else {
                            receiver
                                .recv()
                                .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                        };
                        match event {
                            Ok(value) => active = value,
                            Err(mpsc::RecvTimeoutError::Timeout) => wake.ring(),
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                    }
                });
            match task {
                Ok(task) => {
                    self.timer = Some(Timer {
                        sender: Some(sender),
                        task: Some(task),
                    })
                }
                Err(error) => {
                    warn!("Kamalie clock unavailable: {error}");
                    return;
                }
            }
            self.active = false;
        }
        if self.active != active {
            if let Some(timer) = &self.timer
                && let Some(sender) = &timer.sender
            {
                let _ = sender.send(active);
            }
            self.active = active;
        }
        if !active {
            self.timer = None;
        }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}
