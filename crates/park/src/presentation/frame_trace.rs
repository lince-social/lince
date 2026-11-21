// non-lean-yet
// Scope: static NEXT_RENDER, fn timestamp, fn owner_cpu_timestamp, fn micros.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use bevy::render::{RenderApp, diagnostic::RenderDiagnosticsObserver};
use serde_json::{Value, json};

use super::Presentation;

static NEXT_RENDER: AtomicU64 = AtomicU64::new(1);

pub(crate) fn timestamp() -> u64 {
    micros(crate::runtime::latency::monotonic())
}

#[cfg(feature = "session")]
pub(crate) fn owner_cpu_timestamp() -> u64 {
    let time = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
    (time.tv_sec.max(0) as u64)
        .saturating_mul(1_000_000)
        .saturating_add(time.tv_nsec.clamp(0, 999_999_999) as u64 / 1000)
}

pub(crate) fn micros(time: std::time::Duration) -> u64 {
    time.as_micros().min(u128::from(u64::MAX)) as u64
}

pub(crate) fn record(value: Value) {
    if let Err(value) = lince_interface::host::performance::record_park_frame(value) {
        eprintln!(
            "Park frame link {}",
            json!({"process":std::process::id(), "sample":value})
        );
    }
}

#[derive(bevy::prelude::Resource)]
pub(super) struct RenderId(pub u64);

impl Presentation {
    pub(super) fn trace_render(&mut self, pixel_only: bool) {
        if !crate::runtime::tracing_frames() {
            return;
        }
        let render = NEXT_RENDER.fetch_add(1, Ordering::Relaxed);
        let started = timestamp();
        self.traced_render = Some(render);
        record(
            json!({"event":"render-start", "render":render, "pixel_only":pixel_only,
            "monotonic_us":started}),
        );
        self.app
            .sub_app_mut(RenderApp)
            .insert_resource(RenderId(render));
        self.app.sub_app_mut(RenderApp).insert_resource(RenderDiagnosticsObserver(Arc::new(move |readings| {
            let observed = timestamp();
            let samples: Vec<_> = readings.iter()
                .filter_map(|reading| {
                    let path = reading.path.to_string();
                    (path.ends_with("/elapsed_gpu") && reading.value.is_finite() && reading.value >= 0.0)
                        .then(|| json!({"path":path, "unit":reading.suffix, "value":reading.value}))
                })
                .take(257)
                .collect();
            record(json!({"event":"native-gpu-readback", "render":render, "monotonic_us":observed,
                "age_us":observed.saturating_sub(started), "truncated":samples.len()>256,
                "samples":&samples[..samples.len().min(256)]}));
        })));
    }
}
