//! Frequency wire definitions and compiled schedule results.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    num::NonZeroU32,
};

use serde::{Deserialize, Serialize};

use crate::{
    CadenceBound, CalendarSchedule, CanonicalHash, CivilDateTime, DurationMs, ElapsedSchedule,
    FailurePath, FoldPolicy, GapPolicy, InactiveGapPolicy, InvalidDay, LocalId, MissedPolicy,
    OverloadPolicy, RephasePolicy, Slug, TimeZoneId, TimestampMs, TzdbRevision, WeekdaySet,
};

mod bindings;
mod compile;

type ParameterDefinitions = BTreeMap<LocalId, FrequencyParameterDefinition>;
type ParameterValues = BTreeMap<LocalId, FrequencyParameterValue>;

pub const FREQUENCY_REVISION_HASH_DOMAIN: &str = "karma.frequency-revision.v1";
pub const FREQUENCY_PARAMETER_HASH_DOMAIN: &str = "karma.frequency-parameters.v1";

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
pub enum FrequencySchema {
    #[serde(rename = "karma.frequency.v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum FrequencyParameterDefinition {
    Duration {
        default: DurationMs,
        minimum: DurationMs,
        maximum: DurationMs,
    },
    PositiveInteger {
        default: NonZeroU32,
        minimum: NonZeroU32,
        maximum: NonZeroU32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum FrequencyParameterValue {
    Duration { value: DurationMs },
    PositiveInteger { value: NonZeroU32 },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum DurationBinding {
    Literal { value: DurationMs },
    Parameter { parameter: LocalId },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
pub enum PositiveIntegerBinding {
    Literal { value: NonZeroU32 },
    Parameter { parameter: LocalId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FrequencyTimerAst {
    pub required_resolution: DurationBinding,
    pub max_lateness: DurationBinding,
    pub coalesce_window: DurationBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
#[derive(schemars::JsonSchema)]
pub struct CadenceStepAst {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub years: Option<PositiveIntegerBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub months: Option<PositiveIntegerBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weeks: Option<PositiveIntegerBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days: Option<PositiveIntegerBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hours: Option<PositiveIntegerBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minutes: Option<PositiveIntegerBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seconds: Option<PositiveIntegerBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub milliseconds: Option<PositiveIntegerBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CadenceAst {
    pub every: CadenceStepAst,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub land_on: Option<WeekdaySet>,
    #[serde(default)]
    pub invalid_day: InvalidDay,
    #[serde(default)]
    pub bound: CadenceBound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[derive(schemars::JsonSchema)]
#[expect(
    clippy::large_enum_variant,
    reason = "Boxing the calendar cadence would change public construction and match patterns"
)]
pub enum FrequencyCadenceAst {
    Elapsed {
        interval: DurationBinding,
        anchor: TimestampMs,
    },
    Calendar {
        cadence: CadenceAst,
        anchor: CivilDateTime,
        timezone: TimeZoneId,
        tzdb: TzdbRevision,
        gap: GapPolicy,
        fold: FoldPolicy,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FrequencyAst {
    pub schema: FrequencySchema,
    pub slug: Slug,
    pub purpose: String,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub tags: BTreeSet<Slug>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<LocalId, FrequencyParameterDefinition>,
    pub cadence: FrequencyCadenceAst,
    pub timer: FrequencyTimerAst,
    pub missed: MissedPolicy,
    pub inactive_gap: InactiveGapPolicy,
    pub rephase: RephasePolicy,
    pub overload: OverloadPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CompiledSchedule {
    Elapsed { schedule: ElapsedSchedule },
    Calendar { schedule: CalendarSchedule },
}

/// A compiled schedule and the identities of the definition and bound parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledFrequency {
    /// Hash of the authored definition, including declared parameter defaults.
    pub revision_hash: CanonicalHash,
    /// Hash of the effective values after applying overrides to the defaults.
    pub effective_parameter_hash: CanonicalHash,
    /// Value bound to each declared parameter for this compilation.
    pub effective_parameters: BTreeMap<LocalId, FrequencyParameterValue>,
    pub schedule: CompiledSchedule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FrequencyCompileErrorKind {
    InvalidPurpose,
    InvalidParameterRange,
    UnknownParameter,
    ParameterTypeMismatch,
    ParameterOutOfRange,
    InvalidBinding,
    InvalidSchedule,
    CanonicalizationFailed,
}

/// A compilation failure with a stable category and a path into the definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrequencyCompileError {
    pub kind: FrequencyCompileErrorKind,
    /// JSON pointer to the invalid value, or the empty pointer for the root.
    pub path: FailurePath,
    pub message: String,
}

impl FrequencyCompileError {
    fn new(
        kind: FrequencyCompileErrorKind,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            // Paths are static pointers or end in an ASCII LocalId.
            path: FailurePath::new(path).expect("internal Frequency paths are valid JSON pointers"),
            message: message.into(),
        }
    }
}

impl fmt::Display for FrequencyCompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        use FrequencyCompileErrorKind::*;

        let kind = match self.kind {
            InvalidPurpose => "invalid-purpose",
            InvalidParameterRange => "invalid-parameter-range",
            UnknownParameter => "unknown-parameter",
            ParameterTypeMismatch => "parameter-type-mismatch",
            ParameterOutOfRange => "parameter-out-of-range",
            InvalidBinding => "invalid-binding",
            InvalidSchedule => "invalid-schedule",
            CanonicalizationFailed => "canonicalization-failed",
        };
        write!(formatter, "{kind} at {}: {}", self.path, self.message)
    }
}

impl std::error::Error for FrequencyCompileError {}
