//! Axis 1 (Ontology C7): a rule authored on one Cell runs on another.
//!
//! Before this, `op_in_scope` named the tables that travel and no `karma_*`
//! table was among them — and the Program's Record row did not stand in for
//! it either, because `programs::create` inserts that row raw, with no op. Not
//! even the NAME of a rule reached another Cell. "Karma
//! not synced" was not a supported second mode; it was the only one, by
//! accident.
//!
//! The two tests that matter here are the round trip and the REFUSAL. The
//! refusal is the load-bearing one: a Program is executable content, and the
//! difference between "my other laptop published this" and "a contact pushed
//! this at me" is the difference between sync and remote code execution.

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

/// A Cell of a brand-new Organ.
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

/// Make an already-paired Cell a SECOND CELL OF `organ_uid` — the arrangement
/// axis 1 is about.
///
/// Enrolment (C3) is what will produce this for real. Until it exists the local
/// Organ uid is rewritten here, and it is done AFTER pairing on purpose: pairing
/// two Cells of one Organ through the contact path would have each of them
/// holding a contact row for itself, which is a shape the sync policy code has
/// no reason to support and would make the test pass or fail for reasons that
/// have nothing to do with Karma. Pair as strangers, then become siblings.
///
/// Foreign keys go off for the rewrite because the Organ uid is referenced from
/// several tables; this is a fixture standing in for enrolment, not a supported
/// operation.
async fn become_sibling_of(engine: &Engine, organ_uid: &str) {
    let existing = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .expect("local organ")
        .uid;
    let mut connection = engine.store.pool.acquire().await.unwrap();
    store::sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *connection)
        .await
        .unwrap();
    // Pairing left a contact Record for the other Organ. Becoming that Organ
    // means the two rows are now one identity, so the contact copy goes: an
    // Organ is not its own contact.
    for statement in [
        "DELETE FROM organ_contact WHERE record_uid = ?",
        "DELETE FROM record WHERE uid = ?",
    ] {
        store::sqlx::query(statement)
            .bind(organ_uid)
            .execute(&mut *connection)
            .await
            .unwrap();
    }
    for statement in [
        "UPDATE record SET organ_uid = ? WHERE organ_uid = ?",
        "UPDATE record SET uid = ? WHERE uid = ?",
        "UPDATE sync_op SET organ_uid = ? WHERE organ_uid = ?",
    ] {
        store::sqlx::query(statement)
            .bind(organ_uid)
            .bind(&existing)
            .execute(&mut *connection)
            .await
            .unwrap();
    }
    store::sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *connection)
        .await
        .unwrap();
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

/// Deliver to a SIBLING — another Cell of the same Organ.
///
/// Not `drain_outbox`, and that is a fact about the architecture rather than a
/// shortcut: `organ_contact` is keyed by the contact's Organ uid and a
/// sibling's is OUR OWN, so the contact table cannot represent a sibling and
/// never will (`wire.rs` says so where it resolves one). The roster is the
/// sibling list, and a sibling catches up by PULLING the log — which is what
/// `ops_after` serves and what this mirrors.
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

/// The whole point of axis 1: the rule itself crosses, not merely its name.
///
/// Asserted through the tables `freeze_next_epoch` joins rather than through
/// the extension it arrived in — an extension nothing materialises is a rule
/// the receiving Cell can display and never run, which is the state this test
/// exists to distinguish from success.
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
    // The engine publishes on mutation; these fixtures write through the store
    // directly, so publishing is explicit here for the same reason the store
    // tests call the store — the action layer is exercised by its own suite.
    store::karma::sync::publish_frequency(&a.store.pool, &frequency.record_uid)
        .await
        .unwrap();
    store::karma::sync::publish_program(&a.store.pool, &program.record_uid)
        .await
        .unwrap();

    deliver_to_sibling(&a, &b, &organ).await;

    // Separates "the op never arrived" from "it arrived and did not
    // materialise" — two different bugs that look identical from the handle.
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

/// Layer one: a rule definition never LEAVES for a contact.
///
/// It rides the ordinary extension op path, and `op_in_scope` returns true for
/// an un-narrowed contact before it looks at the field at all — so without the
/// outbound filter, adding sync between your own Cells would have handed every
/// sync contact the full text of every rule you run. A privacy regression
/// introduced by a sync feature is still a privacy regression.
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

/// Layer two: if a definition arrives ANYWAY, it is stored and inert.
///
/// The batch is assembled by hand from the sender's log, which is exactly what
/// a peer that ignores our outbound filter would send — the filter is our
/// policy on our side and proves nothing about what can arrive. What decides
/// here is the batch's AUTHENTICATED origin, not the Record's `organ_uid`,
/// which is a column the sender fills in and can therefore say whatever the
/// sender wants. A Program that materialises on receipt is remote code
/// execution wearing a sync mechanism.
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

/// A pause travels. Otherwise turning a rule off here leaves the always-on Cell
/// running the last definition it heard about — the failure that makes people
/// stop trusting sync.
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

/// The hash is re-derived, never believed. A definition whose content does not
/// match its content-address is refused rather than stored under a name that
/// lies about it.
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
    // Rewrite the published hash to one that does not address this AST — the
    // shape of a corrupted or forged publication.
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

// ---- fixtures, mirroring `store/tests/karma_runs.rs` ----

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

/// A slug already taken on the receiver costs the NAME, never the rule.
///
/// `record.slug` is UNIQUE, and two Cells that each authored a rule called
/// `run.matching` before they ever synced is an ordinary situation rather than
/// an attack. Letting the insert fail there would quarantine the definition and
/// leave the rule silently not running, which is the failure this whole cluster
/// is about — so the arriving rule lands nameless and runs. A missing name is
/// visible on the surface; a missing rule is visible nowhere.
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

    // An unrelated Record on the receiver is already holding that slug.
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
