use std::{
    collections::{BTreeMap, BTreeSet},
    num::{NonZeroU32, NonZeroUsize},
    sync::Arc,
};

use chrono::{TimeDelta, TimeZone, Utc};
use engine::Engine;
use engine::actions::Action;
use engine::karma_runtime::{KarmaDeadlineDirectorConfig, TokioDeadlineClock};
use nucleus::karma::{
    CapabilitySet, DefinitionStatus, DispatcherResourceGrant, DurationBinding, DurationMs,
    FrequencyAst, FrequencyCadenceAst, FrequencyParameterDefinition, FrequencySchema,
    FrequencyTimerAst, HostTimerCapabilities, InactiveGapPolicy, LocalId, MissedPolicy,
    OverloadPolicy, ProgramAst, ProgramSchema, RationalRate, RephasePolicy, ScheduleDemandCapacity,
    ScheduleWorkloadUpperBounds, SchedulerCalibration, Slug, TimestampMs,
};
use protein::{Include, Predicate, Protein, Source};

const PERSON_UID: &str = "r_01APS3NDEKTSV4RRFFQ69G5FAV";

async fn engine_with_person() -> Engine {
    let engine = Engine::open_memory().await.unwrap();
    store::records::create_with_uid(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Principal",
            body: "",
            quantity: store::exact::zero(),
        },
        PERSON_UID,
    )
    .await
    .unwrap();
    engine
}

#[tokio::test]
async fn typed_program_actions_preserve_replay_actor_and_stale_cas() {
    let engine = engine_with_person().await;
    let now = Utc
        .timestamp_millis_opt(
            TimestampMs::parse_canonical("2026-07-22T12:00:00.000Z")
                .unwrap()
                .as_millis(),
        )
        .single()
        .unwrap();
    let create = Action::CreateKarmaProgram {
        request_id: "action-create-program".to_string(),
        program: program("action.program", "Typed Action Program"),
        owner_person_uid: Some(PERSON_UID.to_string()),
    };
    let first = engine
        .act_at(create.clone(), Some(PERSON_UID.to_string()), now)
        .await
        .unwrap();
    let program_uid = first.created.unwrap();
    assert_eq!(first.facts.len(), 1);
    assert_eq!(first.facts[0].actor_uid.as_deref(), Some(PERSON_UID));

    let replay = engine
        .act_at(create, Some(PERSON_UID.to_string()), now)
        .await
        .unwrap();
    assert_eq!(replay.created.as_deref(), Some(program_uid.as_str()));
    assert!(
        replay.facts.is_empty(),
        "a replay must not republish its Fact"
    );

    engine
        .act_at(
            Action::ReviseKarmaProgram {
                request_id: "action-revise-program".to_string(),
                program_uid: program_uid.clone(),
                expected_handle_revision: 1,
                program: program("action.program", "Revised Action Program"),
            },
            Some(PERSON_UID.to_string()),
            now,
        )
        .await
        .unwrap();
    let stale = engine
        .act_at(
            Action::PauseKarmaProgram {
                request_id: "action-stale-program".to_string(),
                program_uid: program_uid.clone(),
                expected_handle_revision: 1,
            },
            Some(PERSON_UID.to_string()),
            now,
        )
        .await
        .unwrap_err();
    assert_eq!(stale.code(), Some("karma_stale_handle_revision"));
    assert_eq!(
        store::karma::programs::get_handle(&engine.store.pool, &program_uid)
            .await
            .unwrap()
            .unwrap()
            .handle_revision,
        2
    );
    let rows = protein::execute(
        &engine.store,
        &karma_query(vec![Predicate::KindEq("program".to_string())]),
    )
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["uid"], program_uid);
    assert_eq!(rows[0]["handle_revision"], 2);
    assert_eq!(
        rows[0].pointer("/action_templates/pause/action"),
        Some(&serde_json::json!("pause-karma-program"))
    );
}

