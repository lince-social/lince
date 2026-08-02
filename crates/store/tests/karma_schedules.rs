use std::{
    collections::{BTreeMap, BTreeSet},
    num::{NonZeroU32, NonZeroU64},
};

use chrono::{DateTime, TimeDelta, Utc};
use nucleus::karma::{
    CadenceAst, CadenceBound, CadenceStepAst, CanonicalHash, CivilDateTime, DefinitionStatus,
    DurationBinding, DurationMs, FoldPolicy, FrequencyAst, FrequencyCadenceAst,
    FrequencyParameterDefinition, FrequencyParameterValue, FrequencySchema, FrequencyTimerAst,
    GapPolicy, InactiveGapPolicy, InvalidDay, KarmaOccurrenceSource, LocalId, LocalTimeResolution,
    MissedPolicy, OverloadPolicy, PositiveIntegerBinding, RationalRate, RephasePolicy,
    ScheduleCursorLifecycle, ScheduleWorkloadUpperBounds, SchedulerCalibration, Slug, TimeZoneId,
    TimeZoneProvider, TimestampMs, TzdbRevision, TzdbVersion,
};
use store::Store;
use store::karma::expansions::{
    expand_pending_schedule_occurrences, expand_schedule_occurrence,
    get_cursor as get_expansion_cursor, has_pending_schedule_occurrences,
};
use store::karma::frequencies::{
    ActivateFrequencyInput, CreateFrequencyInput, FrequencyHandleRow, FrequencyMutationCommit,
    PauseFrequencyInput, SetFrequencyParametersInput, activate, create, pause, set_parameters,
};
use store::karma::schedules::{
    CursorCompletion, CursorMaterialization, ScheduleClaim, ScheduleDemandPolicy, claim_due,
    complete_calendar, complete_elapsed, get_calendar_cursor, get_cursor, get_occurrence,
    list_armed_deadlines, materialize_calendar_cursor, materialize_elapsed_cursor,
    reconcile_active_schedule_cursors, record_deadline_admission,
    supersede_inactive_frequency_cursors,
};

const PERSON_UID: &str = "p_01ARZ3NDEKTSV4RRFFQ69G5FAV";

fn demand_policy() -> ScheduleDemandPolicy {
    ScheduleDemandPolicy {
        workload: ScheduleWorkloadUpperBounds::new(0, NonZeroU32::new(1).unwrap(), 0, 256),
        calibration: SchedulerCalibration::new(NonZeroU32::new(1_000).unwrap()),
    }
}

#[derive(Clone)]
struct FakeProvider {
    revision: TzdbRevision,
    resolutions: BTreeMap<CivilDateTime, LocalTimeResolution>,
}

impl TimeZoneProvider for FakeProvider {
    fn revision(&self) -> &TzdbRevision {
        &self.revision
    }

    fn resolve_local(
        &self,
        _timezone: &TimeZoneId,
        local: CivilDateTime,
    ) -> Result<LocalTimeResolution, nucleus::karma::KarmaBoundaryError> {
        self.resolutions.get(&local).copied().ok_or_else(|| {
            nucleus::karma::KarmaBoundaryError::invalid_input(format!(
                "fixture has no resolution for {local}"
            ))
        })
    }

    fn minimum_interval_ms(
        &self,
        _schedule: &nucleus::karma::CalendarSchedule,
    ) -> Result<NonZeroU64, nucleus::karma::KarmaBoundaryError> {
        Ok(NonZeroU64::new(1).unwrap())
    }
}

