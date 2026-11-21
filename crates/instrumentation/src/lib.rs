//! Records process-wide timings, counters, and opaque operation marks.

use std::{
    collections::hash_map::RandomState,
    fmt::{self, Debug},
    hash::BuildHasher,
    sync::OnceLock,
    time::{Duration, Instant},
};

/// A reading delivered to the installed observer.
#[derive(Debug)]
pub enum Reading {
    /// Time spent in a named span.
    Elapsed(&'static str, Duration),
    /// Increment for a named counter.
    Count(&'static str, u64),
    /// Named event correlated with an opaque operation ID.
    Mark(&'static str, u64),
}

/// Receives a reading synchronously on the thread that emitted it.
pub type Observer = fn(Reading);

static OBSERVER: OnceLock<Observer> = OnceLock::new();

/// Installs the process-wide observer and returns whether this call installed it.
///
/// Observers may be called concurrently, including while a span unwinds.
/// Keep the callback nonblocking and free of panics.
pub fn install(observer: Observer) -> bool {
    OBSERVER.set(observer).is_ok()
}

/// Adds a count to the named metric when observation is enabled.
pub fn add(name: &'static str, count: u64) {
    if let Some(observer) = OBSERVER.get() {
        observer(Reading::Count(name, count));
    }
}

/// Correlates readings for one operation without transmitting its source identity.
#[derive(Clone, Copy, Default)]
pub struct Operation(Option<(Observer, u64)>);

impl Debug for Operation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Operation")
            .field("id", &self.id())
            .finish()
    }
}

impl Operation {
    /// Captures the active observer and hashes the identity with a process-local key.
    pub fn new(identity: &str) -> Self {
        Self::with_observer(identity, OBSERVER.get().copied())
    }

    fn with_observer(identity: &str, observer: Option<Observer>) -> Self {
        static HASHER: OnceLock<RandomState> = OnceLock::new();
        Self(observer.map(|observer| {
            (
                observer,
                HASHER.get_or_init(RandomState::new).hash_one(identity),
            )
        }))
    }

    /// Returns the opaque process-local ID when observation is enabled.
    pub fn id(self) -> Option<u64> {
        self.0.map(|(_, identity)| identity)
    }

    /// Emits a named mark associated with this operation.
    pub fn mark(self, name: &'static str) {
        if let Some((observer, identity)) = self.0 {
            observer(Reading::Mark(name, identity));
        }
    }
}

/// Emits elapsed time on drop when an observer was installed at construction.
pub struct Span {
    name: &'static str,
    started: Option<(Observer, Instant)>,
}

impl Debug for Span {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Span")
            .field("name", &self.name)
            .field("active", &self.started.is_some())
            .finish()
    }
}

impl Span {
    /// Starts measuring a named span if observation is enabled.
    pub fn new(name: &'static str) -> Self {
        Self::with_observer(name, OBSERVER.get().copied())
    }

    fn with_observer(name: &'static str, observer: Option<Observer>) -> Self {
        Self {
            name,
            started: observer.map(|observer| (observer, Instant::now())),
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some((observer, started)) = self.started {
            observer(Reading::Elapsed(self.name, started.elapsed()));
        }
    }
}

#[cfg(test)]
mod tests;
