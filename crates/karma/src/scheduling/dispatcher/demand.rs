use std::num::NonZeroU32;

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{
    CalendarSchedule, ElapsedSchedule, KarmaBoundaryError, RationalRate, TimeZoneProvider,
};

use super::deadline::timer_resolution;

/// Maximum evaluator and persistence work for one semantic tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "ScheduleWorkloadUpperBoundsWire")]
pub struct ScheduleWorkloadUpperBounds {
    evaluator_fuel_per_tick: u64,
    writes_per_tick: NonZeroU32,
    effects_per_tick: u32,
    trace_bytes_per_tick: u64,
}

#[derive(Deserialize)]
struct ScheduleWorkloadUpperBoundsWire {
    evaluator_fuel_per_tick: u64,
    writes_per_tick: NonZeroU32,
    effects_per_tick: u32,
    trace_bytes_per_tick: u64,
}

impl ScheduleWorkloadUpperBounds {
    pub const fn new(
        evaluator_fuel_per_tick: u64,
        writes_per_tick: NonZeroU32,
        effects_per_tick: u32,
        trace_bytes_per_tick: u64,
    ) -> Self {
        Self {
            evaluator_fuel_per_tick,
            writes_per_tick,
            effects_per_tick,
            trace_bytes_per_tick,
        }
    }

    pub const fn evaluator_fuel_per_tick(self) -> u64 {
        self.evaluator_fuel_per_tick
    }

    pub const fn writes_per_tick(self) -> NonZeroU32 {
        self.writes_per_tick
    }

    pub const fn effects_per_tick(self) -> u32 {
        self.effects_per_tick
    }

    pub const fn trace_bytes_per_tick(self) -> u64 {
        self.trace_bytes_per_tick
    }
}

impl From<ScheduleWorkloadUpperBoundsWire> for ScheduleWorkloadUpperBounds {
    fn from(wire: ScheduleWorkloadUpperBoundsWire) -> Self {
        Self::new(
            wire.evaluator_fuel_per_tick,
            wire.writes_per_tick,
            wire.effects_per_tick,
            wire.trace_bytes_per_tick,
        )
    }
}

/// Host cost estimate for processing one timer wake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerCalibration {
    fire_cpu_ns_per_wake: NonZeroU32,
}

impl SchedulerCalibration {
    pub const fn new(fire_cpu_ns_per_wake: NonZeroU32) -> Self {
        Self {
            fire_cpu_ns_per_wake,
        }
    }

    pub const fn fire_cpu_ns_per_wake(self) -> NonZeroU32 {
        self.fire_cpu_ns_per_wake
    }
}

/// Semantic and timer rates with the workload used for admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ScheduleDemand {
    semantic_ticks_per_second: RationalRate,
    timer_wakes_per_second: RationalRate,
    required_resolution_ms: NonZeroU32,
    workload: ScheduleWorkloadUpperBounds,
    calibration: SchedulerCalibration,
}

#[derive(Deserialize)]
struct ScheduleDemandWire {
    semantic_ticks_per_second: RationalRate,
    timer_wakes_per_second: RationalRate,
    required_resolution_ms: NonZeroU32,
    workload: ScheduleWorkloadUpperBounds,
    calibration: SchedulerCalibration,
}

impl ScheduleDemand {
    pub fn for_elapsed(
        schedule: &ElapsedSchedule,
        workload: ScheduleWorkloadUpperBounds,
        calibration: SchedulerCalibration,
    ) -> Result<Self, KarmaBoundaryError> {
        let semantic_ticks_per_second = schedule.semantic_rate();
        Self::new(
            semantic_ticks_per_second,
            semantic_ticks_per_second,
            timer_resolution(schedule.timer()),
            workload,
            calibration,
        )
    }

    pub fn for_calendar(
        schedule: &CalendarSchedule,
        provider: &dyn TimeZoneProvider,
        workload: ScheduleWorkloadUpperBounds,
        calibration: SchedulerCalibration,
    ) -> Result<Self, KarmaBoundaryError> {
        if provider.revision() != &schedule.tzdb {
            return Err(KarmaBoundaryError::invalid_definition(
                "calendar demand provider revision does not match schedule",
            ));
        }
        let minimum_interval_ms = provider.minimum_interval_ms(schedule)?;
        let semantic_ticks_per_second = RationalRate::new(1_000, minimum_interval_ms.get())?;
        Self::new(
            semantic_ticks_per_second,
            semantic_ticks_per_second,
            timer_resolution(schedule.timer),
            workload,
            calibration,
        )
    }

