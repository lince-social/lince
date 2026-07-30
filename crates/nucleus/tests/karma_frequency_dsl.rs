use std::{collections::BTreeMap, num::NonZeroU32};

use nucleus::karma::{
    CadenceAst, CadenceBound, CadenceStepAst, CanonicalHash, CivilDateTime, CivilWeekday,
    CompiledSchedule, DslErrorKind, DurationBinding, DurationMs, FoldPolicy,
    FrequencyAst, FrequencyCadenceAst, FrequencyCompileErrorKind, FrequencyParameterDefinition,
    FrequencyParameterValue, FrequencySchema, FrequencyTimerAst, GapPolicy, InactiveGapPolicy,
    InvalidDay, LocalId, MissedPolicy, OverloadPolicy, PositiveIntegerBinding,
    RephasePolicy, Slug, TimeZoneId, TzdbRevision, TzdbVersion, WeekdaySet, canonical_hash,
    format_frequency, parse_frequency,
};

#[test]
fn elapsed_frequency_has_stable_text_and_compiles_symbolic_parameters() {
    let frequency = elapsed_frequency();
    let formatted = format_frequency(&frequency);
    assert_eq!(formatted, ELAPSED_CANONICAL);
    let parsed = parse_frequency(&formatted).unwrap();
    assert_eq!(parsed, frequency);
    assert_eq!(format_frequency(&parsed), formatted);

    let defaults = parsed.compile(&BTreeMap::new()).unwrap();
    let CompiledSchedule::Elapsed { schedule } = &defaults.schedule else {
        panic!("expected elapsed schedule");
    };
    assert_eq!(schedule.interval_ms(), 3);
    assert_eq!(schedule.rephase_policy(), RephasePolicy::PreserveAnchor);

    let overrides = BTreeMap::from([(
        id("interval"),
        FrequencyParameterValue::Duration {
            value: DurationMs::new(9),
        },
    )]);
    let tuned = parsed.compile(&overrides).unwrap();
    let CompiledSchedule::Elapsed { schedule } = &tuned.schedule else {
        panic!("expected elapsed schedule");
    };
    assert_eq!(schedule.interval_ms(), 9);
    assert_eq!(defaults.revision_hash, tuned.revision_hash);
    assert_ne!(
        defaults.effective_parameter_hash,
        tuned.effective_parameter_hash
    );
}

#[test]
fn calendar_daily_weekly_and_monthly_forms_round_trip_and_compile() {
    for frequency in [
        // The former `daily(...)`.
        calendar_frequency(cadence(step(
            "days",
            PositiveIntegerBinding::Parameter {
                parameter: id("calendar_step"),
            },
        ))),
        // The former `weekly(..., [mon, wed, fri], ...)`. A weekday set is now a
        // landing rule on a daily step, which is what makes it composable with
        // everything else rather than a shape of its own.
        calendar_frequency(CadenceAst {
            every: step("days", literal(1)),
            land_on: Some(
                WeekdaySet::new([
                    CivilWeekday::Friday,
                    CivilWeekday::Monday,
                    CivilWeekday::Wednesday,
                ])
                .unwrap(),
            ),
            invalid_day: InvalidDay::Clamp,
            bound: CadenceBound::Unbounded,
        }),
        // The former `monthly(..., day(31), ..., clamp-to-last-day)`. The day of
        // month comes from the anchor now.
        calendar_frequency(CadenceAst {
            every: step("months", literal(1)),
            land_on: None,
            invalid_day: InvalidDay::Clamp,
            bound: CadenceBound::Unbounded,
        }),
        // And the rule none of the three old shapes could say: a sum of
        // components, landing on a weekday, ending after a fixed count.
        calendar_frequency(CadenceAst {
            every: CadenceStepAst {
                months: Some(literal(1)),
                days: Some(literal(1)),
                seconds: Some(literal(1)),
                milliseconds: Some(literal(10)),
                ..Default::default()
            },
            land_on: Some(WeekdaySet::new([CivilWeekday::Friday]).unwrap()),
            invalid_day: InvalidDay::Skip,
            bound: CadenceBound::Count { occurrences: 12 },
        }),
    ] {
        let formatted = format_frequency(&frequency);
        let parsed = parse_frequency(&formatted).unwrap();
        assert_eq!(parsed, frequency);
        assert_eq!(format_frequency(&parsed), formatted);
        let compiled = parsed.compile(&BTreeMap::new()).unwrap();
        let CompiledSchedule::Calendar { schedule } = compiled.schedule else {
            panic!("expected calendar schedule");
        };
        assert_eq!(schedule.timezone.as_str(), "America/Sao_Paulo");
        assert_eq!(schedule.rephase, RephasePolicy::ImmediateIfOverdue);
    }
}

