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
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

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
