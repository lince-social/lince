use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU32,
};

use chrono::{DateTime, TimeZone, Utc};
use nucleus::karma::{
    BinaryOperator, CandidateRoute, CapabilitySet, DurationBinding, DurationMs,
    EvaluationErrorCode, EvaluationLimits, ExpressionAst, FrequencyAst, FrequencyCadenceAst,
    FrequencyParameterDefinition, FrequencySchema, FrequencyTimerAst, InactiveGapPolicy,
    InputBinding, KarmaOccurrenceEnvelope, KarmaRunOutcome, LateEventPolicy, LiteralValue, LocalId,
    MissedPolicy, NodeAst, NodeOperation, OutputRef, OverloadPolicy, PortContract, ProgramAst,
    ProgramSchema, ProofStatus, ReferenceKind, RephasePolicy, ResolvedReference,
    SemanticScheduleTick, Sensitivity, SimulationStatePolicy, Slug, StateContract,
    StateMigrationPolicy, StatePersistence, StateResetPolicy, TimestampMs, TriggerSource, TypedUid,
    ValueType, prove_program,
};
use store::Store;
use store::karma::frequencies::{
    ActivateFrequencyInput, CreateFrequencyInput, FrequencyHandleRow, FrequencyMutationCommit,
    activate as activate_frequency, create as create_frequency,
};
use store::karma::programs::{
    ActivateProgramInput, CreateProgramInput, ProgramHandleRow, ProgramMutationCommit,
    ReviseProgramInput, activate as activate_program, create as create_program,
    revise as revise_program,
};
use store::karma::runs::{
    has_pending_occurrences, list_epochs, list_runs, process_next_occurrence,
};

#[tokio::test]
async fn frozen_epoch_runs_exact_program_revisions_in_cell_order() {
    let store = Store::open_memory().await.unwrap();
    let now = instant();
    let frequency = active_frequency(&store, now).await;
    let activation_hash = frequency.active_activation_hash.clone().unwrap();
    let matching = active_program(
        &store,
        "run.matching",
        &frequency.record_uid,
        "matching",
        now,
    )
    .await;
    let unrelated_uid = "r_01ARZ3NDEKTSV4RRFFQ69G5FAX";
    let unrelated = active_program(&store, "run.unrelated", unrelated_uid, "unrelated", now).await;

    let tick = SemanticScheduleTick::new(activation_hash, 1, timestamp()).unwrap();
    let envelope = KarmaOccurrenceEnvelope::schedule_tick(hash('7'), tick, None).unwrap();
    let ingested = store::karma::occurrences::ingest(&store.pool, &envelope, now)
        .await
        .unwrap();
    let occurrence_hash = match ingested {
        store::karma::occurrences::KarmaOccurrenceCommit::Inserted(row) => row.occurrence_hash,
        other => panic!("expected inserted occurrence, got {other:?}"),
    };

    let limits = EvaluationLimits {
        fuel: 100,
        max_expression_depth: 16,
    };
    let first =
        process_next_occurrence(&store.pool, limits, NonZeroU32::new(1).unwrap(), now, None)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(first.runs.len(), 1);
    assert!(!first.completed);
    let frozen = list_epochs(&store.pool).await.unwrap().pop().unwrap();
    assert_eq!(frozen.epoch.occurrence_hash, occurrence_hash);
    assert_eq!(frozen.epoch.cell_sequence, 1);
    assert_eq!(frozen.epoch.members.len(), 2);
    assert!(frozen.epoch.members.iter().any(|member| {
        member.program_uid == matching.record_uid
            && member.revision_hash == matching.head_revision_hash
    }));
    assert!(frozen.epoch.members.iter().any(|member| {
        member.program_uid == unrelated.record_uid
            && member.revision_hash == unrelated.head_revision_hash
    }));

    let late = active_program(
        &store,
        "run.too-late",
        &frequency.record_uid,
        "too-late",
        now,
    )
    .await;
    let second =
        process_next_occurrence(&store.pool, limits, NonZeroU32::new(1).unwrap(), now, None)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(second.runs.len(), 1);
    assert!(second.completed);
    assert!(!has_pending_occurrences(&store.pool).await.unwrap());
    assert!(
        process_next_occurrence(&store.pool, limits, NonZeroU32::new(1).unwrap(), now, None)
            .await
            .unwrap()
            .is_none()
    );

    let runs = list_runs(&store.pool).await.unwrap();
    assert_eq!(runs.len(), 2);
    assert!(runs.iter().all(|row| row.run.cell_sequence == 1));
    assert!(
        runs.iter()
            .all(|row| row.run.program_uid != late.record_uid)
    );
    let succeeded = runs
        .iter()
        .find_map(|row| match &row.run.outcome {
            KarmaRunOutcome::Succeeded { replay } => Some(replay),
            _ => None,
        })
        .expect("matching Frequency Program succeeds");
    let replayed = succeeded.verify_and_replay().unwrap();
    assert_eq!(
        replayed.outputs.get(&id("event")),
        Some(&nucleus::karma::LiteralValue::Bool { value: true })
    );
    assert_eq!(
        runs.iter()
            .filter(|row| matches!(row.run.outcome, KarmaRunOutcome::NotApplicable { .. }))
            .count(),
        1
    );
}

