use std::{collections::BTreeMap, fmt, num::NonZeroU32};

use serde::{Deserialize, Serialize};

use super::{
    Cadence, CadenceBound, CadenceStep, CalendarSchedule, CanonicalHash, CivilDateTime, DurationMs,
    ElapsedSchedule, FailurePath, FoldPolicy, GapPolicy, InactiveGapPolicy, InvalidDay, LocalId,
    MissedPolicy, OverloadPolicy, RephasePolicy, Slug, TimeZoneId, TimerPolicy, TimestampMs,
    TzdbRevision, WeekdaySet, canonical_hash,
};

pub const FREQUENCY_REVISION_HASH_DOMAIN: &str = "karma.frequency-revision.v1";
pub const FREQUENCY_PARAMETER_HASH_DOMAIN: &str = "karma.frequency-parameters.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FrequencySchema {
    #[serde(rename = "karma.frequency.v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
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

impl FrequencyParameterDefinition {
    fn validate(&self, path: &str) -> Result<(), FrequencyCompileError> {
        match self {
            Self::Duration {
                default,
                minimum,
                maximum,
            } => {
                if minimum.get() < 0 || minimum > default || default > maximum {
                    Err(FrequencyCompileError::new(
                        FrequencyCompileErrorKind::InvalidParameterRange,
                        path,
                        "duration parameter requires 0 <= minimum <= default <= maximum",
                    ))
                } else {
                    Ok(())
                }
            }
            Self::PositiveInteger {
                default,
                minimum,
                maximum,
            } => {
                if minimum > default || default > maximum {
                    Err(FrequencyCompileError::new(
                        FrequencyCompileErrorKind::InvalidParameterRange,
                        path,
                        "positive-integer parameter requires minimum <= default <= maximum",
                    ))
                } else {
                    Ok(())
                }
            }
        }
    }

    fn default_value(&self) -> FrequencyParameterValue {
        match self {
            Self::Duration { default, .. } => FrequencyParameterValue::Duration { value: *default },
            Self::PositiveInteger { default, .. } => {
                FrequencyParameterValue::PositiveInteger { value: *default }
            }
        }
    }

