//! Cancellation shared by an interface test and its runner.

use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{
            AtomicBool,
            Ordering::{Acquire, Release},
        },
    },
};

#[cfg(feature = "test-driver-async")]
use tokio::sync::Notify;

#[derive(Debug, Default)]
struct State {
    cancelled: AtomicBool,
    #[cfg(feature = "test-driver-async")]
    signal: Notify,
}

thread_local! {
    static ACTIVE: RefCell<Option<Cancellation>> = const { RefCell::new(None) };
}

/// A shared stop signal for one interface test.
#[derive(Clone, Debug, Default)]
pub struct Cancellation(Arc<State>);

impl Cancellation {
    /// Stops the test and wakes any asynchronous waiters.
    pub fn cancel(&self) {
        self.0.cancelled.store(true, Release);
        #[cfg(feature = "test-driver-async")]
        self.0.signal.notify_waiters();
    }

    /// Returns whether the test has been stopped.
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Acquire)
    }

    /// Returns the active test's signal on this thread or creates a fresh one.
    pub fn current() -> Self {
        ACTIVE.with(|active| active.borrow().clone().unwrap_or_default())
    }

    /// Runs a closure with this signal active on the calling thread.
    #[cfg(feature = "test-driver-async")]
    pub fn scope<R>(&self, run: impl FnOnce() -> R) -> R {
        struct Restore(Option<Cancellation>);

        impl Drop for Restore {
            fn drop(&mut self) {
                ACTIVE.with(|active| active.replace(self.0.take()));
            }
        }

        let _restore = Restore(ACTIVE.with(|active| active.replace(Some(self.clone()))));
        run()
    }

    /// Waits until the test is stopped.
    #[cfg(feature = "test-driver-async")]
    pub async fn cancelled(&self) {
        let notified = self.0.signal.notified();
        if !self.is_cancelled() {
            notified.await;
        }
    }
}
