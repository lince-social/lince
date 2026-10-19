use rustls::pki_types::InvalidSignature;
use std::{collections::VecDeque, sync::Mutex};

const CAPACITY: usize = 8;
const MINIMUM_MESSAGE: usize = 256;
const MAXIMUM_MESSAGE: usize = 4096;

#[derive(Clone, Copy)]
struct Input<'a> {
    algorithm: (u32, u32),
    public_key: &'a [u8],
    message: &'a [u8],
    signature: &'a [u8],
}

struct Verified {
    algorithm: (u32, u32),
    public_key: Box<[u8]>,
    message: Box<[u8]>,
    signature: Box<[u8]>,
}

impl Verified {
    fn matches(&self, input: Input<'_>) -> bool {
        self.algorithm == input.algorithm
            && self.public_key.as_ref() == input.public_key
            && self.message.as_ref() == input.message
            && self.signature.as_ref() == input.signature
    }
}

struct Cache {
    entries: VecDeque<Verified>,
    hits: u64,
    misses: u64,
}

impl Cache {
    const fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            hits: 0,
            misses: 0,
        }
    }

    fn lookup(&mut self, input: Input<'_>) -> bool {
        if let Some(index) = self.entries.iter().position(|entry| entry.matches(input)) {
            if let Some(entry) = self.entries.remove(index) {
                self.entries.push_back(entry);
                self.hits = self.hits.saturating_add(1);
                return true;
            }
        }
        self.misses = self.misses.saturating_add(1);
        false
    }

    fn insert(&mut self, input: Input<'_>) {
        if self.entries.iter().any(|entry| entry.matches(input)) {
            return;
        }
        if self.entries.len() == CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(Verified {
            algorithm: input.algorithm,
            public_key: input.public_key.into(),
            message: input.message.into(),
            signature: input.signature.into(),
        });
    }
}

static CACHE: Mutex<Cache> = Mutex::new(Cache::new());

fn verify_with(
    cache: &Mutex<Cache>,
    input: Input<'_>,
    check: impl FnOnce() -> Result<(), InvalidSignature>,
) -> Result<(), InvalidSignature> {
    if !(MINIMUM_MESSAGE..=MAXIMUM_MESSAGE).contains(&input.message.len())
        || input.public_key.len() > 97
        || input.signature.len() > 104
    {
        return check();
    }
    if cache.lock().map_err(|_| InvalidSignature)?.lookup(input) {
        return Ok(());
    }
    check()?;
    cache.lock().map_err(|_| InvalidSignature)?.insert(input);
    Ok(())
}

pub(super) fn verify(
    algorithm: (u32, u32),
    public_key: &[u8],
    message: &[u8],
    signature: &[u8],
    check: impl FnOnce() -> Result<(), InvalidSignature>,
) -> Result<(), InvalidSignature> {
    verify_with(
        &CACHE,
        Input {
            algorithm,
            public_key,
            message,
            signature,
        },
        check,
    )
}

pub(super) fn statistics() -> Option<(usize, u64, u64)> {
    CACHE
        .lock()
        .ok()
        .map(|cache| (cache.entries.len(), cache.hits, cache.misses))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn only_identical_successful_calculations_are_reused() {
        let cache = Mutex::new(Cache::new());
        let message = [1; MINIMUM_MESSAGE];
        let changed_message = [2; MINIMUM_MESSAGE];
        let input = Input {
            algorithm: (384, 256),
            public_key: b"key",
            message: &message,
            signature: b"signature",
        };
        let calls = Cell::new(0);
        let check = || {
            calls.set(calls.get() + 1);
            Ok(())
        };
        verify_with(&cache, input, check).unwrap();
        verify_with(&cache, input, check).unwrap();
        assert_eq!(calls.get(), 1);
        for changed in [
            Input {
                algorithm: (256, 256),
                ..input
            },
            Input {
                algorithm: (384, 384),
                ..input
            },
            Input {
                public_key: b"other",
                ..input
            },
            Input {
                message: &changed_message,
                ..input
            },
            Input {
                signature: b"invalid",
                ..input
            },
        ] {
            for _ in 0..2 {
                assert!(
                    verify_with(&cache, changed, || {
                        calls.set(calls.get() + 1);
                        Err(InvalidSignature)
                    })
                    .is_err()
                );
            }
        }
        assert_eq!(calls.get(), 11);
        assert_eq!(cache.lock().unwrap().entries.len(), 1);
    }

    #[test]
    fn capacity_eviction_and_oversized_inputs_bound_retained_memory() {
        let cache = Mutex::new(Cache::new());
        let input = Input {
            algorithm: (256, 256),
            public_key: b"key",
            message: &[1; MINIMUM_MESSAGE],
            signature: b"signature",
        };
        for index in 0..=CAPACITY {
            let message = [index as u8; MAXIMUM_MESSAGE];
            verify_with(
                &cache,
                Input {
                    message: &message,
                    ..input
                },
                || Ok(()),
            )
            .unwrap();
        }
        assert_eq!(cache.lock().unwrap().entries.len(), CAPACITY);
        let evicted = [0; MAXIMUM_MESSAGE];
        assert!(
            verify_with(
                &cache,
                Input {
                    message: &evicted,
                    ..input
                },
                || Err(InvalidSignature)
            )
            .is_err()
        );
        let oversized = [0; MAXIMUM_MESSAGE + 1];
        let oversized_key = [0; 98];
        let oversized_signature = [0; 105];
        let calls = Cell::new(0);
        for oversized in [
            Input {
                message: b"fresh-handshake",
                ..input
            },
            Input {
                message: &oversized,
                ..input
            },
            Input {
                public_key: &oversized_key,
                ..input
            },
            Input {
                signature: &oversized_signature,
                ..input
            },
        ] {
            for _ in 0..2 {
                verify_with(&cache, oversized, || {
                    calls.set(calls.get() + 1);
                    Ok(())
                })
                .unwrap();
            }
        }
        assert_eq!(calls.get(), 8);
        assert_eq!(cache.lock().unwrap().entries.len(), CAPACITY);
    }
}