    fn accepts(&self, value: &FrequencyParameterValue) -> bool {
        match (self, value) {
            (
                Self::Duration {
                    minimum, maximum, ..
                },
                FrequencyParameterValue::Duration { value },
            ) => minimum <= value && value <= maximum,
            (
                Self::PositiveInteger {
                    minimum, maximum, ..
                },
                FrequencyParameterValue::PositiveInteger { value },
            ) => minimum <= value && value <= maximum,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FrequencyParameterValue {
    Duration { value: DurationMs },
    PositiveInteger { value: NonZeroU32 },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum DurationBinding {
    Literal { value: DurationMs },
    Parameter { parameter: LocalId },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PositiveIntegerBinding {
    Literal { value: NonZeroU32 },
    Parameter { parameter: LocalId },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrequencyTimerAst {
    pub required_resolution: DurationBinding,
    pub max_lateness: DurationBinding,
    pub coalesce_window: DurationBinding,
}

/// A compound step as authored: a sum of components, any of which may be a
/// bound parameter. Absent means zero, which is why every field is optional
/// rather than defaulted — an omitted component and a component set to zero are
/// the same statement and should serialize the same way.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
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

impl CadenceStepAst {
    /// Every component, largest first, paired with the path a compile error
    /// should point at. Ordering is the authored ordering, so a message about a
    /// bad step reads in the same direction the person typed it.
    fn components(&self) -> [(&'static str, &Option<PositiveIntegerBinding>); 8] {
        [
            ("years", &self.years),
            ("months", &self.months),
            ("weeks", &self.weeks),
            ("days", &self.days),
            ("hours", &self.hours),
            ("minutes", &self.minutes),
            ("seconds", &self.seconds),
            ("milliseconds", &self.milliseconds),
        ]
    }
}

/// A schedule as authored, before its parameters are bound.
///
/// The shape mirrors [`Cadence`] exactly, one layer up: every component may be
/// a literal or a named parameter, and everything else is already the compiled
/// vocabulary. There is no second set of rule shapes here, because there is no
/// second idea of what a schedule is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrequencyAst {
    pub schema: FrequencySchema,
    pub slug: Slug,
    pub purpose: String,
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub tags: std::collections::BTreeSet<Slug>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledFrequency {
    pub revision_hash: CanonicalHash,
    pub effective_parameter_hash: CanonicalHash,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrequencyCompileError {
    pub kind: FrequencyCompileErrorKind,
    pub path: FailurePath,
    pub message: String,
}

impl FrequencyCompileError {
    fn new(kind: FrequencyCompileErrorKind, path: &str, message: impl Into<String>) -> Self {
        Self {
            kind,
            path: FailurePath::new(path).expect("internal Frequency paths are valid JSON pointers"),
            message: message.into(),
        }
    }
}

impl fmt::Display for FrequencyCompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at {}: {}",
            serde_json::to_value(self.kind)
                .expect("closed enum serialization cannot fail")
                .as_str()
                .expect("compile error kind serializes as a string"),
            self.path,
            self.message
        )
    }
}

impl std::error::Error for FrequencyCompileError {}

impl FrequencyAst {
    pub fn compile(
        &self,
        overrides: &BTreeMap<LocalId, FrequencyParameterValue>,
    ) -> Result<CompiledFrequency, FrequencyCompileError> {
        self.validate_definition()?;
        let effective_parameters = self.effective_parameters(overrides)?;
        let timer = self.compile_timer(&effective_parameters)?;
        let schedule = match &self.cadence {
            FrequencyCadenceAst::Elapsed { interval, anchor } => {
                let interval =
                    self.resolve_duration(interval, &effective_parameters, "/cadence/interval")?;
                let interval_ms = u64::try_from(interval.get()).map_err(|_| {
                    FrequencyCompileError::new(
                        FrequencyCompileErrorKind::InvalidSchedule,
                        "/cadence/interval",
                        "elapsed interval must be at least 1ms",
                    )
                })?;
                let schedule = ElapsedSchedule::new(
                    *anchor,
                    interval_ms,
                    timer,
                    self.missed,
                    self.inactive_gap,
                    self.rephase,
                    self.overload,
                )
                .map_err(|error| {
                    FrequencyCompileError::new(
                        FrequencyCompileErrorKind::InvalidSchedule,
                        "/cadence",
                        error.to_string(),
                    )
                })?;
                CompiledSchedule::Elapsed { schedule }
            }
            FrequencyCadenceAst::Calendar {
                cadence,
                anchor,
                timezone,
                tzdb,
                gap,
                fold,
            } => {
                let cadence = self.compile_cadence(cadence, &effective_parameters)?;
                CompiledSchedule::Calendar {
                    schedule: CalendarSchedule {
                        anchor: *anchor,
                        cadence,
                        timezone: timezone.clone(),
                        tzdb: tzdb.clone(),
                        gap: *gap,
                        fold: *fold,
                        timer,
                        missed: self.missed,
                        inactive_gap: self.inactive_gap,
                        rephase: self.rephase,
                        overload: self.overload,
                    },
                }
            }
        };
        let revision_hash =
            canonical_hash(FREQUENCY_REVISION_HASH_DOMAIN, self).map_err(|error| {
                FrequencyCompileError::new(
                    FrequencyCompileErrorKind::CanonicalizationFailed,
                    "",
                    error.to_string(),
                )
            })?;
        let effective_parameter_hash =
            canonical_hash(FREQUENCY_PARAMETER_HASH_DOMAIN, &effective_parameters).map_err(
                |error| {
                    FrequencyCompileError::new(
                        FrequencyCompileErrorKind::CanonicalizationFailed,
                        "/parameters",
                        error.to_string(),
                    )
                },
            )?;
        Ok(CompiledFrequency {
            revision_hash,
            effective_parameter_hash,
            effective_parameters,
            schedule,
        })
    }

    fn validate_definition(&self) -> Result<(), FrequencyCompileError> {
        if self.purpose.trim().is_empty() || self.purpose.chars().any(char::is_control) {
            return Err(FrequencyCompileError::new(
                FrequencyCompileErrorKind::InvalidPurpose,
                "/purpose",
                "frequency purpose must be non-empty and contain no control characters",
            ));
        }
        for (id, definition) in &self.parameters {
            definition.validate(&format!("/parameters/{}", id.as_str()))?;
        }
        self.validate_bindings()
    }

    fn validate_bindings(&self) -> Result<(), FrequencyCompileError> {
        let (resolution_min, resolution_max) = self.duration_binding_range(
            &self.timer.required_resolution,
            "/timer/required_resolution",
        )?;
        let (lateness_min, lateness_max) =
            self.duration_binding_range(&self.timer.max_lateness, "/timer/max_lateness")?;
        let (coalesce_min, coalesce_max) =
            self.duration_binding_range(&self.timer.coalesce_window, "/timer/coalesce_window")?;
        validate_timer_range(
            resolution_min,
            resolution_max,
            "/timer/required_resolution",
            true,
        )?;
        validate_timer_range(lateness_min, lateness_max, "/timer/max_lateness", false)?;
        validate_timer_range(coalesce_min, coalesce_max, "/timer/coalesce_window", false)?;
        if self.timer.coalesce_window != self.timer.max_lateness
            && coalesce_max.get() > lateness_min.get()
        {
            return Err(FrequencyCompileError::new(
                FrequencyCompileErrorKind::InvalidBinding,
                "/timer/coalesce_window",
                "independent coalescing range maximum cannot exceed lateness range minimum",
            ));
        }

        match &self.cadence {
            FrequencyCadenceAst::Elapsed { interval, .. } => {
                let (minimum, _) = self.duration_binding_range(interval, "/cadence/interval")?;
                if minimum.get() < 1 {
                    return Err(FrequencyCompileError::new(
                        FrequencyCompileErrorKind::InvalidBinding,
                        "/cadence/interval",
                        "elapsed interval binding range must remain at least 1ms",
                    ));
                }
            }
            FrequencyCadenceAst::Calendar { cadence, .. } => {
                for (name, binding) in cadence.every.components() {
                    if let Some(binding) = binding {
                        self.validate_positive_binding(binding, &format!("/cadence/every/{name}"))?;
                    }
                }
            }
        }
        Ok(())
    }

    fn effective_parameters(
        &self,
        overrides: &BTreeMap<LocalId, FrequencyParameterValue>,
    ) -> Result<BTreeMap<LocalId, FrequencyParameterValue>, FrequencyCompileError> {
        for id in overrides.keys() {
            if !self.parameters.contains_key(id) {
                return Err(FrequencyCompileError::new(
                    FrequencyCompileErrorKind::UnknownParameter,
                    &format!("/parameters/{}", id.as_str()),
                    "override names an undeclared Frequency parameter",
                ));
            }
        }
        let mut effective = BTreeMap::new();
        for (id, definition) in &self.parameters {
            let value = overrides
                .get(id)
                .cloned()
                .unwrap_or_else(|| definition.default_value());
            let same_type = matches!(
                (definition, &value),
                (
                    FrequencyParameterDefinition::Duration { .. },
                    FrequencyParameterValue::Duration { .. }
                ) | (
                    FrequencyParameterDefinition::PositiveInteger { .. },
                    FrequencyParameterValue::PositiveInteger { .. }
                )
            );
            if !same_type {
                return Err(FrequencyCompileError::new(
                    FrequencyCompileErrorKind::ParameterTypeMismatch,
                    &format!("/parameters/{}", id.as_str()),
                    "override type does not match Frequency parameter definition",
                ));
            }
            if !definition.accepts(&value) {
                return Err(FrequencyCompileError::new(
                    FrequencyCompileErrorKind::ParameterOutOfRange,
                    &format!("/parameters/{}", id.as_str()),
                    "override is outside the inclusive Frequency parameter range",
                ));
            }
            effective.insert(id.clone(), value);
        }
        Ok(effective)
    }

    fn compile_timer(
        &self,
        parameters: &BTreeMap<LocalId, FrequencyParameterValue>,
    ) -> Result<TimerPolicy, FrequencyCompileError> {
        let resolution = self.resolve_duration(
            &self.timer.required_resolution,
            parameters,
            "/timer/required_resolution",
        )?;
        let lateness =
            self.resolve_duration(&self.timer.max_lateness, parameters, "/timer/max_lateness")?;
        let coalesce = self.resolve_duration(
            &self.timer.coalesce_window,
            parameters,
            "/timer/coalesce_window",
        )?;
        let to_u32 = |value: DurationMs, path: &str| {
            u32::try_from(value.get()).map_err(|_| {
                FrequencyCompileError::new(
                    FrequencyCompileErrorKind::InvalidSchedule,
                    path,
                    "timer duration is outside the u32 millisecond range",
                )
            })
        };
        TimerPolicy::new(
            to_u32(resolution, "/timer/required_resolution")?,
            to_u32(lateness, "/timer/max_lateness")?,
            to_u32(coalesce, "/timer/coalesce_window")?,
        )
        .map_err(|error| {
            FrequencyCompileError::new(
                FrequencyCompileErrorKind::InvalidSchedule,
                "/timer",
                error.to_string(),
            )
        })
    }

    /// Bind every parameter and hand back the same [`Cadence`] a read path uses.
    ///
    /// The whole compile step is now this: resolve numbers, copy the rest. There
    /// is no translation between an authored shape and a runtime shape, because
    /// there is only one shape.
    fn compile_cadence(
        &self,
        cadence: &CadenceAst,
        parameters: &BTreeMap<LocalId, FrequencyParameterValue>,
    ) -> Result<Cadence, FrequencyCompileError> {
        let mut resolved = [0u32; 8];
        for (slot, (name, binding)) in resolved.iter_mut().zip(cadence.every.components()) {
            if let Some(binding) = binding {
                *slot = self
                    .resolve_positive_integer(
                        binding,
                        parameters,
                        &format!("/cadence/every/{name}"),
                    )?
                    .get();
            }
        }
        let compiled = Cadence {
            every: CadenceStep {
                years: resolved[0],
                months: resolved[1],
                weeks: resolved[2],
                days: resolved[3],
                hours: resolved[4],
                minutes: resolved[5],
                seconds: resolved[6],
                milliseconds: resolved[7],
            },
            land_on: cadence.land_on.clone(),
            invalid_day: cadence.invalid_day,
            bound: cadence.bound,
        };
        // A step that does not advance is caught here rather than at the first
        // wake-up, where it would present as a schedule that fires forever on
        // one instant.
        compiled.validate().map_err(|error| {
            FrequencyCompileError::new(
                FrequencyCompileErrorKind::InvalidBinding,
                "/cadence/every",
                error.to_string(),
            )
        })?;
        Ok(compiled)
    }

    fn resolve_duration(
        &self,
        binding: &DurationBinding,
        parameters: &BTreeMap<LocalId, FrequencyParameterValue>,
        path: &str,
    ) -> Result<DurationMs, FrequencyCompileError> {
        match binding {
            DurationBinding::Literal { value } => Ok(*value),
            DurationBinding::Parameter { parameter } => match parameters.get(parameter) {
                Some(FrequencyParameterValue::Duration { value }) => Ok(*value),
                Some(FrequencyParameterValue::PositiveInteger { .. }) => {
                    Err(FrequencyCompileError::new(
                        FrequencyCompileErrorKind::ParameterTypeMismatch,
                        path,
                        "duration binding names a positive-integer parameter",
                    ))
                }
                None => Err(FrequencyCompileError::new(
                    FrequencyCompileErrorKind::UnknownParameter,
                    path,
                    format!("duration binding names undeclared parameter {parameter:?}"),
                )),
            },
        }
    }

    fn resolve_positive_integer(
        &self,
        binding: &PositiveIntegerBinding,
        parameters: &BTreeMap<LocalId, FrequencyParameterValue>,
        path: &str,
    ) -> Result<NonZeroU32, FrequencyCompileError> {
        match binding {
            PositiveIntegerBinding::Literal { value } => Ok(*value),
            PositiveIntegerBinding::Parameter { parameter } => match parameters.get(parameter) {
                Some(FrequencyParameterValue::PositiveInteger { value }) => Ok(*value),
                Some(FrequencyParameterValue::Duration { .. }) => Err(FrequencyCompileError::new(
                    FrequencyCompileErrorKind::ParameterTypeMismatch,
                    path,
                    "positive-integer binding names a duration parameter",
                )),
                None => Err(FrequencyCompileError::new(
                    FrequencyCompileErrorKind::UnknownParameter,
                    path,
                    format!("positive-integer binding names undeclared parameter {parameter:?}"),
                )),
            },
        }
    }

    fn duration_binding_range(
        &self,
        binding: &DurationBinding,
        path: &str,
    ) -> Result<(DurationMs, DurationMs), FrequencyCompileError> {
        match binding {
            DurationBinding::Literal { value } => Ok((*value, *value)),
            DurationBinding::Parameter { parameter } => match self.parameters.get(parameter) {
                Some(FrequencyParameterDefinition::Duration {
                    minimum, maximum, ..
                }) => Ok((*minimum, *maximum)),
                Some(FrequencyParameterDefinition::PositiveInteger { .. }) => {
                    Err(FrequencyCompileError::new(
                        FrequencyCompileErrorKind::ParameterTypeMismatch,
                        path,
                        "duration binding names a positive-integer parameter",
                    ))
                }
                None => Err(FrequencyCompileError::new(
                    FrequencyCompileErrorKind::UnknownParameter,
                    path,
                    format!("duration binding names undeclared parameter {parameter:?}"),
                )),
            },
        }
    }

    fn validate_positive_binding(
        &self,
        binding: &PositiveIntegerBinding,
        path: &str,
    ) -> Result<(), FrequencyCompileError> {
        match binding {
            PositiveIntegerBinding::Literal { .. } => Ok(()),
            PositiveIntegerBinding::Parameter { parameter } => {
                match self.parameters.get(parameter) {
                    Some(FrequencyParameterDefinition::PositiveInteger { .. }) => Ok(()),
                    Some(FrequencyParameterDefinition::Duration { .. }) => {
                        Err(FrequencyCompileError::new(
                            FrequencyCompileErrorKind::ParameterTypeMismatch,
                            path,
                            "positive-integer binding names a duration parameter",
                        ))
                    }
                    None => Err(FrequencyCompileError::new(
                        FrequencyCompileErrorKind::UnknownParameter,
                        path,
                        format!(
                            "positive-integer binding names undeclared parameter {parameter:?}"
                        ),
                    )),
                }
            }
        }
    }
}

fn validate_timer_range(
    minimum: DurationMs,
    maximum: DurationMs,
    path: &str,
    strictly_positive: bool,
) -> Result<(), FrequencyCompileError> {
    let valid_minimum = if strictly_positive {
        minimum.get() >= 1
    } else {
        minimum.get() >= 0
    };
    if !valid_minimum || maximum.get() > i64::from(u32::MAX) {
        Err(FrequencyCompileError::new(
            FrequencyCompileErrorKind::InvalidBinding,
            path,
            if strictly_positive {
                "timer binding range must remain within 1..=u32::MAX milliseconds"
            } else {
                "timer binding range must remain within 0..=u32::MAX milliseconds"
            },
        ))
    } else {
        Ok(())
    }
}
