use super::Input;
use bevy::input::touch::TouchPhase;
use std::{collections::VecDeque, sync::{Arc, Mutex, atomic::{AtomicU64, Ordering}, mpsc::TrySendError}};

const CAPACITY: usize = 128;

struct Queued {
    serial: u64,
    input: Input,
    started: Option<std::time::Instant>,
    latency: Option<super::latency::Stamp>,
}

#[derive(Default)]
struct State {
    events: VecDeque<Queued>,
    closed: bool,
    retired: Arc<AtomicU64>,
}

pub(super) struct Sender(Arc<Mutex<State>>);
pub(super) struct Receiver(Arc<Mutex<State>>);

pub(super) fn channel(retired: Arc<AtomicU64>) -> (Sender, Receiver) {
    let state = Arc::new(Mutex::new(State { retired, ..Default::default() }));
    (Sender(state.clone()), Receiver(state))
}

fn scroll(input: &Input) -> bool {
    matches!(input, Input::Scroll(..) | Input::CapturedScroll(..))
}

fn combine(previous: &mut Input, next: &Input) -> bool {
    let (delta, incoming) = match (previous, next) {
        (Input::Scroll(a, au, ap, ax, ast), Input::Scroll(b, bu, bp, bx, bst))
            if au == bu && *ap == TouchPhase::Moved && ap == bp && ax == bx && ast == bst => (a, b),
        (Input::CapturedScroll(a, au, ap, ax, ast, at), Input::CapturedScroll(b, bu, bp, bx, bst, bt))
            if au == bu && *ap == TouchPhase::Moved && ap == bp && ax == bx && ast == bst && at == bt => (a, b),
        _ => return false,
    };
    let sum = std::array::from_fn::<_, 2, _>(|axis| delta[axis] + incoming[axis]);
    if (0..2).any(|axis| !sum[axis].is_finite()
        || delta[axis] != 0.0 && incoming[axis] != 0.0 && delta[axis].is_sign_positive() != incoming[axis].is_sign_positive()) {
        return false;
    }
    *delta = sum;
    true
}

impl Sender {
    #[cfg(any(feature = "nested", test))]
    pub(super) fn can_send_controls(&self, count: usize) -> bool {
        let state = self.0.lock().unwrap();
        count <= CAPACITY.saturating_sub(state.events.iter().filter(|event| !scroll(&event.input)).count())
    }

    pub(super) fn try_send(&self, (serial, input): (u64, Input)) -> Result<(), TrySendError<(u64, Input)>> {
        let started = lince_interface::host::performance::start();
        let latency = super::latency::Stamp::current();
        let mut state = self.0.lock().unwrap();
        if state.closed { return Err(TrySendError::Disconnected((serial, input))); }
        let retired = state.retired.load(Ordering::Acquire);
        if let Some(previous) = state.events.back_mut()
            && previous.serial > retired && combine(&mut previous.input, &input) {
                previous.serial = serial;
                previous.latency = super::latency::Stamp::earliest(previous.latency, latency);
                lince_interface::host::performance::count(lince_interface::host::performance::Count::InputAccepted);
                lince_interface::host::performance::count(lince_interface::host::performance::Count::ScrollCombined);
                return Ok(());
            }
        let scrolling = scroll(&input);
        if state.events.iter().filter(|event| scroll(&event.input) == scrolling).count() >= CAPACITY {
            return Err(TrySendError::Full((serial, input)));
        }
        state.events.push_back(Queued { serial, input, started, latency });
        lince_interface::host::performance::count(lince_interface::host::performance::Count::InputAccepted);
        Ok(())
    }
}

impl Receiver {
    #[cfg(test)]
    pub(super) fn pop(&self) -> Option<(u64, Input)> {
        self.pop_timed().map(|(serial, input, _)| (serial, input))
    }

