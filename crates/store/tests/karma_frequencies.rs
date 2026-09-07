use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use nucleus::karma::{
    CapabilitySet, CompiledSchedule, DefinitionStatus, DurationBinding, DurationMs, FrequencyAst,
    FrequencyCadenceAst, FrequencyMutationAction, FrequencyMutationEvidence,
    FrequencyParameterDefinition, FrequencyParameterValue, FrequencySchema, FrequencyTimerAst,
    InactiveGapPolicy, LocalId, MissedPolicy, OverloadPolicy, ProgramAst, ProgramSchema,
    RephasePolicy, Slug, TimestampMs,
};
use store::Store;
use store::karma::frequencies::{
    ActivateFrequencyInput, CreateFrequencyInput, FrequencyHandleRow, FrequencyMutationCommit,
    PauseFrequencyInput, ResetFrequencyParametersInput, ReviseFrequencyInput,
    SetFrequencyParametersInput, activate, create, get_activation, get_handle, get_revision, pause,
    reset_parameters, revise, set_parameters,
};
use store::karma::programs::{CreateProgramInput, create as create_program};

const PERSON_UID: &str = "r_01APS3NDEKTSV4RRFFQ69G5FAV";
const OTHER_PERSON_UID: &str = "r_01APS3NDEKTSV4RRFFQ69G5FAW";

