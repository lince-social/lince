// non-lean-yet
// Scope: fn utc_provider, fn frequency_from_cadence.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::missing_panics_doc] State the panic conditions for exported functions. Sites
//   24.
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 16, 68, 70, 74.
// - [Clippy: clippy::expect_used] Check the stated invariant and handle reachable failures. Sites
//   51, 91.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 12, 24.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 12, 24.

use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU32,
};

use super::{
    ArtifactTimeZoneProvider, Cadence, CadenceAst, CadenceStepAst, CivilDateTime, DurationBinding,
    DurationMs, FoldPolicy, FrequencyAst, FrequencyCadenceAst, FrequencySchema, FrequencyTimerAst,
    GapPolicy, InactiveGapPolicy, KarmaBoundaryError, MissedPolicy, OverloadPolicy,
    PositiveIntegerBinding, RephasePolicy, Slug, TimeZoneArtifact, TimeZoneDefinition, TimeZoneId,
    TimeZoneProvider, TimestampMs, TzdbVersion, UtcOffsetSegment,
};

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

pub fn frequency_from_cadence(
    slug: Slug,
    purpose: String,
    cadence: &Cadence,
    anchor: TimestampMs,
) -> Result<FrequencyAst, KarmaBoundaryError> {
    let step = cadence.every;
    let component =
        |value| NonZeroU32::new(value).map(|value| PositiveIntegerBinding::Literal { value });
    let civil = chrono::DateTime::from_timestamp_millis(anchor.as_millis())
        .ok_or_else(|| KarmaBoundaryError::invalid_input("frequency anchor is out of range"))?
        .naive_utc();
    Ok(FrequencyAst {
        schema: FrequencySchema::V1,
        slug,
        purpose,
        tags: BTreeSet::new(),
        parameters: BTreeMap::new(),
        cadence: if step.calendar_months() == Some(0)
            && cadence.land_on.is_none()
            && cadence.bound == super::CadenceBound::Unbounded
            && step
                .fixed_milliseconds()
                .is_some_and(|milliseconds| milliseconds > 0)
        {
            FrequencyCadenceAst::Elapsed {
                interval: DurationBinding::Literal {
                    value: DurationMs::new(step.fixed_milliseconds().expect("checked interval")),
                },
                anchor,
            }
        } else {
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
                    bound: cadence.bound.clone(),
                },
                anchor: CivilDateTime::from_naive(civil)?,
                timezone: TimeZoneId::new("UTC")?,
                tzdb: utc_provider()?.revision().clone(),
                gap: GapPolicy::Pause,
                fold: FoldPolicy::First,
            }
        },
        timer: FrequencyTimerAst {
            required_resolution: DurationBinding::Literal {
                value: DurationMs::new(10),
            },
            max_lateness: DurationBinding::Literal {
                value: DurationMs::new(60_000),
            },
            coalesce_window: DurationBinding::Literal {
                value: DurationMs::ZERO,
            },
        },
        missed: MissedPolicy::Replay {
            max: NonZeroU32::new(64).expect("nonzero replay limit"),
        },
        inactive_gap: InactiveGapPolicy::SkipToNextAnchor,
        rephase: RephasePolicy::PreserveAnchor,
        overload: OverloadPolicy::RejectActivation,
    })
}