    fn new(
        semantic_ticks_per_second: RationalRate,
        timer_wakes_per_second: RationalRate,
        required_resolution_ms: NonZeroU32,
        workload: ScheduleWorkloadUpperBounds,
        calibration: SchedulerCalibration,
    ) -> Result<Self, KarmaBoundaryError> {
        if timer_wakes_per_second > semantic_ticks_per_second {
            return Err(KarmaBoundaryError::invalid_input(
                "timer wake rate cannot exceed semantic schedule rate",
            ));
        }
        let demand = Self {
            semantic_ticks_per_second,
            timer_wakes_per_second,
            required_resolution_ms,
            workload,
            calibration,
        };
        demand.derived_rates()?;
        Ok(demand)
    }

    pub const fn semantic_ticks_per_second(self) -> RationalRate {
        self.semantic_ticks_per_second
    }

    pub const fn timer_wakes_per_second(self) -> RationalRate {
        self.timer_wakes_per_second
    }

    pub const fn required_resolution_ms(self) -> NonZeroU32 {
        self.required_resolution_ms
    }

    pub const fn workload(self) -> ScheduleWorkloadUpperBounds {
        self.workload
    }

    pub const fn calibration(self) -> SchedulerCalibration {
        self.calibration
    }

    pub fn derived_rates(self) -> Result<ScheduleDemandRates, KarmaBoundaryError> {
        Ok(ScheduleDemandRates {
            semantic_ticks_per_second: self.semantic_ticks_per_second,
            timer_wakes_per_second: self.timer_wakes_per_second,
            scheduler_cpu_ns_per_second: checked_rate_product(
                self.timer_wakes_per_second,
                u64::from(self.calibration.fire_cpu_ns_per_wake.get()),
                "scheduler CPU rate",
            )?,
            evaluator_fuel_per_second: checked_rate_product(
                self.semantic_ticks_per_second,
                self.workload.evaluator_fuel_per_tick,
                "evaluator fuel rate",
            )?,
            writes_per_second: checked_rate_product(
                self.semantic_ticks_per_second,
                u64::from(self.workload.writes_per_tick.get()),
                "write rate",
            )?,
            effects_per_second: checked_rate_product(
                self.semantic_ticks_per_second,
                u64::from(self.workload.effects_per_tick),
                "effect rate",
            )?,
            trace_bytes_per_second: checked_rate_product(
                self.semantic_ticks_per_second,
                self.workload.trace_bytes_per_tick,
                "trace byte rate",
            )?,
        })
    }
}

impl<'de> Deserialize<'de> for ScheduleDemand {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ScheduleDemandWire::deserialize(deserializer)?;
        Self::new(
            wire.semantic_ticks_per_second,
            wire.timer_wakes_per_second,
            wire.required_resolution_ms,
            wire.workload,
            wire.calibration,
        )
        .map_err(de::Error::custom)
    }
}

/// Exact resource rates for a schedule or a set of admitted schedules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleDemandRates {
    pub semantic_ticks_per_second: RationalRate,
    pub timer_wakes_per_second: RationalRate,
    pub scheduler_cpu_ns_per_second: RationalRate,
    pub evaluator_fuel_per_second: RationalRate,
    pub writes_per_second: RationalRate,
    pub effects_per_second: RationalRate,
    pub trace_bytes_per_second: RationalRate,
}

impl ScheduleDemandRates {
    pub fn zero() -> Self {
        let zero = RationalRate::new(0, 1).expect("one is a valid denominator");
        Self {
            semantic_ticks_per_second: zero,
            timer_wakes_per_second: zero,
            scheduler_cpu_ns_per_second: zero,
            evaluator_fuel_per_second: zero,
            writes_per_second: zero,
            effects_per_second: zero,
            trace_bytes_per_second: zero,
        }
    }

