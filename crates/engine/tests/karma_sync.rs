use engine::sync::Delivery;
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use chrono::{DateTime, TimeZone, Utc};
use engine::Engine;
use engine::trust::Signer;
use nucleus::karma::{
    CapabilitySet, DurationBinding, DurationMs, FrequencyCadenceAst, FrequencyParameterDefinition,
    FrequencySchema, FrequencyTimerAst, InactiveGapPolicy, LocalId, MissedPolicy, NodeAst,
    NodeOperation, OutputRef, OverloadPolicy, PortContract, ProgramAst, ProgramSchema, ProofStatus,
    ReferenceKind, RephasePolicy, ResolvedReference, Sensitivity, Slug, TimestampMs, TriggerSource,
    TypedUid, ValueType, prove_program,
};
use store::Store;
use store::karma::frequencies::{
    ActivateFrequencyInput, CreateFrequencyInput, FrequencyHandleRow, FrequencyMutationCommit,
    activate as activate_frequency, create as create_frequency,
};
use store::karma::programs::{
    ActivateProgramInput, CreateProgramInput, ProgramHandleRow, ProgramMutationCommit,
    activate as activate_program, create as create_program,
};

async fn cell() -> (Engine, String) {
    let engine = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&engine.store.pool, "http://cell.test")
        .await
        .expect("local organ")
        .uid;
    engine
        .set_signer(Signer::generate(&organ, "k1"))
        .await
        .expect("signer");
    (engine, organ)
}

