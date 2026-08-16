//! Op log foundation (Ontology §11): every local write on a syncable table
//! becomes a field-level op — (tbl, uid, field, kind, value, hlc, actor) —
//! stamped by the Cell's HLC, with the local organ as actor. A Cell without a
//! local organ has no sync identity and logs nothing.

use engine::sync::Delivery;
use engine::Engine;
use engine::actions::Action;
use engine::trust::Signer;
use nucleus::RecordKind;
use store::records::NewRecord;
use store::sync_ops;

async fn cell() -> (Engine, String) {
    let e = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&e.store.pool, "http://cell.test")
        .await
        .expect("local organ")
        .uid;
    e.set_signer(Signer::generate(&organ, "k1"))
        .await
        .expect("signer");
    (e, organ)
}

async fn plain(e: &Engine, slug: &str) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head: slug,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .map(|r| r.uid)
    .expect("record")
}

async fn ops_for(e: &Engine, tbl: &str, uid: &str, field: &str) -> Vec<sync_ops::OpRow> {
    sync_ops::for_field(&e.store.pool, tbl, uid, field)
        .await
        .expect("ops")
}

#[tokio::test]
async fn record_create_and_edit_log_field_ops() {
    let (e, organ) = cell().await;
    let uid = plain(&e, "apples").await;

    // Creation logged the initial fields.
    for field in ["kind", "head", "body", "slug", "organ_uid"] {
        let ops = ops_for(&e, "record", &uid, field).await;
        assert_eq!(ops.len(), 1, "one create op for {field}");
        assert_eq!(ops[0].kind, "set");
        assert_eq!(ops[0].organ_uid, organ);
    }

    // A text edit logs ONE cumulative crdt op (the record-doc owns text);
    // head/body set ops stay create-era only.
    e.act(
        Action::EditRecordText {
            target: uid.clone(),
            head: Some("Apples".into()),
            body: Some("crisp".into()),
        },
        None,
    )
    .await
    .expect("edit");
    assert_eq!(ops_for(&e, "record", &uid, "head").await.len(), 1);
    let doc_ops = ops_for(&e, "record", &uid, "").await;
    assert_eq!(doc_ops.len(), 1);
    assert_eq!(doc_ops[0].kind, "crdt");
    assert!(
        doc_ops[0].hlc > ops_for(&e, "record", &uid, "head").await[0].hlc,
        "HLC is monotonic"
    );

    // Delete is a tombstone op, never a missing row in the log.
    e.act(
        Action::DeleteRecord {
            target: uid.clone(),
        },
        None,
    )
    .await
    .expect("delete");
    let tombstones: Vec<_> = ops_for(&e, "record", &uid, "")
        .await
        .into_iter()
        .filter(|op| op.kind == "tombstone")
        .collect();
    assert_eq!(tombstones.len(), 1);
}

#[tokio::test]
async fn extension_writes_diff_per_key() {
    let (e, _) = cell().await;
    let uid = plain(&e, "task").await;
    let ns = "work.tracking";

    store::records::set_extension(
        &e.store.pool,
        &uid,
        ns,
        &serde_json::json!({ "estimate": 3, "owner": "ana" }),
    )
    .await
    .expect("first set");
    // Change one key, drop one, keep nothing else equal.
    store::records::set_extension(
        &e.store.pool,
        &uid,
        ns,
        &serde_json::json!({ "estimate": 5 }),
    )
    .await
    .expect("second set");

    let estimate = ops_for(&e, "record_extension", &uid, "work.tracking.estimate").await;
    assert_eq!(estimate.len(), 2, "changed key logs a new set op");
    assert_eq!(estimate[1].value.as_deref(), Some("5"));

    let owner = ops_for(&e, "record_extension", &uid, "work.tracking.owner").await;
    assert_eq!(owner.len(), 2);
    assert_eq!(owner[1].kind, "tombstone", "removed key logs a tombstone");

    // An unchanged write logs nothing new.
    store::records::set_extension(
        &e.store.pool,
        &uid,
        ns,
        &serde_json::json!({ "estimate": 5 }),
    )
    .await
    .expect("noop set");
    assert_eq!(
        ops_for(&e, "record_extension", &uid, "work.tracking.estimate")
            .await
            .len(),
        2
    );
}

