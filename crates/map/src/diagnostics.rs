// non-lean-yet
// Scope: struct Counters, struct Snapshot, fn registry, fn snapshot.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 89, 90, 94, 95.
// - [Clippy: clippy::cast_sign_loss] Prove the sign or preserve behavior with an explicit
//   conversion. Sites 89, 90, 94, 95.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 8.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 8, 9,
//   10, 11, 12, 13, 17, 18, 19, 20, 21, 22, 44, 55, 79, 80, 81, 82, 83, 84, 87.
// - [rustc: unreachable_pub] unreachable `pub` item. Sites 67.

use std::sync::{
    Arc, Mutex, OnceLock, Weak,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use serde::Serialize;

#[derive(Default)]
pub struct Counters {
    pub loads: AtomicU64,
    pub processors: AtomicU64,
    pub decoded_bytes: AtomicU64,
    pub alive: AtomicBool,
    pub id: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub id: u64,
    pub alive: bool,
    pub loads: u64,
    pub processors: u64,
    pub decoded_bytes_total: u64,
}

fn registry() -> &'static Mutex<Vec<Weak<Counters>>> {
    static REGISTRY: OnceLock<Mutex<Vec<Weak<Counters>>>> = OnceLock::new();
    REGISTRY.get_or_init(Mutex::default)
}

impl Counters {
    pub(crate) fn new() -> Arc<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let counters = Arc::new(Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            alive: AtomicBool::new(true),
            ..Default::default()
        });
        let mut registry = registry()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        registry.retain(|entry| entry.strong_count() > 0);
        registry.push(Arc::downgrade(&counters));
        counters
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            id: self.id,
            alive: self.alive.load(Ordering::Relaxed),
            loads: self.loads.load(Ordering::Relaxed),
            processors: self.processors.load(Ordering::Relaxed),
            decoded_bytes_total: self.decoded_bytes.load(Ordering::Relaxed),
        }
    }
}

pub fn snapshot() -> Vec<Snapshot> {
    let mut registry = registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    registry.retain(|entry| entry.strong_count() > 0);
    registry
        .iter()
        .filter_map(Weak::upgrade)
        .map(|entry| entry.snapshot())
        .collect()
}

pub(crate) struct Work<'a>(&'a AtomicU64);
impl<'a> Work<'a> {
    pub fn new(counter: &'a AtomicU64) -> Self {
        counter.fetch_add(1, Ordering::Relaxed);
        Self(counter)
    }
}
impl Drop for Work<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct DeviceSnapshot {
    pub buffers: u64,
    pub textures: u64,
    pub buffer_bytes: u64,
    pub texture_bytes: u64,
    pub counters_available: bool,
}

pub fn device_snapshot(device: &wgpu::Device) -> DeviceSnapshot {
    let counters = device.get_internal_counters().hal;
    let buffers = counters.buffers.read().max(0) as u64;
    let textures = counters.textures.read().max(0) as u64;
    DeviceSnapshot {
        buffers,
        textures,
        buffer_bytes: counters.buffer_memory.read().max(0) as u64,
        texture_bytes: counters.texture_memory.read().max(0) as u64,
        counters_available: buffers > 0 || textures > 0,
    }
}
