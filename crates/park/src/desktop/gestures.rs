use super::Desktop;
use crate::runtime::{Completed, Input};
use lince_interface::host::{
    OutputViewport,
    input::{GestureContext, GestureDelivery, GestureDeliveryBuffer, GestureRecognizer},
};
use std::{io, sync::mpsc::TrySendError};

struct Capture {
    context: GestureContext,
    viewport: OutputViewport,
    device: String,
}

#[derive(Default)]
pub(super) struct State {
    available: bool,
    next: u64,
    claimed: bool,
    recognizer: GestureRecognizer,
    buffer: GestureDeliveryBuffer,
    capture: Option<Capture>,
}

impl Desktop {
    pub(super) fn pointer_gesture_active(&self) -> bool {
        self.gesture_devices.iter().any(Option::is_some) || self.gestures.buffer.busy()
    }

    pub(crate) fn gesture_swipe_begin(&mut self, device: &str, fingers: u32, time: u32) {
        if self.gesture_devices[0].is_some() {
            if self.gestures.claimed {
                self.gestures.recognizer.cancel();
                self.gestures.buffer.cancel();
            } else {
                self.server.gesture_swipe_end(true, time);
            }
        }
        self.gesture_devices[0] = Some(device.into());
        self.gestures.claimed = false;
        if let Some((capture, settings)) = self.gesture_capture(device, fingers) {
            self.gestures.claimed = true;
            let Some(sequence) = self.gestures.next.checked_add(1) else {
                return;
            };
            self.gestures.next = sequence;
            let capture = Capture {
                context: GestureContext {
                    sequence,
                    ..capture.context
                },
                ..capture
            };
            if !self.gestures.buffer.begin(capture.context) {
                let _ = self.worker.input(Input::Feedback(
                    "The previous desktop gesture is still completing. Try the gesture again."
                        .into(),
                ));
                return;
            }
            if self
                .gestures
                .recognizer
                .begin(sequence, capture.context.output, fingers, settings)
                != Ok(true)
            {
                self.gestures.buffer.cancel();
                return;
            }
            self.gestures.capture = Some(capture);
            return;
        }
        self.server.gesture_swipe_begin(fingers, time);
    }

    pub(crate) fn gesture_swipe_update(&mut self, device: &str, delta: [f64; 2], time: u32) {
        if self.gesture_devices[0].as_deref() == Some(device) {
            if self.gestures.claimed {
                if let Some((sequence, _)) = self.gestures.recognizer.active() {
                    match self.gestures.recognizer.update(sequence, delta) {
                        Ok(Some(progress)) => {
                            self.gestures.buffer.progress(progress);
                        }
                        Ok(None) => {}
                        Err(_) => {
                            self.gestures.recognizer.cancel();
                            self.gestures.buffer.cancel();
                        }
                    }
                }
            } else {
                self.server.gesture_swipe_update(delta, time);
            }
        }
    }

    pub(crate) fn gesture_swipe_end(&mut self, device: &str, cancelled: bool, time: u32) {
        if self.gesture_devices[0].as_deref() == Some(device) {
            self.gesture_devices[0] = None;
            if self.gestures.claimed {
                if let Some((sequence, _)) = self.gestures.recognizer.active()
                    && let Some(completion) = self.gestures.recognizer.end(sequence, cancelled)
                {
                    self.gestures.buffer.finish(completion);
                }
                self.gestures.claimed = false;
            } else {
                self.server.gesture_swipe_end(cancelled, time);
            }
        }
    }

    pub(crate) fn gesture_pinch_begin(&mut self, device: &str, fingers: u32, time: u32) {
        self.gestures.recognizer.cancel();
        self.gestures.buffer.cancel();
        if self.gesture_devices[1].is_some() {
            self.server.gesture_pinch_end(true, time);
        }
        self.gesture_devices[1] = Some(device.into());
        self.server.gesture_pinch_begin(fingers, time);
    }

    pub(crate) fn gesture_pinch_update(
        &mut self,
        device: &str,
        delta: [f64; 2],
        scale: f64,
        rotation: f64,
        time: u32,
    ) {
        if self.gesture_devices[1].as_deref() == Some(device) {
            self.server
                .gesture_pinch_update(delta, scale, rotation, time);
        }
    }

    pub(crate) fn gesture_pinch_end(&mut self, device: &str, cancelled: bool, time: u32) {
        if self.gesture_devices[1].as_deref() == Some(device) {
            self.gesture_devices[1] = None;
            self.server.gesture_pinch_end(cancelled, time);
        }
    }

