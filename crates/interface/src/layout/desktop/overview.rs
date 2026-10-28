use crate::host::Rectangle;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OverviewSettings {
    pub hover_delay_ms: u16,
    pub edge_scroll_percent: u16,
}

impl Default for OverviewSettings {
    fn default() -> Self {
        Self {
            hover_delay_ms: 400,
            edge_scroll_percent: 100,
        }
    }
}

impl OverviewSettings {
    pub fn validate(self) -> Result<(), String> {
        if !(100..=2000).contains(&self.hover_delay_ms) {
            return Err("Use an overview hover delay from 100 to 2000 milliseconds.".into());
        }
        if self.edge_scroll_percent > 100 {
            return Err(
                "Use an overview edge speed from 0% to 100% of the viewport per second.".into(),
            );
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct HoverClock {
    target: Option<u64>,
    deadline: Option<Instant>,
    consumed: bool,
}

impl HoverClock {
    pub fn target(&self) -> Option<u64> {
        self.target
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    pub fn update(&mut self, target: Option<u64>, now: Instant, delay: Duration) -> Option<u64> {
        let Some(target) = target.filter(|target| *target != 0) else {
            *self = Self::default();
            return None;
        };
        if self.target != Some(target) {
            let Some(deadline) = now.checked_add(delay) else {
                *self = Self::default();
                return None;
            };
            self.target = Some(target);
            self.deadline = Some(deadline);
            self.consumed = false;
        }
        if !self.consumed && self.deadline.is_some_and(|deadline| now >= deadline) {
            self.consumed = true;
            self.deadline = None;
            return Some(target);
        }
        None
    }
}

pub fn edge_scroll(
    point: [f64; 2],
    bounds: Rectangle,
    elapsed: Duration,
    settings: OverviewSettings,
) -> f64 {
    if settings.validate().is_err() || !bounds.contains(point) || settings.edge_scroll_percent == 0
    {
        return 0.0;
    }
    let height = bounds.size()[1];
    let band = (height * 0.1).min(32.0);
    let offset = point[1] - bounds.origin()[1];
    let pressure = if offset < band {
        -(1.0 - offset / band)
    } else if offset > height - band {
        1.0 - (height - offset) / band
    } else {
        0.0
    };
    pressure * height * f64::from(settings.edge_scroll_percent) / 100.0
        * elapsed.as_secs_f64().min(0.1)
}

#[cfg(test)]
mod tests;