#[tokio::test]
async fn leases_are_fenced_occurrences_are_unique_and_restart_rebuild_is_exact() {
    let path = std::env::temp_dir().join(format!(
        "lince-karma-schedule-{}.db",
        nucleus::new_uid("test")
    ));
    let url = format!("sqlite://{}", path.display());
    let store = Store::open(&url).await.unwrap();
    let active = active_frequency(&store, "schedule.persisted").await;
    let activation_hash = active.active_activation_hash.clone().unwrap();
    let materialized =
        materialize_elapsed_cursor(&store.pool, &activation_hash, demand_policy(), at_ms(0))
            .await
            .unwrap();
    let cursor = created(materialized);
    assert_eq!(cursor.cursor_revision, 1);
    assert_eq!(cursor.lifecycle, ScheduleCursorLifecycle::Armed);
    assert_eq!(
        cursor.demand.semantic_ticks_per_second(),
        RationalRate::new(1_000, 3).unwrap()
    );
    assert_eq!(
        cursor.deadline.as_ref().unwrap().next_intended_at(),
        timestamp("2026-07-22T12:00:00.003Z")
    );
    assert!(
        claim_due(
            &store.pool,
            &activation_hash,
            1,
            "worker-before-admission",
            at_ms(3),
            DurationMs::new(5),
        )
        .await
        .is_err()
    );
    assert!(
        record_deadline_admission(
            &store.pool,
            &activation_hash,
            1,
            NonZeroU32::new(1).unwrap(),
            false,
            at_ms(0),
        )
        .await
        .unwrap()
    );
    let admitted = get_cursor(&store.pool, &activation_hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        admitted.admitted_resolution_ms,
        Some(NonZeroU32::new(1).unwrap())
    );
    assert!(!admitted.admission_degraded);
    assert!(admitted.admitted_at.is_some());
    assert_eq!(list_armed_deadlines(&store.pool).await.unwrap().len(), 1);
    assert!(
        claim_due(
            &store.pool,
            &activation_hash,
            1,
            "worker-early",
            at_ms(2),
            DurationMs::new(5),
        )
        .await
        .is_err()
    );

    let first_lease = claimed(
        claim_due(
            &store.pool,
            &activation_hash,
            1,
            "worker-a",
            at_ms(3),
            DurationMs::new(5),
        )
        .await
        .unwrap(),
    );
    assert_eq!(first_lease.fencing_token, 1);
    assert!(matches!(
        claim_due(
            &store.pool,
            &activation_hash,
            1,
            "worker-b",
            at_ms(3),
            DurationMs::new(5),
        )
        .await
        .unwrap(),
        ScheduleClaim::Contended { lease_expires_at }
            if lease_expires_at == timestamp("2026-07-22T12:00:00.008Z")
    ));
    let (after_first, first_occurrence) = completed(
        complete_elapsed(&store.pool, &first_lease, at_ms(3), at_ms(4))
            .await
            .unwrap(),
    );
    assert_eq!(after_first.cursor_revision, 2);
    assert_eq!(after_first.last_occurrence_sequence, 1);
    let first_occurrence = first_occurrence.unwrap();
    assert_eq!(first_occurrence.occurrence.sequence(), 1);
    assert_eq!(
        get_occurrence(&store.pool, &first_occurrence.occurrence_hash)
            .await
            .unwrap()
            .unwrap(),
        first_occurrence
    );
    assert_eq!(
        after_first.deadline.as_ref().unwrap().next_intended_at(),
        timestamp("2026-07-22T12:00:00.006Z")
    );
    assert!(matches!(
        complete_elapsed(&store.pool, &first_lease, at_ms(3), at_ms(4))
            .await
            .unwrap(),
        CursorCompletion::LostLease
    ));

    let expired_lease = claimed(
        claim_due(
            &store.pool,
            &activation_hash,
            2,
            "worker-expired",
            at_ms(6),
            DurationMs::new(1),
        )
        .await
        .unwrap(),
    );
    let reclaimed = claimed(
        claim_due(
            &store.pool,
            &activation_hash,
            2,
            "worker-recovery",
            at_ms(7),
            DurationMs::new(5),
        )
        .await
        .unwrap(),
    );
    assert_eq!(reclaimed.fencing_token, 3);
    assert!(matches!(
        complete_elapsed(&store.pool, &expired_lease, at_ms(6), at_ms(7))
            .await
            .unwrap(),
        CursorCompletion::LostLease
    ));
    let (after_second, second_occurrence) = completed(
        complete_elapsed(&store.pool, &reclaimed, at_ms(7), at_ms(8))
            .await
            .unwrap(),
    );
    assert_eq!(after_second.cursor_revision, 3);
    assert_eq!(after_second.lease_fencing_token, 3);
    assert_eq!(after_second.last_occurrence_sequence, 2);
    assert_eq!(second_occurrence.unwrap().occurrence.sequence(), 2);
    assert_eq!(count(&store, "karma_schedule_occurrence").await, 2);

    store.pool.close().await;
    let reopened = Store::open(&url).await.unwrap();
    let deadlines = list_armed_deadlines(&reopened.pool).await.unwrap();
    assert_eq!(deadlines.len(), 1);
    assert_eq!(deadlines[0].cursor_revision(), 3);
    assert_eq!(
        deadlines[0].next_intended_at(),
        timestamp("2026-07-22T12:00:00.009Z")
    );
    assert!(matches!(
        materialize_elapsed_cursor(&reopened.pool, &activation_hash, demand_policy(), at_ms(9),)
            .await
            .unwrap(),
        CursorMaterialization::Existing(_)
    ));
    assert!(
        has_pending_schedule_occurrences(&reopened.pool)
            .await
            .unwrap()
    );
    let recovered_first = expand_pending_schedule_occurrences(
        &reopened.pool,
        NonZeroU32::new(1).unwrap(),
        NonZeroU32::new(2).unwrap(),
        at_ms(9),
    )
    .await
    .unwrap();
    assert_eq!(recovered_first.len(), 1);
    assert!(
        has_pending_schedule_occurrences(&reopened.pool)
            .await
            .unwrap()
    );
    let recovered_second = expand_pending_schedule_occurrences(
        &reopened.pool,
        NonZeroU32::new(1).unwrap(),
        NonZeroU32::new(2).unwrap(),
        at_ms(9),
    )
    .await
    .unwrap();
    assert_eq!(recovered_second.len(), 1);
    assert!(
        !has_pending_schedule_occurrences(&reopened.pool)
            .await
            .unwrap()
    );
    assert_eq!(
        store::karma::occurrences::list(&reopened.pool)
            .await
            .unwrap()
            .iter()
            .map(|row| row.cell_sequence)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    reopened.pool.close().await;
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn late_elapsed_boundaries_persist_as_one_reconstructible_batch() {
    let store = Store::open_memory().await.unwrap();
    let active = active_frequency(&store, "schedule.dense-batch").await;
    let activation_hash = active.active_activation_hash.unwrap();
    let cursor = created(
        materialize_elapsed_cursor(&store.pool, &activation_hash, demand_policy(), at_ms(0))
            .await
            .unwrap(),
    );
    assert!(
        record_deadline_admission(
            &store.pool,
            &activation_hash,
            cursor.cursor_revision,
            NonZeroU32::new(1).unwrap(),
            false,
            at_ms(0),
        )
        .await
        .unwrap()
    );
    let lease = claimed(
        claim_due(
            &store.pool,
            &activation_hash,
            cursor.cursor_revision,
            "dense-batch-worker",
            at_ms(15),
            DurationMs::new(10),
        )
        .await
        .unwrap(),
    );
    let (cursor, occurrence) = completed(
        complete_elapsed(&store.pool, &lease, at_ms(15), at_ms(15))
            .await
            .unwrap(),
    );
    let occurrence = occurrence.unwrap();
    let store::karma::schedules::ScheduleOccurrencePayload::Elapsed {
        occurrence: elapsed,
    } = &occurrence.occurrence
    else {
        panic!("expected elapsed occurrence batch");
    };
    assert_eq!(elapsed.batch.first_schedule_ordinal, 1);
    assert_eq!(elapsed.batch.covered_boundary_count(), 5);
    assert_eq!(elapsed.batch.semantic_occurrence_count(), 5);
    assert_eq!(
        elapsed
            .batch
            .individual_page(0, NonZeroU32::new(2).unwrap())
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        cursor.deadline.unwrap().next_intended_at(),
        timestamp("2026-07-22T12:00:00.018Z")
    );

    let projection: (String, String, String, i64, i64) = store::sqlx::query_as(
        "SELECT emission_kind, first_intended_at, last_intended_at,
                covered_boundary_count, semantic_occurrence_count
         FROM karma_schedule_occurrence WHERE occurrence_hash = ?",
    )
    .bind(occurrence.occurrence_hash.as_str())
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(
        projection,
        (
            "individual".to_string(),
            "2026-07-22T12:00:00.003Z".to_string(),
            "2026-07-22T12:00:00.015Z".to_string(),
            5,
            5,
        )
    );
    assert_eq!(count(&store, "karma_schedule_occurrence").await, 1);
    assert_eq!(
        get_occurrence(&store.pool, &occurrence.occurrence_hash)
            .await
            .unwrap()
            .unwrap(),
        occurrence
    );

    let first_page = expand_schedule_occurrence(
        &store.pool,
        &occurrence.occurrence_hash,
        NonZeroU32::new(2).unwrap(),
        at_ms(16),
    )
    .await
    .unwrap();
    assert_eq!(first_page.occurrences.len(), 2);
    assert_eq!(first_page.cursor.next_ordinal, 2);
    assert!(!first_page.cursor.completed);
    let second_page = expand_schedule_occurrence(
        &store.pool,
        &occurrence.occurrence_hash,
        NonZeroU32::new(2).unwrap(),
        at_ms(17),
    )
    .await
    .unwrap();
    assert_eq!(second_page.occurrences.len(), 2);
    let final_page = expand_schedule_occurrence(
        &store.pool,
        &occurrence.occurrence_hash,
        NonZeroU32::new(2).unwrap(),
        at_ms(18),
    )
    .await
    .unwrap();
    assert_eq!(final_page.occurrences.len(), 1);
    assert!(final_page.cursor.completed);
    assert_eq!(final_page.cursor.next_ordinal, 5);
    assert_eq!(final_page.cursor.total_items, 5);
    let semantic_occurrences = store::karma::occurrences::list(&store.pool).await.unwrap();
    assert_eq!(
        semantic_occurrences
            .iter()
            .map(|row| row.cell_sequence)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
    assert!(semantic_occurrences.iter().all(|row| matches!(
        row.envelope.source,
        KarmaOccurrenceSource::ScheduleTick { .. }
    )));
    let replay = expand_schedule_occurrence(
        &store.pool,
        &occurrence.occurrence_hash,
        NonZeroU32::new(2).unwrap(),
        at_ms(19),
    )
    .await
    .unwrap();
    assert!(replay.occurrences.is_empty());
    assert_eq!(
        get_expansion_cursor(&store.pool, &occurrence.occurrence_hash)
            .await
            .unwrap()
            .unwrap(),
        replay.cursor
    );
}

#[tokio::test]
async fn new_activation_supersedes_old_cursor_and_pause_removes_all_armed_work() {
    let store = Store::open_memory().await.unwrap();
    let first = active_frequency(&store, "schedule.rephase").await;
    let first_activation_hash = first.active_activation_hash.clone().unwrap();
    materialize_elapsed_cursor(
        &store.pool,
        &first_activation_hash,
        demand_policy(),
        at_ms(0),
    )
    .await
    .unwrap();

    let (tuned, _) = committed(
        set_parameters(
            &store.pool,
            SetFrequencyParametersInput {
                request_id: "schedule-set-9ms".to_string(),
                frequency_uid: first.record_uid.clone(),
                expected_handle_revision: 2,
                expected_active_revision_hash: first.head_revision_hash.clone(),
                parameter_overrides: BTreeMap::from([(
                    id("interval"),
                    FrequencyParameterValue::Duration {
                        value: DurationMs::new(9),
                    },
                )]),
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            at_ms(1),
            signer,
        )
        .await
        .unwrap(),
    );
    let tuned_activation_hash = tuned.active_activation_hash.clone().unwrap();
    let tuned_cursor = created(
        materialize_elapsed_cursor(
            &store.pool,
            &tuned_activation_hash,
            demand_policy(),
            at_ms(1),
        )
        .await
        .unwrap(),
    );
    assert_eq!(
        tuned_cursor.deadline.as_ref().unwrap().next_intended_at(),
        timestamp("2026-07-22T12:00:00.009Z")
    );
    assert_eq!(
        get_cursor(&store.pool, &first_activation_hash)
            .await
            .unwrap()
            .unwrap()
            .lifecycle,
        ScheduleCursorLifecycle::Superseded
    );
    let armed = list_armed_deadlines(&store.pool).await.unwrap();
    assert_eq!(armed.len(), 1);
    assert_eq!(armed[0].activation_hash(), &tuned_activation_hash);

    let (paused, _) = committed(
        pause(
            &store.pool,
            PauseFrequencyInput {
                request_id: "schedule-pause".to_string(),
                frequency_uid: first.record_uid.clone(),
                expected_handle_revision: 3,
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            at_ms(2),
            signer,
        )
        .await
        .unwrap(),
    );
    assert_eq!(paused.status, DefinitionStatus::Paused);
    assert!(list_armed_deadlines(&store.pool).await.unwrap().is_empty());
    assert_eq!(
        supersede_inactive_frequency_cursors(&store.pool, &first.record_uid, at_ms(2))
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        get_cursor(&store.pool, &tuned_activation_hash)
            .await
            .unwrap()
            .unwrap()
            .lifecycle,
        ScheduleCursorLifecycle::Superseded
    );
}

#[tokio::test]
async fn one_shot_boot_reconciliation_materializes_missing_and_supersedes_inactive() {
    let store = Store::open_memory().await.unwrap();
    let active = active_frequency(&store, "schedule.boot").await;
    let first = reconcile_active_schedule_cursors(&store.pool, demand_policy(), at_ms(0))
        .await
        .unwrap();
    assert_eq!(first.created_elapsed, 1);
    assert_eq!(first.existing_elapsed, 0);
    assert_eq!(first.calendar_pending_provider, 0);
    assert_eq!(first.superseded, 0);
    let second = reconcile_active_schedule_cursors(&store.pool, demand_policy(), at_ms(1))
        .await
        .unwrap();
    assert_eq!(second.created_elapsed, 0);
    assert_eq!(second.existing_elapsed, 1);

    committed(
        pause(
            &store.pool,
            PauseFrequencyInput {
                request_id: "schedule-boot-pause".to_string(),
                frequency_uid: active.record_uid,
                expected_handle_revision: 2,
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            at_ms(2),
            signer,
        )
        .await
        .unwrap(),
    );
    let paused = reconcile_active_schedule_cursors(&store.pool, demand_policy(), at_ms(2))
        .await
        .unwrap();
    assert_eq!(paused.superseded, 0);
    assert!(list_armed_deadlines(&store.pool).await.unwrap().is_empty());
}

#[tokio::test]
async fn calendar_cursor_requires_its_pinned_provider_and_persists_typed_pause() {
    let store = Store::open_memory().await.unwrap();
    let provider = calendar_provider(LocalTimeResolution::Unique {
        instant: timestamp("2026-01-01T11:00:00.000Z"),
    });
    let active = active_custom_frequency(
        &store,
        calendar_frequency("schedule.calendar", GapPolicy::ShiftForward),
    )
    .await;
    let activation_hash = active.active_activation_hash.clone().unwrap();
    let row = created(
        materialize_calendar_cursor(
            &store.pool,
            &activation_hash,
            None,
            &provider,
            demand_policy(),
            at_ms(0),
        )
        .await
        .unwrap(),
    );
    assert_eq!(row.lifecycle, ScheduleCursorLifecycle::Armed);
    assert_eq!(
        row.deadline.as_ref().unwrap().next_intended_at(),
        timestamp("2026-01-01T11:00:00.000Z")
    );
    assert!(matches!(
        row.cursor,
        store::karma::schedules::StoredScheduleCursor::Calendar { .. }
    ));
    assert_eq!(
        get_calendar_cursor(&store.pool, &activation_hash, &provider)
            .await
            .unwrap()
            .unwrap(),
        row
    );
    assert!(get_cursor(&store.pool, &activation_hash).await.is_err());
    assert!(
        record_deadline_admission(
            &store.pool,
            &activation_hash,
            1,
            NonZeroU32::new(1).unwrap(),
            false,
            at_ms(0),
        )
        .await
        .unwrap()
    );

    let calendar_lease = claimed(
        claim_due(
            &store.pool,
            &activation_hash,
            1,
            "calendar-worker",
            date_time("2026-01-02T11:00:00.000Z"),
            DurationMs::new(10),
        )
        .await
        .unwrap(),
    );
    let (advanced, occurrence) = completed(
        complete_calendar(
            &store.pool,
            &calendar_lease,
            &provider,
            date_time("2026-01-02T11:00:00.000Z"),
            date_time("2026-01-02T11:00:00.001Z"),
            std::num::NonZeroU32::new(4).unwrap(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(advanced.cursor_revision, 2);
    assert_eq!(
        advanced.deadline.as_ref().unwrap().next_intended_at(),
        timestamp("2026-01-03T11:00:00.000Z")
    );
    let occurrence = occurrence.unwrap();
    assert_eq!(occurrence.occurrence.sequence(), 1);
    assert!(matches!(
        occurrence.occurrence,
        store::karma::schedules::ScheduleOccurrencePayload::Calendar { .. }
    ));
    assert_eq!(
        get_occurrence(&store.pool, &occurrence.occurrence_hash)
            .await
            .unwrap()
            .unwrap(),
        occurrence
    );
    let expanded = expand_schedule_occurrence(
        &store.pool,
        &occurrence.occurrence_hash,
        NonZeroU32::new(4).unwrap(),
        date_time("2026-01-02T11:00:00.002Z"),
    )
    .await
    .unwrap();
    assert!(expanded.cursor.completed);
    assert_eq!(expanded.cursor.total_items, 1);
    assert!(matches!(
        expanded.occurrences[0].envelope.source,
        KarmaOccurrenceSource::CalendarCoalesced { .. }
    ));

    let wrong_provider = FakeProvider {
        revision: TzdbRevision {
            version: TzdbVersion::new("2026b+lince.1").unwrap(),
            digest: CanonicalHash::parse(format!("sha256:{}", "9".repeat(64))).unwrap(),
        },
        resolutions: provider.resolutions.clone(),
    };
    assert!(
        get_calendar_cursor(&store.pool, &activation_hash, &wrong_provider)
            .await
            .is_err()
    );

    let gap_provider = calendar_provider(LocalTimeResolution::Gap {
        before: timestamp("2026-01-01T10:59:59.999Z"),
        first_valid_after: timestamp("2026-01-01T12:00:00.000Z"),
    });
    let gap_active = active_custom_frequency(
        &store,
        calendar_frequency("schedule.calendar-gap", GapPolicy::Pause),
    )
    .await;
    let gap_hash = gap_active.active_activation_hash.unwrap();
    let paused = created(
        materialize_calendar_cursor(
            &store.pool,
            &gap_hash,
            None,
            &gap_provider,
            demand_policy(),
            at_ms(1),
        )
        .await
        .unwrap(),
    );
    assert_eq!(paused.lifecycle, ScheduleCursorLifecycle::Paused);
    assert!(paused.deadline.is_none());
    assert!(matches!(
        paused.cursor,
        store::karma::schedules::StoredScheduleCursor::CalendarPaused { .. }
    ));
    let loaded = get_calendar_cursor(&store.pool, &gap_hash, &gap_provider)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded, paused);
    let reconciliation = reconcile_active_schedule_cursors(&store.pool, demand_policy(), at_ms(2))
        .await
        .unwrap();
    assert_eq!(reconciliation.calendar_pending_provider, 2);
}

async fn active_frequency(store: &Store, slug: &str) -> FrequencyHandleRow {
    active_custom_frequency(store, frequency(slug)).await
}

async fn active_custom_frequency(store: &Store, frequency: FrequencyAst) -> FrequencyHandleRow {
    let slug = frequency.slug.as_str().to_string();
    let (created, _) = committed(
        create(
            &store.pool,
            CreateFrequencyInput {
                request_id: format!("create-{slug}"),
                frequency,
                owner_person_uid: Some(PERSON_UID.to_string()),
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            at_ms(-1),
            signer,
        )
        .await
        .unwrap(),
    );
    committed(
        activate(
            &store.pool,
            ActivateFrequencyInput {
                request_id: format!("activate-{slug}"),
                frequency_uid: created.record_uid,
                expected_handle_revision: 1,
                revision_hash: created.head_revision_hash,
                parameter_overrides: BTreeMap::new(),
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            at_ms(0),
            signer,
        )
        .await
        .unwrap(),
    )
    .0
}

fn frequency(slug: &str) -> FrequencyAst {
    FrequencyAst {
        schema: FrequencySchema::V1,
        slug: Slug::new(slug).unwrap(),
        purpose: "Exercise durable cursor scheduling".to_string(),
        tags: BTreeSet::new(),
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
            anchor: timestamp("2026-07-22T12:00:00.000Z"),
        },
        timer: FrequencyTimerAst {
            required_resolution: duration(1),
            max_lateness: duration(5),
            coalesce_window: duration(0),
        },
        missed: MissedPolicy::Replay {
            max: std::num::NonZeroU32::new(64).unwrap(),
        },
        inactive_gap: InactiveGapPolicy::SkipToNextAnchor,
        rephase: RephasePolicy::PreserveAnchor,
        overload: OverloadPolicy::PauseAndAsk,
    }
}

fn calendar_frequency(slug: &str, gap: GapPolicy) -> FrequencyAst {
    FrequencyAst {
        schema: FrequencySchema::V1,
        slug: Slug::new(slug).unwrap(),
        purpose: "Exercise provider-pinned calendar cursor scheduling".to_string(),
        tags: BTreeSet::new(),
        parameters: BTreeMap::new(),
        cadence: FrequencyCadenceAst::Calendar {
            cadence: CadenceAst {
                every: CadenceStepAst {
                    days: Some(PositiveIntegerBinding::Literal {
                        value: std::num::NonZeroU32::new(1).unwrap(),
                    }),
                    ..Default::default()
                },
                land_on: None,
                invalid_day: InvalidDay::Clamp,
                bound: CadenceBound::Unbounded,
            },
            anchor: CivilDateTime::parse_canonical("2026-01-01T08:00:00.000").unwrap(),
            timezone: TimeZoneId::new("America/Sao_Paulo").unwrap(),
            tzdb: calendar_revision(),
            gap,
            fold: FoldPolicy::Both,
        },
        timer: FrequencyTimerAst {
            required_resolution: duration(1),
            max_lateness: duration(5),
            coalesce_window: duration(0),
        },
        missed: MissedPolicy::Coalesce,
        inactive_gap: InactiveGapPolicy::ReplayByMissedPolicy,
        rephase: RephasePolicy::PreserveAnchor,
        overload: OverloadPolicy::PauseAndAsk,
    }
}

fn calendar_provider(first: LocalTimeResolution) -> FakeProvider {
    let mut resolutions = BTreeMap::from([(
        CivilDateTime::parse_canonical("2026-01-01T08:00:00.000").unwrap(),
        first,
    )]);
    for day in 2..=5 {
        resolutions.insert(
            CivilDateTime::parse_canonical(&format!("2026-01-{day:02}T08:00:00.000")).unwrap(),
            LocalTimeResolution::Unique {
                instant: timestamp(&format!("2026-01-{day:02}T11:00:00.000Z")),
            },
        );
    }
    FakeProvider {
        revision: calendar_revision(),
        resolutions,
    }
}

fn calendar_revision() -> TzdbRevision {
    TzdbRevision {
        version: TzdbVersion::new("2026a+lince.1").unwrap(),
        digest: CanonicalHash::parse(format!("sha256:{}", "8".repeat(64))).unwrap(),
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

fn created(value: CursorMaterialization) -> store::karma::schedules::ScheduleCursorRow {
    match value {
        CursorMaterialization::Created(row) => row,
        other => panic!("expected created cursor, got {other:?}"),
    }
}

fn claimed(value: ScheduleClaim) -> store::karma::schedules::ScheduleLease {
    match value {
        ScheduleClaim::Claimed(lease) => lease,
        other => panic!("expected claimed cursor, got {other:?}"),
    }
}

fn completed(
    value: CursorCompletion,
) -> (
    store::karma::schedules::ScheduleCursorRow,
    Option<store::karma::schedules::StoredScheduleOccurrence>,
) {
    match value {
        CursorCompletion::Completed { cursor, occurrence } => (cursor, occurrence),
        other => panic!("expected completed cursor, got {other:?}"),
    }
}

fn committed(commit: FrequencyMutationCommit) -> (FrequencyHandleRow, nucleus::Fact) {
    match commit {
        FrequencyMutationCommit::Committed { handle, fact } => (handle, fact),
        other => panic!("expected committed Frequency mutation, got {other:?}"),
    }
}

fn signer(hash: &str) -> Option<String> {
    Some(format!("signed:{hash}"))
}

fn at_ms(offset_ms: i64) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-07-22T12:00:00.000Z")
        .unwrap()
        .with_timezone(&Utc)
        + TimeDelta::milliseconds(offset_ms)
}

fn date_time(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

fn timestamp(value: &str) -> TimestampMs {
    TimestampMs::parse_canonical(value).unwrap()
}

async fn count(store: &Store, table: &str) -> i64 {
    let query = format!("SELECT COUNT(*) FROM {table}");
    sqlx::query_scalar(&query)
        .fetch_one(&store.pool)
        .await
        .unwrap()
}
