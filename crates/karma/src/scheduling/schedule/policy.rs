//! Timer constraints and policies shared by elapsed and calendar schedules.

use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::KarmaBoundaryError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename(serialize = "TimerPolicyWire"), try_from = "TimerPolicyWire")]
pub struct TimerPolicy {
    required_resolution_ms: u32,
    max_lateness_ms: u32,
    coalesce_window_ms: u32,
}

impl TimerPolicy {
    pub fn new(
        required_resolution_ms: u32,
        max_lateness_ms: u32,
        coalesce_window_ms: u32,
    ) -> Result<Self, KarmaBoundaryError> {
        if required_resolution_ms == 0 {
            return Err(KarmaBoundaryError::invalid_input(
                "required timer resolution must be at least 1ms",
            ));
        }
        if coalesce_window_ms > max_lateness_ms {
            return Err(KarmaBoundaryError::invalid_input(
                "timer coalescing window cannot exceed maximum lateness",
            ));
        }
        Ok(Self {
            required_resolution_ms,
            max_lateness_ms,
            coalesce_window_ms,
        })
    }

    pub const fn required_resolution_ms(self) -> u32 {
        self.required_resolution_ms
    }

    pub const fn max_lateness_ms(self) -> u32 {
        self.max_lateness_ms
    }

    pub const fn coalesce_window_ms(self) -> u32 {
        self.coalesce_window_ms
    }
}

#[derive(Deserialize)]
struct TimerPolicyWire {
    required_resolution_ms: u32,
    max_lateness_ms: u32,
    coalesce_window_ms: u32,
}

impl TryFrom<TimerPolicyWire> for TimerPolicy {
    type Error = KarmaBoundaryError;

    fn try_from(wire: TimerPolicyWire) -> Result<Self, Self::Error> {
        Self::new(
            wire.required_resolution_ms,
            wire.max_lateness_ms,
            wire.coalesce_window_ms,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum MissedPolicy {
    Skip,
    Coalesce,
    Replay { max: NonZeroU32 },
    PauseOnLag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum InactiveGapPolicy {
    SkipToNextAnchor,
    ReplayByMissedPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum RephasePolicy {
    PreserveAnchor,
    FromLastIntended,
    FromChange,
    ImmediateIfOverdue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum OverloadPolicy {
    PauseAndAsk,
    RejectActivation,
    DegradeWithinGrant,
}
