use crate::host::OutputId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct GestureSettings {
    pub enabled: bool,
    pub sensitivity_percent: u16,
    pub commit_threshold: u16,
}

impl Default for GestureSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            sensitivity_percent: 100,
            commit_threshold: 80,
        }
    }
}

impl GestureSettings {
    pub fn validate(self) -> Result<(), String> {
        if !(10..=800).contains(&self.sensitivity_percent) {
            return Err("Use a gesture sensitivity from 10% to 800%.".into());
        }
        if !(16..=400).contains(&self.commit_threshold) {
            return Err("Use a gesture commit threshold from 16 to 400 logical units.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GestureAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureProgress {
    pub sequence: u64,
    pub output: OutputId,
    pub horizontal_travel: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GestureResult {
    Noop,
    Cancelled,
    Pan(f64),
    Workspace(i8),
    Overview(bool),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureCompletion {
    pub sequence: u64,
    pub output: OutputId,
    pub result: GestureResult,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GestureError {
    InvalidSettings,
    InvalidSequence,
    AlreadyActive,
    InvalidDelta,
}

struct Active {
    sequence: u64,
    output: OutputId,
    fingers: u32,
    settings: GestureSettings,
    travel: [f64; 2],
    axis: Option<GestureAxis>,
    invalid: bool,
}

#[derive(Default)]
pub struct GestureRecognizer {
    last_sequence: u64,
    active: Option<Active>,
}

impl GestureRecognizer {
    pub fn begin(
        &mut self,
        sequence: u64,
        output: OutputId,
        fingers: u32,
        settings: GestureSettings,
    ) -> Result<bool, GestureError> {
        if self.active.is_some() {
            return Err(GestureError::AlreadyActive);
        }
        if sequence == 0 || sequence <= self.last_sequence {
            return Err(GestureError::InvalidSequence);
        }
        settings
            .validate()
            .map_err(|_| GestureError::InvalidSettings)?;
        self.last_sequence = sequence;
        if !settings.enabled || !matches!(fingers, 3 | 4) {
            return Ok(false);
        }
        self.active = Some(Active {
            sequence,
            output,
            fingers,
            settings,
            travel: [0.0; 2],
            axis: None,
            invalid: false,
        });
        Ok(true)
    }

    pub fn active(&self) -> Option<(u64, OutputId)> {
        self.active
            .as_ref()
            .map(|active| (active.sequence, active.output))
    }

    pub fn update(
        &mut self,
        sequence: u64,
        delta: [f64; 2],
    ) -> Result<Option<GestureProgress>, GestureError> {
        let Some(active) = self
            .active
            .as_mut()
            .filter(|active| active.sequence == sequence)
        else {
            return Ok(None);
        };
        let sensitivity = f64::from(active.settings.sensitivity_percent) / 100.0;
        let travel: [f64; 2] =
            std::array::from_fn(|axis| active.travel[axis] + delta[axis] * sensitivity);
        if active.invalid
            || delta.iter().any(|value| !value.is_finite())
            || travel
                .iter()
                .any(|value| !value.is_finite() || value.abs() > 1_000_000.0)
        {
            active.invalid = true;
            return Err(GestureError::InvalidDelta);
        }
        active.travel = travel;
        if active.axis.is_none() {
            let [horizontal, vertical] = travel.map(f64::abs);
            active.axis = if horizontal >= 12.0 && horizontal >= vertical * 1.5 {
                Some(GestureAxis::Horizontal)
            } else if vertical >= 12.0 && vertical >= horizontal * 1.5 {
                Some(GestureAxis::Vertical)
            } else {
                None
            };
        }
        Ok(
            (active.fingers == 3 && active.axis == Some(GestureAxis::Horizontal)).then_some(
                GestureProgress {
                    sequence,
                    output: active.output,
                    horizontal_travel: travel[0],
                },
            ),
        )
    }

    pub fn end(&mut self, sequence: u64, cancelled: bool) -> Option<GestureCompletion> {
        if self.active.as_ref()?.sequence != sequence {
            return None;
        }
        let active = self.active.take().unwrap();
        let result = if cancelled || active.invalid {
            GestureResult::Cancelled
        } else {
            let vertical = active.travel[1];
            let commits = vertical.abs() >= f64::from(active.settings.commit_threshold);
            match (active.fingers, active.axis) {
                (3, Some(GestureAxis::Horizontal)) => GestureResult::Pan(active.travel[0]),
                (3, Some(GestureAxis::Vertical)) if commits => {
                    GestureResult::Workspace(if vertical > 0.0 { 1 } else { -1 })
                }
                (4, Some(GestureAxis::Vertical)) if commits => {
                    GestureResult::Overview(vertical > 0.0)
                }
                _ => GestureResult::Noop,
            }
        };
        Some(GestureCompletion {
            sequence,
            output: active.output,
            result,
        })
    }

    pub fn cancel(&mut self) -> Option<GestureCompletion> {
        self.end(self.active.as_ref()?.sequence, true)
    }
}

#[cfg(test)]
mod tests;