#[tokio::test]
async fn assertions_and_facts_join_the_log() {
    let (e, _) = cell().await;
    let subject = plain(&e, "note").await;
    let concept = store::concepts::create(&e.store.pool, "todo", &[])
        .await
        .expect("concept");

    // Concept creation is a plain per-field set op.
    let concept_ops = ops_for(&e, "concept", &concept, "canonical_name").await;
    assert_eq!(concept_ops.len(), 1);
    assert_eq!(concept_ops[0].value.as_deref(), Some("\"todo\""));

    // Assert / retract are set / tombstone per assertion uid.
    let assertion = store::assertions::assert(
        &e.store.pool,
        store::assertions::NewAssertion {
            subject_uid: &subject,
            predicate_uid: &concept,
            object_uid: None,
            role: store::assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await
    .expect("assert");
    let ops = ops_for(&e, "record_assertion", &assertion, "").await;
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].kind, "set");
    let value: serde_json::Value = serde_json::from_str(ops[0].value.as_deref().unwrap()).unwrap();
    assert_eq!(value["subject_uid"], subject.as_str());
    assert_eq!(value["predicate_uid"], concept.as_str());

    store::assertions::retract(&e.store.pool, &assertion, None)
        .await
        .expect("retract");
    let ops = ops_for(&e, "record_assertion", &assertion, "").await;
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[1].kind, "tombstone");

    // A quantity write is a fact — it joins the log as kind `fact`.
    e.append_user(&subject, 2.0).await.expect("bump");
    let facts = store::facts::for_record(&e.store.pool, &subject, 100)
        .await
        .expect("facts");
    let bump_fact = facts
        .iter()
        .find(|f| !f.delta.is_zero())
        .expect("bump fact");
    let fact_ops = ops_for(&e, "fact", &bump_fact.uid, "").await;
    assert_eq!(fact_ops.len(), 1);
    assert_eq!(fact_ops[0].kind, "fact");
    assert!(fact_ops[0].value.is_none());
}

