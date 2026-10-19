use super::{status, sys};
use crate::client::CapturedFrame;
use ailuros_protocol::Bytes;
use anyhow::{Context, ensure};
use esp_idf_svc::sys::ailuros;
use std::{
    ffi::CString,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{mpsc, oneshot};

enum Command {
    Active(bool, oneshot::Sender<anyhow::Result<()>>),
    Capture(
        usize,
        oneshot::Sender<anyhow::Result<Option<CapturedFrame>>>,
    ),
}

pub struct Camera {
    commands: mpsc::Sender<Command>,
}

struct Driver {
    workflow: *mut ailuros::cJSON,
    active: bool,
    latency: crate::power::Latency<Wifi>,
}

struct Wifi;

impl crate::power::Power for Wifi {
    type Mode = sys::wifi_ps_type_t;
    const LOW_LATENCY: Self::Mode = sys::wifi_ps_type_t_WIFI_PS_NONE;

    fn current(&mut self) -> anyhow::Result<Self::Mode> {
        let mut mode = Self::LOW_LATENCY;
        sys::esp!(unsafe { sys::esp_wifi_get_ps(&mut mode) })?;
        Ok(mode)
    }

    fn set(&mut self, mode: Self::Mode) -> anyhow::Result<()> {
        sys::esp!(unsafe { sys::esp_wifi_set_ps(mode) })?;
        Ok(())
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        if self.active {
            unsafe { ailuros::lince_camera_close() };
        }
        unsafe { ailuros::cJSON_Delete(self.workflow) };
    }
}

struct Frame(ailuros::lince_frame_t);

impl Drop for Frame {
    fn drop(&mut self) {
        unsafe { ailuros::lince_camera_release(&self.0) };
    }
}

impl Driver {
    fn set_active(&mut self, active: bool) -> anyhow::Result<()> {
        if active && !self.active {
            self.latency.enable()?;
            if let Err(error) = sys::esp!(unsafe { ailuros::lince_camera_open(self.workflow) }) {
                self.latency
                    .disable()
                    .context("Restore Wi-Fi power after camera startup failure")?;
                return Err(error.into());
            }
            self.active = true;
            super::resources("camera-open");
        } else if !active {
            let was_active = self.active;
            if self.active {
                unsafe { ailuros::lince_camera_close() };
                self.active = false;
            }
            self.latency.disable()?;
            if was_active {
                super::resources("camera-closed");
            }
        }
        status(if active {
            c"streaming"
        } else {
            c"awaiting-viewers"
        });
        Ok(())
    }

    fn capture(&mut self, maximum: usize) -> anyhow::Result<Option<CapturedFrame>> {
        ensure!(self.active, "Camera capture requires viewer demand");
        let mut raw = unsafe { std::mem::zeroed() };
        if !unsafe { ailuros::lince_camera_capture(&mut raw) } {
            anyhow::bail!("Camera capture failed");
        }
        let frame = Frame(raw);
        let age = unsafe { sys::esp_timer_get_time() }
            .checked_sub(frame.0.captured_us)
            .context("Invalid camera timestamp")?;
        if !frame.0.jpeg
            || frame.0.bytes.is_null()
            || frame.0.length == 0
            || frame.0.length > maximum
            || !(0..=5_000_000).contains(&age)
        {
            return Ok(None);
        }
        let wall = SystemTime::now().duration_since(UNIX_EPOCH)?;
        let age = Duration::from_micros(u64::try_from(age)?);
        let captured_at = i64::try_from(
            wall.checked_sub(age)
                .context("Invalid camera capture time")?
                .as_millis(),
        )?;
        let bytes = Bytes::copy_from_slice(unsafe {
            std::slice::from_raw_parts(frame.0.bytes, frame.0.length)
        });
        Ok(Some(CapturedFrame {
            bytes,
            captured_at,
            age,
        }))
    }
}

impl Camera {
    pub(super) fn new(workflow: Vec<u8>) -> anyhow::Result<Self> {
        let (commands, mut receiver) = mpsc::channel(1);
        let workflow = CString::new(workflow)?;
        std::thread::Builder::new()
            .name("ailuros-camera".into())
            .stack_size(8192)
            .spawn(move || {
                let workflow = unsafe { ailuros::cJSON_Parse(workflow.as_ptr()) };
                if workflow.is_null() {
                    return;
                }
                let mut driver = Driver {
                    workflow,
                    active: false,
                    latency: crate::power::Latency::new(Wifi),
                };
                while let Some(command) = receiver.blocking_recv() {
                    match command {
                        Command::Active(active, reply) => {
                            let _ = reply.send(driver.set_active(active));
                        }
                        Command::Capture(maximum, reply) => {
                            if !reply.is_closed() {
                                let _ = reply.send(driver.capture(maximum));
                            }
                        }
                    }
                }
            })?;
        Ok(Self { commands })
    }
}

impl crate::client::Camera for Camera {
    fn report(&mut self, statistics: crate::client::Statistics) {
        super::report(serde_json::json!({"type":"ailuros-camera-statistics",
            "monotonic_ms":unsafe { sys::esp_timer_get_time() } / 1000,
            "statistics":statistics}));
    }

    async fn set_active(&mut self, active: bool) -> anyhow::Result<()> {
        let (send, receive) = oneshot::channel();
        self.commands
            .send(Command::Active(active, send))
            .await
            .context("Camera worker stopped")?;
        receive.await.context("Camera worker stopped")?
    }

    async fn capture(&mut self, maximum: usize) -> anyhow::Result<Option<CapturedFrame>> {
        let (send, receive) = oneshot::channel();
        self.commands
            .send(Command::Capture(maximum, send))
            .await
            .context("Camera worker stopped")?;
        receive.await.context("Camera worker stopped")?
    }
}