#[tokio::test]
async fn mid_epoch_restart_preserves_order_deduplication_and_no_effect_boundary() {
    let path =
        std::env::temp_dir().join(format!("lince-karma-runs-{}.db", nucleus::new_uid("test")));
    let url = format!("sqlite://{}", path.display());
    let store = Store::open(&url).await.unwrap();
    let now = instant();
    let frequency = active_frequency(&store, now).await;
    let activation_hash = frequency.active_activation_hash.clone().unwrap();
    active_program(
        &store,
        "restart.matching",
        &frequency.record_uid,
        "restart-matching",
        now,
    )
    .await;
    active_program(
        &store,
        "restart.unrelated",
        "r_01ARZ3NDEKTSV4RRFFQ69G5FAY",
        "restart-unrelated",
        now,
    )
    .await;
    for (ordinal, digit) in [(1, '5'), (2, '6')] {
        let tick =
            SemanticScheduleTick::new(activation_hash.clone(), ordinal, timestamp()).unwrap();
        let envelope = KarmaOccurrenceEnvelope::schedule_tick(hash(digit), tick, None).unwrap();
        store::karma::occurrences::ingest(&store.pool, &envelope, now)
            .await
            .unwrap();
    }
    let first = process_next_occurrence(
        &store.pool,
        EvaluationLimits::default(),
        NonZeroU32::new(1).unwrap(),
        now,
        None,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(first.cell_sequence, 1);
    assert!(!first.completed);
    assert_eq!(list_runs(&store.pool).await.unwrap().len(), 1);
    store.pool.close().await;

    let reopened = Store::open(&url).await.unwrap();
    active_program(
        &reopened,
        "restart.late",
        &frequency.record_uid,
        "restart-late",
        now,
    )
    .await;
    let counts_before = reaction_counts(&reopened).await;
    let resumed = process_next_occurrence(
        &reopened.pool,
        EvaluationLimits::default(),
        NonZeroU32::new(8).unwrap(),
        now,
        None,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(resumed.cell_sequence, 1);
    assert!(resumed.completed);
    assert_eq!(resumed.runs.len(), 1);
    let next = process_next_occurrence(
        &reopened.pool,
        EvaluationLimits::default(),
        NonZeroU32::new(8).unwrap(),
        now,
        None,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(next.cell_sequence, 2);
    assert!(next.completed);
    assert_eq!(next.runs.len(), 3);
    assert_eq!(reaction_counts(&reopened).await, counts_before);
    assert!(
        process_next_occurrence(
            &reopened.pool,
            EvaluationLimits::default(),
            NonZeroU32::new(8).unwrap(),
            now,
            None,
        )
        .await
        .unwrap()
        .is_none()
    );
    let epochs = list_epochs(&reopened.pool).await.unwrap();
    assert_eq!(epochs.len(), 2);
    assert_eq!(epochs[0].epoch.members.len(), 2);
    assert_eq!(epochs[1].epoch.members.len(), 3);
    let runs = list_runs(&reopened.pool).await.unwrap();
    assert_eq!(
        runs.iter()
            .map(|row| (row.run.cell_sequence, row.run.member_ordinal))
            .collect::<Vec<_>>(),
        vec![(1, 0), (1, 1), (2, 0), (2, 1), (2, 2)]
    );
    let hashes = runs
        .iter()
        .map(|row| row.run_hash.clone())
        .collect::<Vec<_>>();
    for row in &runs {
        if let KarmaRunOutcome::Succeeded { replay } = &row.run.outcome {
            replay.verify_and_replay().unwrap();
        }
    }
    reopened.pool.close().await;

    let verified = Store::open(&url).await.unwrap();
    assert_eq!(
        list_runs(&verified.pool)
            .await
            .unwrap()
            .iter()
            .map(|row| row.run_hash.clone())
            .collect::<Vec<_>>(),
        hashes
    );
    assert_eq!(reaction_counts(&verified).await, counts_before);
    verified.pool.close().await;
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn deterministic_evaluation_failure_is_terminal_and_does_not_poison_cell_order() {
    let store = Store::open_memory().await.unwrap();
    let now = instant();
    let frequency = active_frequency(&store, now).await;
    let created = committed_program(
        create_program(
            &store.pool,
            CreateProgramInput {
                request_id: "create-failing".to_string(),
                program: failing_program(&frequency.record_uid),
                owner_person_uid: None,
                actor_person_uid: None,
            },
            now,
            |_| None,
        )
        .await
        .unwrap(),
    );
    committed_program(
        activate_program(
            &store.pool,
            ActivateProgramInput {
                request_id: "activate-failing".to_string(),
                program_uid: created.record_uid,
                expected_handle_revision: 1,
                revision_hash: created.head_revision_hash,
                actor_person_uid: None,
            },
            now,
            |_| None,
        )
        .await
        .unwrap(),
    );
    for (ordinal, digit) in [(1, '3'), (2, '4')] {
        let tick = SemanticScheduleTick::new(
            frequency.active_activation_hash.clone().unwrap(),
            ordinal,
            timestamp(),
        )
        .unwrap();
        store::karma::occurrences::ingest(
            &store.pool,
            &KarmaOccurrenceEnvelope::schedule_tick(hash(digit), tick, None).unwrap(),
            now,
        )
        .await
        .unwrap();
    }
    for expected_sequence in [1, 2] {
        let turn = process_next_occurrence(
            &store.pool,
            EvaluationLimits::default(),
            NonZeroU32::new(8).unwrap(),
            now,
            None,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(turn.cell_sequence, expected_sequence);
        assert!(turn.completed);
        assert!(matches!(
            turn.runs[0].run.outcome,
            KarmaRunOutcome::EvaluationFailed {
                ref error,
                ..
            } if error.code == EvaluationErrorCode::DivisionByZero
        ));
    }
    assert!(!has_pending_occurrences(&store.pool).await.unwrap());
    assert_eq!(list_runs(&store.pool).await.unwrap().len(), 2);
}

#[tokio::test]
async fn program_control_state_commits_with_runs_and_survives_restart() {
    let path =
        std::env::temp_dir().join(format!("lince-karma-state-{}.db", nucleus::new_uid("test")));
    let url = format!("sqlite://{}", path.display());
    let store = Store::open(&url).await.unwrap();
    let now = instant();
    let frequency = active_frequency(&store, now).await;
    let program = active_program_definition(
        &store,
        cooldown_program(&frequency.record_uid),
        "cooldown-state",
        now,
    )
    .await;
    let activation_hash = frequency.active_activation_hash.clone().unwrap();

    for (sequence, offset, expected, digit) in [(1, 0, true, 'a'), (2, 5, false, 'b')] {
        ingest_tick(&store, &activation_hash, sequence, offset, digit, now).await;
        let turn = process_next_occurrence(
            &store.pool,
            EvaluationLimits::default(),
            NonZeroU32::new(8).unwrap(),
            now,
            None,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(successful_bool(&turn.runs[0], "allowed"), expected);
    }
    let before_restart =
        store::karma::states::get_node_state(&store.pool, &program.record_uid, &id("cooldown"))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(before_restart.state_revision, 2);
    store.pool.close().await;

    let reopened = Store::open(&url).await.unwrap();
    ingest_tick(&reopened, &activation_hash, 3, 10, 'c', now).await;
    let third = process_next_occurrence(
        &reopened.pool,
        EvaluationLimits::default(),
        NonZeroU32::new(8).unwrap(),
        now,
        None,
    )
    .await
    .unwrap()
    .unwrap();
    assert!(successful_bool(&third.runs[0], "allowed"));
    let state =
        store::karma::states::get_node_state(&reopened.pool, &program.record_uid, &id("cooldown"))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(state.state_revision, 3);
    assert!(matches!(
        state.state,
        Some(nucleus::karma::PersistedProgramNodeState::Control {
            value: nucleus::karma::ControlState::Cooldown {
                last_allowed_at: Some(value),
                last_observed_at: Some(observed),
            },
        }) if value == timestamp_ms(10) && observed == timestamp_ms(10)
    ));
    let events = store::karma::states::list_events(&reopened.pool)
        .await
        .unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].event.previous_event_hash, None);
    assert_eq!(
        events[1].event.previous_event_hash.as_ref(),
        Some(&events[0].event_hash)
    );
    assert_eq!(
        events[2].event.previous_event_hash.as_ref(),
        Some(&events[1].event_hash)
    );
    reopened.pool.close().await;
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn activation_reset_uses_frozen_handle_generation_and_is_auditable() {
    let store = Store::open_memory().await.unwrap();
    let now = instant();
    let frequency = active_frequency(&store, now).await;
    let first_definition = cooldown_program_policy(
        &frequency.record_uid,
        "Initial activation generation",
        StateResetPolicy::OnProgramActivation,
        StateMigrationPolicy::CompatibleTypeOnly,
    );
    let first = active_program_definition(&store, first_definition, "activation-reset", now).await;
    let activation_hash = frequency.active_activation_hash.clone().unwrap();
    ingest_tick(&store, &activation_hash, 1, 0, 'd', now).await;
    let initial = process_next_occurrence(
        &store.pool,
        EvaluationLimits::default(),
        NonZeroU32::new(8).unwrap(),
        now,
        None,
    )
    .await
    .unwrap()
    .unwrap();
    assert!(successful_bool(&initial.runs[0], "allowed"));

    let revised = committed_program(
        revise_program(
            &store.pool,
            ReviseProgramInput {
                request_id: "revise-activation-reset".to_string(),
                program_uid: first.record_uid.clone(),
                expected_handle_revision: 2,
                program: cooldown_program_policy(
                    &frequency.record_uid,
                    "New activation generation",
                    StateResetPolicy::OnProgramActivation,
                    StateMigrationPolicy::CompatibleTypeOnly,
                ),
                actor_person_uid: None,
            },
            now,
            |_| None,
        )
        .await
        .unwrap(),
    );
    let reactivated = committed_program(
        activate_program(
            &store.pool,
            ActivateProgramInput {
                request_id: "reactivate-reset".to_string(),
                program_uid: first.record_uid.clone(),
                expected_handle_revision: 3,
                revision_hash: revised.head_revision_hash,
                actor_person_uid: None,
            },
            now,
            |_| None,
        )
        .await
        .unwrap(),
    );
    assert_eq!(reactivated.handle_revision, 4);
    ingest_tick(&store, &activation_hash, 2, 5, 'e', now).await;
    let after_activation = process_next_occurrence(
        &store.pool,
        EvaluationLimits::default(),
        NonZeroU32::new(8).unwrap(),
        now,
        None,
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        successful_bool(&after_activation.runs[0], "allowed"),
        "activation reset must discard the previous cooldown window"
    );
    let events = store::karma::states::list_events(&store.pool)
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(
        events[1].event.reset_reason,
        Some(nucleus::karma::ProgramStateResetReason::ProgramActivation)
    );
    assert_eq!(events[1].event.activation_handle_revision, 4);
}

#[tokio::test]
async fn act_routed_candidate_is_atomic_durable_and_still_inert() {
    const ACTOR_PERSON_UID: &str = "p_01ARZ3NDEKTSV4RRFFQ69G5FAW";

    let store = Store::open_memory().await.unwrap();
    let now = instant();
    let frequency = active_frequency(&store, now).await;
    let program = active_program_definition(
        &store,
        candidate_program(&frequency.record_uid),
        "candidate",
        now,
    )
    .await;
    let before = reaction_counts(&store).await;
    ingest_tick(
        &store,
        &frequency.active_activation_hash.unwrap(),
        1,
        0,
        'f',
        now,
    )
    .await;
    let after_ingress = reaction_counts(&store).await;
    let turn = process_next_occurrence(
        &store.pool,
        EvaluationLimits::default(),
        NonZeroU32::new(8).unwrap(),
        now,
        None,
    )
    .await
    .unwrap()
    .unwrap();
    assert!(matches!(
        turn.runs[0].run.outcome,
        KarmaRunOutcome::Succeeded { .. }
    ));
    let candidates = store::karma::candidates::list(&store.pool).await.unwrap();
    assert_eq!(candidates.len(), 1);
    let candidate = &candidates[0];
    assert_eq!(candidate.proposal.program_uid, program.record_uid);
    assert_eq!(candidate.proposal.route, CandidateRoute::Act);
    assert_eq!(candidate.proposal.template.as_str(), "test.act");
    assert_eq!(
        candidate.proposal.status,
        nucleus::karma::CandidateStatus::Proposed
    );
    assert_eq!(candidate.proposal.source_run_hash, turn.runs[0].run_hash);
    let after = reaction_counts(&store).await;
    assert_eq!(after.0, after_ingress.0);
    assert_eq!((after.1, after.2, after.3), (before.1, before.2, before.3));

    let review = store::karma::candidates::RespondCandidateInput {
        request_id: "accept-candidate".to_string(),
        candidate_hash: candidate.candidate_hash.clone(),
        expected_state_revision: 1,
        response: nucleus::karma::CandidateReviewAction::Accept,
        actor_person_uid: Some(ACTOR_PERSON_UID.to_string()),
        authorizing_grant_uid: None,
    };
    let accepted = store::karma::candidates::respond(&store.pool, review.clone(), now, |_| None)
        .await
        .unwrap();
    let store::karma::candidates::CandidateReviewCommit::Committed {
        state: accepted_state,
        fact: accepted_fact,
        ..
    } = accepted
    else {
        panic!("expected committed candidate acceptance");
    };
    assert_eq!(accepted_state.state_revision, 2);
    assert_eq!(
        accepted_state.actor_person_uid.as_deref(),
        Some(ACTOR_PERSON_UID)
    );
    assert_eq!(accepted_fact.actor_uid.as_deref(), Some(ACTOR_PERSON_UID));
    assert_eq!(
        accepted_state.status,
        nucleus::karma::CandidateStatus::Accepted
    );
    assert!(matches!(
        store::karma::candidates::respond(&store.pool, review, now, |_| None)
            .await
            .unwrap(),
        store::karma::candidates::CandidateReviewCommit::Replayed { state, .. }
            if state == accepted_state
    ));
    assert!(matches!(
        store::karma::candidates::respond(
            &store.pool,
            store::karma::candidates::RespondCandidateInput {
                request_id: "stale-dismiss".to_string(),
                candidate_hash: candidate.candidate_hash.clone(),
                expected_state_revision: 1,
                response: nucleus::karma::CandidateReviewAction::Dismiss,
                actor_person_uid: None,
                authorizing_grant_uid: None,
            },
            now,
            |_| None,
        )
        .await
        .unwrap(),
        store::karma::candidates::CandidateReviewCommit::Stale {
            current_state_revision: 2
        }
    ));
    let dismissed = store::karma::candidates::respond(
        &store.pool,
        store::karma::candidates::RespondCandidateInput {
            request_id: "dismiss-after-accept".to_string(),
            candidate_hash: candidate.candidate_hash.clone(),
            expected_state_revision: 2,
            response: nucleus::karma::CandidateReviewAction::Dismiss,
            actor_person_uid: None,
            authorizing_grant_uid: None,
        },
        now,
        |_| None,
    )
    .await
    .unwrap();
    assert!(matches!(
        dismissed,
        store::karma::candidates::CandidateReviewCommit::Committed { state, .. }
            if state.state_revision == 3
                && state.status == nucleus::karma::CandidateStatus::Dismissed
    ));
    let snoozed = store::karma::candidates::respond(
        &store.pool,
        store::karma::candidates::RespondCandidateInput {
            request_id: "snooze-after-dismiss".to_string(),
            candidate_hash: candidate.candidate_hash.clone(),
            expected_state_revision: 3,
            response: nucleus::karma::CandidateReviewAction::Snooze {
                until: timestamp_ms(1_000),
            },
            actor_person_uid: None,
            authorizing_grant_uid: None,
        },
        now,
        |_| None,
    )
    .await
    .unwrap();
    assert!(matches!(
        snoozed,
        store::karma::candidates::CandidateReviewCommit::Committed { state, .. }
            if state.state_revision == 4
                && state.status == nucleus::karma::CandidateStatus::Snoozed
                && state.snoozed_until == Some(timestamp_ms(1_000))
    ));
    let reviewed = reaction_counts(&store).await;
    assert_eq!(reviewed.1, after.1 + 3);
    assert_eq!(
        (reviewed.0, reviewed.2, reviewed.3),
        (after.0, after.2, after.3)
    );
}

#[tokio::test]
async fn a_program_this_cell_does_not_execute_is_not_in_the_epoch() {
    let store = Store::open_memory().await.unwrap();
    let now = instant();
    let frequency = active_frequency(&store, now).await;
    let activation_hash = frequency.active_activation_hash.clone().unwrap();
    let running = active_program(&store, "run.here", &frequency.record_uid, "here", now).await;
    let dormant = active_program(&store, "run.elsewhere", &frequency.record_uid, "away", now).await;

    store::karma::execution::set_executes(
        &store.pool,
        &dormant.record_uid,
        false,
        Some("the always-on Cell owns this one"),
        now,
    )
    .await
    .unwrap();
    assert!(
        store::karma::execution::executes(&store.pool, &running.record_uid)
            .await
            .unwrap()
    );
    assert!(
        !store::karma::execution::executes(&store.pool, &dormant.record_uid)
            .await
            .unwrap()
    );

    ingest_tick(&store, &activation_hash, 1, 0, '7', now).await;
    let limits = EvaluationLimits {
        fuel: 100,
        max_expression_depth: 16,
    };
    let turn = process_next_occurrence(&store.pool, limits, NonZeroU32::new(4).unwrap(), now, None)
        .await
        .unwrap()
        .unwrap();
    assert!(turn.completed);
    let frozen = list_epochs(&store.pool).await.unwrap().pop().unwrap();
    let members: Vec<&str> = frozen
        .epoch
        .members
        .iter()
        .map(|member| member.program_uid.as_str())
        .collect();
    assert_eq!(members, vec![running.record_uid.as_str()]);
    assert!(
        list_runs(&store.pool)
            .await
            .unwrap()
            .iter()
            .all(|row| row.run.program_uid != dormant.record_uid)
    );
}

#[tokio::test]
async fn a_program_nobody_configured_runs_and_the_table_stays_empty() {
    let store = Store::open_memory().await.unwrap();
    let now = instant();
    let frequency = active_frequency(&store, now).await;
    let activation_hash = frequency.active_activation_hash.clone().unwrap();
    let program =
        active_program(&store, "run.default", &frequency.record_uid, "default", now).await;

    assert_eq!(count(&store, "karma_program_execution").await, 0);
    ingest_tick(&store, &activation_hash, 1, 0, '7', now).await;
    let limits = EvaluationLimits {
        fuel: 100,
        max_expression_depth: 16,
    };
    let turn = process_next_occurrence(&store.pool, limits, NonZeroU32::new(4).unwrap(), now, None)
        .await
        .unwrap()
        .unwrap();
    assert!(turn.completed);
    assert!(
        turn.runs
            .iter()
            .any(|row| row.run.program_uid == program.record_uid)
    );

    let listed = store::karma::execution::list(&store.pool).await.unwrap();
    let row = listed
        .iter()
        .find(|row| row.program_uid == program.record_uid)
        .unwrap();
    assert!(row.executes);
    assert_eq!(row.note, None);
}

#[tokio::test]
async fn re_enabling_execution_clears_the_deviation() {
    let store = Store::open_memory().await.unwrap();
    let now = instant();
    let frequency = active_frequency(&store, now).await;
    let program = active_program(&store, "run.toggle", &frequency.record_uid, "toggle", now).await;

    store::karma::execution::set_executes(&store.pool, &program.record_uid, false, None, now)
        .await
        .unwrap();
    assert_eq!(count(&store, "karma_program_execution").await, 1);
    store::karma::execution::set_executes(&store.pool, &program.record_uid, true, None, now)
        .await
        .unwrap();
    assert_eq!(count(&store, "karma_program_execution").await, 0);
    assert!(
        store::karma::execution::executes(&store.pool, &program.record_uid)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn execution_cannot_be_set_for_a_program_that_does_not_exist() {
    let store = Store::open_memory().await.unwrap();
    let error = store::karma::execution::set_executes(
        &store.pool,
        "r_01ARZ3NDEKTSV4RRFFQ69G5FAX",
        false,
        None,
        instant(),
    )
    .await
    .unwrap_err();
    assert!(format!("{error}").contains("no such Karma Program"));
    assert_eq!(count(&store, "karma_program_execution").await, 0);
}

#[tokio::test]
async fn the_execute_flag_never_becomes_a_synced_op() {
    let store = Store::open_memory().await.unwrap();
    let now = instant();
    let frequency = active_frequency(&store, now).await;
    let program = active_program(&store, "run.local", &frequency.record_uid, "local", now).await;

    let before: i64 = sqlx_count(&store, "SELECT COUNT(*) FROM sync_op").await;
    store::karma::execution::set_executes(
        &store.pool,
        &program.record_uid,
        false,
        Some("laptop holds it without running it"),
        now,
    )
    .await
    .unwrap();
    assert_eq!(
        sqlx_count(&store, "SELECT COUNT(*) FROM sync_op").await,
        before
    );
    assert_eq!(
        sqlx_count(
            &store,
            "SELECT COUNT(*) FROM sync_op WHERE tbl = 'karma_program_execution'",
        )
        .await,
        0
    );
}

#[tokio::test]
async fn a_program_designated_to_another_cell_does_not_run_here() {
    let store = Store::open_memory().await.unwrap();
    let now = instant();
    let frequency = active_frequency(&store, now).await;
    let activation_hash = frequency.active_activation_hash.clone().unwrap();
    let mine = active_program(&store, "run.mine", &frequency.record_uid, "mine", now).await;
    let theirs = active_program(&store, "run.theirs", &frequency.record_uid, "theirs", now).await;

    let this_cell: String =
        store::sqlx::query_scalar("SELECT uid FROM record WHERE kind = 'device' LIMIT 1")
            .fetch_one(&store.pool)
            .await
            .unwrap();

    store::executor::designate(&store.pool, &mine.record_uid, Some(&this_cell))
        .await
        .unwrap();
    store::executor::designate(
        &store.pool,
        &theirs.record_uid,
        Some("r_01ARZ3NDEKTSV4RRFFQ69G5FAX"),
    )
    .await
    .unwrap();
    assert_eq!(
        store::executor::designated(&store.pool, &theirs.record_uid)
            .await
            .unwrap()
            .as_deref(),
        Some("r_01ARZ3NDEKTSV4RRFFQ69G5FAX")
    );

    ingest_tick(&store, &activation_hash, 1, 0, '7', now).await;
    let limits = EvaluationLimits {
        fuel: 100,
        max_expression_depth: 16,
    };
    let turn = process_next_occurrence(&store.pool, limits, NonZeroU32::new(4).unwrap(), now, None)
        .await
        .unwrap()
        .unwrap();
    assert!(turn.completed);
    let frozen = list_epochs(&store.pool).await.unwrap().pop().unwrap();
    let members: Vec<&str> = frozen
        .epoch
        .members
        .iter()
        .map(|member| member.program_uid.as_str())
        .collect();
    assert_eq!(members, vec![mine.record_uid.as_str()]);
}

#[tokio::test]
async fn a_cleared_designation_lets_every_cell_run_it_again() {
    let store = Store::open_memory().await.unwrap();
    let now = instant();
    let frequency = active_frequency(&store, now).await;
    let program = active_program(&store, "run.freed", &frequency.record_uid, "freed", now).await;

    store::executor::designate(
        &store.pool,
        &program.record_uid,
        Some("r_01ARZ3NDEKTSV4RRFFQ69G5FAX"),
    )
    .await
    .unwrap();
    store::executor::designate(&store.pool, &program.record_uid, None)
        .await
        .unwrap();
    assert_eq!(
        store::executor::designated(&store.pool, &program.record_uid)
            .await
            .unwrap(),
        None
    );

    let activation_hash = frequency.active_activation_hash.clone().unwrap();
    ingest_tick(&store, &activation_hash, 1, 0, '7', now).await;
    let limits = EvaluationLimits {
        fuel: 100,
        max_expression_depth: 16,
    };
    let turn = process_next_occurrence(&store.pool, limits, NonZeroU32::new(4).unwrap(), now, None)
        .await
        .unwrap()
        .unwrap();
    assert!(
        turn.runs
            .iter()
            .any(|row| row.run.program_uid == program.record_uid)
    );
}

#[tokio::test]
async fn the_designation_travels_even_though_the_local_flag_does_not() {
    let store = Store::open_memory().await.unwrap();
    let now = instant();
    let frequency = active_frequency(&store, now).await;
    let program = active_program(&store, "run.shared", &frequency.record_uid, "shared", now).await;

    let before = sqlx_count(
        &store,
        "SELECT COUNT(*) FROM sync_op WHERE tbl = 'record_extension'",
    )
    .await;
    store::executor::designate(
        &store.pool,
        &program.record_uid,
        Some("r_01ARZ3NDEKTSV4RRFFQ69G5FAX"),
    )
    .await
    .unwrap();
    assert!(
        sqlx_count(
            &store,
            "SELECT COUNT(*) FROM sync_op WHERE tbl = 'record_extension'",
        )
        .await
            > before,
        "the designated executor has to reach the other Cells"
    );

    let local_before = sqlx_count(&store, "SELECT COUNT(*) FROM sync_op").await;
    store::karma::execution::set_executes(&store.pool, &program.record_uid, false, None, now)
        .await
        .unwrap();
    assert_eq!(
        sqlx_count(&store, "SELECT COUNT(*) FROM sync_op").await,
        local_before,
        "the per-Cell flag must never travel"
    );
}

#[test]
fn only_an_act_routed_rule_counts_as_acting_outside_the_cell() {
    let plain = program("run.plain", "r_01ARZ3NDEKTSV4RRFFQ69G5FAX");
    assert!(!plain.is_externally_observable());

    let acting = candidate_program("r_01ARZ3NDEKTSV4RRFFQ69G5FAX");
    assert!(
        acting
            .nodes
            .values()
            .any(|node| matches!(node.operation, NodeOperation::RouteCandidate { .. })),
        "the fixture must actually route a candidate for this to mean anything"
    );
    assert!(acting.is_externally_observable());

    let mut asking = acting.clone();
    for node in asking.nodes.values_mut() {
        if let NodeOperation::RouteCandidate { route, .. } = &mut node.operation {
            *route = CandidateRoute::Ask;
        }
    }
    assert!(
        asking
            .nodes
            .values()
            .any(|node| matches!(node.operation, NodeOperation::RouteCandidate { .. })),
    );
    assert!(!asking.is_externally_observable());
}

async fn sqlx_count(store: &Store, query: &str) -> i64 {
    store::sqlx::query_scalar(query)
        .fetch_one(&store.pool)
        .await
        .unwrap()
}

async fn active_frequency(store: &Store, now: DateTime<Utc>) -> FrequencyHandleRow {
    let created = committed_frequency(
        create_frequency(
            &store.pool,
            CreateFrequencyInput {
                request_id: "create-run-frequency".to_string(),
                frequency: frequency(),
                owner_person_uid: None,
                actor_person_uid: None,
            },
            now,
            |_| None,
        )
        .await
        .unwrap(),
    );
    committed_frequency(
        activate_frequency(
            &store.pool,
            ActivateFrequencyInput {
                request_id: "activate-run-frequency".to_string(),
                frequency_uid: created.record_uid,
                expected_handle_revision: 1,
                revision_hash: created.head_revision_hash,
                parameter_overrides: BTreeMap::new(),
                actor_person_uid: None,
            },
            now,
            |_| None,
        )
        .await
        .unwrap(),
    )
}

async fn active_program(
    store: &Store,
    slug: &str,
    frequency_uid: &str,
    request_suffix: &str,
    now: DateTime<Utc>,
) -> ProgramHandleRow {
    active_program_definition(store, program(slug, frequency_uid), request_suffix, now).await
}

async fn active_program_definition(
    store: &Store,
    program: ProgramAst,
    request_suffix: &str,
    now: DateTime<Utc>,
) -> ProgramHandleRow {
    let created = committed_program(
        create_program(
            &store.pool,
            CreateProgramInput {
                request_id: format!("create-{request_suffix}"),
                program,
                owner_person_uid: None,
                actor_person_uid: None,
            },
            now,
            |_| None,
        )
        .await
        .unwrap(),
    );
    committed_program(
        activate_program(
            &store.pool,
            ActivateProgramInput {
                request_id: format!("activate-{request_suffix}"),
                program_uid: created.record_uid,
                expected_handle_revision: 1,
                revision_hash: created.head_revision_hash,
                actor_person_uid: None,
            },
            now,
            |_| None,
        )
        .await
        .unwrap(),
    )
}

fn program(slug: &str, frequency_uid: &str) -> ProgramAst {
    let trigger = id("trigger");
    let event = id("event");
    let program = ProgramAst {
        schema: ProgramSchema::V1,
        slug: Slug::new(slug).unwrap(),
        purpose: "Evaluate a frozen Frequency occurrence without effects".to_string(),
        tags: BTreeSet::new(),
        parameters: BTreeMap::new(),
        nodes: BTreeMap::from([(
            trigger.clone(),
            NodeAst {
                inputs: BTreeMap::new(),
                outputs: BTreeMap::from([(
                    event.clone(),
                    PortContract {
                        value_type: ValueType::Bool,
                        sensitivity: Sensitivity::Private,
                        freshness: None,
                    },
                )]),
                operation: NodeOperation::Trigger {
                    source: TriggerSource::Frequency {
                        frequency: ResolvedReference {
                            target: TypedUid::new(ReferenceKind::Frequency, frequency_uid).unwrap(),
                            display_slug: None,
                        },
                    },
                    output: event.clone(),
                },
            },
        )]),
        outputs: BTreeMap::from([(
            event.clone(),
            OutputRef {
                node: trigger,
                port: event,
            },
        )]),
        required_capabilities: CapabilitySet::default(),
    };
    assert_eq!(prove_program(&program).status, ProofStatus::Accepted);
    program
}

fn failing_program(frequency_uid: &str) -> ProgramAst {
    let mut value = program("run.failing", frequency_uid);
    value.nodes.insert(
        id("calculation"),
        NodeAst {
            inputs: BTreeMap::new(),
            outputs: BTreeMap::from([(
                id("result"),
                PortContract {
                    value_type: ValueType::I64,
                    sensitivity: Sensitivity::Private,
                    freshness: None,
                },
            )]),
            operation: NodeOperation::Derive {
                expressions: BTreeMap::from([(
                    id("result"),
                    ExpressionAst::Binary {
                        operator: BinaryOperator::Divide,
                        left: Box::new(ExpressionAst::Literal {
                            value: LiteralValue::I64 { value: 1 },
                        }),
                        right: Box::new(ExpressionAst::Literal {
                            value: LiteralValue::I64 { value: 0 },
                        }),
                        precision: None,
                    },
                )]),
            },
        },
    );
    value.outputs = BTreeMap::from([(
        id("result"),
        OutputRef {
            node: id("calculation"),
            port: id("result"),
        },
    )]);
    assert_eq!(prove_program(&value).status, ProofStatus::Accepted);
    value
}

fn cooldown_program(frequency_uid: &str) -> ProgramAst {
    cooldown_program_policy(
        frequency_uid,
        "Evaluate a durable cooldown without effects",
        StateResetPolicy::Never,
        StateMigrationPolicy::CompatibleTypeOnly,
    )
}

fn cooldown_program_policy(
    frequency_uid: &str,
    purpose: &str,
    reset: StateResetPolicy,
    migration: StateMigrationPolicy,
) -> ProgramAst {
    let mut value = program("run.cooldown", frequency_uid);
    value.purpose = purpose.to_string();
    value.nodes.insert(
        id("cooldown"),
        NodeAst {
            inputs: BTreeMap::from([(
                id("input"),
                InputBinding {
                    source: OutputRef {
                        node: id("trigger"),
                        port: id("event"),
                    },
                    expected_type: ValueType::Bool,
                },
            )]),
            outputs: BTreeMap::from([(
                id("allowed"),
                PortContract {
                    value_type: ValueType::Bool,
                    sensitivity: Sensitivity::Private,
                    freshness: None,
                },
            )]),
            operation: NodeOperation::Cooldown {
                input: id("input"),
                allowed: id("allowed"),
                cooldown: DurationMs::new(10),
                state: StateContract {
                    persistence: StatePersistence::Program,
                    reset,
                    late_event: LateEventPolicy::Reject,
                    migration,
                    simulation: SimulationStatePolicy::Clone,
                },
            },
        },
    );
    value.outputs = BTreeMap::from([(
        id("allowed"),
        OutputRef {
            node: id("cooldown"),
            port: id("allowed"),
        },
    )]);
    assert_eq!(prove_program(&value).status, ProofStatus::Accepted);
    value
}

fn candidate_program(frequency_uid: &str) -> ProgramAst {
    let mut value = program("run.candidate", frequency_uid);
    value.nodes.insert(
        id("candidate"),
        NodeAst {
            inputs: BTreeMap::from([(
                id("condition"),
                InputBinding {
                    source: OutputRef {
                        node: id("trigger"),
                        port: id("event"),
                    },
                    expected_type: ValueType::Bool,
                },
            )]),
            outputs: BTreeMap::from([(
                id("proposal"),
                PortContract {
                    value_type: ValueType::Datum {
                        value: Box::new(ValueType::Candidate {
                            route: CandidateRoute::Act,
                            template: Slug::new("test.act").unwrap(),
                            fields: BTreeMap::new(),
                        }),
                    },
                    sensitivity: Sensitivity::Private,
                    freshness: None,
                },
            )]),
            operation: NodeOperation::RouteCandidate {
                condition: id("condition"),
                output: id("proposal"),
                route: CandidateRoute::Act,
                template: Slug::new("test.act").unwrap(),
                fields: BTreeMap::new(),
            },
        },
    );
    value.outputs = BTreeMap::from([(
        id("proposal"),
        OutputRef {
            node: id("candidate"),
            port: id("proposal"),
        },
    )]);
    assert_eq!(prove_program(&value).status, ProofStatus::Accepted);
    value
}

fn frequency() -> FrequencyAst {
    FrequencyAst {
        schema: FrequencySchema::V1,
        slug: Slug::new("run.frequency").unwrap(),
        purpose: "Produce deterministic run test occurrences".to_string(),
        tags: BTreeSet::new(),
        parameters: BTreeMap::from([(
            id("interval"),
            FrequencyParameterDefinition::Duration {
                default: DurationMs::new(1_000),
                minimum: DurationMs::new(1),
                maximum: DurationMs::new(10_000),
            },
        )]),
        cadence: FrequencyCadenceAst::Elapsed {
            interval: DurationBinding::Parameter {
                parameter: id("interval"),
            },
            anchor: timestamp(),
        },
        timer: FrequencyTimerAst {
            required_resolution: duration(1),
            max_lateness: duration(10),
            coalesce_window: duration(0),
        },
        missed: MissedPolicy::Replay {
            max: NonZeroU32::new(8).unwrap(),
        },
        inactive_gap: InactiveGapPolicy::SkipToNextAnchor,
        rephase: RephasePolicy::PreserveAnchor,
        overload: OverloadPolicy::PauseAndAsk,
    }
}

fn committed_frequency(commit: FrequencyMutationCommit) -> FrequencyHandleRow {
    match commit {
        FrequencyMutationCommit::Committed { handle, .. } => handle,
        other => panic!("expected committed Frequency mutation, got {other:?}"),
    }
}

fn committed_program(commit: ProgramMutationCommit) -> ProgramHandleRow {
    match commit {
        ProgramMutationCommit::Committed { handle, .. } => handle,
        other => panic!("expected committed Program mutation, got {other:?}"),
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

fn hash(digit: char) -> nucleus::karma::CanonicalHash {
    nucleus::karma::CanonicalHash::parse(format!("sha256:{}", digit.to_string().repeat(64)))
        .unwrap()
}

fn timestamp() -> TimestampMs {
    TimestampMs::parse_canonical("2026-07-22T12:00:00.000Z").unwrap()
}

fn instant() -> DateTime<Utc> {
    Utc.timestamp_millis_opt(timestamp().as_millis())
        .single()
        .unwrap()
}

async fn ingest_tick(
    store: &Store,
    activation_hash: &nucleus::karma::CanonicalHash,
    schedule_ordinal: u64,
    offset_ms: i64,
    hash_digit: char,
    now: DateTime<Utc>,
) {
    let tick = SemanticScheduleTick::new(
        activation_hash.clone(),
        schedule_ordinal,
        timestamp_ms(offset_ms),
    )
    .unwrap();
    store::karma::occurrences::ingest(
        &store.pool,
        &KarmaOccurrenceEnvelope::schedule_tick(hash(hash_digit), tick, None).unwrap(),
        now,
    )
    .await
    .unwrap();
}

fn successful_bool(row: &store::karma::runs::KarmaRunRow, output: &str) -> bool {
    let KarmaRunOutcome::Succeeded { replay } = &row.run.outcome else {
        panic!("expected successful run, got {:?}", row.run.outcome);
    };
    match replay.capsule.expected_result.outputs.get(&id(output)) {
        Some(LiteralValue::Bool { value }) => *value,
        other => panic!("expected bool output, got {other:?}"),
    }
}

fn timestamp_ms(offset_ms: i64) -> TimestampMs {
    TimestampMs::from_millis(timestamp().as_millis() + offset_ms).unwrap()
}

async fn reaction_counts(store: &Store) -> (i64, i64, i64, i64) {
    (
        count(store, "karma_occurrence").await,
        count(store, "fact").await,
        count(store, "signed_action_intent").await,
        count(store, "transfer").await,
    )
}

async fn count(store: &Store, table: &str) -> i64 {
    store::sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(&store.pool)
        .await
        .unwrap()
}

const GRANT_PERSON_UID: &str = "p_01ARZ3NDEKTSV4RRFFQ69G5FAW";

#[tokio::test]
async fn accepting_with_a_grant_authorizes_one_intent_and_spends_its_budget() {
    let store = Store::open_memory().await.unwrap();
    let now = instant();
    let candidate = act_candidate(&store, now).await;
    let grant = active_grant(
        &store,
        &candidate.proposal.program_uid,
        budget_of(Some(2), None),
        "budgeted",
        now,
    )
    .await;

    let inert = accept(
        &store,
        &candidate.candidate_hash,
        1,
        None,
        "accept-inert",
        now,
    )
    .await;
    let store::karma::candidates::CandidateReviewCommit::Committed { intent, .. } = &inert else {
        panic!("expected a committed acceptance");
    };
    assert!(intent.is_none(), "no grant named, so no authority is taken");
    assert_eq!(count(&store, "karma_intent").await, 0);

    let second = second_act_candidate(&store, now).await;
    let authorized = accept(
        &store,
        &second.candidate_hash,
        1,
        Some(&grant),
        "accept-authorized",
        now,
    )
    .await;
    let store::karma::candidates::CandidateReviewCommit::Committed {
        intent,
        intent_fact,
        ..
    } = &authorized
    else {
        panic!("expected a committed authorization");
    };
    let intent_hash = intent
        .clone()
        .expect("an authorized acceptance has an intent");
    assert!(intent_fact.is_some(), "the intent joins the Ledger too");

    let row = store::karma::intents::get(&store.pool, &intent_hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.candidate_hash, second.candidate_hash);
    assert_eq!(row.grant_uid, grant);
    assert_eq!(
        row.intent.idempotency_key,
        format!("karma-intent:{}", second.candidate_hash.as_str())
    );
    assert!(row.intent.authorization.decision.allowed);
    assert_eq!(row.intent.authorization.budget.intents_before, 0);
    assert_eq!(row.intent.authorization.budget.intents_after, 1);
    let revision =
        store::karma::grants::get_revision(&store.pool, &row.grant_uid, &row.grant_revision_hash)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(row.intent.deadline, revision.revision.spec.expires_at);

    let state = store::karma::intents::get_state(&store.pool, &intent_hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(state.status, nucleus::karma::IntentStatus::Authorized);
    assert_eq!(state.actor_person_uid, GRANT_PERSON_UID);

    assert_eq!(count(&store, "karma_intent").await, 1);
    assert_eq!(count(&store, "transfer").await, 0);
}

#[tokio::test]
async fn an_exhausted_budget_refuses_the_acceptance_entirely() {
    let store = Store::open_memory().await.unwrap();
    let now = instant();
    let candidate = act_candidate(&store, now).await;
    let grant = active_grant(
        &store,
        &candidate.proposal.program_uid,
        budget_of(Some(1), None),
        "single",
        now,
    )
    .await;
    accept(
        &store,
        &candidate.candidate_hash,
        1,
        Some(&grant),
        "accept-first",
        now,
    )
    .await;
    assert_eq!(count(&store, "karma_intent").await, 1);

    let second = second_act_candidate(&store, now).await;
    let refused = store::karma::candidates::respond(
        &store.pool,
        store::karma::candidates::RespondCandidateInput {
            request_id: "accept-over-budget".to_string(),
            candidate_hash: second.candidate_hash.clone(),
            expected_state_revision: 1,
            response: nucleus::karma::CandidateReviewAction::Accept,
            actor_person_uid: Some(GRANT_PERSON_UID.to_string()),
            authorizing_grant_uid: Some(grant.clone()),
        },
        now,
        |_| None,
    )
    .await;
    let message = format!("{:?}", refused.unwrap_err());
    assert!(
        message.contains("IntentCapExhausted"),
        "expected an exhausted budget, got {message}"
    );
    let state = store::karma::candidates::get_state(&store.pool, &second.candidate_hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(state.status, nucleus::karma::CandidateStatus::Proposed);
    assert_eq!(state.state_revision, 1);
    assert_eq!(count(&store, "karma_intent").await, 1);
}

#[tokio::test]
async fn revoking_a_grant_cancels_its_intents_and_releases_their_budget() {
    let store = Store::open_memory().await.unwrap();
    let now = instant();
    let candidate = act_candidate(&store, now).await;
    let grant = active_grant(
        &store,
        &candidate.proposal.program_uid,
        budget_of(Some(2), None),
        "revoked",
        now,
    )
    .await;
    let accepted = accept(
        &store,
        &candidate.candidate_hash,
        1,
        Some(&grant),
        "accept-before-revoke",
        now,
    )
    .await;
    let store::karma::candidates::CandidateReviewCommit::Committed { intent, .. } = &accepted
    else {
        panic!("expected a committed authorization");
    };
    let intent_hash = intent.clone().unwrap();
    let second = second_act_candidate(&store, now).await;
    let also_accepted = accept(
        &store,
        &second.candidate_hash,
        1,
        Some(&grant),
        "second-accept-before-revoke",
        now,
    )
    .await;
    let store::karma::candidates::CandidateReviewCommit::Committed { intent, .. } = &also_accepted
    else {
        panic!("expected a second committed authorization");
    };
    let second_intent_hash = intent.clone().unwrap();

    let handle = store::karma::grants::get_handle(&store.pool, &grant)
        .await
        .unwrap()
        .unwrap();
    store::karma::grants::revoke(
        &store.pool,
        store::karma::grants::RevokeGrantInput {
            request_id: "revoke-with-intents".to_string(),
            grant_uid: grant.clone(),
            expected_handle_revision: handle.handle_revision,
            actor_person_uid: GRANT_PERSON_UID.to_string(),
        },
        now,
        grant_signer,
    )
    .await
    .unwrap();

    let state = store::karma::intents::get_state(&store.pool, &intent_hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(state.status, nucleus::karma::IntentStatus::Cancelled);
    assert_eq!(
        state.cancelled_reason.as_deref(),
        Some("karma_grant_revoked")
    );
    assert_eq!(state.state_revision, 2);
    let second_state = store::karma::intents::get_state(&store.pool, &second_intent_hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        second_state.status,
        nucleus::karma::IntentStatus::Cancelled,
        "one revocation cancels every intent it authorized"
    );
    assert!(
        store::karma::intents::get(&store.pool, &intent_hash)
            .await
            .unwrap()
            .is_some()
    );

    let history = store::karma::intents::history(&store.pool, &intent_hash)
        .await
        .unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(
        history[0].transition.status,
        nucleus::karma::IntentStatus::Authorized
    );
    assert!(history[0].transition.previous_event_hash.is_none());
    assert_eq!(
        history[1].transition.previous_event_hash.as_ref(),
        Some(&history[0].event_hash),
        "a transition chains to the one it replaced"
    );
    assert_eq!(
        history[1].transition.cause_request_id,
        "revoke-with-intents"
    );
    assert_eq!(state.current_event_hash, history[1].event_hash);
    let second_history = store::karma::intents::history(&store.pool, &second_intent_hash)
        .await
        .unwrap();
    assert_eq!(
        second_history[1].transition.cause_request_id, "revoke-with-intents",
        "one cause legitimately owns many transitions"
    );
    let still_reserved: i64 = store::sqlx::query_scalar(
        "SELECT COUNT(*) FROM karma_intent intent
         JOIN karma_intent_state state ON state.intent_hash = intent.intent_hash
         JOIN karma_intent_status kind ON kind.status = state.status
         WHERE intent.grant_uid = ? AND kind.holds_reservation = 1",
    )
    .bind(&grant)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(still_reserved, 0, "a cancelled intent reserves nothing");
}

#[tokio::test]
async fn the_stored_status_vocabulary_agrees_with_the_kernel() {
    let store = Store::open_memory().await.unwrap();
    let rows: Vec<(String, i64)> =
        store::sqlx::query_as("SELECT status, holds_reservation FROM karma_intent_status")
            .fetch_all(&store.pool)
            .await
            .unwrap();
    assert!(!rows.is_empty());
    for (status, holds_reservation) in &rows {
        let parsed = nucleus::karma::IntentStatus::parse(status)
            .unwrap_or_else(|| panic!("stored status {status} is not in the frozen vocabulary"));
        assert_eq!(
            parsed.holds_reservation(),
            *holds_reservation == 1,
            "{status} disagrees with the kernel about holding a reservation"
        );
    }
    let seeded: BTreeSet<&str> = rows.iter().map(|(status, _)| status.as_str()).collect();
    let expected: BTreeSet<&str> = nucleus::karma::K5_2_INTENT_STATES
        .iter()
        .map(|status| status.as_str())
        .collect();
    assert_eq!(seeded, expected);
}

#[tokio::test]
async fn an_unreachable_state_cannot_be_forced_into_the_history() {
    let store = Store::open_memory().await.unwrap();
    let now = instant();
    let candidate = act_candidate(&store, now).await;
    let grant = active_grant(
        &store,
        &candidate.proposal.program_uid,
        budget_of(Some(1), None),
        "forced",
        now,
    )
    .await;
    let accepted = accept(
        &store,
        &candidate.candidate_hash,
        1,
        Some(&grant),
        "accept-then-tamper",
        now,
    )
    .await;
    let store::karma::candidates::CandidateReviewCommit::Committed { intent, .. } = &accepted
    else {
        panic!("expected a committed authorization");
    };
    let intent_hash = intent.clone().unwrap();
    let state = store::karma::intents::get_state(&store.pool, &intent_hash)
        .await
        .unwrap()
        .unwrap();

    let forced = store::sqlx::query(
        "INSERT INTO karma_intent_event
            (event_hash, intent_hash, state_revision, previous_event_hash, status, reason,
             cause_request_id, actor_person_uid, event_json, created_at)
         VALUES (?, ?, 2, ?, 'leased', 'forced', 'accept-then-tamper', 'person:x', '{}', ?)",
    )
    .bind(format!("sha256:{}", "0".repeat(64)))
    .bind(intent_hash.as_str())
    .bind(state.current_event_hash.as_str())
    .bind(now.to_rfc3339())
    .execute(&store.pool)
    .await;
    let refusal = forced.expect_err("a state K5.2 cannot reach must not be writable");
    assert!(
        refusal.to_string().to_uppercase().contains("FOREIGN KEY"),
        "the vocabulary foreign key must be what refuses it, got: {refusal}"
    );

    let rewritten = store::sqlx::query(
        "UPDATE karma_intent_event SET status = 'cancelled' WHERE intent_hash = ?",
    )
    .bind(intent_hash.as_str())
    .execute(&store.pool)
    .await;
    assert!(rewritten.is_err(), "intent history is immutable");

    let drifted = store::sqlx::query(
        "UPDATE karma_intent_state SET status = 'cancelled', cancelled_reason = 'forced'
         WHERE intent_hash = ?",
    )
    .bind(intent_hash.as_str())
    .execute(&store.pool)
    .await;
    assert!(
        drifted.is_err(),
        "the projection may not drift from its history"
    );
}

#[tokio::test]
async fn authorization_replays_exactly_and_never_mints_a_second_intent() {
    let store = Store::open_memory().await.unwrap();
    let now = instant();
    let candidate = act_candidate(&store, now).await;
    let grant = active_grant(
        &store,
        &candidate.proposal.program_uid,
        budget_of(Some(5), None),
        "replayed",
        now,
    )
    .await;
    let first = accept(
        &store,
        &candidate.candidate_hash,
        1,
        Some(&grant),
        "accept-replay",
        now,
    )
    .await;
    let store::karma::candidates::CandidateReviewCommit::Committed { intent, .. } = &first else {
        panic!("expected a committed authorization");
    };
    let facts_after_first = count(&store, "fact").await;

    let replay = accept(
        &store,
        &candidate.candidate_hash,
        1,
        Some(&grant),
        "accept-replay",
        now,
    )
    .await;
    let store::karma::candidates::CandidateReviewCommit::Replayed {
        intent: replayed, ..
    } = &replay
    else {
        panic!("expected a replayed authorization");
    };
    assert_eq!(replayed, intent);
    assert_eq!(count(&store, "karma_intent").await, 1);
    assert_eq!(count(&store, "fact").await, facts_after_first);

    let other = active_grant(
        &store,
        &candidate.proposal.program_uid,
        budget_of(Some(5), None),
        "replayed-other",
        now,
    )
    .await;
    assert!(
        store::karma::candidates::respond(
            &store.pool,
            store::karma::candidates::RespondCandidateInput {
                request_id: "accept-replay".to_string(),
                candidate_hash: candidate.candidate_hash.clone(),
                expected_state_revision: 1,
                response: nucleus::karma::CandidateReviewAction::Accept,
                actor_person_uid: Some(GRANT_PERSON_UID.to_string()),
                authorizing_grant_uid: Some(other),
            },
            now,
            |_| None,
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn a_grant_must_be_active_and_cover_the_template() {
    let store = Store::open_memory().await.unwrap();
    let now = instant();
    let candidate = act_candidate(&store, now).await;

    let draft = create_grant(
        &store,
        &candidate.proposal.program_uid,
        budget_of(Some(1), None),
        "draft-only",
        now,
    )
    .await;
    let refused = store::karma::candidates::respond(
        &store.pool,
        store::karma::candidates::RespondCandidateInput {
            request_id: "accept-draft-grant".to_string(),
            candidate_hash: candidate.candidate_hash.clone(),
            expected_state_revision: 1,
            response: nucleus::karma::CandidateReviewAction::Accept,
            actor_person_uid: Some(GRANT_PERSON_UID.to_string()),
            authorizing_grant_uid: Some(draft),
        },
        now,
        |_| None,
    )
    .await;
    assert!(
        format!("{:?}", refused.unwrap_err()).contains("not active"),
        "a draft grant must not authorize"
    );

    let narrow = active_grant_with(
        &store,
        &candidate.proposal.program_uid,
        budget_of(Some(1), None),
        nucleus::karma::GrantTemplateScope::Only {
            templates: BTreeSet::from([Slug::new("something.else").unwrap()]),
        },
        "narrow-template",
        now,
    )
    .await;
    let denied = store::karma::candidates::respond(
        &store.pool,
        store::karma::candidates::RespondCandidateInput {
            request_id: "accept-wrong-template".to_string(),
            candidate_hash: candidate.candidate_hash.clone(),
            expected_state_revision: 1,
            response: nucleus::karma::CandidateReviewAction::Accept,
            actor_person_uid: Some(GRANT_PERSON_UID.to_string()),
            authorizing_grant_uid: Some(narrow),
        },
        now,
        |_| None,
    )
    .await;
    assert!(
        format!("{:?}", denied.unwrap_err()).contains("CandidateTemplateMismatch"),
        "a grant must cover the template it authorizes"
    );
    assert_eq!(count(&store, "karma_intent").await, 0);
}

async fn accept(
    store: &Store,
    candidate_hash: &nucleus::karma::CanonicalHash,
    expected_state_revision: u64,
    grant_uid: Option<&str>,
    request_id: &str,
    now: DateTime<Utc>,
) -> store::karma::candidates::CandidateReviewCommit {
    store::karma::candidates::respond(
        &store.pool,
        store::karma::candidates::RespondCandidateInput {
            request_id: request_id.to_string(),
            candidate_hash: candidate_hash.clone(),
            expected_state_revision,
            response: nucleus::karma::CandidateReviewAction::Accept,
            actor_person_uid: Some(GRANT_PERSON_UID.to_string()),
            authorizing_grant_uid: grant_uid.map(str::to_string),
        },
        now,
        |_| None,
    )
    .await
    .unwrap()
}

async fn act_candidate(
    store: &Store,
    now: DateTime<Utc>,
) -> store::karma::candidates::KarmaCandidateRow {
    let frequency = active_frequency(store, now).await;
    active_program_definition(
        store,
        authorized_candidate_program(&frequency.record_uid),
        "intent-candidate",
        now,
    )
    .await;
    ingest_tick(
        store,
        &frequency.active_activation_hash.unwrap(),
        1,
        0,
        'f',
        now,
    )
    .await;
    process_next_occurrence(
        &store.pool,
        EvaluationLimits::default(),
        NonZeroU32::new(8).unwrap(),
        now,
        None,
    )
    .await
    .unwrap()
    .unwrap();
    let candidates = store::karma::candidates::list(&store.pool).await.unwrap();
    candidates.into_iter().next().expect("one act candidate")
}

async fn second_act_candidate(
    store: &Store,
    now: DateTime<Utc>,
) -> store::karma::candidates::KarmaCandidateRow {
    let before: BTreeSet<String> = store::karma::candidates::list(&store.pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.candidate_hash.as_str().to_string())
        .collect();
    let frequency = store::karma::frequencies::list_handles(&store.pool)
        .await
        .unwrap()
        .into_iter()
        .next()
        .expect("an active frequency");
    ingest_tick(
        store,
        &frequency.active_activation_hash.unwrap(),
        2,
        1,
        'a',
        now,
    )
    .await;
    process_next_occurrence(
        &store.pool,
        EvaluationLimits::default(),
        NonZeroU32::new(8).unwrap(),
        now,
        None,
    )
    .await
    .unwrap()
    .unwrap();
    store::karma::candidates::list(&store.pool)
        .await
        .unwrap()
        .into_iter()
        .find(|row| !before.contains(row.candidate_hash.as_str()))
        .expect("a second act candidate")
}

fn budget_of(
    max_intents: Option<u64>,
    per_window: Option<nucleus::karma::GrantWindowLimit>,
) -> nucleus::karma::GrantBudget {
    nucleus::karma::GrantBudget {
        max_intents,
        per_window,
        quantity_limit: None,
    }
}

async fn active_grant(
    store: &Store,
    program_uid: &str,
    budget: nucleus::karma::GrantBudget,
    suffix: &str,
    now: DateTime<Utc>,
) -> String {
    active_grant_with(
        store,
        program_uid,
        budget,
        nucleus::karma::GrantTemplateScope::Any,
        suffix,
        now,
    )
    .await
}

async fn active_grant_with(
    store: &Store,
    program_uid: &str,
    budget: nucleus::karma::GrantBudget,
    templates: nucleus::karma::GrantTemplateScope,
    suffix: &str,
    now: DateTime<Utc>,
) -> String {
    let grant_uid = create_grant_with(store, program_uid, budget, templates, suffix, now).await;
    let handle = store::karma::grants::get_handle(&store.pool, &grant_uid)
        .await
        .unwrap()
        .unwrap();
    store::karma::grants::activate(
        &store.pool,
        store::karma::grants::ActivateGrantInput {
            request_id: format!("activate-grant-{suffix}"),
            grant_uid: grant_uid.clone(),
            expected_handle_revision: handle.handle_revision,
            revision_hash: handle.head_revision_hash,
            actor_person_uid: GRANT_PERSON_UID.to_string(),
        },
        now,
        grant_signer,
    )
    .await
    .unwrap();
    grant_uid
}

async fn create_grant(
    store: &Store,
    program_uid: &str,
    budget: nucleus::karma::GrantBudget,
    suffix: &str,
    now: DateTime<Utc>,
) -> String {
    create_grant_with(
        store,
        program_uid,
        budget,
        nucleus::karma::GrantTemplateScope::Any,
        suffix,
        now,
    )
    .await
}

async fn create_grant_with(
    store: &Store,
    program_uid: &str,
    budget: nucleus::karma::GrantBudget,
    templates: nucleus::karma::GrantTemplateScope,
    suffix: &str,
    now: DateTime<Utc>,
) -> String {
    let commit = store::karma::grants::create(
        &store.pool,
        store::karma::grants::CreateGrantInput {
            request_id: format!("create-grant-{suffix}"),
            slug: Slug::new(format!("intent.grant.{suffix}")).unwrap(),
            grant: nucleus::karma::DelegationGrantSpec {
                schema: nucleus::karma::DelegationGrantSchema::V1,
                purpose: "Authorize the test pantry restock".to_string(),
                program_uid: TypedUid::new(ReferenceKind::Program, program_uid.to_string())
                    .unwrap(),
                program_revision: nucleus::karma::GrantProgramRevisionScope::AnyActive,
                candidate_templates: templates,
                capabilities: CapabilitySet::new([nucleus::karma::Capability::RecordAddQuantity]),
                targets: nucleus::karma::GrantTargetScope::Any,
                budget,
                valid_from: TimestampMs::from_millis(0).unwrap(),
                expires_at: TimestampMs::from_millis(4_102_444_800_000).unwrap(),
            },
            actor_person_uid: GRANT_PERSON_UID.to_string(),
        },
        now,
        grant_signer,
    )
    .await
    .unwrap();
    match commit {
        store::karma::grants::GrantMutationCommit::Committed { handle, .. } => handle.record_uid,
        other => panic!("expected a created grant, got {other:?}"),
    }
}

fn grant_signer(hash: &str) -> Option<nucleus::karma::DelegationSignature> {
    Some(nucleus::karma::DelegationSignature {
        signer_person_uid: TypedUid::new(ReferenceKind::Person, GRANT_PERSON_UID.to_string())
            .unwrap(),
        key_id: "key-intent-test".to_string(),
        signature: format!("signed:{hash}"),
    })
}

fn authorized_candidate_program(frequency_uid: &str) -> ProgramAst {
    let mut value = program("run.authorized", frequency_uid);
    value.nodes.insert(
        id("candidate"),
        NodeAst {
            inputs: BTreeMap::from([(
                id("condition"),
                InputBinding {
                    source: OutputRef {
                        node: id("trigger"),
                        port: id("event"),
                    },
                    expected_type: ValueType::Bool,
                },
            )]),
            outputs: BTreeMap::from([(
                id("proposal"),
                PortContract {
                    value_type: ValueType::Datum {
                        value: Box::new(ValueType::Candidate {
                            route: CandidateRoute::Act,
                            template: Slug::new("record.add-quantity").unwrap(),
                            fields: BTreeMap::new(),
                        }),
                    },
                    sensitivity: Sensitivity::Private,
                    freshness: None,
                },
            )]),
            operation: NodeOperation::RouteCandidate {
                condition: id("condition"),
                output: id("proposal"),
                route: CandidateRoute::Act,
                template: Slug::new("record.add-quantity").unwrap(),
                fields: BTreeMap::new(),
            },
        },
    );
    value.outputs = BTreeMap::from([(
        id("proposal"),
        OutputRef {
            node: id("candidate"),
            port: id("proposal"),
        },
    )]);
    assert_eq!(prove_program(&value).status, ProofStatus::Accepted);
    value
}
