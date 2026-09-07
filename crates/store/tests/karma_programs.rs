use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use nucleus::karma::{
    CapabilitySet, DefinitionStatus, ProgramAst, ProgramMutationAction, ProgramMutationEvidence,
    ProgramSchema, ProofStatus, Slug, prove_program,
};
use store::Store;
use store::karma::programs::{
    ActivateProgramInput, CreateProgramInput, PauseProgramInput, ProgramMutationCommit,
    ReviseProgramInput, activate, create, get_handle, get_revision, pause, revise,
};

const PERSON_UID: &str = "r_01APS3NDEKTSV4RRFFQ69G5FAV";
const OTHER_PERSON_UID: &str = "r_01APS3NDEKTSV4RRFFQ69G5FAW";

#[tokio::test]
async fn create_replay_and_reopen_preserve_the_exact_result() {
    let path = std::env::temp_dir().join(format!("lince-karma-{}.db", nucleus::new_uid("test")));
    let url = format!("sqlite://{}", path.display());
    let store = Store::open(&url).await.unwrap();
    let input = CreateProgramInput {
        request_id: "program-create-1".to_string(),
        program: program("stored.one", "Stored one"),
        owner_person_uid: Some(PERSON_UID.to_string()),
        actor_person_uid: Some(PERSON_UID.to_string()),
    };
    let (created_handle, created_fact) = committed(
        create(&store.pool, input.clone(), now(0), signer)
            .await
            .unwrap(),
    );
    assert_eq!(created_handle.handle_revision, 1);
    assert_eq!(created_handle.status, DefinitionStatus::Proven);
    assert_eq!(created_handle.active_revision_hash, None);
    assert_eq!(created_fact.delta, store::exact::from_f64(0.0));
    assert_eq!(
        created_fact.signature,
        Some(format!("signed:{}", created_fact.hash))
    );

    let revision = get_revision(&store.pool, &created_handle.head_revision_hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(revision.program, input.program);
    assert_eq!(revision.proof.status, ProofStatus::Accepted);

    store.pool.close().await;
    let reopened = Store::open(&url).await.unwrap();
    let (replayed_handle, replayed_fact) =
        replayed(create(&reopened.pool, input, now(0), signer).await.unwrap());
    assert_eq!(replayed_handle, created_handle);
    assert_eq!(replayed_fact, created_fact);
    assert_eq!(count(&reopened, "karma_program").await, 1);
    assert_eq!(count(&reopened, "karma_program_revision").await, 1);
    assert_eq!(count(&reopened, "karma_program_request").await, 1);
    assert_eq!(count(&reopened, "fact").await, 1);
    reopened.pool.close().await;
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn revise_activate_switch_and_pause_are_atomic_and_cas_controlled() {
    let store = Store::open_memory().await.unwrap();
    let (created, create_fact) = committed(
        create(
            &store.pool,
            create_input("flow.main", "Create flow", "create-flow"),
            now(0),
            signer,
        )
        .await
        .unwrap(),
    );
    let first_hash = created.head_revision_hash.clone();

    let (active, activate_fact) = committed(
        activate(
            &store.pool,
            ActivateProgramInput {
                request_id: "activate-flow-1".to_string(),
                program_uid: created.record_uid.clone(),
                expected_handle_revision: 1,
                revision_hash: first_hash.clone(),
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
    assert_eq!(activate_fact.delta, store::exact::from_f64(1.0));
    assert_eq!(record_quantity(&store, &created.record_uid).await, 1.0);

    let revised_program = program("flow.main", "Revised while old stays active");
    let (revised, revise_fact) = committed(
        revise(
            &store.pool,
            ReviseProgramInput {
                request_id: "revise-flow-1".to_string(),
                program_uid: created.record_uid.clone(),
                expected_handle_revision: 2,
                program: revised_program,
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            now(2),
            signer,
        )
        .await
        .unwrap(),
    );
    assert_eq!(revised.handle_revision, 3);
    assert_eq!(revised.status, DefinitionStatus::Active);
    assert_eq!(revised.active_revision_hash, Some(first_hash.clone()));
    assert_ne!(revised.head_revision_hash, first_hash);
    assert_eq!(revise_fact.delta, store::exact::from_f64(0.0));

    let stale = revise(
        &store.pool,
        ReviseProgramInput {
            request_id: "stale-revise".to_string(),
            program_uid: created.record_uid.clone(),
            expected_handle_revision: 2,
            program: program("flow.main", "Must not be inserted"),
            actor_person_uid: Some(PERSON_UID.to_string()),
        },
        now(3),
        signer,
    )
    .await
    .unwrap();
    assert!(matches!(
        stale,
        ProgramMutationCommit::Stale {
            current_handle_revision: 3
        }
    ));
    assert_eq!(count(&store, "karma_program_revision").await, 2);
    assert_eq!(count(&store, "karma_program_request").await, 3);

    let second_hash = revised.head_revision_hash.clone();
    let (switched, switch_fact) = committed(
        activate(
            &store.pool,
            ActivateProgramInput {
                request_id: "activate-flow-2".to_string(),
                program_uid: created.record_uid.clone(),
                expected_handle_revision: 3,
                revision_hash: second_hash.clone(),
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            now(4),
            signer,
        )
        .await
        .unwrap(),
    );
    assert_eq!(switched.handle_revision, 4);
    assert_eq!(switched.active_revision_hash, Some(second_hash));
    assert_eq!(switch_fact.delta, store::exact::from_f64(0.0));
    assert_eq!(record_quantity(&store, &created.record_uid).await, 1.0);

    let (paused, pause_fact) = committed(
        pause(
            &store.pool,
            PauseProgramInput {
                request_id: "pause-flow".to_string(),
                program_uid: created.record_uid.clone(),
                expected_handle_revision: 4,
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            now(5),
            signer,
        )
        .await
        .unwrap(),
    );
    assert_eq!(paused.handle_revision, 5);
    assert_eq!(paused.status, DefinitionStatus::Paused);
    assert_eq!(paused.active_revision_hash, None);
    assert_eq!(pause_fact.delta, store::exact::from_f64(-1.0));
    assert_eq!(record_quantity(&store, &created.record_uid).await, 0.0);

    let (original_activation, replayed_fact) = replayed(
        activate(
            &store.pool,
            ActivateProgramInput {
                request_id: "activate-flow-1".to_string(),
                program_uid: created.record_uid.clone(),
                expected_handle_revision: 1,
                revision_hash: first_hash,
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
    assert_eq!(replayed_fact, activate_fact);

    let mut facts = store::facts::for_record(&store.pool, &created.record_uid, 20)
        .await
        .unwrap();
    facts.reverse();
    assert_eq!(facts.len(), 5);
    assert_eq!(facts[0], create_fact);
    for fact in &facts {
        assert!(nucleus::fact::verify_chain_step(fact));
        let evidence: ProgramMutationEvidence =
            serde_json::from_str(fact.payload.as_deref().unwrap()).unwrap();
        assert_eq!(evidence.program_uid, created.record_uid);
    }
    assert_eq!(
        serde_json::from_str::<ProgramMutationEvidence>(pause_fact.payload.as_deref().unwrap())
            .unwrap()
            .action,
        ProgramMutationAction::Pause
    );
}

#[tokio::test]
async fn rejected_cross_program_collision_and_corruption_paths_fail_closed() {
    let store = Store::open_memory().await.unwrap();
    let rejected_input = CreateProgramInput {
        request_id: "create-rejected".to_string(),
        program: program("rejected.program", ""),
        owner_person_uid: None,
        actor_person_uid: Some(PERSON_UID.to_string()),
    };
    let (rejected, _) = committed(
        create(&store.pool, rejected_input, now(0), signer)
            .await
            .unwrap(),
    );
    assert_eq!(rejected.status, DefinitionStatus::Draft);
    let rejected_revision = get_revision(&store.pool, &rejected.head_revision_hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rejected_revision.proof.status, ProofStatus::Rejected);
    assert!(
        activate(
            &store.pool,
            ActivateProgramInput {
                request_id: "activate-rejected".to_string(),
                program_uid: rejected.record_uid.clone(),
                expected_handle_revision: 1,
                revision_hash: rejected.head_revision_hash.clone(),
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            now(1),
            signer,
        )
        .await
        .is_err()
    );
    assert_eq!(
        get_handle(&store.pool, &rejected.record_uid)
            .await
            .unwrap()
            .unwrap()
            .handle_revision,
        1
    );

    let (other, _) = committed(
        create(
            &store.pool,
            create_input("other.program", "Other", "create-other"),
            now(2),
            signer,
        )
        .await
        .unwrap(),
    );
    assert!(
        activate(
            &store.pool,
            ActivateProgramInput {
                request_id: "cross-activate".to_string(),
                program_uid: rejected.record_uid.clone(),
                expected_handle_revision: 1,
                revision_hash: other.head_revision_hash,
                actor_person_uid: Some(PERSON_UID.to_string()),
            },
            now(3),
            signer,
        )
        .await
        .is_err()
    );

    let collision = CreateProgramInput {
        request_id: "create-other".to_string(),
        program: program("other.program", "Other"),
        owner_person_uid: Some(PERSON_UID.to_string()),
        actor_person_uid: Some(OTHER_PERSON_UID.to_string()),
    };
    assert!(
        create(&store.pool, collision, now(4), signer)
            .await
            .is_err()
    );

    sqlx::query("DROP TRIGGER karma_program_revision_immutable_update")
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE karma_program_revision SET canonical_dsl = 'corrupt' WHERE revision_hash = ?",
    )
    .bind(rejected.head_revision_hash.as_str())
    .execute(&store.pool)
    .await
    .unwrap();
    assert!(
        get_revision(&store.pool, &rejected.head_revision_hash)
            .await
            .is_err()
    );
}

fn create_input(slug: &str, purpose: &str, request_id: &str) -> CreateProgramInput {
    CreateProgramInput {
        request_id: request_id.to_string(),
        program: program(slug, purpose),
        owner_person_uid: Some(PERSON_UID.to_string()),
        actor_person_uid: Some(PERSON_UID.to_string()),
    }
}

fn program(slug: &str, purpose: &str) -> ProgramAst {
    let program = ProgramAst {
        schema: ProgramSchema::V1,
        slug: Slug::new(slug).unwrap(),
        purpose: purpose.to_string(),
        tags: BTreeSet::new(),
        parameters: BTreeMap::new(),
        nodes: BTreeMap::new(),
        outputs: BTreeMap::new(),
        required_capabilities: CapabilitySet::default(),
    };
    assert_eq!(
        prove_program(&program).status,
        if purpose.is_empty() {
            ProofStatus::Rejected
        } else {
            ProofStatus::Accepted
        }
    );
    program
}

fn committed(
    commit: ProgramMutationCommit,
) -> (store::karma::programs::ProgramHandleRow, nucleus::Fact) {
    match commit {
        ProgramMutationCommit::Committed { handle, fact } => (handle, fact),
        other => panic!("expected committed Program mutation, got {other:?}"),
    }
}

fn replayed(
    commit: ProgramMutationCommit,
) -> (store::karma::programs::ProgramHandleRow, nucleus::Fact) {
    match commit {
        ProgramMutationCommit::Replayed { handle, fact } => (handle, fact),
        other => panic!("expected replayed Program mutation, got {other:?}"),
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