    pub(crate) fn gesture_hold_begin(&mut self, device: &str, fingers: u32, time: u32) {
        self.gestures.recognizer.cancel();
        self.gestures.buffer.cancel();
        if self.gesture_devices[2].is_some() {
            self.server.gesture_hold_end(true, time);
        }
        self.gesture_devices[2] = Some(device.into());
        self.server.gesture_hold_begin(fingers, time);
    }

    pub(crate) fn gesture_hold_end(&mut self, device: &str, cancelled: bool, time: u32) {
        if self.gesture_devices[2].as_deref() == Some(device) {
            self.gesture_devices[2] = None;
            self.server.gesture_hold_end(cancelled, time);
        }
    }

    pub(crate) fn cancel_device_gestures(&mut self, device: &str, time: u32) {
        if self
            .gestures
            .capture
            .as_ref()
            .is_some_and(|capture| capture.device == device)
        {
            self.gestures.recognizer.cancel();
            self.gestures.buffer.cancel();
        }
        self.gesture_swipe_end(device, true, time);
        self.gesture_pinch_end(device, true, time);
        self.gesture_hold_end(device, true, time);
    }

    pub(crate) fn cancel_client_gestures(&mut self, time: u32) {
        self.gestures.recognizer.cancel();
        self.gestures.buffer.cancel();
        self.gestures.claimed = false;
        self.server.gesture_swipe_end(true, time);
        self.server.gesture_pinch_end(true, time);
        self.server.gesture_hold_end(true, time);
        self.gesture_devices.fill(None);
    }

    pub(crate) fn cancel_desktop_gesture(&mut self) -> io::Result<()> {
        self.gestures.recognizer.cancel();
        self.gestures.buffer.cancel();
        self.flush_gesture()
    }

    pub(crate) fn gesture_support(&mut self, available: bool) -> io::Result<()> {
        if self.gestures.available != available {
            if !available {
                self.cancel_desktop_gesture()?;
            }
            self.send(Input::GestureSupport(available))?;
            self.gestures.available = available;
        }
        Ok(())
    }

    fn gesture_capture(
        &self,
        device: &str,
        fingers: u32,
    ) -> Option<(Capture, lince_interface::host::input::GestureSettings)> {
        if !self.gestures.available
            || !matches!(fingers, 3 | 4)
            || !self.focused
            || self.server.locked()
            || self.server.shortcuts_inhibited()
            || self.server.pointer_grabbed()
            || self.server.popup_grab_active()
            || self.server.dragging()
            || self.server.input_method_grabbed()
            || self.grabbed.is_some()
            || !self.buttons.is_empty()
            || self.manipulation.owns()
            || self.pending_output.is_some() && !self.gestures.buffer.busy()
            || self.layer_focus.is_some()
        {
            return None;
        }
        let frame = self.frame_at(self.pointer)?;
        let input = &frame.frame.input;
        if !input.gestures.enabled
            || input.gestures.validate().is_err()
            || input.keyboard_capture
            || (input.modal || input.keyboard_modal) && !(fingers == 4 && input.overview)
        {
            return None;
        }
        let viewport = frame
            .frame
            .geometry
            .as_ref()?
            .outputs
            .iter()
            .find(|output| output.bounds.contains(self.pointer))?
            .clone();
        Some((
            Capture {
                context: GestureContext {
                    sequence: 0,
                    owner: input.workspaces.owner,
                    workspace: viewport.workspace,
                    output: viewport.id,
                    fingers,
                },
                viewport,
                device: device.into(),
            },
            input.gestures,
        ))
    }

    pub(crate) fn observe_gesture(&mut self, frame: &Completed) {
        self.gestures.buffer.acknowledge(frame.input_serial);
        if !self.gestures.buffer.busy() {
            self.gestures.capture = None;
        } else if self.gestures.capture.as_ref().is_some_and(|capture| {
            frame
                .frame
                .geometry
                .as_ref()
                .is_none_or(|geometry| !geometry.outputs.contains(&capture.viewport))
                || frame.frame.input.keyboard_capture
                || frame.frame.input.keyboard_modal
                    && !(capture.context.fingers == 4 && frame.frame.input.overview)
        }) {
            self.gestures.recognizer.cancel();
            self.gestures.buffer.cancel();
        }
    }

    pub(crate) fn flush_gesture(&mut self) -> io::Result<()> {
        if let Some(delivery) = self.gestures.buffer.next_delivery() {
            match self.worker.input(Input::Gesture(delivery)) {
                Ok(serial) => {
                    self.gestures.buffer.sent(serial);
                    if let GestureDelivery::Begin(context) = delivery {
                        self.pending_output = Some((serial, Some(context.output)));
                    }
                }
                Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => {
                    return Err(io::Error::other(
                        "The Park canvas stopped during a desktop gesture.",
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
