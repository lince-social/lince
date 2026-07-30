use std::num::NonZeroU32;

use chrono::{DateTime, TimeDelta, Utc};
use nucleus::karma::{
    CanonicalHash, ElapsedSchedule, InactiveGapPolicy, KarmaOccurrenceEnvelope, MissedPolicy,
    OccurrenceBatch, OccurrenceRange, OverloadPolicy, RephasePolicy, ScheduleEmission, TimerPolicy,
    TimestampMs,
};
use store::Store;
use store::karma::occurrences::{KarmaOccurrenceCommit, ingest, list};

#[tokio::test]
async fn ingress_assigns_one_cell_order_replays_and_reopens_without_sequence_gaps() {
    let path = std::env::temp_dir().join(format!(
        "lince-karma-occurrence-{}.db",
        nucleus::new_uid("test")
    ));
    let store = Store::open(&format!("sqlite://{}", path.display()))
        .await
        .unwrap();
    let batch = OccurrenceBatch::new(
        digest('1'),
        1,
        &schedule(),
        &ScheduleEmission::Individual(
            OccurrenceRange::new(timestamp("2026-07-22T12:00:00.003Z"), 3, 2).unwrap(),
        ),
    )
    .unwrap();
    let first = KarmaOccurrenceEnvelope::schedule_tick(
        digest('a'),
        batch.individual_tick(0).unwrap(),
        None,
    )
    .unwrap();
    let first_row = match ingest(&store.pool, &first, now(0)).await.unwrap() {
        KarmaOccurrenceCommit::Inserted(row) => row,
        other => panic!("expected inserted occurrence, got {other:?}"),
    };
    assert_eq!(first_row.cell_sequence, 1);

    let replay = match ingest(&store.pool, &first, now(50)).await.unwrap() {
        KarmaOccurrenceCommit::Existing(row) => row,
        other => panic!("expected existing occurrence, got {other:?}"),
    };
    assert_eq!(replay, first_row);

    let second = KarmaOccurrenceEnvelope::schedule_tick(
        digest('a'),
        batch.individual_tick(1).unwrap(),
        Some(first_row.occurrence_hash.clone()),
    )
    .unwrap();
    let second_row = match ingest(&store.pool, &second, now(1)).await.unwrap() {
        KarmaOccurrenceCommit::Inserted(row) => row,
        other => panic!("expected inserted occurrence, got {other:?}"),
    };
    assert_eq!(second_row.cell_sequence, 2);
    assert_eq!(
        list(&store.pool).await.unwrap(),
        vec![first_row.clone(), second_row]
    );

    let collision = KarmaOccurrenceEnvelope::schedule_tick(
        digest('b'),
        batch.individual_tick(0).unwrap(),
        None,
    )
    .unwrap();
    assert!(ingest(&store.pool, &collision, now(2)).await.is_err());
    let next_sequence: i64 = store::sqlx::query_scalar(
        "SELECT next_sequence FROM karma_occurrence_sequence WHERE singleton = 1",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(next_sequence, 3);

    store.pool.close().await;
    let reopened = Store::open(&format!("sqlite://{}", path.display()))
        .await
        .unwrap();
    let rows = list(&reopened.pool).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0], first_row);
    reopened.pool.close().await;
    std::fs::remove_file(path).unwrap();
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

fn now(offset_ms: i64) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-07-22T12:00:00.000Z")
        .unwrap()
        .with_timezone(&Utc)
        + TimeDelta::milliseconds(offset_ms)
}