/// The inverse of the rule this test used to assert.
///
/// It was `no_local_organ_means_no_ops`: a Cell without an Organ wrote to the
/// read model and logged nothing, so the write synced to nobody and no error
/// said so. That silent-loss path is gone — `Store::open` mints the identity,
/// so a store that opened at all has one, and there is no such thing as a
/// local write that fails to become an op.
#[tokio::test]
async fn a_bare_store_still_has_an_identity_and_still_logs() {
    let e = Engine::open_memory().await.expect("engine");
    let uid = store::records::create(
        &e.store.pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "loose",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid;
    let ops = ops_for(&e, "record", &uid, "head").await;
    assert_eq!(ops.len(), 1, "the write became an op");
    assert!(!ops[0].actor_cell.is_empty());
    assert!(!ops[0].organ_uid.is_empty());
}

#[tokio::test]
async fn op_identity_is_actor_plus_hlc() {
    let (e, organ) = cell().await;
    let cell_uid = store::cells::local(&e.store.pool)
        .await
        .expect("cell")
        .expect("cell record")
        .uid;
    let hlc = nucleus::hlc::next();
    let first = sync_ops::append(
        &e.store.pool,
        "record",
        "r-x",
        "head",
        sync_ops::OpKind::Set,
        Some("\"a\""),
        hlc,
        &cell_uid,
        &organ,
        None,
        None,
    )
    .await
    .expect("append");
    assert!(first.is_some());
    // Same (actor, hlc) again FROM A CONTACT: idempotent no-op, not an error.
    //
    // The import path is where this property belongs. A LOCAL write that finds
    // its identity taken has met another PROCESS writing as the same Cell, so
    // it re-mints its stamp instead of skipping — see `insert_local`. Skipping
    // there loses a write that exists in the read model and reaches nobody.
    let dup = sync_ops::append(
        &e.store.pool,
        "record",
        "r-x",
        "head",
        sync_ops::OpKind::Set,
        Some("\"b\""),
        hlc,
        &cell_uid,
        &organ,
        Some("r-a-contact"),
        None,
    )
    .await
    .expect("dup append");
    assert!(dup.is_none());

    // ...while the same collision on a LOCAL write lands, under a fresh stamp.
    let relocal = sync_ops::append(
        &e.store.pool,
        "record",
        "r-x",
        "head",
        sync_ops::OpKind::Set,
        Some("\"c\""),
        hlc,
        &cell_uid,
        &organ,
        None,
        None,
    )
    .await
    .expect("local append");
    assert!(relocal.is_some(), "a local write is never silently dropped");
}

// ---- retention (Ontology §11 "Op log") --------------------------------------
//
// Pruning is the one op-log operation that DESTROYS data, so what is pinned
// here is mostly what it must refuse to do.

/// Rewrite one extension key `n` times, leaving `n - 1` SUPERSEDED set ops.
///
/// Retention only drops ops a newer op has replaced, so a record written once
/// and never touched again has nothing prunable — it is all current state.
/// Anything asserting that pruning removed something has to create history
/// first, which is the honest shape of the feature.
async fn churn(e: &Engine, uid: &str, n: i64) {
    for i in 1..=n {
        store::records::set_extension(
            &e.store.pool,
            uid,
            "work.tracking",
            &serde_json::json!({ "estimate": i }),
        )
        .await
        .expect("extension");
    }
}

async fn contact(e: &Engine, uid: &str, sync_out: bool) {
    store::organs::add_contact(&e.store.pool, uid, None, uid, "http://peer", 1)
        .await
        .expect("contact");
    store::organs::set_sync_policy(&e.store.pool, uid, sync_out, true)
        .await
        .expect("policy");
}

/// With nobody to relay to there is no floor, and pruning must do nothing.
///
/// The tempting reading of "no contacts" is "nobody needs these ops, delete
/// them all". That is wrong while replica bootstrap does not exist: a contact
/// added tomorrow can only be brought up to date by replaying the log.
#[tokio::test]
async fn no_contacts_means_no_floor_and_nothing_is_pruned() {
    let (e, _organ) = cell().await;
    plain(&e, "one").await;
    let before = sync_ops::max_seq(&e.store.pool).await.expect("max");
    assert!(before > 0, "the record wrote ops");

    assert_eq!(
        sync_ops::retention_floor(&e.store.pool)
            .await
            .expect("floor"),
        None,
    );
    let report = e.prune_op_log(false).await.expect("prune");
    assert_eq!(report.removed, 0);
    assert_eq!(
        sync_ops::max_seq(&e.store.pool).await.expect("max"),
        before,
        "the log is untouched",
    );
}

/// The floor is the SLOWEST contact. A peer that has seen nothing pins it at
/// zero even when another is fully caught up — otherwise catching one peer up
/// would delete what the other still needs.
#[tokio::test]
async fn the_floor_is_the_least_advanced_contact() {
    let (e, _organ) = cell().await;
    let uid = plain(&e, "one").await;
    churn(&e, &uid, 5).await;
    let head = sync_ops::max_seq(&e.store.pool).await.expect("max");
    contact(&e, "organ-fast", true).await;
    contact(&e, "organ-slow", true).await;

    store::organs::advance_peer_acked_seq(&e.store.pool, "organ-fast", head)
        .await
        .expect("advance");
    assert_eq!(
        sync_ops::retention_floor(&e.store.pool)
            .await
            .expect("floor"),
        Some(0),
        "the slow contact holds the floor down",
    );
    assert_eq!(e.prune_op_log(false).await.expect("prune").removed, 0);

    store::organs::advance_peer_acked_seq(&e.store.pool, "organ-slow", head)
        .await
        .expect("advance");
    assert_eq!(
        sync_ops::retention_floor(&e.store.pool)
            .await
            .expect("floor"),
        Some(head),
    );
    let report = e.prune_op_log(false).await.expect("prune");
    assert!(report.removed > 0, "now there is something to drop");

    // The seqs are gone, and the next op does NOT reuse them — that is what
    // AUTOINCREMENT buys, and checkpoints depend on it: a recycled seq would
    // make a peer's checkpoint silently skip real ops.
    plain(&e, "two").await;
    let next = sync_ops::max_seq(&e.store.pool).await.expect("max");
    assert!(
        next > head,
        "a pruned seq must never be handed out again (got {next}, pruned through {head})",
    );
}

/// A blocked contact must not freeze retention forever: we will never send to
/// them again, so what they have not received is not owed to anyone.
#[tokio::test]
async fn a_blocked_contact_does_not_hold_the_floor() {
    let (e, _organ) = cell().await;
    plain(&e, "one").await;
    let head = sync_ops::max_seq(&e.store.pool).await.expect("max");
    contact(&e, "organ-live", true).await;
    contact(&e, "organ-gone", true).await;
    store::organs::advance_peer_acked_seq(&e.store.pool, "organ-live", head)
        .await
        .expect("advance");
    store::organs::set_trust(&e.store.pool, "organ-gone", "blocked")
        .await
        .expect("block");

    assert_eq!(
        sync_ops::retention_floor(&e.store.pool)
            .await
            .expect("floor"),
        Some(head),
    );
}

/// The floor moves FORWARD only. A peer may legitimately ask from an older
/// point (a rebuild, a restored backup); that must never rewind the floor.
#[tokio::test]
async fn the_floor_never_goes_backwards() {
    let (e, _organ) = cell().await;
    plain(&e, "one").await;
    let head = sync_ops::max_seq(&e.store.pool).await.expect("max");
    contact(&e, "organ-a", true).await;

    store::organs::advance_peer_acked_seq(&e.store.pool, "organ-a", head)
        .await
        .expect("advance");
    store::organs::advance_peer_acked_seq(&e.store.pool, "organ-a", 0)
        .await
        .expect("advance");
    assert_eq!(
        sync_ops::retention_floor(&e.store.pool)
            .await
            .expect("floor"),
        Some(head),
    );
}

/// An op still queued for delivery survives pruning even when it sits below
/// the floor. The outbox points into the log by `seq`, so deleting it would
/// turn a pending delivery into a silent no-op — `drain_outbox` would find
/// nothing and drop the row.
#[tokio::test]
async fn a_queued_op_is_never_pruned_out_from_under_the_outbox() {
    let (e, _organ) = cell().await;
    contact(&e, "organ-a", true).await;
    // Written AFTER the contact exists, so the write enqueues to them.
    plain(&e, "one").await;
    let head = sync_ops::max_seq(&e.store.pool).await.expect("max");
    let queued = sync_ops::outbox_due(&e.store.pool).await.expect("outbox");
    assert!(!queued.is_empty(), "the write queued something to send");

    // Claim the peer is fully caught up even though delivery never happened —
    // the contradiction pruning has to survive.
    store::organs::advance_peer_acked_seq(&e.store.pool, "organ-a", head)
        .await
        .expect("advance");
    let report = e.prune_op_log(false).await.expect("prune");
    assert!(report.retained > 0, "queued ops were kept back");

    for row in &queued {
        assert!(
            sync_ops::get_by_seq(&e.store.pool, row.seq)
                .await
                .expect("get")
                .is_some(),
            "seq {} is still queued and must still exist",
            row.seq,
        );
    }
}

/// A dry run reports exactly what the real one would delete, and changes
/// nothing. Same predicate, so the two cannot drift apart.
#[tokio::test]
async fn a_dry_run_reports_without_deleting() {
    let (e, _organ) = cell().await;
    let uid = plain(&e, "one").await;
    churn(&e, &uid, 5).await;
    let head = sync_ops::max_seq(&e.store.pool).await.expect("max");
    contact(&e, "organ-a", true).await;
    store::organs::advance_peer_acked_seq(&e.store.pool, "organ-a", head)
        .await
        .expect("advance");

    let dry = e.prune_op_log(true).await.expect("dry");
    assert!(dry.removed > 0);
    assert_eq!(
        sync_ops::max_seq(&e.store.pool).await.expect("max"),
        head,
        "a dry run deletes nothing",
    );
    let wet = e.prune_op_log(false).await.expect("prune");
    assert_eq!(dry.removed, wet.removed, "the report matched the deletion");
}

/// A grant-holding contact with the broad feed OFF must not raise the floor
/// past ops an ordinary contact still needs.
///
/// The two are counted together in `retention_floor` on purpose — a grant is
/// its own permission and keeps flowing while `sync_out` is off — so this pins
/// that including it cannot let one contact's progress speak for another's.
#[tokio::test]
async fn a_grant_only_contact_cannot_raise_the_floor_alone() {
    let (e, _organ) = cell().await;
    let root = plain(&e, "conversation").await;
    store::replica::make_own_root(&e.store.pool, &root)
        .await
        .expect("root");

    // One ordinary feed contact, one grant-only contact.
    contact(&e, "organ-feed", true).await;
    contact(&e, "organ-grant", false).await;
    store::replica::offer(&e.store.pool, &root, "organ-grant")
        .await
        .expect("offer");
    store::replica::accept(&e.store.pool, &root, "organ-grant")
        .await
        .expect("accept");

    let head = sync_ops::max_seq(&e.store.pool).await.expect("max");

    // The grant contact races ahead; the feed contact has received nothing.
    store::organs::advance_peer_acked_seq(&e.store.pool, "organ-grant", head)
        .await
        .expect("advance");

    assert_eq!(
        sync_ops::retention_floor(&e.store.pool)
            .await
            .expect("floor"),
        Some(0),
        "the feed contact still pins the floor at zero",
    );
    assert_eq!(
        e.prune_op_log(false).await.expect("prune").removed,
        0,
        "nothing may be dropped while a contact is still owed it",
    );
}

/// C7 — the executor designation survives the trip to another Cell.
///
/// The load-bearing half of "it syncs, so every Cell learns the same answer",
/// and the half a write-side op count cannot reach. The hazard is specific:
/// the op's field is `"{namespace}.{key}"`, and this namespace is the first one
/// carrying TWO dots (`lince.schedule.executor` + `cell`). A `split_once` on the
/// receiving side would yield namespace `"lince"` and key
/// `"karma.executor.cell"` — the value would land in a namespace nothing reads,
/// the receiving Cell would never learn the designation, and it would keep
/// running a Program designated elsewhere. Silent, and exactly the duplicate
/// the whole mechanism exists to prevent.
#[tokio::test]
async fn the_executor_designation_survives_the_wire() {
    let (a, a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    let a_intro = a.introduction().await.unwrap();
    let b_intro = b.introduction().await.unwrap();
    b.adopt_introduction(&a_intro, 1).await.unwrap();
    a.adopt_introduction(&b_intro, 1).await.unwrap();
    store::organs::set_sync_policy(&a.store.pool, &b_organ, true, false)
        .await
        .unwrap();
    store::organs::set_sync_policy(&b.store.pool, &a_organ, true, false)
        .await
        .unwrap();

    let uid = plain(&a, "rule").await;
    store::records::set_extension(
        &a.store.pool,
        &uid,
        store::executor::NAMESPACE,
        &serde_json::json!({ "cell": "r_01ARZ3NDEKTSV4RRFFQ69G5FAX" }),
    )
    .await
    .unwrap();

    let target = &b;
    a.drain_outbox(|_contact, root, batch| async move {
        match root {
            Some(root) => target.import_grant_batch(&root, &batch).await,
            None => target.import_op_batch(&batch).await,
        }
        .map_or_else(|e| Delivery::Failed(e.to_string()), |_| Delivery::Sent)
    })
    .await
    .expect("drain");

    let landed = store::records::get_extension(&b.store.pool, &uid, store::executor::NAMESPACE)
        .await
        .unwrap();
    assert_eq!(
        landed
            .as_ref()
            .and_then(|fds| fds.get("cell"))
            .and_then(|cell| cell.as_str()),
        Some("r_01ARZ3NDEKTSV4RRFFQ69G5FAX"),
        "the designation must arrive under the namespace the reader queries"
    );
}