async fn become_sibling_of(engine: &Engine, organ_uid: &str) {
    let existing = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .expect("local organ")
        .uid;
    let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
    store::sqlx::query("UPDATE record SET slug = NULL WHERE uid = ?")
        .bind(&existing)
        .execute(&mut *tx)
        .await
        .unwrap();
    let assigned = store::sqlx::query(
        "UPDATE record SET slug = ? WHERE uid = ? AND kind = 'organ' AND deleted_at IS NULL",
    )
    .bind(store::organs::LOCAL_ORGAN_SLUG)
    .bind(organ_uid)
    .execute(&mut *tx)
    .await
    .unwrap();
    assert_eq!(assigned.rows_affected(), 1);
    for statement in [
        "UPDATE record SET organ_uid = ? WHERE organ_uid = ?",
        "UPDATE sync_op SET organ_uid = ? WHERE organ_uid = ?",
    ] {
        store::sqlx::query(statement)
            .bind(organ_uid)
            .bind(&existing)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
    assert_eq!(
        store::organs::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid,
        organ_uid
    );
    assert_eq!(
        store::cells::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap()
            .organ_uid,
        organ_uid
    );
    assert!(
        store::records::get(&engine.store.pool, &existing)
            .await
            .unwrap()
            .is_some()
    );
}

async fn pair(from: &Engine, from_organ: &str, to: &Engine, to_organ: &str) {
    let from_intro = from.introduction().await.unwrap();
    let to_intro = to.introduction().await.unwrap();
    to.adopt_introduction(&from_intro, 1).await.unwrap();
    from.adopt_introduction(&to_intro, 1).await.unwrap();
    store::organs::set_sync_policy(&from.store.pool, to_organ, true, false)
        .await
        .unwrap();
    store::organs::set_sync_policy(&to.store.pool, from_organ, true, false)
        .await
        .unwrap();
}

async fn deliver_to_sibling(from: &Engine, to: &Engine, organ: &str) {
    let (ops, _head) = from.ops_after(0, 1_000).await.unwrap();
    to.import_op_batch(&engine::sync::OpBatch {
        from_organ: organ.to_string(),
        ops,
    })
    .await
    .expect("import");
}

async fn deliver(from: &Engine, to: &Engine) {
    let target = to;
    from.drain_outbox(|_contact, root, batch| async move {
        match root {
            Some(root) => target.import_grant_batch(&root, &batch).await,
            None => target.import_op_batch(&batch).await,
        }
        .map_or_else(|e| Delivery::Failed(e.to_string()), |_| Delivery::Sent)
    })
    .await
    .expect("drain");
}

#[tokio::test]
async fn a_rule_authored_on_one_cell_becomes_runnable_on_another() {
    let (a, organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair(&a, &organ, &b, &b_organ).await;
    become_sibling_of(&b, &organ).await;

    let frequency = active_frequency(&a.store, instant()).await;
    let program = active_program(
        &a.store,
        "run.matching",
        &frequency.record_uid,
        "one",
        instant(),
    )
    .await;
    store::karma::sync::publish_frequency(&a.store.pool, &frequency.record_uid)
        .await
        .unwrap();
    store::karma::sync::publish_program(&a.store.pool, &program.record_uid)
        .await
        .unwrap();

    deliver_to_sibling(&a, &b, &organ).await;

    assert!(
        store::records::get_extension(
            &b.store.pool,
            &program.record_uid,
            store::karma::sync::NAMESPACE,
        )
        .await
        .unwrap()
        .is_some(),
        "precondition: the published definition reached the other Cell at all"
    );
    let landed = store::karma::programs::get_handle(&b.store.pool, &program.record_uid)
        .await
        .unwrap()
        .expect("the Program must exist on the receiving Cell");
    assert_eq!(
        landed.active_revision_hash, program.active_revision_hash,
        "the receiving Cell must run the same revision, byte for byte"
    );
    let revision = store::karma::programs::get_revision(
        &b.store.pool,
        landed.active_revision_hash.as_ref().unwrap(),
    )
    .await
    .unwrap()
    .expect("the definition itself must have arrived, not only the handle");
    assert_eq!(revision.program.slug.as_str(), "run.matching");

    let frequency_landed =
        store::karma::frequencies::get_handle(&b.store.pool, &frequency.record_uid)
            .await
            .unwrap()
            .expect("the Frequency must arrive too, or the Program has nothing to schedule it");
    assert_eq!(
        frequency_landed.active_activation_hash, frequency.active_activation_hash,
        "the activation epoch is what drives the schedule; the same one must be in force"
    );
}

#[tokio::test]
async fn a_contacts_feed_never_carries_a_rule_definition() {
    let (a, a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair(&a, &a_organ, &b, &b_organ).await;

    let frequency = active_frequency(&a.store, instant()).await;
    let program = active_program(
        &a.store,
        "run.matching",
        &frequency.record_uid,
        "one",
        instant(),
    )
    .await;
    store::karma::sync::publish_frequency(&a.store.pool, &frequency.record_uid)
        .await
        .unwrap();
    store::karma::sync::publish_program(&a.store.pool, &program.record_uid)
        .await
        .unwrap();

    deliver(&a, &b).await;

    assert!(
        store::records::get_extension(
            &b.store.pool,
            &program.record_uid,
            store::karma::sync::NAMESPACE,
        )
        .await
        .unwrap()
        .is_none(),
        "a contact must not receive the text of a rule at all"
    );
}

#[tokio::test]
async fn a_rule_arriving_from_a_contact_is_stored_but_never_becomes_runnable() {
    let (a, a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair(&a, &a_organ, &b, &b_organ).await;

    let frequency = active_frequency(&a.store, instant()).await;
    let program = active_program(
        &a.store,
        "run.matching",
        &frequency.record_uid,
        "one",
        instant(),
    )
    .await;
    store::karma::sync::publish_frequency(&a.store.pool, &frequency.record_uid)
        .await
        .unwrap();
    store::karma::sync::publish_program(&a.store.pool, &program.record_uid)
        .await
        .unwrap();

    let (ops, _head) = a.ops_after(0, 1_000).await.unwrap();
    b.import_op_batch(&engine::sync::OpBatch {
        from_organ: a_organ,
        ops,
    })
    .await
    .unwrap();

    assert!(
        store::records::get_extension(
            &b.store.pool,
            &program.record_uid,
            store::karma::sync::NAMESPACE,
        )
        .await
        .unwrap()
        .is_some(),
        "precondition: it did arrive, so the assertions below are about the gate"
    );
    assert!(
        store::karma::programs::get_handle(&b.store.pool, &program.record_uid)
            .await
            .unwrap()
            .is_none(),
        "a contact's Program must never become a runnable handle"
    );
    assert!(
        store::karma::frequencies::get_handle(&b.store.pool, &frequency.record_uid)
            .await
            .unwrap()
            .is_none(),
        "nor its Frequency, or the schedule would exist waiting for a Program"
    );
}

#[tokio::test]
async fn pausing_a_rule_stops_it_on_the_other_cell_too() {
    let (a, organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair(&a, &organ, &b, &b_organ).await;
    become_sibling_of(&b, &organ).await;

    let frequency = active_frequency(&a.store, instant()).await;
    let program = active_program(
        &a.store,
        "run.matching",
        &frequency.record_uid,
        "one",
        instant(),
    )
    .await;
    store::karma::sync::publish_frequency(&a.store.pool, &frequency.record_uid)
        .await
        .unwrap();
    store::karma::sync::publish_program(&a.store.pool, &program.record_uid)
        .await
        .unwrap();
    deliver_to_sibling(&a, &b, &organ).await;
    assert!(
        store::karma::programs::get_handle(&b.store.pool, &program.record_uid)
            .await
            .unwrap()
            .and_then(|handle| handle.active_revision_hash)
            .is_some(),
        "precondition: it is running there before the pause"
    );

    store::karma::programs::pause(
        &a.store.pool,
        store::karma::programs::PauseProgramInput {
            request_id: "pause-one".into(),
            program_uid: program.record_uid.clone(),
            expected_handle_revision: program.handle_revision,
            actor_person_uid: None,
        },
        instant(),
        |_| None,
    )
    .await
    .unwrap();
    store::karma::sync::publish_program(&a.store.pool, &program.record_uid)
        .await
        .unwrap();
    deliver_to_sibling(&a, &b, &organ).await;

    let after = store::karma::programs::get_handle(&b.store.pool, &program.record_uid)
        .await
        .unwrap()
        .expect("the handle stays, paused");
    assert_eq!(
        after.active_revision_hash, None,
        "a pause must cross, or the other Cell keeps running a rule you switched off"
    );
}

#[tokio::test]
async fn a_definition_whose_hash_does_not_match_its_content_is_refused() {
    let (a, organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair(&a, &organ, &b, &b_organ).await;
    become_sibling_of(&b, &organ).await;

    let frequency = active_frequency(&a.store, instant()).await;
    let program = active_program(
        &a.store,
        "run.matching",
        &frequency.record_uid,
        "one",
        instant(),
    )
    .await;
    store::karma::sync::publish_program(&a.store.pool, &program.record_uid)
        .await
        .unwrap();
    let mut fds = store::records::get_extension(
        &a.store.pool,
        &program.record_uid,
        store::karma::sync::NAMESPACE,
    )
    .await
    .unwrap()
    .unwrap();
    fds["program"]["hash"] = serde_json::json!(format!("sha256:{}", "a".repeat(64)));
    store::records::set_extension(
        &a.store.pool,
        &program.record_uid,
        store::karma::sync::NAMESPACE,
        &fds,
    )
    .await
    .unwrap();

    deliver_to_sibling(&a, &b, &organ).await;

    assert!(
        store::karma::programs::get_handle(&b.store.pool, &program.record_uid)
            .await
            .unwrap()
            .is_none(),
        "a hash that does not address its content must not become a runnable rule"
    );
}

async fn active_frequency(store: &Store, now: DateTime<Utc>) -> FrequencyHandleRow {
    let created = committed_frequency(
        create_frequency(
            &store.pool,
            CreateFrequencyInput {
                request_id: "create-sync-frequency".to_string(),
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
                request_id: "activate-sync-frequency".to_string(),
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
    let created = committed_program(
        create_program(
            &store.pool,
            CreateProgramInput {
                request_id: format!("create-{request_suffix}"),
                program: program(slug, frequency_uid),
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

fn frequency() -> nucleus::karma::FrequencyAst {
    nucleus::karma::FrequencyAst {
        schema: FrequencySchema::V1,
        slug: Slug::new("run.frequency").unwrap(),
        purpose: "Produce deterministic sync test occurrences".to_string(),
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

fn timestamp() -> TimestampMs {
    TimestampMs::parse_canonical("2026-07-22T12:00:00.000Z").unwrap()
}

fn instant() -> DateTime<Utc> {
    Utc.timestamp_millis_opt(timestamp().as_millis())
        .single()
        .unwrap()
}

#[tokio::test]
async fn a_slug_already_taken_costs_the_name_and_not_the_rule() {
    let (a, organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair(&a, &organ, &b, &b_organ).await;
    become_sibling_of(&b, &organ).await;

    let frequency = active_frequency(&a.store, instant()).await;
    let program = active_program(
        &a.store,
        "run.matching",
        &frequency.record_uid,
        "one",
        instant(),
    )
    .await;
    store::karma::sync::publish_frequency(&a.store.pool, &frequency.record_uid)
        .await
        .unwrap();
    store::karma::sync::publish_program(&a.store.pool, &program.record_uid)
        .await
        .unwrap();

    store::sqlx::query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                             organ_uid, created_at, updated_at)
         VALUES ('r_01ARZ3NDEKTSV4RRFFQ69G5FAX', 'run.matching', 'thing', 'squatter', '',
                 '1', 0, ?, '2026-08-15T00:00:00Z', '2026-08-15T00:00:00Z')",
    )
    .bind(&organ)
    .execute(&b.store.pool)
    .await
    .unwrap();

    deliver_to_sibling(&a, &b, &organ).await;

    let landed = store::karma::programs::get_handle(&b.store.pool, &program.record_uid)
        .await
        .unwrap()
        .expect("the rule must arrive and be runnable even with its name taken");
    assert_eq!(landed.active_revision_hash, program.active_revision_hash);
    assert_ne!(
        landed.slug, "run.matching",
        "the arriving rule gives up the contested name rather than the run"
    );
    assert!(
        landed.slug.starts_with("run.matching-"),
        "and the name it takes instead is still recognisably the same rule, got {:?}",
        landed.slug
    );
}
