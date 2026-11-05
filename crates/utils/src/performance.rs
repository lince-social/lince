use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};

pub enum Reading {
    Elapsed(&'static str, Duration),
    Count(&'static str, u64),
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
