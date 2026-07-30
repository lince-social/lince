use std::num::NonZeroU32;

use nucleus::karma::{
    CanonicalHash, ElapsedSchedule, InactiveGapPolicy, KarmaOccurrenceEnvelope,
    KarmaOccurrenceSource, MissedPolicy, OccurrenceBatch, OccurrenceRange, OverloadPolicy,
    RephasePolicy, ScheduleEmission, TimerPolicy, TimestampMs,
};

#[test]
fn schedule_tick_identity_is_independent_from_batch_and_arrival_metadata() {
    let schedule = schedule();
    let activation = digest('1');
    let first_batch = OccurrenceBatch::new(
        activation.clone(),
        1,
        &schedule,
        &ScheduleEmission::Individual(
            OccurrenceRange::new(timestamp("2026-07-22T12:00:00.003Z"), 3, 4).unwrap(),
        ),
    )
    .unwrap();
    let later_batch = OccurrenceBatch::new(
        activation,
        2,
        &schedule,
        &ScheduleEmission::Individual(
            OccurrenceRange::new(timestamp("2026-07-22T12:00:00.009Z"), 3, 2).unwrap(),
        ),
    )
    .unwrap();
    let from_first = first_batch.individual_tick(3).unwrap();
    let from_later = later_batch.individual_tick(1).unwrap();
    assert_eq!(
        from_first.tick_hash().unwrap(),
        from_later.tick_hash().unwrap()
    );

    let envelope = KarmaOccurrenceEnvelope::schedule_tick(digest('a'), from_first, None).unwrap();
    assert_eq!(envelope.logical_at, timestamp("2026-07-22T12:00:00.012Z"));
    assert_eq!(envelope.source.kind_name(), "schedule-tick");
    assert_eq!(
        envelope.source_identity().unwrap(),
        from_later.tick_hash().unwrap()
    );

    let mut hostile = serde_json::to_value(&envelope).unwrap();
    hostile["logical_at"] = serde_json::json!("2026-07-22T12:00:00.013Z");
    assert!(serde_json::from_value::<KarmaOccurrenceEnvelope>(hostile).is_err());
}

#[test]
fn coalesced_source_cannot_disguise_an_individual_batch() {
    let batch = OccurrenceBatch::new(
        digest('2'),
        1,
        &schedule(),
        &ScheduleEmission::Individual(
            OccurrenceRange::new(timestamp("2026-07-22T12:00:00.003Z"), 3, 2).unwrap(),
        ),
    )
    .unwrap();
    assert!(KarmaOccurrenceEnvelope::schedule_coalesced(digest('b'), batch.clone(), None).is_err());

    let source = KarmaOccurrenceSource::ScheduleCoalesced {
        schedule_occurrence_hash: digest('b'),
        batch,
    };
    let hostile = serde_json::json!({
        "schema": "karma.occurrence.v1",
        "source": source,
        "logical_at": "2026-07-22T12:00:00.006Z"
    });
    assert!(serde_json::from_value::<KarmaOccurrenceEnvelope>(hostile).is_err());
}

fn schedule() -> ElapsedSchedule {
    ElapsedSchedule::new(
        timestamp("2026-07-22T12:00:00.000Z"),
        3,
        TimerPolicy::new(1, 5, 0).unwrap(),
        MissedPolicy::Replay {
            max: NonZeroU32::new(64).unwrap(),
        },
        InactiveGapPolicy::SkipToNextAnchor,
        RephasePolicy::PreserveAnchor,
        OverloadPolicy::PauseAndAsk,
    )
    .unwrap()
}

fn timestamp(value: &str) -> TimestampMs {
    TimestampMs::parse_canonical(value).unwrap()
}

fn digest(digit: char) -> CanonicalHash {
    CanonicalHash::parse(format!("sha256:{}", digit.to_string().repeat(64))).unwrap()
}
