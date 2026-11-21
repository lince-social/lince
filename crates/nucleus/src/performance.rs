// non-lean-yet
// Scope: enum Reading, type Observer, static OBSERVER, fn install.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::panic] `panic` should not be present in production code. Sites 98, 130, 140.
// - [Clippy: clippy::needless_pass_by_value] Borrow eligible inputs after checking caller and
//   framework contracts. Sites 92, 125.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 8.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 8, 29, 57.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 8, 9,
//   10, 11, 14, 18, 22, 29, 32, 46, 50, 57, 63.
// - [review: §5 Panics] Verify panic isolation boundary and preserve the failure contract. Sites
//   138.

use std::{
    collections::hash_map::RandomState,
    hash::BuildHasher,
    sync::OnceLock,
    time::{Duration, Instant},
};

pub enum Reading {
    Elapsed(&'static str, Duration),
    Count(&'static str, u64),
    Mark(&'static str, u64),
}

pub type Observer = fn(Reading);

static OBSERVER: OnceLock<Observer> = OnceLock::new();

pub fn install(observer: Observer) -> bool {
    OBSERVER.set(observer).is_ok()
}

pub fn add(name: &'static str, count: u64) {
    if let Some(observer) = OBSERVER.get() {
        observer(Reading::Count(name, count));
    }
}

#[derive(Clone, Copy, Default)]
pub struct Operation(Option<(Observer, u64)>);

impl Operation {
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

    pub fn id(self) -> Option<u64> {
        self.0.map(|(_, identity)| identity)
    }

    pub fn mark(self, name: &'static str) {
        if let Some((observer, identity)) = self.0 {
            observer(Reading::Mark(name, identity));
        }
    }
}

pub struct Span {
    name: &'static str,
    started: Option<(Observer, Instant)>,
}

impl Span {
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
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    #[test]
    fn operation_identity_is_optional_opaque_and_stable_across_threads() {
        static MARKS: AtomicU64 = AtomicU64::new(0);
        static IDENTITY: AtomicU64 = AtomicU64::new(0);
        fn observe(reading: Reading) {
            match reading {
                Reading::Mark("private-operation-boundary", identity) => {
                    assert_eq!(identity, IDENTITY.load(Ordering::Relaxed));
                    MARKS.fetch_add(1, Ordering::Relaxed);
                }
                _ => panic!("Unexpected operation reading"),
            }
        }
        let inactive = Operation::with_observer("private request identity", None);
        assert_eq!(inactive.id(), None);
        inactive.mark("private-operation-boundary");
        assert_eq!(MARKS.load(Ordering::Relaxed), 0);
        let first = Operation::with_observer("private request identity", Some(observe));
        IDENTITY.store(first.id().unwrap(), Ordering::Relaxed);
        let same = std::thread::spawn(|| {
            Operation::with_observer("private request identity", Some(observe))
        })
        .join()
        .unwrap();
        assert_eq!(first.id(), same.id());
        assert_ne!(
            first.id(),
            Operation::with_observer("another request identity", Some(observe)).id()
        );
        first.mark("private-operation-boundary");
        same.mark("private-operation-boundary");
        assert_eq!(MARKS.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn only_active_spans_report_and_unwinding_closes_each_span_once() {
        static SAMPLES: AtomicU64 = AtomicU64::new(0);
        fn observe(reading: Reading) {
            match reading {
                Reading::Elapsed("bounded-private-phase", _) => {
                    SAMPLES.fetch_add(1, Ordering::Relaxed);
                }
                _ => panic!("Unexpected timing sample"),
            }
        }
        let inactive = Span::with_observer("bounded-private-phase", None);
        drop(inactive);
        assert_eq!(SAMPLES.load(Ordering::Relaxed), 0);
        drop(Span::with_observer("bounded-private-phase", Some(observe)));
        assert_eq!(SAMPLES.load(Ordering::Relaxed), 1);
        let result = std::panic::catch_unwind(|| {
            let _timing = Span::with_observer("bounded-private-phase", Some(observe));
            panic!("cancelled private work");
        });
        assert!(result.is_err());
        assert_eq!(SAMPLES.load(Ordering::Relaxed), 2);
    }
}