#[tokio::test]
async fn typed_frequency_action_requires_runtime_and_never_uses_legacy_frequency_rows() {
    let engine = engine_with_person().await;
    let anchor = TimestampMs::parse_canonical("2026-07-22T12:00:00.000Z").unwrap();
    let now = Utc
        .timestamp_millis_opt(anchor.as_millis())
        .single()
        .unwrap();
    let created = engine
        .act_at(
            Action::CreateKarmaFrequency {
                request_id: "action-create-frequency".to_string(),
                frequency: frequency(anchor),
                owner_person_uid: Some(PERSON_UID.to_string()),
            },
            Some(PERSON_UID.to_string()),
            now,
        )
        .await
        .unwrap();
    let frequency_uid = created.created.unwrap();
    let handle = store::karma::frequencies::get_handle(&engine.store.pool, &frequency_uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(handle.status, DefinitionStatus::Proven);

    let activation = Action::ActivateKarmaFrequency {
        request_id: "action-activate-frequency".to_string(),
        frequency_uid: frequency_uid.clone(),
        expected_handle_revision: 1,
        revision_hash: handle.head_revision_hash,
        parameter_overrides: BTreeMap::new(),
    };
    let activation_wire = serde_json::to_value(&activation).unwrap();
    assert_eq!(activation_wire["action"], "activate-karma-frequency");
    assert!(matches!(
        serde_json::from_value::<Action>(activation_wire).unwrap(),
        Action::ActivateKarmaFrequency {
            expected_handle_revision: 1,
            ..
        }
    ));
    let unavailable = engine
        .act_at(activation.clone(), Some(PERSON_UID.to_string()), now)
        .await
        .unwrap_err();
    assert_eq!(unavailable.code(), Some("karma_runtime_unconfigured"));

    engine
        .install_karma_runtime_config(runtime_config())
        .unwrap();
    let active = engine
        .act_at(activation, Some(PERSON_UID.to_string()), now)
        .await
        .unwrap();
    assert_eq!(active.created.as_deref(), Some(frequency_uid.as_str()));
    let handle = store::karma::frequencies::get_handle(&engine.store.pool, &frequency_uid)
        .await
        .unwrap()
        .unwrap();
    let activation_hash = handle.active_activation_hash.unwrap();
    let cursor = store::karma::schedules::get_cursor(&engine.store.pool, &activation_hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cursor.admitted_resolution_ms, NonZeroU32::new(1));

    let lease = match store::karma::schedules::claim_due(
        &engine.store.pool,
        &activation_hash,
        cursor.cursor_revision,
        "karma-action-occurrence",
        now + TimeDelta::milliseconds(3),
        DurationMs::new(100),
    )
    .await
    .unwrap()
    {
        store::karma::schedules::ScheduleClaim::Claimed(lease) => lease,
        other => panic!("expected typed Action cursor claim, got {other:?}"),
    };
    store::karma::schedules::complete_elapsed(
        &engine.store.pool,
        &lease,
        now + TimeDelta::milliseconds(3),
        now + TimeDelta::milliseconds(3),
    )
    .await
    .unwrap();

    let rows = protein::execute(&engine.store, &karma_query(Vec::new()))
        .await
        .unwrap();
    let kinds = rows
        .iter()
        .filter_map(|row| row["object_kind"].as_str())
        .collect::<BTreeSet<_>>();
    assert!(kinds.contains("frequency"));
    assert!(kinds.contains("frequency_revision"));
    assert!(kinds.contains("frequency_activation"));
    assert!(kinds.contains("schedule_cursor"));
    assert!(kinds.contains("schedule_occurrence"));
    let occurrence_row = rows
        .iter()
        .find(|row| row["object_kind"] == "schedule_occurrence")
        .unwrap();
    assert_eq!(
        occurrence_row.pointer("/occurrence/cadence"),
        Some(&serde_json::json!("elapsed"))
    );
    assert_eq!(
        occurrence_row.pointer("/occurrence/occurrence/batch/range/count"),
        Some(&serde_json::json!(1))
    );
    let remote = protein::execute_for(&engine.store, &karma_query(Vec::new()), Some(PERSON_UID))
        .await
        .unwrap();
    assert!(remote.is_empty());
    let unsupported = protein::execute(
        &engine.store,
        &karma_query(vec![Predicate::QuantityGt(store::exact::zero())]),
    )
    .await
    .unwrap_err();
    assert_eq!(
        protein::error_code(&unsupported).as_deref(),
        Some("protein_karma_unsupported_predicate")
    );
}

fn karma_query(filter: Vec<Predicate>) -> Protein {
    Protein {
        source: Source::Karma,
        filter,
        fields: None,
        include: Include::default(),
        aggregate: None,
        order: Vec::new(),
        limit: None,
    }
}

fn program(slug: &str, purpose: &str) -> ProgramAst {
    ProgramAst {
        schema: ProgramSchema::V1,
        slug: Slug::new(slug).unwrap(),
        purpose: purpose.to_string(),
        tags: BTreeSet::new(),
        parameters: BTreeMap::new(),
        nodes: BTreeMap::new(),
        outputs: BTreeMap::new(),
        required_capabilities: CapabilitySet::default(),
    }
}

fn frequency(anchor: TimestampMs) -> FrequencyAst {
    FrequencyAst {
        schema: FrequencySchema::V1,
        slug: Slug::new("action.frequency").unwrap(),
        purpose: "Typed Action Frequency".to_string(),
        tags: BTreeSet::new(),
        parameters: BTreeMap::from([(
            LocalId::new("interval").unwrap(),
            FrequencyParameterDefinition::Duration {
                default: DurationMs::new(3),
                minimum: DurationMs::new(1),
                maximum: DurationMs::new(1_000),
            },
        )]),
        cadence: FrequencyCadenceAst::Elapsed {
            interval: DurationBinding::Parameter {
                parameter: LocalId::new("interval").unwrap(),
            },
            anchor,
        },
        timer: FrequencyTimerAst {
            required_resolution: duration(1),
            max_lateness: duration(5),
            coalesce_window: duration(0),
        },
        missed: MissedPolicy::Replay {
            max: NonZeroU32::new(16).unwrap(),
        },
        inactive_gap: InactiveGapPolicy::SkipToNextAnchor,
        rephase: RephasePolicy::PreserveAnchor,
        overload: OverloadPolicy::PauseAndAsk,
    }
}

fn runtime_config() -> KarmaDeadlineDirectorConfig {
    let maximum = RationalRate::new(u64::MAX, 1).unwrap();
    KarmaDeadlineDirectorConfig::new(
        HostTimerCapabilities::new(
            [NonZeroU32::new(1).unwrap()],
            NonZeroUsize::new(100).unwrap(),
        )
        .unwrap(),
        DispatcherResourceGrant::new(
            NonZeroU32::new(1).unwrap(),
            NonZeroUsize::new(4).unwrap(),
            NonZeroUsize::new(100).unwrap(),
            false,
        ),
        ScheduleWorkloadUpperBounds::new(0, NonZeroU32::new(1).unwrap(), 0, 256),
        SchedulerCalibration::new(NonZeroU32::new(1_000).unwrap()),
        ScheduleDemandCapacity {
            semantic_ticks_per_second: maximum,
            timer_wakes_per_second: maximum,
            scheduler_cpu_ns_per_second: maximum,
            evaluator_fuel_per_second: maximum,
            writes_per_second: maximum,
            effects_per_second: maximum,
            trace_bytes_per_second: maximum,
        },
        Arc::new(TokioDeadlineClock::new(DurationMs::new(250)).unwrap()),
        "karma-action-test".to_string(),
        DurationMs::new(100),
        NonZeroU32::new(16).unwrap(),
        std::iter::empty(),
    )
    .unwrap()
}

fn duration(milliseconds: i64) -> DurationBinding {
    DurationBinding::Literal {
        value: DurationMs::new(milliseconds),
    }
}

#[tokio::test]
async fn acting_on_a_program_publishes_it_to_the_organs_other_cells() {
    let engine = engine_with_person().await;
    let now = Utc
        .timestamp_millis_opt(
            TimestampMs::parse_canonical("2026-07-22T12:00:00.000Z")
                .unwrap()
                .as_millis(),
        )
        .single()
        .unwrap();
    let created = engine
        .act_at(
            Action::CreateKarmaProgram {
                request_id: "publish-create".to_string(),
                program: program("publish.program", "Published Action Program"),
                owner_person_uid: Some(PERSON_UID.to_string()),
            },
            Some(PERSON_UID.to_string()),
            now,
        )
        .await
        .unwrap();
    let program_uid = created.created.unwrap();

    let before = store::records::get_extension(
        &engine.store.pool,
        &program_uid,
        store::karma::sync::NAMESPACE,
    )
    .await
    .unwrap();
    assert_eq!(
        before.as_ref().and_then(|fds| fds.get("program")),
        Some(&serde_json::Value::Null),
        "an inactive Program publishes as null rather than not publishing at all"
    );

    let handle = store::karma::programs::get_handle(&engine.store.pool, &program_uid)
        .await
        .unwrap()
        .unwrap();
    engine
        .act_at(
            Action::ActivateKarmaProgram {
                request_id: "publish-activate".to_string(),
                program_uid: program_uid.clone(),
                expected_handle_revision: handle.handle_revision,
                revision_hash: handle.head_revision_hash.clone(),
            },
            Some(PERSON_UID.to_string()),
            now,
        )
        .await
        .unwrap();

    let published = store::records::get_extension(
        &engine.store.pool,
        &program_uid,
        store::karma::sync::NAMESPACE,
    )
    .await
    .unwrap()
    .expect("activating publishes the definition");
    assert_eq!(
        published["program"]["hash"].as_str(),
        Some(handle.head_revision_hash.as_str()),
        "what is published must be the revision that is actually active"
    );
    assert!(
        published["program"]["ast"].is_object(),
        "the rule itself must travel, not merely its hash"
    );
}