    pub fn checked_add(self, demand: ScheduleDemand) -> Result<Self, KarmaBoundaryError> {
        let rhs = demand.derived_rates()?;
        Ok(Self {
            semantic_ticks_per_second: checked_rate_sum(
                self.semantic_ticks_per_second,
                rhs.semantic_ticks_per_second,
                "semantic tick rate",
            )?,
            timer_wakes_per_second: checked_rate_sum(
                self.timer_wakes_per_second,
                rhs.timer_wakes_per_second,
                "timer wake rate",
            )?,
            scheduler_cpu_ns_per_second: checked_rate_sum(
                self.scheduler_cpu_ns_per_second,
                rhs.scheduler_cpu_ns_per_second,
                "scheduler CPU rate",
            )?,
            evaluator_fuel_per_second: checked_rate_sum(
                self.evaluator_fuel_per_second,
                rhs.evaluator_fuel_per_second,
                "evaluator fuel rate",
            )?,
            writes_per_second: checked_rate_sum(
                self.writes_per_second,
                rhs.writes_per_second,
                "write rate",
            )?,
            effects_per_second: checked_rate_sum(
                self.effects_per_second,
                rhs.effects_per_second,
                "effect rate",
            )?,
            trace_bytes_per_second: checked_rate_sum(
                self.trace_bytes_per_second,
                rhs.trace_bytes_per_second,
                "trace byte rate",
            )?,
        })
    }

    /// Lists exceeded resources in admission priority order.
    pub fn exceeded_resources(self, capacity: ScheduleDemandCapacity) -> Vec<DemandResource> {
        self.exceeded(capacity).collect()
    }

    pub(super) fn exceeded(
        self,
        capacity: ScheduleDemandCapacity,
    ) -> impl Iterator<Item = DemandResource> {
        let checks = [
            (
                DemandResource::SemanticTicks,
                self.semantic_ticks_per_second,
                capacity.semantic_ticks_per_second,
            ),
            (
                DemandResource::TimerWakes,
                self.timer_wakes_per_second,
                capacity.timer_wakes_per_second,
            ),
            (
                DemandResource::SchedulerCpu,
                self.scheduler_cpu_ns_per_second,
                capacity.scheduler_cpu_ns_per_second,
            ),
            (
                DemandResource::EvaluatorFuel,
                self.evaluator_fuel_per_second,
                capacity.evaluator_fuel_per_second,
            ),
            (
                DemandResource::Writes,
                self.writes_per_second,
                capacity.writes_per_second,
            ),
            (
                DemandResource::Effects,
                self.effects_per_second,
                capacity.effects_per_second,
            ),
            (
                DemandResource::TraceBytes,
                self.trace_bytes_per_second,
                capacity.trace_bytes_per_second,
            ),
        ];
        checks
            .into_iter()
            .filter_map(|(resource, demand, limit)| (demand > limit).then_some(resource))
    }
}

/// Maximum aggregate rate on each resource checked during admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleDemandCapacity {
    pub semantic_ticks_per_second: RationalRate,
    pub timer_wakes_per_second: RationalRate,
    pub scheduler_cpu_ns_per_second: RationalRate,
    pub evaluator_fuel_per_second: RationalRate,
    pub writes_per_second: RationalRate,
    pub effects_per_second: RationalRate,
    pub trace_bytes_per_second: RationalRate,
}

/// Resource whose estimated rate can exceed the granted capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DemandResource {
    SemanticTicks,
    TimerWakes,
    SchedulerCpu,
    EvaluatorFuel,
    Writes,
    Effects,
    TraceBytes,
}

fn checked_rate_product(
    rate: RationalRate,
    multiplier: u64,
    label: &str,
) -> Result<RationalRate, KarmaBoundaryError> {
    rate.checked_mul_u64(multiplier)
        .ok_or_else(|| KarmaBoundaryError::invalid_input(format!("{label} overflowed")))
}

fn checked_rate_sum(
    left: RationalRate,
    right: RationalRate,
    label: &str,
) -> Result<RationalRate, KarmaBoundaryError> {
    left.checked_add(right)
        .ok_or_else(|| KarmaBoundaryError::invalid_input(format!("{label} overflowed")))
}
