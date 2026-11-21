//! Deadline admission, dispatch, and elapsed occurrence identity.

use serde::{Deserialize, Serialize};

mod admission;
mod deadline;
mod demand;
mod occurrence;

pub use admission::{
    DeadlineAdmission, DeadlinePlan, DeadlinePlanError, DeadlineRejectionReason, DemandedDeadline,
    DemandedDeadlinePlan, DispatcherResourceGrant, HostTimerCapabilities, plan_deadlines,
    plan_demanded_deadlines,
};
pub use deadline::{ArmedDeadline, DeadlineEntry, DeadlineIndex, DispatchBatch};
pub use demand::{
    DemandResource, ScheduleDemand, ScheduleDemandCapacity, ScheduleDemandRates,
    ScheduleWorkloadUpperBounds, SchedulerCalibration,
};
pub use occurrence::{
    ELAPSED_SCHEDULE_OCCURRENCE_HASH_DOMAIN, ElapsedScheduleOccurrence,
    ElapsedScheduleOccurrenceSchema, MAX_OCCURRENCE_BATCH_PAGE_TICKS, OCCURRENCE_BATCH_HASH_DOMAIN,
    OccurrenceBatch, OccurrenceBatchEmission, OccurrenceBatchSchema,
    SEMANTIC_SCHEDULE_TICK_HASH_DOMAIN, SemanticScheduleTick, SemanticScheduleTickSchema,
};

/// Durable phases of an elapsed or calendar schedule cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScheduleCursorLifecycle {
    Armed,
    Leased,
    Paused,
    Retired,
    Superseded,
    Failed,
}

impl ScheduleCursorLifecycle {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Armed => "armed",
            Self::Leased => "leased",
            Self::Paused => "paused",
            Self::Retired => "retired",
            Self::Superseded => "superseded",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "armed" => Self::Armed,
            "leased" => Self::Leased,
            "paused" => Self::Paused,
            "retired" => Self::Retired,
            "superseded" => Self::Superseded,
            "failed" => Self::Failed,
            _ => return None,
        })
    }
}
