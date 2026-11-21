// non-lean-yet
// Scope: struct Geometry, fn transverse.
// BeautifulRust (baseline lines):
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   46.

use super::{Settings, TimeRange};

pub struct Geometry {
    now_ms: i64,
    aperture: f64,
    horizon: f64,
    phase: f64,
    radius: f64,
    pitch: f64,
    width: f64,
    unwind: f32,
    position_unwind: f64,
}

impl Geometry {
    pub(super) fn new(settings: &Settings, now_ms: i64, size: [f32; 2], unwind: f32) -> Self {
        let _prepare = nucleus::performance::Span::new("time-geometry-preparation");
        nucleus::performance::add("time-geometries-prepared", 1);
        let radius = f64::from(size[0].min(size[1]).max(1.0)) * 0.40;
        let pitch = (radius * 0.45)
            .min(100_000.0 / (settings.horizon_ms as f64 / settings.aperture_ms as f64).max(1.0));
        Self {
            now_ms,
            aperture: settings.aperture_ms as f64,
            horizon: settings.horizon_ms as f64,
            phase: settings.phase(now_ms),
            radius,
            pitch,
            width: f64::from(size[0]),
            unwind,
            position_unwind: f64::from(unwind.clamp(0.0, 1.0)),
        }
    }

    pub fn position(&self, at_ms: i64) -> [f32; 3] {
        let elapsed = at_ms.saturating_sub(self.now_ms).max(0) as f64;
        let turns = elapsed / self.aperture;
        let angle = self.phase + std::f64::consts::TAU * turns;
        let coiled = [
            self.radius * angle.sin(),
            -self.pitch * turns,
            -self.radius * angle.cos(),
        ];
        let straight = [self.width * (elapsed / self.horizon - 0.5) * 0.9, 0.0, 0.0];
        std::array::from_fn(|index| {
            (coiled[index] * (1.0 - self.position_unwind) + straight[index] * self.position_unwind)
                as f32
        })
    }

    pub fn transverse(&self, at_ms: i64) -> [f32; 3] {
        transverse(at_ms, self.now_ms, self.aperture, self.phase, self.unwind)
    }

    pub fn samples(&self, time: &TimeRange, budget: usize) -> impl Iterator<Item = [f32; 3]> + '_ {
        let from = time.from_ms.max(self.now_ms);
        let end = time.until_ms.unwrap_or(from);
        let count = if end == from {
            1
        } else {
            (((end - from) as f64 / self.aperture * 512.0).ceil() as usize).clamp(2, budget.max(2))
        };
        nucleus::performance::add("time-geometry-requested-samples", count as u64);
        (0..count).map(move |index| {
            let at = from
                + ((end - from) as f64 * index as f64 / (count - 1).max(1) as f64).round() as i64;
            self.position(at)
        })
    }
}

pub(super) fn transverse(
    at_ms: i64,
    now_ms: i64,
    aperture: f64,
    phase: f64,
    unwind: f32,
) -> [f32; 3] {
    let angle =
        phase + std::f64::consts::TAU * at_ms.saturating_sub(now_ms).max(0) as f64 / aperture;
    let value = [
        (angle.sin() as f32) * (1.0 - unwind),
        0.0,
        (-angle.cos() as f32) * (1.0 - unwind) + unwind,
    ];
    let length = value[0].hypot(value[2]);
    if length < 0.001 {
        return [0.0, 0.0, 1.0];
    }
    [value[0] / length, 0.0, value[2] / length]
}
