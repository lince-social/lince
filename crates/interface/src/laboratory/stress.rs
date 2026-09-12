use crate::{
    area::{AreaShape, InfluenceArea, Property, PropertyRule, RecordProperties},
    canvas::CanvasItem,
    sand_store::{SandKind, spawn_sand},
};
use bevy::{math::DVec2, prelude::*};
use serde::Serialize;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct StressConfig {
    pub max_sands: usize,
    pub batch: usize,
    pub warmup_frames: usize,
    pub sample_frames: usize,
    pub budget_ms: f64,
}

impl Default for StressConfig {
    fn default() -> Self {
        Self {
            max_sands: 8192,
            batch: 64,
            warmup_frames: 15,
            sample_frames: 60,
            budget_ms: 20.0,
        }
    }
}

impl StressConfig {
    pub fn validate(&self) -> bool {
        (1..=32768).contains(&self.max_sands)
            && (1..=128).contains(&self.batch)
            && (1..=600).contains(&self.warmup_frames)
            && (3..=1200).contains(&self.sample_frames)
            && self.budget_ms.is_finite()
            && (1.0..=1000.0).contains(&self.budget_ms)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Measurement {
    pub kind: SandKind,
    pub physics: bool,
    pub sands: usize,
    pub visible_sands: Option<usize>,
    pub entities: usize,
    pub samples: usize,
    pub mean_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub worst_ms: f64,
    pub spawn_ms: f64,
    pub within_budget: bool,
    pub emergency_stop: bool,
}

impl Measurement {
    fn times(samples: &mut [f64]) -> (f64, f64, f64, f64) {
        samples.sort_by(f64::total_cmp);
        let percentile =
            |p: f64| samples[((samples.len() as f64 * p).ceil() as usize).saturating_sub(1)];
        (
            samples.iter().sum::<f64>() / samples.len() as f64,
            percentile(0.95),
            percentile(0.99),
            samples[samples.len() - 1],
        )
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct StressReport {
    pub config: StressConfig,
    pub clock: String,
    pub rendered: bool,
    pub build: String,
    pub platform: String,
    pub graphics: super::GraphicsDevice,
    pub viewport: [f32; 2],
    pub measurements: Vec<Measurement>,
    pub stops: Vec<String>,
    pub complete: bool,
}

#[derive(Component)]
pub(crate) struct StressSand;

pub(crate) struct StressRun {
    pub report: StressReport,
    pub count: usize,
    pub target: usize,
    workload: usize,
    warmup: usize,
    samples: Vec<f64>,
    spawn_ms: f64,
    initialized: bool,
    slow_frames: usize,
}

impl StressRun {
    pub fn new(config: StressConfig, rendered: bool, viewport: Vec2) -> Self {
        Self {
            report: StressReport {
                config,
                rendered,
                clock: if rendered {
                    "Rendered frame interval, including presentation pacing"
                } else {
                    "Headless Sand and physics update interval; no UI layout, rendering, or GPU measurement"
                }
                .into(),
                build: format!(
                    "{} {}",
                    env!("CARGO_PKG_VERSION"),
                    if cfg!(debug_assertions) {
                        "debug"
                    } else {
                        "release"
                    }
                ),
                platform: format!("{} / {}", std::env::consts::OS, std::env::consts::ARCH),
                graphics: super::GraphicsDevice::default(),
                viewport: viewport.to_array(),
                measurements: Vec::new(),
                stops: Vec::new(),
                complete: false,
            },
            count: 0,
            target: 0,
            workload: 0,
            warmup: config.warmup_frames,
            samples: Vec::new(),
            spawn_ms: 0.0,
            initialized: false,
            slow_frames: 0,
        }
    }

    pub fn kind(&self) -> SandKind {
        SandKind::ALL[(self.workload / 2).min(2)]
    }
    pub fn physics(&self) -> bool {
        self.workload % 2 == 1
    }

    pub fn clear(world: &mut World, root: Entity) {
        let entities: Vec<_> = world
            .query_filtered::<(Entity, &ChildOf), With<StressSand>>()
            .iter(world)
            .filter(|(_, parent)| parent.parent() == root)
            .map(|(entity, _)| entity)
            .collect();
        for entity in entities {
            world.despawn(entity);
        }
    }

    pub fn advance(&mut self, world: &mut World, root: Entity, elapsed_ms: f64) {
        if self.report.complete {
            return;
        }
        if !self.initialized {
            self.report.graphics = super::GraphicsDevice::read(world);
            Self::clear(world, root);
            let enabled = self.physics();
            crate::workspace_config::set_physics(world, root, 1, enabled);
            if enabled {
                let mut area =
                    InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(100_000.0));
                area.rules.push(PropertyRule {
                    property: Property::Quantity,
                    value: "-3".into(),
                });
                area.strength = 80.0;
                if let Some(area) = crate::area::spawn_area(world, root, 1, area) {
                    world.entity_mut(area).insert(StressSand);
                }
            }
            self.initialized = true;
            return;
        }
        self.slow_frames = if elapsed_ms > 1000.0 {
            self.slow_frames + 1
        } else {
            0
        };
        let emergency_stop = self.slow_frames >= 3;
        if !emergency_stop && self.count < self.target {
            let started = Instant::now();
            let end = (self.count + self.report.config.batch).min(self.target);
            let viewport = Vec2::from_array(self.report.viewport);
            let span = (viewport - Vec2::new(80.0, 360.0)).max(Vec2::splat(100.0));
            for index in self.count..end {
                let x = ((index * 977 + 17) % 8191) as f64 / 8191.0 - 0.5;
                let y = ((index * 619 + 43) % 8191) as f64 / 8191.0 - 0.5;
                let sand = spawn_sand(
                    world,
                    root,
                    1,
                    self.kind(),
                    "Sand",
                    DVec2::new(x * f64::from(span.x), y * f64::from(span.y) + 130.0),
                );
                let size = if self.kind() == SandKind::Square {
                    Vec2::splat(12.0)
                } else {
                    Vec2::new(64.0, 32.0)
                };
                world.get_mut::<CanvasItem>(sand).unwrap().size = size;
                if let Some(content) = world
                    .get::<crate::sand_store::StoredSand>(sand)
                    .and_then(|sand| sand.content)
                {
                    let mut area = world
                        .get_mut::<crate::sand_text::SandText>(content)
                        .unwrap();
                    area.offset = [2.0; 2];
                    area.size = [60.0, 28.0];
                    let mut node = world.get_mut::<Node>(content).unwrap();
                    node.left = px(2);
                    node.top = px(2);
                    node.width = px(60);
                    node.height = px(28);
                }
                world.entity_mut(sand).insert((
                    StressSand,
                    RecordProperties(serde_json::json!({"quantity":-3})),
                ));
            }
            self.count = end;
            self.spawn_ms += started.elapsed().as_secs_f64() * 1000.0;
            return;
        }
        if !emergency_stop && self.warmup > 0 {
            self.warmup -= 1;
            return;
        }
        if !elapsed_ms.is_finite() || elapsed_ms <= 0.0 {
            return;
        }
        self.samples.push(elapsed_ms);
        if !emergency_stop && self.samples.len() < self.report.config.sample_frames {
            return;
        }
        let (mean_ms, p95_ms, p99_ms, worst_ms) = Measurement::times(&mut self.samples);
        let within_budget = p95_ms <= self.report.config.budget_ms;
        let visible_sands = world.query_filtered::<(&Node, &ChildOf), (With<StressSand>, With<crate::sand_store::StoredSand>)>()
            .iter(world).filter(|(node, parent)| parent.parent() == root && node.display != Display::None).count();
        self.report.measurements.push(Measurement {
            kind: self.kind(),
            physics: self.physics(),
            sands: self.count,
            visible_sands: self.report.rendered.then_some(visible_sands),
            entities: world.entities().len() as usize,
            samples: self.samples.len(),
            mean_ms,
            p95_ms,
            p99_ms,
            worst_ms,
            spawn_ms: self.spawn_ms,
            within_budget,
            emergency_stop,
        });
        self.samples.clear();
        self.spawn_ms = 0.0;
        self.warmup = self.report.config.warmup_frames;
        if !within_budget || self.count == self.report.config.max_sands {
            self.report.stops.push(format!(
                "{} / physics {}: {} at {} Sands (p95 {:.2} ms)",
                self.kind().name(),
                if self.physics() { "on" } else { "off" },
                if emergency_stop {
                    "stopped after three frames over one second"
                } else if within_budget {
                    "limit reached; capacity not exhausted"
                } else {
                    "frame budget exceeded"
                },
                self.count,
                p95_ms
            ));
            self.workload += 1;
            self.count = 0;
            self.target = 0;
            self.initialized = false;
            self.slow_frames = 0;
            Self::clear(world, root);
            crate::workspace_config::set_physics(world, root, 1, false);
            self.report.complete = self.workload == SandKind::ALL.len() * 2;
        } else {
            self.target = self
                .target
                .saturating_mul(2)
                .max(self.report.config.batch)
                .min(self.report.config.max_sands);
        }
    }
}

pub(crate) mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    fn percentiles_and_limits_reject_misleading_or_unbounded_runs() {
        let mut values: Vec<_> = (1..=100).map(f64::from).collect();
        assert_eq!(Measurement::times(&mut values), (50.5, 95.0, 99.0, 100.0));
        for config in [
            StressConfig {
                max_sands: 0,
                ..default()
            },
            StressConfig {
                batch: 129,
                ..default()
            },
            StressConfig {
                budget_ms: f64::NAN,
                ..default()
            },
            StressConfig {
                sample_frames: 0,
                ..default()
            },
        ] {
            assert!(!config.validate());
        }
    }
    crate::laboratory_cases! {
        percentiles_and_limits_reject_misleading_or_unbounded_runs,
    }
}
