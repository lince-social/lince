use std::{
    collections::BTreeSet,
    fmt::{self, Display},
    num::{NonZeroU32, NonZeroUsize},
};

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{CanonicalHash, KarmaBoundaryError, OverloadPolicy, TimestampMs};

use super::{
    deadline::{ArmedDeadline, DeadlineEntry, DeadlineIndex},
    demand::{DemandResource, ScheduleDemand, ScheduleDemandCapacity, ScheduleDemandRates},
};

/// Supported timer resolutions and the host limit on armed entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostTimerCapabilities {
    supported_resolution_ms: BTreeSet<NonZeroU32>,
    maximum_armed_entries: NonZeroUsize,
}

#[derive(Deserialize)]
struct HostTimerCapabilitiesWire {
    supported_resolution_ms: BTreeSet<NonZeroU32>,
    maximum_armed_entries: NonZeroUsize,
}

impl HostTimerCapabilities {
    pub fn new(
        supported_resolution_ms: impl IntoIterator<Item = NonZeroU32>,
        maximum_armed_entries: NonZeroUsize,
    ) -> Result<Self, KarmaBoundaryError> {
        let supported_resolution_ms = supported_resolution_ms.into_iter().collect::<BTreeSet<_>>();
        if supported_resolution_ms.is_empty() {
            return Err(KarmaBoundaryError::invalid_input(
                "host timer must expose at least one supported resolution",
            ));
        }
        Ok(Self {
            supported_resolution_ms,
            maximum_armed_entries,
        })
    }

    pub fn supported_resolutions(&self) -> &BTreeSet<NonZeroU32> {
        &self.supported_resolution_ms
    }

    pub const fn maximum_armed_entries(&self) -> NonZeroUsize {
        self.maximum_armed_entries
    }
}

impl<'de> Deserialize<'de> for HostTimerCapabilities {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = HostTimerCapabilitiesWire::deserialize(deserializer)?;
        Self::new(wire.supported_resolution_ms, wire.maximum_armed_entries)
            .map_err(de::Error::custom)
    }
}

/// Upper bounds on entries and lanes, including permitted timer degradation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispatcherResourceGrant {
    finest_resolution_ms: NonZeroU32,
    maximum_lanes: NonZeroUsize,
    maximum_entries: NonZeroUsize,
    allow_bounded_degradation: bool,
}

impl DispatcherResourceGrant {
    pub const fn new(
        finest_resolution_ms: NonZeroU32,
        maximum_lanes: NonZeroUsize,
        maximum_entries: NonZeroUsize,
        allow_bounded_degradation: bool,
    ) -> Self {
        Self {
            finest_resolution_ms,
            maximum_lanes,
            maximum_entries,
            allow_bounded_degradation,
        }
    }

    pub const fn finest_resolution_ms(&self) -> NonZeroU32 {
        self.finest_resolution_ms
    }
}

/// The first unmet admission constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeadlineRejectionReason {
    DuplicateActivation,
    UnsupportedResolution,
    EntryLimit,
    LaneLimit,
    DegradationExceedsLateness,
    SemanticRateLimit,
    WakeRateLimit,
    SchedulerCpuLimit,
    EvaluatorFuelLimit,
    WriteRateLimit,
    EffectRateLimit,
    TraceRateLimit,
    DemandArithmeticOverflow,
}

/// The disposition of an activation checked by the planner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum DeadlineAdmission {
    Admitted {
        activation_hash: CanonicalHash,
        lane_resolution_ms: NonZeroU32,
        degraded: bool,
    },
    Rejected {
        activation_hash: CanonicalHash,
        reason: DeadlineRejectionReason,
    },
    Paused {
        activation_hash: CanonicalHash,
        reason: DeadlineRejectionReason,
    },
}

/// Ordered admission outcomes and the index of armed deadlines.
#[derive(Debug, Clone)]
pub struct DeadlinePlan {
    pub admissions: Vec<DeadlineAdmission>,
    pub index: DeadlineIndex,
}

/// A deadline and its projected work at the same required resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DemandedDeadline {
    pub entry: DeadlineEntry,
    pub demand: ScheduleDemand,
}

