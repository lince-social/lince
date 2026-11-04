use super::{WakeSignal, performance};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct WakeDeadline {
    pending: Option<Pending>,
}

struct Pending {
    due: Instant,
    armed: Arc<AtomicBool>,
    signal: WakeSignal,
    task: tokio::task::JoinHandle<()>,
}

impl WakeDeadline {
    pub fn after(&mut self, signal: &WakeSignal, delay: Duration) -> bool {
        self.at(signal, Instant::now() + delay)
    }

    pub fn at(&mut self, signal: &WakeSignal, due: Instant) -> bool {
        if self.pending.as_ref().is_some_and(|pending| {
            pending.due <= due
                && pending.armed.load(Ordering::Acquire)
                && !pending.task.is_finished()
                && Arc::ptr_eq(&pending.signal.0, &signal.0)
        }) {
            return false;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return false;
        };
        self.cancel();
        let armed = Arc::new(AtomicBool::new(true));
        let active = armed.clone();
        let wake = signal.clone();
        let task = runtime.spawn(async move {
            tokio::time::sleep_until(due.into()).await;
            if active.swap(false, Ordering::AcqRel) {
                wake.ring_reason(performance::Count::TimerFired);
            }
        });
        performance::count(performance::Count::TimerScheduled);
        self.pending = Some(Pending {
            due,
            armed,
            signal: signal.clone(),
            task,
        });
        true
    }

    pub fn cancel(&mut self) {
        if let Some(pending) = self.pending.take() {
            pending.armed.store(false, Ordering::Release);
            pending.task.abort();
        }
    }

    #[cfg(test)]
    pub(crate) fn due(&self) -> Option<Instant> {
        self.pending
            .as_ref()
            .filter(|pending| pending.armed.load(Ordering::Acquire))
            .map(|pending| pending.due)
    }
}

impl Drop for WakeDeadline {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests;