#[tokio::test]
async fn create_replay_and_reopen_preserve_definition_and_original_result() {
    let path = std::env::temp_dir().join(format!(
        "lince-karma-frequency-{}.db",
        nucleus::new_uid("test")
    ));
    let url = format!("sqlite://{}", path.display());
    let store = Store::open(&url).await.unwrap();
    let input = create_input("sensor.persisted", "Persisted sensor", "frequency-create-1");
    let (created, created_fact) = committed(
        create(&store.pool, input.clone(), now(0), signer)
            .await
            .unwrap(),
    );
    assert_eq!(created.handle_revision, 1);
    assert_eq!(created.status, DefinitionStatus::Proven);
    assert_eq!(created.active_activation_hash, None);
    assert_eq!(created.latest_activation_hash, None);
    assert_eq!(created_fact.delta, store::exact::from_f64(0.0));
    let revision = get_revision(&store.pool, &created.head_revision_hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(revision.frequency, input.frequency);
    assert_eq!(elapsed_interval(&revision.default_compiled), 3);

    store.pool.close().await;
    let reopened = Store::open(&url).await.unwrap();
    let (replayed_handle, replayed_fact) = replayed(
        create(&reopened.pool, input, now(99), signer)
            .await
            .unwrap(),
    );
    assert_eq!(replayed_handle, created);
    assert_eq!(replayed_fact, created_fact);
    assert_eq!(count(&reopened, "karma_frequency").await, 1);
    assert_eq!(count(&reopened, "karma_frequency_revision").await, 1);
    assert_eq!(count(&reopened, "karma_frequency_activation").await, 0);
    assert_eq!(count(&reopened, "karma_frequency_request").await, 1);
    reopened.pool.close().await;
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn activation_parameter_epochs_revision_switch_and_pause_keep_exact_history() {
    let store = Store::open_memory().await.unwrap();
    let (created, _) = committed(
        create(
            &store.pool,
            create_input("sensor.flow", "Flow sensor", "create-flow"),
            now(0),
            signer,
        )
        .await
        .unwrap(),
    );
    let first_revision_hash = created.head_revision_hash.clone();
    let (active, activation_fact) = committed(
        activate(
            &store.pool,
            ActivateFrequencyInput {
                request_id: "activate-flow-1".to_string(),
                frequency_uid: created.record_uid.clone(),
                expected_handle_revision: 1,
                revision_hash: first_revision_hash.clone(),
                parameter_overrides: BTreeMap::new(),
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            now(1),
            signer,
        )
        .await
        .unwrap(),
    );
    assert_eq!(active.handle_revision, 2);
    assert_eq!(active.status, DefinitionStatus::Active);
    assert_eq!(activation_fact.delta, store::exact::from_f64(1.0));
    let first_activation_hash = active.active_activation_hash.clone().unwrap();
    let first_activation = get_activation(&store.pool, &first_activation_hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first_activation.epoch.previous_activation_hash(), None);
    assert_eq!(elapsed_interval(first_activation.epoch.compiled()), 3);

    let overrides = interval_overrides(9);
    let (tuned, tuned_fact) = committed(
        set_parameters(
            &store.pool,
            SetFrequencyParametersInput {
                request_id: "set-flow-9ms".to_string(),
                frequency_uid: created.record_uid.clone(),
                expected_handle_revision: 2,
                expected_active_revision_hash: first_revision_hash.clone(),
                parameter_overrides: overrides.clone(),
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            now(2),
            signer,
        )
        .await
        .unwrap(),
    );
    assert_eq!(tuned_fact.delta, store::exact::from_f64(0.0));
    let tuned_hash = tuned.active_activation_hash.clone().unwrap();
    let tuned_epoch = get_activation(&store.pool, &tuned_hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        tuned_epoch.epoch.previous_activation_hash(),
        Some(&first_activation_hash)
    );
    assert_eq!(elapsed_interval(tuned_epoch.epoch.compiled()), 9);
    assert!(
        set_parameters(
            &store.pool,
            SetFrequencyParametersInput {
                request_id: "set-flow-noop".to_string(),
                frequency_uid: created.record_uid.clone(),
                expected_handle_revision: 3,
                expected_active_revision_hash: first_revision_hash.clone(),
                parameter_overrides: overrides,
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            now(3),
            signer,
        )
        .await
        .is_err()
    );
    assert_eq!(count(&store, "karma_frequency_activation").await, 2);

    let (reset, _) = committed(
        reset_parameters(
            &store.pool,
            ResetFrequencyParametersInput {
                request_id: "reset-flow".to_string(),
                frequency_uid: created.record_uid.clone(),
                expected_handle_revision: 3,
                expected_active_revision_hash: first_revision_hash.clone(),
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            now(4),
            signer,
        )
        .await
        .unwrap(),
    );
    let reset_epoch = get_activation(&store.pool, reset.active_activation_hash.as_ref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(elapsed_interval(reset_epoch.epoch.compiled()), 3);

    let (revised, _) = committed(
        revise(
            &store.pool,
            ReviseFrequencyInput {
                request_id: "revise-flow".to_string(),
                frequency_uid: created.record_uid.clone(),
                expected_handle_revision: 4,
                frequency: frequency("sensor.flow", "Revised while old remains active"),
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            now(5),
            signer,
        )
        .await
        .unwrap(),
    );
    assert_ne!(revised.head_revision_hash, first_revision_hash);
    assert_eq!(
        revised.active_revision_hash,
        Some(first_revision_hash.clone())
    );
    assert_eq!(revised.active_activation_hash, reset.active_activation_hash);

    let stale = revise(
        &store.pool,
        ReviseFrequencyInput {
            request_id: "stale-frequency-revise".to_string(),
            frequency_uid: created.record_uid.clone(),
            expected_handle_revision: 4,
            frequency: frequency("sensor.flow", "Must never be inserted"),
            actor_person_uid: Some(PERSON_UID.to_string()),
        },
        now(6),
        signer,
    )
    .await
    .unwrap();
    assert!(matches!(
        stale,
        FrequencyMutationCommit::Stale {
            current_handle_revision: 5
        }
    ));
    assert_eq!(count(&store, "karma_frequency_revision").await, 2);

    let second_revision_hash = revised.head_revision_hash.clone();
    let (switched, switch_fact) = committed(
        activate(
            &store.pool,
            ActivateFrequencyInput {
                request_id: "activate-flow-2".to_string(),
                frequency_uid: created.record_uid.clone(),
                expected_handle_revision: 5,
                revision_hash: second_revision_hash.clone(),
                parameter_overrides: interval_overrides(5),
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            now(7),
            signer,
        )
        .await
        .unwrap(),
    );
    assert_eq!(switch_fact.delta, store::exact::from_f64(0.0));
    assert_eq!(switched.active_revision_hash, Some(second_revision_hash));
    assert_eq!(
        elapsed_interval(
            get_activation(
                &store.pool,
                switched.active_activation_hash.as_ref().unwrap()
            )
            .await
            .unwrap()
            .unwrap()
            .epoch
            .compiled()
        ),
        5
    );

    let (paused, pause_fact) = committed(
        pause(
            &store.pool,
            PauseFrequencyInput {
                request_id: "pause-flow".to_string(),
                frequency_uid: created.record_uid.clone(),
                expected_handle_revision: 6,
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            now(8),
            signer,
        )
        .await
        .unwrap(),
    );
    assert_eq!(paused.handle_revision, 7);
    assert_eq!(paused.status, DefinitionStatus::Paused);
    assert_eq!(paused.active_revision_hash, None);
    assert_eq!(paused.active_activation_hash, None);
    let before_pause_activation_hash = paused.latest_activation_hash.clone().unwrap();
    assert_eq!(pause_fact.delta, store::exact::from_f64(-1.0));
    assert_eq!(record_quantity(&store, &created.record_uid).await, 0.0);
    assert_eq!(count(&store, "karma_frequency_activation").await, 4);

    let (original_activation, replayed_fact) = replayed(
        activate(
            &store.pool,
            ActivateFrequencyInput {
                request_id: "activate-flow-1".to_string(),
                frequency_uid: created.record_uid.clone(),
                expected_handle_revision: 1,
                revision_hash: first_revision_hash,
                parameter_overrides: BTreeMap::new(),
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            now(99),
            signer,
        )
        .await
        .unwrap(),
    );
    assert_eq!(original_activation.handle_revision, 2);
    assert_eq!(original_activation.status, DefinitionStatus::Active);
    assert_eq!(replayed_fact, activation_fact);

    let (reactivated, reactivation_fact) = committed(
        activate(
            &store.pool,
            ActivateFrequencyInput {
                request_id: "reactivate-after-pause".to_string(),
                frequency_uid: created.record_uid.clone(),
                expected_handle_revision: 7,
                revision_hash: revised.head_revision_hash,
                parameter_overrides: interval_overrides(5),
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            now(100),
            signer,
        )
        .await
        .unwrap(),
    );
    assert_eq!(reactivation_fact.delta, store::exact::from_f64(1.0));
    assert_eq!(reactivated.handle_revision, 8);
    assert_eq!(
        reactivated.active_activation_hash,
        reactivated.latest_activation_hash
    );
    let reactivation = get_activation(
        &store.pool,
        reactivated.active_activation_hash.as_ref().unwrap(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        reactivation.epoch.previous_activation_hash(),
        Some(&before_pause_activation_hash)
    );
    assert_eq!(record_quantity(&store, &created.record_uid).await, 1.0);

    let facts = store::facts::for_record(&store.pool, &created.record_uid, 20)
        .await
        .unwrap();
    assert_eq!(facts.len(), 8);
    for fact in facts {
        let evidence: FrequencyMutationEvidence =
            serde_json::from_str(fact.payload.as_deref().unwrap()).unwrap();
        assert_eq!(evidence.frequency_uid, created.record_uid);
    }
    assert_eq!(
        serde_json::from_str::<FrequencyMutationEvidence>(pause_fact.payload.as_deref().unwrap())
            .unwrap()
            .action,
        FrequencyMutationAction::Pause
    );
}

#[tokio::test]
async fn invalid_cross_scope_collision_and_corruption_paths_fail_closed() {
    let store = Store::open_memory().await.unwrap();
    let (first, _) = committed(
        create(
            &store.pool,
            create_input("frequency.first", "First", "create-first"),
            now(0),
            signer,
        )
        .await
        .unwrap(),
    );
    let (other, _) = committed(
        create(
            &store.pool,
            create_input("frequency.other", "Other", "create-other-frequency"),
            now(1),
            signer,
        )
        .await
        .unwrap(),
    );
    assert!(
        activate(
            &store.pool,
            ActivateFrequencyInput {
                request_id: "cross-frequency-activation".to_string(),
                frequency_uid: first.record_uid.clone(),
                expected_handle_revision: 1,
                revision_hash: other.head_revision_hash,
                parameter_overrides: BTreeMap::new(),
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            now(2),
            signer,
        )
        .await
        .is_err()
    );
    assert!(
        activate(
            &store.pool,
            ActivateFrequencyInput {
                request_id: "invalid-frequency-override".to_string(),
                frequency_uid: first.record_uid.clone(),
                expected_handle_revision: 1,
                revision_hash: first.head_revision_hash.clone(),
                parameter_overrides: interval_overrides(1_001),
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            now(3),
            signer,
        )
        .await
        .is_err()
    );
    assert_eq!(count(&store, "karma_frequency_activation").await, 0);
    assert_eq!(count(&store, "karma_frequency_request").await, 2);

    let collision = CreateFrequencyInput {
        request_id: "create-first".to_string(),
        frequency: frequency("frequency.first", "First"),
        owner_person_uid: Some(PERSON_UID.to_string()),
        actor_person_uid: Some(OTHER_PERSON_UID.to_string()),
    };
    assert!(
        create(&store.pool, collision, now(4), signer)
            .await
            .is_err()
    );
    assert!(
        create_program(
            &store.pool,
            CreateProgramInput {
                request_id: "create-first".to_string(),
                program: ProgramAst {
                    schema: ProgramSchema::V1,
                    slug: Slug::new("cross-family.request").unwrap(),
                    purpose: "Must collide across request families".to_string(),
                    tags: BTreeSet::new(),
                    parameters: BTreeMap::new(),
                    nodes: BTreeMap::new(),
                    outputs: BTreeMap::new(),
                    required_capabilities: CapabilitySet::default(),
                },
                owner_person_uid: Some(PERSON_UID.to_string()),
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            now(4),
            signer,
        )
        .await
        .is_err()
    );
    assert_eq!(count(&store, "karma_program").await, 0);

    let (active, _) = committed(
        activate(
            &store.pool,
            ActivateFrequencyInput {
                request_id: "activate-for-corruption".to_string(),
                frequency_uid: first.record_uid.clone(),
                expected_handle_revision: 1,
                revision_hash: first.head_revision_hash.clone(),
                parameter_overrides: BTreeMap::new(),
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            now(5),
            signer,
        )
        .await
        .unwrap(),
    );
    let activation_hash = active.active_activation_hash.unwrap();
    sqlx::query("DROP TRIGGER karma_frequency_activation_immutable_update")
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE karma_frequency_activation SET compiled_json = '{}' WHERE activation_hash = ?",
    )
    .bind(activation_hash.as_str())
    .execute(&store.pool)
    .await
    .unwrap();
    assert!(get_activation(&store.pool, &activation_hash).await.is_err());

    sqlx::query("DROP TRIGGER karma_frequency_revision_immutable_update")
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE karma_frequency_revision SET canonical_dsl = 'corrupt' WHERE revision_hash = ?",
    )
    .bind(first.head_revision_hash.as_str())
    .execute(&store.pool)
    .await
    .unwrap();
    assert!(
        get_revision(&store.pool, &first.head_revision_hash)
            .await
            .is_err()
    );
    assert_eq!(
        get_handle(&store.pool, &first.record_uid)
            .await
            .unwrap()
            .unwrap()
            .handle_revision,
        2
    );
}

fn create_input(slug: &str, purpose: &str, request_id: &str) -> CreateFrequencyInput {
    CreateFrequencyInput {
        request_id: request_id.to_string(),
        frequency: frequency(slug, purpose),
        owner_person_uid: Some(PERSON_UID.to_string()),
        actor_person_uid: Some(PERSON_UID.to_string()),
    }
}

fn frequency(slug: &str, purpose: &str) -> FrequencyAst {
    FrequencyAst {
        schema: FrequencySchema::V1,
        slug: Slug::new(slug).unwrap(),
        purpose: purpose.to_string(),
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
            anchor: TimestampMs::parse_canonical("2026-07-22T12:00:00.000Z").unwrap(),
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

fn interval_overrides(milliseconds: i64) -> BTreeMap<LocalId, FrequencyParameterValue> {
    BTreeMap::from([(
        id("interval"),
        FrequencyParameterValue::Duration {
            value: DurationMs::new(milliseconds),
        },
    )])
}

fn id(value: &str) -> LocalId {
    LocalId::new(value).unwrap()
}

fn duration(milliseconds: i64) -> DurationBinding {
    DurationBinding::Literal {
        value: DurationMs::new(milliseconds),
    }
}

fn elapsed_interval(compiled: &nucleus::karma::CompiledFrequency) -> u64 {
    let CompiledSchedule::Elapsed { schedule } = &compiled.schedule else {
        panic!("expected elapsed schedule");
    };
    schedule.interval_ms()
}

fn committed(commit: FrequencyMutationCommit) -> (FrequencyHandleRow, nucleus::Fact) {
    match commit {
        FrequencyMutationCommit::Committed { handle, fact } => (handle, fact),
        other => panic!("expected committed Frequency mutation, got {other:?}"),
    }
}

fn replayed(commit: FrequencyMutationCommit) -> (FrequencyHandleRow, nucleus::Fact) {
    match commit {
        FrequencyMutationCommit::Replayed { handle, fact } => (handle, fact),
        other => panic!("expected replayed Frequency mutation, got {other:?}"),
    }
}

fn signer(hash: &str) -> Option<String> {
    Some(format!("signed:{hash}"))
}

fn now(offset_seconds: i64) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-07-22T12:00:00.000Z")
        .unwrap()
        .with_timezone(&Utc)
        + chrono::TimeDelta::seconds(offset_seconds)
}

async fn count(store: &Store, table: &str) -> i64 {
    let query = format!("SELECT COUNT(*) FROM {table}");
    sqlx::query_scalar(&query)
        .fetch_one(&store.pool)
        .await
        .unwrap()
}

async fn record_quantity(store: &Store, uid: &str) -> f64 {
    store::records::get(&store.pool, uid)
        .await
        .unwrap()
        .unwrap()
        .quantity_f64()
}