impl DemandedDeadline {
    pub fn new(entry: DeadlineEntry, demand: ScheduleDemand) -> Result<Self, KarmaBoundaryError> {
        if entry.required_resolution_ms() != demand.required_resolution_ms() {
            return Err(KarmaBoundaryError::invalid_input(
                "deadline and demand required resolutions disagree",
            ));
        }
        Ok(Self { entry, demand })
    }
}

#[derive(Deserialize)]
struct DemandedDeadlineWire {
    entry: DeadlineEntry,
    demand: ScheduleDemand,
}

impl<'de> Deserialize<'de> for DemandedDeadline {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = DemandedDeadlineWire::deserialize(deserializer)?;
        Self::new(wire.entry, wire.demand).map_err(de::Error::custom)
    }
}

/// Ordered admission outcomes, armed deadlines, and accepted work rates.
#[derive(Debug, Clone)]
pub struct DemandedDeadlinePlan {
    pub admissions: Vec<DeadlineAdmission>,
    pub index: DeadlineIndex,
    pub admitted_demand: ScheduleDemandRates,
}

/// The activation and constraint that stopped planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlinePlanError {
    pub activation_hash: CanonicalHash,
    pub reason: DeadlineRejectionReason,
}

impl Display for DeadlinePlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "deadline {} could not be admitted: {:?}",
            self.activation_hash.as_str(),
            self.reason
        )
    }
}

impl std::error::Error for DeadlinePlanError {}

/// Selects deadlines that fit the host timer and granted lanes.
///
/// # Errors
///
/// Returns the first duplicate activation or rejection that must abort planning.
pub fn plan_deadlines(
    entries: impl IntoIterator<Item = DeadlineEntry>,
    host: &HostTimerCapabilities,
    grant: &DispatcherResourceGrant,
) -> Result<DeadlinePlan, DeadlinePlanError> {
    plan(
        entries.into_iter().map(|entry| (entry, ())),
        host,
        grant,
        |()| Ok(()),
    )
}

/// Selects deadlines that also fit their exact work-rate capacity.
///
/// # Errors
///
/// Returns the first duplicate activation or rejection that must abort planning.
pub fn plan_demanded_deadlines(
    entries: impl IntoIterator<Item = DemandedDeadline>,
    host: &HostTimerCapabilities,
    grant: &DispatcherResourceGrant,
    capacity: ScheduleDemandCapacity,
) -> Result<DemandedDeadlinePlan, DeadlinePlanError> {
    let mut admitted_demand = ScheduleDemandRates::zero();
    let DeadlinePlan { admissions, index } = plan(
        entries
            .into_iter()
            .map(|deadline| (deadline.entry, deadline.demand)),
        host,
        grant,
        |demand| {
            let candidate = admitted_demand
                .checked_add(demand)
                .map_err(|_| DeadlineRejectionReason::DemandArithmeticOverflow)?;
            if let Some(resource) = candidate.exceeded(capacity).next() {
                return Err(demand_rejection_reason(resource));
            }
            admitted_demand = candidate;
            Ok(())
        },
    )?;
    Ok(DemandedDeadlinePlan {
        admissions,
        index,
        admitted_demand,
    })
}