#[test]
fn frequency_comments_order_and_calendar_parameter_overrides_are_canonical() {
    let source = ELAPSED_CANONICAL
        .replace(
            "karma-frequency 1;",
            "# canonical format version\n karma-frequency 1 ; # ignored",
        )
        .replace(
            "  schema karma.frequency.v1;\n  purpose \"Sample the local sensor\";",
            "  purpose \"Sample the local sensor\";\n  schema karma.frequency.v1;",
        )
        .replace(
            "    resolution duration(1);\n    max-lateness duration(5);\n    coalesce-window duration(0);",
            "    coalesce-window duration(0);\n    resolution duration(1);\n    max-lateness duration(5);",
        );
    assert_eq!(
        format_frequency(&parse_frequency(&source).unwrap()),
        ELAPSED_CANONICAL
    );

    let daily = calendar_frequency(cadence(step(
        "days",
        PositiveIntegerBinding::Parameter {
            parameter: id("calendar_step"),
        },
    )));
    let compiled = daily
        .compile(&BTreeMap::from([(
            id("calendar_step"),
            FrequencyParameterValue::PositiveInteger { value: nonzero(3) },
        )]))
        .unwrap();
    let CompiledSchedule::Calendar { schedule } = compiled.schedule else {
        panic!("expected calendar schedule");
    };
    assert_eq!(schedule.cadence.every.days, 3);
}

#[test]
fn overrides_fail_for_unknown_type_and_range_without_changing_revision() {
    let frequency = elapsed_frequency();
    let unknown = BTreeMap::from([(
        id("ghost"),
        FrequencyParameterValue::Duration {
            value: DurationMs::new(3),
        },
    )]);
    assert_eq!(
        frequency.compile(&unknown).unwrap_err().kind,
        FrequencyCompileErrorKind::UnknownParameter
    );

    let wrong_type = BTreeMap::from([(
        id("interval"),
        FrequencyParameterValue::PositiveInteger { value: nonzero(3) },
    )]);
    assert_eq!(
        frequency.compile(&wrong_type).unwrap_err().kind,
        FrequencyCompileErrorKind::ParameterTypeMismatch
    );

    let out_of_range = BTreeMap::from([(
        id("interval"),
        FrequencyParameterValue::Duration {
            value: DurationMs::new(1_001),
        },
    )]);
    assert_eq!(
        frequency.compile(&out_of_range).unwrap_err().kind,
        FrequencyCompileErrorKind::ParameterOutOfRange
    );
}

#[test]
fn parameter_domains_must_keep_every_possible_schedule_valid() {
    let mut zero_interval = elapsed_frequency();
    zero_interval.parameters.insert(
        id("interval"),
        FrequencyParameterDefinition::Duration {
            default: DurationMs::new(3),
            minimum: DurationMs::new(0),
            maximum: DurationMs::new(1_000),
        },
    );
    assert_eq!(
        zero_interval.compile(&BTreeMap::new()).unwrap_err().kind,
        FrequencyCompileErrorKind::InvalidBinding
    );

    let mut unsafe_timer = elapsed_frequency();
    unsafe_timer.parameters.insert(
        id("coalescing"),
        FrequencyParameterDefinition::Duration {
            default: DurationMs::new(0),
            minimum: DurationMs::new(0),
            maximum: DurationMs::new(10),
        },
    );
    unsafe_timer.timer.coalesce_window = DurationBinding::Parameter {
        parameter: id("coalescing"),
    };
    assert_eq!(
        unsafe_timer.compile(&BTreeMap::new()).unwrap_err().kind,
        FrequencyCompileErrorKind::InvalidBinding
    );

    let mut correlated = elapsed_frequency();
    correlated.parameters.insert(
        id("lateness"),
        FrequencyParameterDefinition::Duration {
            default: DurationMs::new(5),
            minimum: DurationMs::new(0),
            maximum: DurationMs::new(10),
        },
    );
    let binding = DurationBinding::Parameter {
        parameter: id("lateness"),
    };
    correlated.timer.max_lateness = binding.clone();
    correlated.timer.coalesce_window = binding;
    assert!(correlated.compile(&BTreeMap::new()).is_ok());
}

