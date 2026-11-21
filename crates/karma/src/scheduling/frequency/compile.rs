//! Validation and compilation of elapsed and calendar schedules.

use std::collections::BTreeMap;

use serde::Serialize;

use super::{
    CadenceAst, CadenceStepAst, CompiledFrequency, CompiledSchedule,
    FREQUENCY_PARAMETER_HASH_DOMAIN, FREQUENCY_REVISION_HASH_DOMAIN, FrequencyAst,
    FrequencyCadenceAst, FrequencyCompileError as CompileError,
    FrequencyCompileErrorKind::{
        CanonicalizationFailed, InvalidBinding, InvalidPurpose, InvalidSchedule,
    },
    FrequencyParameterValue, FrequencyTimerAst, ParameterDefinitions, ParameterValues,
    PositiveIntegerBinding,
    bindings::effective_parameters,
};
use crate::{
    Cadence, CadenceStep, CalendarSchedule, CanonicalHash, DurationMs, ElapsedSchedule, LocalId,
    TimerPolicy, canonical_hash,
};

const RESOLUTION: &str = "/timer/required_resolution";
const LATENESS: &str = "/timer/max_lateness";
const COALESCE: &str = "/timer/coalesce_window";
const INTERVAL: &str = "/cadence/interval";

impl FrequencyAst {
    /// Compiles the Frequency with the supplied overrides and defaults for all
    /// other parameters. An override must match its declared type and range.
    ///
    /// # Errors
    ///
    /// Returns the first invalid definition, binding, override, or schedule
    /// error, or a failure to hash the compiled inputs.
    pub fn compile(
        &self,
        overrides: &BTreeMap<LocalId, FrequencyParameterValue>,
    ) -> Result<CompiledFrequency, super::FrequencyCompileError> {
        self.validate_definition()?;
        let effective_parameters = effective_parameters(&self.parameters, overrides)?;
        let timer = self.timer.compile(&effective_parameters)?;
        let schedule = match &self.cadence {
            FrequencyCadenceAst::Elapsed { interval, anchor } => {
                let interval = interval.resolve(&effective_parameters, INTERVAL)?;
                let interval_ms = u64::try_from(interval.get()).map_err(|_| {
                    CompileError::new(
                        InvalidSchedule,
                        INTERVAL,
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
                    CompileError::new(InvalidSchedule, "/cadence", error.to_string())
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
                let cadence = cadence.compile(&effective_parameters)?;
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
        let revision_hash = hash(FREQUENCY_REVISION_HASH_DOMAIN, "", self)?;
        let effective_parameter_hash = hash(
            FREQUENCY_PARAMETER_HASH_DOMAIN,
            "/parameters",
            &effective_parameters,
        )?;
        Ok(CompiledFrequency {
            revision_hash,
            effective_parameter_hash,
            effective_parameters,
            schedule,
        })
    }

    fn validate_definition(&self) -> Result<(), CompileError> {
        if self.purpose.trim().is_empty() || self.purpose.chars().any(char::is_control) {
            return Err(CompileError::new(
                InvalidPurpose,
                "/purpose",
                "frequency purpose must be non-empty and contain no control characters",
            ));
        }
        for (id, definition) in &self.parameters {
            definition.validate(id)?;
        }
        self.timer.validate(&self.parameters)?;
        match &self.cadence {
            FrequencyCadenceAst::Elapsed { interval, .. } => {
                let (minimum, _) = interval.range(&self.parameters, INTERVAL)?;
                if minimum.get() < 1 {
                    return Err(CompileError::new(
                        InvalidBinding,
                        INTERVAL,
                        "elapsed interval binding range must remain at least 1ms",
                    ));
                }
            }
            FrequencyCadenceAst::Calendar { cadence, .. } => {
                cadence.every.validate(&self.parameters)?;
            }
        }
        Ok(())
    }
}

fn hash(domain: &str, path: &str, value: &impl Serialize) -> Result<CanonicalHash, CompileError> {
    canonical_hash(domain, value)
        .map_err(|error| CompileError::new(CanonicalizationFailed, path, error.to_string()))
}

impl FrequencyTimerAst {
    fn validate(&self, parameters: &ParameterDefinitions) -> Result<(), CompileError> {
        let resolution = self.required_resolution.range(parameters, RESOLUTION)?;
        let lateness = self.max_lateness.range(parameters, LATENESS)?;
        let coalesce = self.coalesce_window.range(parameters, COALESCE)?;
        validate_timer_range(resolution, RESOLUTION, 1)?;
        validate_timer_range(lateness, LATENESS, 0)?;
        validate_timer_range(coalesce, COALESCE, 0)?;
        if self.coalesce_window != self.max_lateness && coalesce.1 > lateness.0 {
            return Err(CompileError::new(
                InvalidBinding,
                COALESCE,
                "independent coalescing range maximum cannot exceed lateness range minimum",
            ));
        }
        Ok(())
    }

    fn compile(&self, parameters: &ParameterValues) -> Result<TimerPolicy, CompileError> {
        let resolution = self.required_resolution.resolve(parameters, RESOLUTION)?;
        let lateness = self.max_lateness.resolve(parameters, LATENESS)?;
        let coalesce = self.coalesce_window.resolve(parameters, COALESCE)?;
        TimerPolicy::new(
            timer_millis(resolution, RESOLUTION)?,
            timer_millis(lateness, LATENESS)?,
            timer_millis(coalesce, COALESCE)?,
        )
        .map_err(|error| CompileError::new(InvalidSchedule, "/timer", error.to_string()))
    }
}

fn timer_millis(value: DurationMs, path: &str) -> Result<u32, CompileError> {
    u32::try_from(value.get()).map_err(|_| {
        CompileError::new(
            InvalidSchedule,
            path,
            "timer duration is outside the u32 millisecond range",
        )
    })
}

fn validate_timer_range(
    (minimum, maximum): (DurationMs, DurationMs),
    path: &str,
    minimum_allowed: i64,
) -> Result<(), CompileError> {
    if minimum.get() < minimum_allowed || maximum.get() > i64::from(u32::MAX) {
        return Err(CompileError::new(
            InvalidBinding,
            path,
            format!(
                "timer binding range must remain within {minimum_allowed}..=u32::MAX milliseconds"
            ),
        ));
    }
    Ok(())
}

// The field names also define the diagnostic paths and their validation order.
macro_rules! cadence_step_fields {
    ($($field:ident),+ $(,)?) => {
        fn validate(&self, parameters: &ParameterDefinitions) -> Result<(), CompileError> {
            let Self { $($field),+ } = self;
            for (binding, path) in [$(($field, concat!("/cadence/every/", stringify!($field)))),+] {
                if let Some(binding) = binding {
                    binding.validate(parameters, path)?;
                }
            }
            Ok(())
        }

        fn compile(&self, parameters: &ParameterValues) -> Result<CadenceStep, CompileError> {
            let resolve = |binding: &Option<PositiveIntegerBinding>, path| match binding {
                Some(binding) => Ok(binding.resolve(parameters, path)?.get()),
                None => Ok(0),
            };
            Ok(CadenceStep {
                $($field: resolve(&self.$field, concat!("/cadence/every/", stringify!($field)))?,)+
            })
        }
    };
}

impl CadenceStepAst {
    cadence_step_fields!(
        years,
        months,
        weeks,
        days,
        hours,
        minutes,
        seconds,
        milliseconds
    );
}

impl CadenceAst {
    fn compile(&self, parameters: &ParameterValues) -> Result<Cadence, CompileError> {
        let cadence = Cadence {
            every: self.every.compile(parameters)?,
            land_on: self.land_on.clone(),
            invalid_day: self.invalid_day,
            bound: self.bound,
        };
        cadence.validate().map_err(|error| {
            CompileError::new(InvalidBinding, "/cadence/every", error.to_string())
        })?;
        Ok(cadence)
    }
}
