use std::cell::RefCell;
use std::future::{Future, poll_fn};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

thread_local! {
    static CURRENT: RefCell<Option<Execution>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub seed: [u8; 32],
    pub now_ms: i64,
    pub entropy_counter: u64,
    pub hlc: i64,
}

#[derive(Debug, Clone)]
pub struct Execution(Arc<Mutex<State>>);

impl Execution {
    pub fn new(seed: [u8; 32], now_ms: i64) -> Result<Self, &'static str> {
        Self::restore(State {
            seed,
            now_ms,
            entropy_counter: 0,
            hlc: 0,
        })
    }

    pub fn restore(state: State) -> Result<Self, &'static str> {
        validate_time(state.now_ms)?;
        Ok(Self(Arc::new(Mutex::new(state))))
    }

    pub fn snapshot(&self) -> State {
        self.0.lock().expect("execution state").clone()
    }

    pub fn set_time(&self, now_ms: i64) -> Result<(), &'static str> {
        validate_time(now_ms)?;
        self.0.lock().expect("execution state").now_ms = now_ms;
        Ok(())
    }

    pub fn now(&self) -> DateTime<Utc> {
        DateTime::from_timestamp_millis(self.snapshot().now_ms).expect("validated clock")
    }

    pub fn with<T>(&self, work: impl FnOnce() -> T) -> T {
        struct Reset(Option<Execution>);
        impl Drop for Reset {
            fn drop(&mut self) {
                CURRENT.with(|current| {
                    current.replace(self.0.take());
                });
            }
        }
        let _reset = Reset(CURRENT.with(|current| current.replace(Some(self.clone()))));
        work()
    }

    pub async fn scope<F: Future>(&self, work: F) -> F::Output {
        let mut work = std::pin::pin!(work);
        poll_fn(|context| self.with(|| work.as_mut().poll(context))).await
    }

    pub fn entropy(&self) -> [u8; 32] {
        let mut state = self.0.lock().expect("execution state");
        let mut hash = Sha256::new();
        hash.update(b"lince.execution.entropy.v1\0");
        hash.update(state.seed);
        hash.update(state.entropy_counter.to_be_bytes());
        state.entropy_counter = state
            .entropy_counter
            .checked_add(1)
            .expect("entropy budget exhausted");
        hash.finalize().into()
    }

    pub(crate) fn next_hlc(&self) -> i64 {
        let mut state = self.0.lock().expect("execution state");
        state.hlc = (state.now_ms << crate::hlc::COUNTER_BITS).max(state.hlc.saturating_add(1));
        state.hlc
    }

    pub(crate) fn observe_hlc(&self, seen: i64) {
        let mut state = self.0.lock().expect("execution state");
        state.hlc = state.hlc.max(seen);
    }
}

fn validate_time(now_ms: i64) -> Result<(), &'static str> {
    if !(0..=(i64::MAX >> crate::hlc::COUNTER_BITS)).contains(&now_ms) {
        return Err("clock is outside the nonnegative HLC range");
    }
    Ok(())
}

pub fn current() -> Option<Execution> {
    CURRENT.with(|current| current.borrow().clone())
}

pub fn now() -> DateTime<Utc> {
    current().map_or_else(Utc::now, |execution| execution.now())
}

pub fn uuid() -> uuid::Uuid {
    current().map_or_else(uuid::Uuid::new_v4, |execution| {
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&execution.entropy()[..16]);
        uuid::Builder::from_random_bytes(bytes).into_uuid()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_worlds_restore_ids_and_causal_clocks_without_touching_host() {
        let first = Execution::new([7; 32], 1_900_000_000_000).unwrap();
        let second = Execution::new([7; 32], 1_900_000_000_000).unwrap();
        let sample = || (crate::new_uid("r"), crate::hlc::next(), now());
        let expected = first.with(sample);
        let _host = sample();
        assert_eq!(second.with(sample), expected);
        let restored = Execution::restore(first.snapshot()).unwrap();
        assert_eq!(restored.with(sample), first.with(sample));
        assert!(current().is_none());
    }

    #[test]
    fn nested_scopes_and_panics_restore_the_previous_environment() {
        let first = Execution::new([1; 32], 1000).unwrap();
        let second = Execution::new([2; 32], 2000).unwrap();
        first.with(|| {
            let failed = std::panic::catch_unwind(|| second.with(|| panic!("test")));
            assert!(failed.is_err());
            assert_eq!(now().timestamp_millis(), 1000);
        });
        assert!(current().is_none());
    }
}