#[test]
fn frequency_parser_rejects_cross_cadence_fields_and_invalid_definitions() {
    let elapsed_with_zone =
        ELAPSED_CANONICAL.replace("  timer {", "  timezone America/Sao_Paulo;\n  timer {");
    assert_eq!(
        parse_frequency(&elapsed_with_zone).unwrap_err().kind,
        DslErrorKind::UnexpectedToken
    );

    let empty_purpose = ELAPSED_CANONICAL.replace("\"Sample the local sensor\"", "\"\"");
    assert_eq!(
        parse_frequency(&empty_purpose).unwrap_err().kind,
        DslErrorKind::InvalidAtom
    );

    let zero_minimum = ELAPSED_CANONICAL.replace(
        "range [duration(1), duration(1000)]",
        "range [duration(0), duration(1000)]",
    );
    assert_eq!(
        parse_frequency(&zero_minimum).unwrap_err().kind,
        DslErrorKind::InvalidAtom
    );

    let calendar = format_frequency(&calendar_frequency(cadence(step("days", literal(1)))));
    let missing_fold = calendar
        .lines()
        .filter(|line| !line.trim_start().starts_with("fold "))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        parse_frequency(&missing_fold).unwrap_err().kind,
        DslErrorKind::MissingDeclaration
    );
}

#[test]
fn frequency_wire_vocabulary_has_a_golden_hash() {
    let elapsed = elapsed_frequency();
    let calendar = calendar_frequency(CadenceAst {
        every: step("days", literal(1)),
        land_on: Some(
            WeekdaySet::new([
                CivilWeekday::Monday,
                CivilWeekday::Tuesday,
                CivilWeekday::Wednesday,
                CivilWeekday::Thursday,
                CivilWeekday::Friday,
                CivilWeekday::Saturday,
                CivilWeekday::Sunday,
            ])
            .unwrap(),
        ),
        invalid_day: InvalidDay::Clamp,
        bound: CadenceBound::Unbounded,
    });
    let mut invalid = elapsed.clone();
    invalid.purpose.clear();
    let fixture = (
        [FrequencySchema::V1],
        [
            FrequencyCompileErrorKind::InvalidPurpose,
            FrequencyCompileErrorKind::InvalidParameterRange,
            FrequencyCompileErrorKind::UnknownParameter,
            FrequencyCompileErrorKind::ParameterTypeMismatch,
            FrequencyCompileErrorKind::ParameterOutOfRange,
            FrequencyCompileErrorKind::InvalidBinding,
            FrequencyCompileErrorKind::InvalidSchedule,
            FrequencyCompileErrorKind::CanonicalizationFailed,
        ],
        invalid.compile(&BTreeMap::new()).unwrap_err(),
        [
            FrequencyParameterValue::Duration {
                value: DurationMs::new(3),
            },
            FrequencyParameterValue::PositiveInteger { value: nonzero(2) },
        ],
        [
            DurationBinding::Literal {
                value: DurationMs::new(3),
            },
            DurationBinding::Parameter {
                parameter: id("interval"),
            },
        ],
        [
            PositiveIntegerBinding::Literal { value: nonzero(1) },
            PositiveIntegerBinding::Parameter {
                parameter: id("calendar_step"),
            },
        ],
        [
            cadence(step("days", literal(1))),
            CadenceAst {
                every: step("days", literal(1)),
                land_on: Some(WeekdaySet::new([CivilWeekday::Monday]).unwrap()),
                invalid_day: InvalidDay::Clamp,
                bound: CadenceBound::Unbounded,
            },
            CadenceAst {
                every: step("months", literal(1)),
                land_on: None,
                invalid_day: InvalidDay::Skip,
                bound: CadenceBound::Count { occurrences: 6 },
            },
        ],
        elapsed.compile(&BTreeMap::new()).unwrap(),
        calendar.compile(&BTreeMap::new()).unwrap(),
        elapsed,
        calendar,
        [
            MissedPolicy::Skip,
            MissedPolicy::Coalesce,
            MissedPolicy::Replay { max: nonzero(64) },
            MissedPolicy::PauseOnLag,
        ],
    );
    assert_eq!(
        canonical_hash("karma.frequency-wire.v1", &fixture)
            .unwrap()
            .as_str(),
        "sha256:a9295931c9f75d119f556bcb12e757ca349a56da79aabbc43c00e55015be3253"
    );
}

