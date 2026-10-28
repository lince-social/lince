use super::{Server, State};
use smithay::{
    input::pointer::{
        GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent, GesturePinchEndEvent,
        GesturePinchUpdateEvent, GestureSwipeBeginEvent, GestureSwipeEndEvent,
        GestureSwipeUpdateEvent,
    },
    utils::SERIAL_COUNTER,
};

fn valid_delta(delta: [f64; 2]) -> bool {
    delta
        .into_iter()
        .all(|value| value.is_finite() && value.abs() <= 1_000_000.0)
}

impl Server {
    pub fn gesture_swipe_begin(&mut self, fingers: u32, time: u32) {
        if self.locked() || fingers == 0 {
            self.gesture_swipe_end(true, time);
            return;
        }
        if let Some(pointer) = self.state.seat.get_pointer() {
            pointer.gesture_swipe_begin(
                &mut self.state,
                &GestureSwipeBeginEvent {
                    serial: SERIAL_COUNTER.next_serial(),
                    time,
                    fingers,
                },
            );
        }
    }

    pub fn gesture_swipe_update(&mut self, delta: [f64; 2], time: u32) {
        if self.locked() || !valid_delta(delta) {
            self.gesture_swipe_end(true, time);
            return;
        }
        if let Some(pointer) = self.state.seat.get_pointer() {
            pointer.gesture_swipe_update(
                &mut self.state,
                &GestureSwipeUpdateEvent {
                    time,
                    delta: (delta[0], delta[1]).into(),
                },
            );
        }
    }

    pub fn gesture_swipe_end(&mut self, cancelled: bool, time: u32) {
        let cancelled = cancelled || self.locked();
        if let Some(pointer) = self.state.seat.get_pointer() {
            pointer.gesture_swipe_end(
                &mut self.state,
                &GestureSwipeEndEvent {
                    serial: SERIAL_COUNTER.next_serial(),
                    time,
                    cancelled,
                },
            );
        }
    }

    pub fn gesture_pinch_begin(&mut self, fingers: u32, time: u32) {
        if self.locked() || fingers == 0 {
            self.gesture_pinch_end(true, time);
            return;
        }
        if let Some(pointer) = self.state.seat.get_pointer() {
            pointer.gesture_pinch_begin(
                &mut self.state,
                &GesturePinchBeginEvent {
                    serial: SERIAL_COUNTER.next_serial(),
                    time,
                    fingers,
                },
            );
        }
    }

    pub fn gesture_pinch_update(&mut self, delta: [f64; 2], scale: f64, rotation: f64, time: u32) {
        if self.locked()
            || !valid_delta(delta)
            || !scale.is_finite()
            || !(0.0..=1_000_000.0).contains(&scale)
            || scale == 0.0
            || !rotation.is_finite()
            || rotation.abs() > 1_000_000.0
        {
            self.gesture_pinch_end(true, time);
            return;
        }
        if let Some(pointer) = self.state.seat.get_pointer() {
            pointer.gesture_pinch_update(
                &mut self.state,
                &GesturePinchUpdateEvent {
                    time,
                    delta: (delta[0], delta[1]).into(),
                    scale,
                    rotation,
                },
            );
        }
    }

    pub fn gesture_pinch_end(&mut self, cancelled: bool, time: u32) {
        let cancelled = cancelled || self.locked();
        if let Some(pointer) = self.state.seat.get_pointer() {
            pointer.gesture_pinch_end(
                &mut self.state,
                &GesturePinchEndEvent {
                    serial: SERIAL_COUNTER.next_serial(),
                    time,
                    cancelled,
                },
            );
        }
    }

    pub fn gesture_hold_begin(&mut self, fingers: u32, time: u32) {
        if self.locked() || fingers == 0 {
            self.gesture_hold_end(true, time);
            return;
        }
        if let Some(pointer) = self.state.seat.get_pointer() {
            pointer.gesture_hold_begin(
                &mut self.state,
                &GestureHoldBeginEvent {
                    serial: SERIAL_COUNTER.next_serial(),
                    time,
                    fingers,
                },
            );
        }
    }

    pub fn gesture_hold_end(&mut self, cancelled: bool, time: u32) {
        let cancelled = cancelled || self.locked();
        if let Some(pointer) = self.state.seat.get_pointer() {
            pointer.gesture_hold_end(
                &mut self.state,
                &GestureHoldEndEvent {
                    serial: SERIAL_COUNTER.next_serial(),
                    time,
                    cancelled,
                },
            );
        }
    }
}

smithay::delegate_pointer_gestures!(State);