fn plan<D>(
    entries: impl Iterator<Item = (DeadlineEntry, D)>,
    host: &HostTimerCapabilities,
    grant: &DispatcherResourceGrant,
    mut admit_demand: impl FnMut(D) -> Result<(), DeadlineRejectionReason>,
) -> Result<DeadlinePlan, DeadlinePlanError> {
    let mut entries: Vec<_> = entries.collect();
    entries.sort_by(|(left, _), (right, _)| deadline_order(left).cmp(&deadline_order(right)));

    let mut seen = BTreeSet::new();
    let mut admissions = Vec::with_capacity(entries.len());
    let mut index = DeadlineIndex::default();
    let limit = host.maximum_armed_entries.min(grant.maximum_entries).get();

    for (entry, demand) in entries {
        if !seen.insert(entry.activation_hash().clone()) {
            return Err(DeadlinePlanError {
                activation_hash: entry.into_activation_hash(),
                reason: DeadlineRejectionReason::DuplicateActivation,
            });
        }
        if index.len() >= limit {
            admissions.push(overload_admission(
                entry,
                DeadlineRejectionReason::EntryLimit,
            )?);
            continue;
        }
        let lane = select_lane(&entry, host, grant, &index).and_then(|lane| {
            admit_demand(demand)?;
            Ok(lane)
        });
        let (lane_resolution_ms, degraded) = match lane {
            Ok(lane) => lane,
            Err(reason) => {
                admissions.push(overload_admission(entry, reason)?);
                continue;
            }
        };
        let activation_hash = entry.activation_hash().clone();
        index.upsert(ArmedDeadline {
            entry,
            lane_resolution_ms,
            degraded,
        });
        admissions.push(DeadlineAdmission::Admitted {
            activation_hash,
            lane_resolution_ms,
            degraded,
        });
    }
    Ok(DeadlinePlan { admissions, index })
}

fn deadline_order(entry: &DeadlineEntry) -> (TimestampMs, &CanonicalHash, u64) {
    (
        entry.next_intended_at(),
        entry.activation_hash(),
        entry.cursor_revision(),
    )
}

fn select_lane(
    entry: &DeadlineEntry,
    host: &HostTimerCapabilities,
    grant: &DispatcherResourceGrant,
    index: &DeadlineIndex,
) -> Result<(NonZeroU32, bool), DeadlineRejectionReason> {
    use DeadlineRejectionReason::{DegradationExceedsLateness, LaneLimit, UnsupportedResolution};

    let supported = host.supported_resolutions();
    let finest = grant.finest_resolution_ms;
    let required = entry.required_resolution_ms();
    let direct = supported
        .range(..=required)
        .next_back()
        .copied()
        .filter(|&lane| lane >= finest);
    let (mut lane, degraded) = if let Some(resolution) = direct {
        (resolution, false)
    } else if entry.overload_policy() == OverloadPolicy::DegradeWithinGrant
        && grant.allow_bounded_degradation
    {
        let resolution = *supported
            .range(finest..)
            .next()
            .ok_or(UnsupportedResolution)?;
        if resolution.get().saturating_sub(required.get()) > entry.max_lateness_ms() {
            return Err(DegradationExceedsLateness);
        }
        (resolution, true)
    } else {
        return Err(UnsupportedResolution);
    };

    if !index.has_lane(lane) && index.lane_count() >= grant.maximum_lanes.get() {
        lane = index.compatible_lane(finest, required).ok_or(LaneLimit)?;
    }
    Ok((lane, degraded))
}

const fn demand_rejection_reason(resource: DemandResource) -> DeadlineRejectionReason {
    match resource {
        DemandResource::SemanticTicks => DeadlineRejectionReason::SemanticRateLimit,
        DemandResource::TimerWakes => DeadlineRejectionReason::WakeRateLimit,
        DemandResource::SchedulerCpu => DeadlineRejectionReason::SchedulerCpuLimit,
        DemandResource::EvaluatorFuel => DeadlineRejectionReason::EvaluatorFuelLimit,
        DemandResource::Writes => DeadlineRejectionReason::WriteRateLimit,
        DemandResource::Effects => DeadlineRejectionReason::EffectRateLimit,
        DemandResource::TraceBytes => DeadlineRejectionReason::TraceRateLimit,
    }
}

fn overload_admission(
    entry: DeadlineEntry,
    reason: DeadlineRejectionReason,
) -> Result<DeadlineAdmission, DeadlinePlanError> {
    let overload = entry.overload_policy();
    let activation_hash = entry.into_activation_hash();
    match overload {
        OverloadPolicy::RejectActivation => Err(DeadlinePlanError {
            activation_hash,
            reason,
        }),
        OverloadPolicy::PauseAndAsk | OverloadPolicy::DegradeWithinGrant => {
            Ok(DeadlineAdmission::Paused {
                activation_hash,
                reason,
            })
        }
    }
}