const ELAPSED_CANONICAL: &str = r#"karma-frequency 1;
frequency sensor.fast {
  schema karma.frequency.v1;
  purpose "Sample the local sensor";
  tags [fast, sensor];
  param interval: duration default duration(3) range [duration(1), duration(1000)];
  every elapsed parameter(interval);
  anchor timestamp("2026-07-21T09:00:00.000Z");
  timer {
    resolution duration(1);
    max-lateness duration(5);
    coalesce-window duration(0);
  }
  missed replay(64);
  inactive-gap skip-to-next-anchor;
  rephase preserve-anchor;
  overload pause-and-ask;
}
"#;

fn elapsed_frequency() -> FrequencyAst {
    FrequencyAst {
        schema: FrequencySchema::V1,
        slug: Slug::new("sensor.fast").unwrap(),
        purpose: "Sample the local sensor".to_string(),
        tags: [Slug::new("sensor").unwrap(), Slug::new("fast").unwrap()]
            .into_iter()
            .collect(),
        parameters: BTreeMap::from([(
            id("interval"),
            FrequencyParameterDefinition::Duration {
                default: DurationMs::new(3),
                minimum: DurationMs::new(1),
                maximum: DurationMs::new(1_000),
            },
        )]),
        cadence: FrequencyCadenceAst::Elapsed {
            interval: DurationBinding::Parameter {
                parameter: id("interval"),
            },
            anchor: nucleus::karma::TimestampMs::parse_canonical("2026-07-21T09:00:00.000Z")
                .unwrap(),
        },
        timer: FrequencyTimerAst {
            required_resolution: duration(1),
            max_lateness: duration(5),
            coalesce_window: duration(0),
        },
        missed: MissedPolicy::Replay { max: nonzero(64) },
        inactive_gap: InactiveGapPolicy::SkipToNextAnchor,
        rephase: RephasePolicy::PreserveAnchor,
        overload: OverloadPolicy::PauseAndAsk,
    }
}

/// A step with exactly one component set, which is what most rules are.
fn step(component: &str, binding: PositiveIntegerBinding) -> CadenceStepAst {
    let mut step = CadenceStepAst::default();
    match component {
        "years" => step.years = Some(binding),
        "months" => step.months = Some(binding),
        "weeks" => step.weeks = Some(binding),
        "days" => step.days = Some(binding),
        "hours" => step.hours = Some(binding),
        "minutes" => step.minutes = Some(binding),
        "seconds" => step.seconds = Some(binding),
        "milliseconds" => step.milliseconds = Some(binding),
        other => panic!("unknown step component {other}"),
    }
    step
}

fn cadence(every: CadenceStepAst) -> CadenceAst {
    CadenceAst {
        every,
        land_on: None,
        invalid_day: InvalidDay::Clamp,
        bound: CadenceBound::Unbounded,
    }
}

fn literal(value: u32) -> PositiveIntegerBinding {
    PositiveIntegerBinding::Literal {
        value: nonzero(value),
    }
}

fn calendar_frequency(cadence: CadenceAst) -> FrequencyAst {
    FrequencyAst {
        schema: FrequencySchema::V1,
        slug: Slug::new("household.calendar").unwrap(),
        purpose: "Run on a local civil cadence".to_string(),
        tags: [Slug::new("calendar").unwrap()].into_iter().collect(),
        parameters: BTreeMap::from([(
            id("calendar_step"),
            FrequencyParameterDefinition::PositiveInteger {
                default: nonzero(1),
                minimum: nonzero(1),
                maximum: nonzero(12),
            },
        )]),
        cadence: FrequencyCadenceAst::Calendar {
            cadence,
            anchor: CivilDateTime::parse_canonical("2026-01-01T08:00:00.000").unwrap(),
            timezone: TimeZoneId::new("America/Sao_Paulo").unwrap(),
            tzdb: TzdbRevision {
                version: TzdbVersion::new("2026a+lince.1").unwrap(),
                digest: CanonicalHash::parse(format!("sha256:{}", "8".repeat(64))).unwrap(),
            },
            gap: GapPolicy::ShiftForward,
            fold: FoldPolicy::Both,
        },
        timer: FrequencyTimerAst {
            required_resolution: duration(1),
            max_lateness: duration(20),
            coalesce_window: duration(0),
        },
        missed: MissedPolicy::Coalesce,
        inactive_gap: InactiveGapPolicy::ReplayByMissedPolicy,
        rephase: RephasePolicy::ImmediateIfOverdue,
        overload: OverloadPolicy::RejectActivation,
    }
}

fn id(value: &str) -> LocalId {
    LocalId::new(value).unwrap()
}

fn duration(milliseconds: i64) -> DurationBinding {
    DurationBinding::Literal {
        value: DurationMs::new(milliseconds),
    }
}

fn nonzero(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).unwrap()
}

