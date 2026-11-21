//! Builds typed frequencies from a Cadence and the bundled fixed UTC rules.

use std::{collections::BTreeMap, num::NonZeroU32};

use chrono::DateTime;

use crate::{
    ArtifactTimeZoneProvider, Cadence, CadenceAst, CadenceBound, CadenceStepAst, CivilDateTime,
    DurationBinding, DurationMs, FoldPolicy, FrequencyAst, FrequencyCadenceAst, FrequencySchema,
    FrequencyTimerAst, GapPolicy, InactiveGapPolicy, KarmaBoundaryError, MissedPolicy,
    OverloadPolicy, PositiveIntegerBinding, RephasePolicy, Slug, TimeZoneArtifact,
    TimeZoneDefinition, TimeZoneId, TimeZoneProvider, TimestampMs, TzdbVersion, UtcOffsetSegment,
};

const REPLAY_LIMIT: NonZeroU32 = NonZeroU32::new(64).expect("nonzero replay limit");

/// Builds the versioned timezone provider for UTC and its supported aliases.
///
/// # Errors
///
/// Returns a boundary error if the built-in timezone rules fail validation.
pub fn utc_provider() -> Result<ArtifactTimeZoneProvider, KarmaBoundaryError> {
    let fixed = TimeZoneDefinition::new(vec![UtcOffsetSegment::new(None, None, 0)?])?;
    let zones = ["Etc/GMT", "Etc/UTC", "GMT", "UTC"]
        .into_iter()
        .map(|name| Ok((TimeZoneId::new(name)?, fixed.clone())))
        .collect::<Result<BTreeMap<_, _>, KarmaBoundaryError>>()?;
    ArtifactTimeZoneProvider::from_artifact(TimeZoneArtifact::new(
        TzdbVersion::new("lince-utc.1")?,
        zones,
    )?)
}

/// Builds a Frequency with the standard timer and catch-up policy.
///
/// Positive, unbounded fixed intervals use elapsed time. Other cadences use
/// calendar time in UTC and retain the given landing and end conditions.
///
/// # Errors
///
/// Returns a boundary error if calendar anchor conversion or UTC setup fails.
pub fn frequency_from_cadence(
    slug: Slug,
    purpose: String,
    cadence: &Cadence,
    anchor: TimestampMs,
) -> Result<FrequencyAst, KarmaBoundaryError> {
    let step = cadence.every;
    let literal_ms = |ms| DurationBinding::Literal {
        value: DurationMs::new(ms),
    };
    let cadence = if step.calendar_months() == Some(0)
        && cadence.land_on.is_none()
        && cadence.bound == CadenceBound::Unbounded
        && let Some(interval) = step.fixed_milliseconds()
        && interval > 0
    {
        FrequencyCadenceAst::Elapsed {
            interval: literal_ms(interval),
            anchor,
        }
    } else {
        let civil = DateTime::from_timestamp_millis(anchor.as_millis())
            .ok_or_else(|| KarmaBoundaryError::invalid_input("frequency anchor is out of range"))?
            .naive_utc();
        let component =
            |value| NonZeroU32::new(value).map(|value| PositiveIntegerBinding::Literal { value });
        FrequencyCadenceAst::Calendar {
            cadence: CadenceAst {
                every: CadenceStepAst {
                    years: component(step.years),
                    months: component(step.months),
                    weeks: component(step.weeks),
                    days: component(step.days),
                    hours: component(step.hours),
                    minutes: component(step.minutes),
                    seconds: component(step.seconds),
                    milliseconds: component(step.milliseconds),
                },
                land_on: cadence.land_on.clone(),
                invalid_day: cadence.invalid_day,
                bound: cadence.bound,
            },
            anchor: CivilDateTime::from_naive(civil)?,
            timezone: TimeZoneId::new("UTC")?,
            tzdb: utc_provider()?.revision().clone(),
            gap: GapPolicy::Pause,
            fold: FoldPolicy::First,
        }
    };
    Ok(FrequencyAst {
        schema: FrequencySchema::V1,
        slug,
        purpose,
        tags: Default::default(),
        parameters: BTreeMap::new(),
        cadence,
        timer: FrequencyTimerAst {
            required_resolution: literal_ms(10),
            max_lateness: literal_ms(60_000),
            coalesce_window: literal_ms(0),
        },
        missed: MissedPolicy::Replay { max: REPLAY_LIMIT },
        inactive_gap: InactiveGapPolicy::SkipToNextAnchor,
        rephase: RephasePolicy::PreserveAnchor,
        overload: OverloadPolicy::RejectActivation,
    })
}