    pub(super) fn pop_timed(&self) -> Option<(u64, Input, Option<super::latency::Stamp>)> {
        let event = self.0.lock().unwrap().events.pop_front()?;
        lince_interface::host::performance::record_since(lince_interface::host::performance::Stage::InputQueueWait, event.started);
        Some((event.serial, event.input, event.latency))
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        let mut state = self.0.lock().unwrap();
        state.closed = true;
        state.events.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::mouse::MouseScrollUnit;

    fn wheel(delta: f32, stamp: u64) -> Input {
        Input::Scroll([0.0, delta], MouseScrollUnit::Pixel, TouchPhase::Moved, [10.0, 20.0], stamp)
    }

    #[test]
    fn a_full_control_queue_accepts_and_combines_scroll_bursts_without_reordering() {
        let (sender, receiver) = channel(Arc::default());
        for serial in 1..=128 { assert!(sender.try_send((serial, Input::Pointer([0.0; 2]))).is_ok()); }
        assert!(!sender.can_send_controls(1));
        assert!(matches!(sender.try_send((129, Input::Pointer([0.0; 2]))), Err(TrySendError::Full(_))));
        for serial in 130..10130 { assert!(sender.try_send((serial, wheel(0.5, 1))).is_ok()); }
        for serial in 1..=128 { assert_eq!(receiver.pop().unwrap().0, serial); }
        let (serial, Input::Scroll(delta, ..)) = receiver.pop().unwrap() else { panic!("Missing scroll batch"); };
        assert_eq!(serial, 10129);
        assert_eq!(delta, [0.0, 5000.0]);
        assert!(receiver.pop().is_none());
        assert!(sender.can_send_controls(128));
        assert!(!sender.can_send_controls(129));
    }

    #[test]
    fn scroll_batches_preserve_direction_phase_units_pointer_epochs_and_control_boundaries() {
        let (sender, receiver) = channel(Arc::default());
        let events = [wheel(2.0, 1), wheel(-2.0, 1), wheel(-2.0, 2),
            Input::Scroll([0.0, -2.0], MouseScrollUnit::Line, TouchPhase::Moved, [10.0, 20.0], 2),
            Input::Scroll([0.0, -2.0], MouseScrollUnit::Line, TouchPhase::Ended, [10.0, 20.0], 2),
            Input::Pointer([10.0, 20.0]), wheel(-2.0, 2)];
        for (serial, event) in events.into_iter().enumerate() { assert!(sender.try_send((serial as u64, event)).is_ok()); }
        for serial in 0..7 { assert_eq!(receiver.pop().unwrap().0, serial); }
        assert!(receiver.pop().is_none());
    }

    #[test]
    fn separate_scroll_capacity_preserves_control_space_and_reports_worker_disconnect() {
        let (sender, receiver) = channel(Arc::default());
        for serial in 0..128 { assert!(sender.try_send((serial, wheel(1.0, serial))).is_ok()); }
        assert!(matches!(sender.try_send((128, wheel(1.0, 128))), Err(TrySendError::Full(_))));
        assert!(sender.try_send((129, Input::CancelKeys)).is_ok());
        drop(receiver);
        assert!(matches!(sender.try_send((130, wheel(1.0, 130))), Err(TrySendError::Disconnected(_))));
    }

    #[test]
    fn a_new_scroll_cannot_revive_delta_retired_by_focus_loss() {
        let retired = Arc::new(AtomicU64::new(0));
        let (sender, receiver) = channel(retired.clone());
        assert!(sender.try_send((1, wheel(12.0, 1))).is_ok());
        retired.store(1, Ordering::Release);
        assert!(sender.try_send((2, wheel(3.0, 1))).is_ok());
        assert_eq!(receiver.pop().unwrap().0, 1);
        let (serial, Input::Scroll(delta, ..)) = receiver.pop().unwrap() else { panic!("Missing current scroll"); };
        assert_eq!(serial, 2);
        assert_eq!(delta, [0.0, 3.0]);
    }

    #[test]
    fn combined_scroll_keeps_its_oldest_queue_timestamp_and_latest_serial() {
        let (sender, receiver) = channel(Arc::default());
        assert!(sender.try_send((1, wheel(2.0, 1))).is_ok());
        let oldest = std::time::Instant::now() - std::time::Duration::from_millis(25);
        sender.0.lock().unwrap().events.front_mut().unwrap().started = Some(oldest);
        assert!(sender.try_send((2, wheel(3.0, 1))).is_ok());
        let state = sender.0.lock().unwrap();
        assert_eq!(state.events.front().unwrap().serial, 2);
        assert_eq!(state.events.front().unwrap().started, Some(oldest));
        drop(state);
        let (serial, Input::Scroll(delta, ..)) = receiver.pop().unwrap() else { panic!("Missing scroll batch"); };
        assert_eq!(serial, 2);
        assert_eq!(delta, [0.0, 5.0]);
    }
}
