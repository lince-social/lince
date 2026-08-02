//! A repeating rule is read far more often than it is written, and every read
//! is someone deciding whether to pay something. These pin the cases where a
//! plausible-looking implementation quietly lies: a short month, a window that
//! opens mid-period, a landing rule that collapses two dates onto one, and a
//! step fast enough that no honest answer fits in a page.

use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use nucleus::karma::{Cadence, CadenceError, CadenceStep, CivilWeekday, InvalidDay, WeekdaySet};

fn at(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .expect("a valid instant")
        .with_timezone(&Utc)
}

fn day(year: i32, month: u32, day: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, 0, 0, 0).unwrap()
}

fn fridays() -> WeekdaySet {
    WeekdaySet::new([CivilWeekday::Friday]).expect("one weekday is a set")
}

fn dates(
    cadence: &Cadence,
    anchor: DateTime<Utc>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Vec<DateTime<Utc>> {
    cadence
        .between(anchor, from, to)
        .expect("a valid rule")
        .dates
}

#[test]
fn a_step_with_no_components_is_refused() {
    // Otherwise the rule repeats one instant forever and every derivation is an
    // infinite loop waiting to happen.
    let cadence = Cadence::every(CadenceStep::default());
    assert_eq!(cadence.validate(), Err(CadenceError::ZeroInterval));
    assert!(
        cadence
            .between(day(2026, 1, 1), day(2026, 1, 1), day(2027, 1, 1))
            .is_err()
    );
}

#[test]
fn components_sum_rather_than_compete() {
    // The rule asked for by name: one month plus one day plus one second plus
    // ten milliseconds is a single step, not four rules.
    let cadence = Cadence::every(CadenceStep {
        months: 1,
        days: 1,
        seconds: 1,
        milliseconds: 10,
        ..Default::default()
    });
    let anchor = at("2026-01-01T00:00:00Z");
    let out = dates(&cadence, anchor, anchor, at("2026-04-01T00:00:00Z"));

    assert_eq!(out[0], anchor);
    // One month on from 1 January is 1 February; then one day, one second and
    // ten milliseconds on top of it.
    assert_eq!(out[1], at("2026-02-02T00:00:01.010Z"));
    // Two months on from the *anchor*, with the fixed part doubled — not the
    // previous result stepped again.
    assert_eq!(out[2], at("2026-03-03T00:00:02.020Z"));
}

#[test]
fn milliseconds_survive_into_the_result() {
    // A ten-millisecond step is meaningless if the instant is rounded to the
    // second anywhere in the arithmetic.
    let cadence = Cadence::every(CadenceStep {
        milliseconds: 10,
        ..Default::default()
    });
    let anchor = at("2026-01-01T00:00:00Z");
    let out = dates(&cadence, anchor, anchor, at("2026-01-01T00:00:00.050Z"));
    assert_eq!(
        out,
        vec![
            at("2026-01-01T00:00:00.000Z"),
            at("2026-01-01T00:00:00.010Z"),
            at("2026-01-01T00:00:00.020Z"),
            at("2026-01-01T00:00:00.030Z"),
            at("2026-01-01T00:00:00.040Z"),
        ]
    );
}

#[test]
fn the_thirty_first_means_the_end_of_a_short_month_not_a_skipped_one() {
    // A rule anchored on the 31st is asking for the end of the month. Skipping
    // February would drop a real cost from the year.
    let cadence = Cadence::every_months(1);
    let anchor = day(2026, 1, 31);
    let out = dates(&cadence, anchor, anchor, day(2026, 4, 1));

    assert_eq!(out[0], day(2026, 1, 31));
    assert_eq!(out[1], day(2026, 2, 28), "February clamps to its last day");
    assert_eq!(out[2], day(2026, 3, 31));
}

#[test]
fn clamping_never_drags_a_later_month_back_to_the_short_one() {
    // The bug this pins: stepping one month at a time from the *previous*
    // result makes January 31st become February 28th and then carry the 28th
    // forever. Every month must be measured against the anchor.
    let cadence = Cadence::every_months(1);
    let anchor = day(2026, 1, 31);
    let out = dates(&cadence, anchor, anchor, day(2026, 7, 1));

    assert_eq!(out[3], day(2026, 4, 30));
    assert_eq!(out[4], day(2026, 5, 31), "May has a 31st and must use it");
    assert_eq!(out[5], day(2026, 6, 30));
}

#[test]
fn skip_refuses_the_month_that_has_no_such_day() {
    // The other honest answer, for a rule where the date is the point.
    let cadence = Cadence::every_months(1).with_invalid_day(InvalidDay::Skip);
    let anchor = day(2026, 1, 31);
    let out = dates(&cadence, anchor, anchor, day(2026, 5, 1));

    assert_eq!(out, vec![day(2026, 1, 31), day(2026, 3, 31)]);
}

#[test]
fn a_skipped_month_does_not_end_the_series() {
    // A short month must not be mistaken for the calendar running out. If the
    // scan stopped at February, every later occurrence would vanish.
    let cadence = Cadence::every_months(1).with_invalid_day(InvalidDay::Skip);
    let anchor = day(2026, 1, 31);
    let out = dates(&cadence, anchor, anchor, day(2027, 1, 1));
    assert!(
        out.contains(&day(2026, 12, 31)),
        "December is still reached"
    );
}

#[test]
fn a_window_opening_mid_period_keeps_the_rules_own_phase() {
    // A fortnightly rule stays on *its* fortnight. Restarting at the window's
    // edge would silently re-phase the rule every time it was read.
    let cadence = Cadence::every_days(14);
    let anchor = day(2026, 1, 1);
    let out = dates(&cadence, anchor, day(2026, 2, 1), day(2026, 3, 1));

    assert_eq!(out[0], day(2026, 2, 12), "14 days on from 29 January");
    assert_eq!(out[1], day(2026, 2, 26));
}

#[test]
fn adjacent_windows_tile_without_claiming_the_same_date_twice() {
    // Half-open windows are what let a surface page through time without
    // double-counting the boundary.
    let cadence = Cadence::every_days(1);
    let anchor = day(2026, 1, 1);
    let first = dates(&cadence, anchor, day(2026, 1, 1), day(2026, 1, 15));
    let second = dates(&cadence, anchor, day(2026, 1, 15), day(2026, 2, 1));

    assert!(first.contains(&day(2026, 1, 14)));
    assert!(
        !first.contains(&day(2026, 1, 15)),
        "the upper bound is excluded"
    );
    assert!(
        second.contains(&day(2026, 1, 15)),
        "and belongs to the next window"
    );
}

#[test]
fn nothing_is_produced_before_the_anchor() {
    // A rule declared today does not retroactively claim last year.
    let cadence = Cadence::every_days(1);
    let anchor = day(2026, 6, 1);
    let out = dates(&cadence, anchor, day(2026, 1, 1), day(2026, 6, 5));
    assert_eq!(out[0], day(2026, 6, 1));
}

#[test]
fn landing_rolls_forward_to_the_allowed_weekday() {
    // "Skip days until I land on a Friday", applied after the step.
    let cadence = Cadence::every_months(1).landing_on(fridays());
    // 1 January 2026 is a Thursday, so the first occurrence rolls one day.
    let anchor = day(2026, 1, 1);
    let out = dates(&cadence, anchor, anchor, day(2026, 4, 1));

    for date in &out {
        assert_eq!(
            CivilWeekday::from(date.weekday()),
            CivilWeekday::Friday,
            "{date} should have landed on a Friday"
        );
    }
    assert_eq!(out[0], day(2026, 1, 2));
}

#[test]
fn an_instant_already_on_an_allowed_weekday_does_not_move() {
    let cadence = Cadence::every_weeks(1).landing_on(fridays());
    // 2 January 2026 is a Friday.
    let anchor = day(2026, 1, 2);
    let out = dates(&cadence, anchor, anchor, day(2026, 1, 20));
    assert_eq!(out[0], day(2026, 1, 2), "no roll when it already matches");
    assert_eq!(out[1], day(2026, 1, 9));
}

#[test]
fn landing_does_not_drift_the_phase_it_only_moves_the_result() {
    // The bug this pins: feeding the landed instant back in as the next
    // anchor. A monthly rule would gain a few days every month and slowly stop
    // being monthly. Phase must come from the anchor alone.
    let cadence = Cadence::every_months(1).landing_on(fridays());
    let anchor = day(2026, 1, 1);
    let out = dates(&cadence, anchor, anchor, day(2026, 6, 1));

    // Each landed date stays within a week of its own unlanded base date.
    let plain = Cadence::every_months(1);
    let bases = dates(&plain, anchor, anchor, day(2026, 6, 1));
    for base in &bases {
        assert!(
            out.iter()
                .any(|landed| *landed >= *base && *landed < *base + Duration::days(7)),
            "the occurrence for {base} drifted more than a week"
        );
    }
}

#[test]
fn two_dates_landing_on_one_friday_are_reported_once() {
    // Landing collapses a run of dates onto the same weekday. Two occurrences
    // on one instant would share an idempotency key, so the second would
    // silently replay as an already-applied change — a date the person can see
    // but never actually apply.
    let cadence = Cadence::every_days(1).landing_on(fridays());
    let anchor = day(2026, 1, 5); // Monday
    let out = dates(&cadence, anchor, anchor, day(2026, 1, 12));

    assert_eq!(
        out,
        vec![day(2026, 1, 9)],
        "Monday through Friday all land on the 9th"
    );
}

#[test]
fn results_come_back_in_order() {
    // Landing can move a later base date less than an earlier one, so emission
    // order is not derivation order.
    let cadence = Cadence::every_days(3).landing_on(fridays());
    let anchor = day(2026, 1, 1);
    let out = dates(&cadence, anchor, anchor, day(2026, 3, 1));

    let mut sorted = out.clone();
    sorted.sort();
    assert_eq!(out, sorted);
}

#[test]
fn a_base_date_just_before_the_window_can_land_inside_it() {
    // The scan has to start before the window does, or a date that belongs in
    // the window is lost because its unlanded base sat outside.
    let cadence = Cadence::every_days(30).landing_on(fridays());
    let anchor = day(2026, 1, 5); // Monday; lands on the 9th
    // A window that opens after the base but before the landed date.
    let out = dates(&cadence, anchor, day(2026, 1, 7), day(2026, 1, 20));
    assert_eq!(out, vec![day(2026, 1, 9)]);
}

#[test]
fn a_far_window_on_a_fast_rule_is_bounded_not_unbounded() {
    // A read path must never be able to allocate a year of milliseconds.
    let cadence = Cadence::every(CadenceStep {
        milliseconds: 10,
        ..Default::default()
    });
    let anchor = day(2026, 1, 1);
    let derived = cadence
        .between(anchor, anchor, day(2027, 1, 1))
        .expect("a valid rule");

    assert!(derived.dates.len() <= 512);
    assert!(
        derived.truncated,
        "a prefix must announce itself rather than pass as the whole set"
    );
}

#[test]
fn a_complete_answer_is_not_marked_truncated() {
    // The other half of the signal: if `truncated` were always true a surface
    // would permanently show "and more" and the flag would mean nothing.
    let cadence = Cadence::every_days(1);
    let anchor = day(2026, 1, 1);
    let derived = cadence
        .between(anchor, anchor, day(2026, 1, 10))
        .expect("a valid rule");

    assert_eq!(derived.dates.len(), 9);
    assert!(!derived.truncated);
}

#[test]
fn a_far_window_on_a_fast_rule_still_starts_at_the_right_phase() {
    // The closed-form jump has to land on a real multiple of the step. Being
    // one step out would put every derived instant permanently off-beat.
    let cadence = Cadence::every(CadenceStep {
        seconds: 1,
        ..Default::default()
    });
    let anchor = at("2026-01-01T00:00:00Z");
    let out = dates(
        &cadence,
        anchor,
        at("2026-06-01T00:00:00Z"),
        at("2026-06-01T00:00:03Z"),
    );
    assert_eq!(
        out,
        vec![
            at("2026-06-01T00:00:00Z"),
            at("2026-06-01T00:00:01Z"),
            at("2026-06-01T00:00:02Z"),
        ]
    );
}

#[test]
fn next_on_or_after_reaches_across_a_yearly_step() {
    // A fixed horizon sized for daily rules would report "never" for a yearly
    // one, which reads as a rule that has stopped.
    let cadence = Cadence::every_years(1);
    let anchor = day(2026, 3, 10);
    let next = cadence
        .next_on_or_after(anchor, day(2026, 6, 1))
        .expect("a valid rule");
    assert_eq!(next, Some(day(2027, 3, 10)));
}

#[test]
fn a_yearly_rule_clamps_the_twenty_ninth_of_february() {
    let cadence = Cadence::every_years(1);
    let anchor = day(2028, 2, 29);
    let out = dates(&cadence, anchor, anchor, day(2030, 1, 1));
    assert_eq!(out[0], day(2028, 2, 29));
    assert_eq!(out[1], day(2029, 2, 28), "a common year clamps back");
}

#[test]
fn an_empty_window_produces_nothing() {
    let cadence = Cadence::every_days(1);
    let anchor = day(2026, 1, 1);
    assert!(dates(&cadence, anchor, day(2026, 5, 1), day(2026, 5, 1)).is_empty());
    assert!(dates(&cadence, anchor, day(2026, 5, 2), day(2026, 5, 1)).is_empty());
}

#[test]
fn a_cadence_round_trips_through_json() {
    // The sand sends this shape back verbatim, so the wire form is part of the
    // contract rather than an implementation detail.
    let cadence = Cadence::every(CadenceStep {
        months: 1,
        days: 1,
        seconds: 1,
        milliseconds: 10,
        ..Default::default()
    })
    .landing_on(fridays());

    let json = serde_json::to_string(&cadence).expect("serializes");
    let back: Cadence = serde_json::from_str(&json).expect("deserializes");
    assert_eq!(back, cadence);
    assert!(
        json.contains("\"friday\""),
        "weekdays travel as words: {json}"
    );
}

#[test]
fn an_omitted_component_defaults_to_zero() {
    // The sand only sends the fields a person filled in.
    let cadence: Cadence = serde_json::from_str(r#"{"every":{"months":1}}"#).expect("deserializes");
    assert_eq!(cadence.every.months, 1);
    assert_eq!(cadence.every.milliseconds, 0);
    assert_eq!(cadence.land_on, None);
    assert_eq!(cadence.invalid_day, InvalidDay::Clamp);
}

// ---------------------------------------------------------------- the bound
//
// A bound is what turns one primitive into every "when" in the system. These
// tests are the argument that a one-shot promise and a standing order are the
// same object, and that nothing downstream needs to know which it is holding.

#[test]
fn once_on_a_day_is_a_rule_that_retires_rather_than_a_second_kind_of_thing() {
    // The whole case for the merge. An empty step is meaningless for a rule that
    // repeats, and exactly right for one that cannot reach a second occurrence.
    let cadence = Cadence::once();
    assert_eq!(cadence.validate(), Ok(()));
    assert_eq!(
        dates(&cadence, day(2026, 3, 14), day(2026, 1, 1), day(2027, 1, 1)),
        vec![day(2026, 3, 14)]
    );
}

#[test]
fn an_empty_step_is_still_refused_when_the_rule_could_reach_a_second_date() {
    // The permission above is granted by the bound, not by the empty step. A
    // rule bounded at two would repeat one instant forever.
    let mut cadence = Cadence::once();
    cadence.bound = nucleus::karma::CadenceBound::Count { occurrences: 2 };
    assert_eq!(cadence.validate(), Err(CadenceError::ZeroInterval));
}

#[test]
fn a_bound_of_zero_occurrences_is_refused_rather_than_silently_empty() {
    // Returning nothing would be indistinguishable from a rule whose window
    // simply missed, and the author would never learn they said nothing.
    let cadence = Cadence::every_days(1).taking(0);
    assert_eq!(cadence.validate(), Err(CadenceError::EmptyBound));
}

#[test]
fn a_counted_rule_stops_after_its_count_not_at_the_window_edge() {
    let cadence = Cadence::every_months(1).taking(3);
    assert_eq!(
        dates(&cadence, day(2026, 1, 10), day(2026, 1, 1), day(2027, 1, 1)),
        vec![day(2026, 1, 10), day(2026, 2, 10), day(2026, 3, 10)]
    );
}

#[test]
fn a_count_is_of_occurrences_produced_not_of_candidates_examined() {
    // February has no 31st, and under `Skip` it yields nothing. If the count
    // were of indices, that empty month would consume one of the three the
    // author asked for and the rule would end a month early.
    let cadence = Cadence::every_months(1)
        .with_invalid_day(InvalidDay::Skip)
        .taking(3);
    let produced = dates(&cadence, day(2026, 1, 31), day(2026, 1, 1), day(2027, 6, 1));
    assert_eq!(produced.len(), 3, "three real dates, not three attempts");
    assert_eq!(produced[0], day(2026, 1, 31));
    assert_eq!(produced[1], day(2026, 3, 31));
}

#[test]
fn a_closing_date_is_exclusive_so_rules_can_be_laid_end_to_end() {
    // Half-open like every other window here. A rule ending on the 1st and its
    // replacement starting on the 1st must not both claim that day.
    let cadence = Cadence::every_months(1)
        .until(nucleus::karma::CivilDateTime::parse_canonical("2026-04-01T00:00:00.000").unwrap());
    assert_eq!(
        dates(&cadence, day(2026, 1, 1), day(2026, 1, 1), day(2027, 1, 1)),
        vec![day(2026, 1, 1), day(2026, 2, 1), day(2026, 3, 1)]
    );
}

#[test]
fn a_bound_is_measured_on_the_landed_instant_not_the_one_before_landing() {
    // Landing is what the person sees and what gets applied, so it is what the
    // close has to be compared against. Comparing the pre-landing instant would
    // let a rule produce a date past its own end.
    let cadence = Cadence::every_weeks(1)
        .landing_on(fridays())
        .until(nucleus::karma::CivilDateTime::parse_canonical("2026-01-09T00:00:00.000").unwrap());
    // Anchored on a Thursday: each occurrence lands on the following Friday.
    let produced = dates(&cadence, day(2026, 1, 1), day(2026, 1, 1), day(2027, 1, 1));
    assert_eq!(produced, vec![day(2026, 1, 2)]);
}

// ------------------------------------------------- one generator, two readers
//
// The read path and the scheduler now derive from the same function. These
// pin that down, because the failure they prevent is silent: a rule that means
// one thing on the screen and another in the runtime.

#[test]
fn the_civil_generator_agrees_with_the_utc_derivation() {
    // In UTC the two spaces coincide, so any disagreement here is the two
    // callers having drifted apart rather than a timezone effect.
    let cadence = Cadence::every_months(1);
    let anchor = nucleus::karma::CivilDateTime::parse_canonical("2026-01-31T08:00:00.000").unwrap();
    let utc = dates(
        &cadence,
        at("2026-01-31T08:00:00Z"),
        at("2026-01-01T00:00:00Z"),
        at("2026-05-01T00:00:00Z"),
    );
    let civil: Vec<String> = (0..utc.len())
        .map(|index| cadence.civil_at(anchor, index as u64).unwrap().to_string())
        .collect();
    let expected: Vec<String> = utc
        .iter()
        .map(|instant| instant.format("%Y-%m-%dT%H:%M:%S%.3f").to_string())
        .collect();
    assert_eq!(civil, expected);
}

#[test]
fn membership_recognises_exactly_what_the_generator_produces() {
    // The scheduler hands a boundary back and asks "is this yours". Answering
    // from a second hand-written rule is how a checker and a generator disagree.
    let cadence = Cadence::every(CadenceStep {
        months: 1,
        days: 1,
        ..Default::default()
    })
    .landing_on(fridays());
    let anchor = nucleus::karma::CivilDateTime::parse_canonical("2026-01-01T08:00:00.000").unwrap();
    for index in 0..6u64 {
        let produced = cadence.civil_at(anchor, index).expect("an occurrence");
        assert!(
            cadence.produces_civil(anchor, produced),
            "index {index} produced {produced}, which the rule then disowned"
        );
    }
    let not_ours =
        nucleus::karma::CivilDateTime::parse_canonical("2026-01-03T08:00:00.000").unwrap();
    assert!(!cadence.produces_civil(anchor, not_ours));
}

#[test]
fn the_instant_before_a_cut_is_the_one_a_rule_last_produced() {
    // `preceding` is what gives a rule the window it reads over: the gap back
    // to its own previous instant. Getting it wrong by one step makes every
    // rhythm a rule counts either double or vanish.
    let cadence = Cadence::every_days(7);
    let anchor = at("2026-03-02T00:00:00Z");

    // Strictly before: an instant the rule produces is not its own predecessor.
    assert_eq!(
        cadence
            .preceding(anchor, at("2026-03-09T00:00:00Z"))
            .unwrap(),
        Some(at("2026-03-02T00:00:00Z")),
        "a cut landing exactly on an occurrence belongs to the one beneath it"
    );
    assert_eq!(
        cadence
            .preceding(anchor, at("2026-03-09T00:00:01Z"))
            .unwrap(),
        Some(at("2026-03-09T00:00:00Z"))
    );
    // Before the anchor there is nothing to have missed.
    assert_eq!(cadence.preceding(anchor, anchor).unwrap(), None);
    assert_eq!(
        cadence
            .preceding(anchor, at("2026-01-01T00:00:00Z"))
            .unwrap(),
        None
    );
}

#[test]
fn looking_back_over_a_fast_rule_does_not_walk_from_the_anchor() {
    // The reason this is not a backwards `between`: a lookback wide enough for
    // a yearly step truncates a millisecond one, and the last element of a
    // truncated prefix is the wrong answer by millions of steps. Ten
    // milliseconds, a year on from the anchor, has to be exact and immediate.
    let cadence = Cadence::every(CadenceStep {
        milliseconds: 10,
        ..CadenceStep::default()
    });
    let anchor = at("2026-01-01T00:00:00Z");
    assert_eq!(
        cadence
            .preceding(anchor, at("2027-01-01T00:00:00.005Z"))
            .unwrap(),
        Some(at("2027-01-01T00:00:00Z"))
    );
}

#[test]
fn looking_back_respects_the_calendar_and_the_bound() {
    // A monthly rule anchored on the 31st, clamping: the instant before March
    // is February's clamped end, not the 31st of a month that has none.
    let cadence = Cadence::every(CadenceStep {
        months: 1,
        ..CadenceStep::default()
    });
    let anchor = at("2026-01-31T09:00:00Z");
    assert_eq!(
        cadence
            .preceding(anchor, at("2026-03-31T09:00:00Z"))
            .unwrap(),
        Some(at("2026-02-28T09:00:00Z")),
        "the clamped instant is the one the rule actually produced"
    );

    // A retired rule has a last instant and then nothing later.
    let mut counted = Cadence::every_days(1);
    counted.bound = nucleus::karma::CadenceBound::Count { occurrences: 3 };
    let anchor = at("2026-01-01T00:00:00Z");
    assert_eq!(
        counted
            .preceding(anchor, at("2026-06-01T00:00:00Z"))
            .unwrap(),
        Some(at("2026-01-03T00:00:00Z")),
        "looking back from long after a bound finds the last instant, not none"
    );
}
