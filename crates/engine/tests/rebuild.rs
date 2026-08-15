//! Rebuild and audit (Ontology §11, cluster C2b): "the log is authoritative"
//! as something checkable rather than a slogan.
//!
//! Every test here corrupts the READ MODEL behind the log's back — writing the
//! `record` row directly, which is what a torn concurrent import effectively
//! does — and then asks whether the audit notices and the rebuild repairs.

use engine::Engine;
use engine::trust::Signer;
use nucleus::RecordKind;
use store::records::NewRecord;

async fn cell() -> (Engine, String) {
    let e = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&e.store.pool, "http://cell-a")
        .await
        .expect("local organ")
        .uid;
    e.set_signer(Signer::generate(&organ, "k1"))
        .await
        .expect("signer");
    (e, organ)
}

async fn plain(e: &Engine, slug: &str, head: &str) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .map(|r| r.uid)
    .expect("record")
}

/// Corrupt the read model without touching the log — the shape a torn
/// concurrent import leaves behind.
async fn scribble(e: &Engine, uid: &str, head: &str) {
    store::sqlx::query("UPDATE record SET head = ? WHERE uid = ?")
        .bind(head)
        .bind(uid)
        .execute(&e.store.pool)
        .await
        .expect("scribble");
}

#[tokio::test]
async fn a_healthy_cell_audits_clean() {
    let (e, _organ) = cell().await;
    plain(&e, "apples", "Apples").await;
    plain(&e, "pears", "Pears").await;

    let audit = e.audit_read_model().await.expect("audit");
    assert!(audit.checked > 0, "it actually compared something");
    assert!(audit.is_clean(), "no divergence: {:?}", audit.diverged);
}

/// The failure this exists for: the log holds the right value and the read
/// model does not. Nothing else in the system notices, and it does not
/// self-heal.
#[tokio::test]
async fn the_audit_finds_a_read_model_that_drifted_from_the_log() {
    let (e, _organ) = cell().await;
    let uid = plain(&e, "apples", "Apples").await;
    scribble(&e, &uid, "Something Else").await;

    let audit = e.audit_read_model().await.expect("audit");
    assert!(!audit.is_clean(), "the drift was noticed");
    let found = audit
        .diverged
        .iter()
        .find(|d| d.record_uid == uid && d.field == "head")
        .expect("the drifted field is named");
    assert_eq!(found.expected, "Apples");
    assert_eq!(found.found, "Something Else");
}

#[tokio::test]
async fn the_rebuild_repairs_what_the_audit_found() {
    let (e, _organ) = cell().await;
    let uid = plain(&e, "apples", "Apples").await;
    scribble(&e, &uid, "Something Else").await;

    let (audit, rebuild) = e.audit_and_repair().await.expect("audit and repair");
    assert!(!audit.is_clean());
    let rebuild = rebuild.expect("a repair ran");
    assert!(rebuild.replayed > 0);

    let row = store::records::get(&e.store.pool, &uid)
        .await
        .expect("get")
        .expect("record");
    assert_eq!(row.head, "Apples", "the log's value won");
    assert!(
        e.audit_read_model().await.expect("audit").is_clean(),
        "and the Cell audits clean afterwards"
    );
}

/// A clean Cell is not rebuilt. Repairing unconditionally and calling that
/// health is how a detector stops being able to report anything.
#[tokio::test]
async fn a_clean_cell_is_not_rebuilt() {
    let (e, _organ) = cell().await;
    plain(&e, "apples", "Apples").await;

    let (audit, rebuild) = e.audit_and_repair().await.expect("audit and repair");
    assert!(audit.is_clean());
    assert!(rebuild.is_none(), "nothing to repair, so nothing ran");
}

/// Collaborative text comes back from the doc, not from the create-era `set`
/// op that the log also still holds.
#[tokio::test]
async fn a_rebuild_restores_collaborative_text_from_the_doc() {
    let (e, _organ) = cell().await;
    let uid = plain(&e, "notes", "Notes").await;
    e.write_record_text(&uid, None, Some("the real body"))
        .await
        .expect("write");
    e.compact_doc(&uid).await.expect("compact");
    store::sqlx::query("UPDATE record SET body = 'clobbered' WHERE uid = ?")
        .bind(&uid)
        .execute(&e.store.pool)
        .await
        .expect("scribble");

    let report = e.rebuild_read_model().await.expect("rebuild");
    assert!(report.docs_rebuilt > 0, "a doc was re-materialized");
    let row = store::records::get(&e.store.pool, &uid)
        .await
        .expect("get")
        .expect("record");
    assert_eq!(row.body, "the real body");
}

/// A rebuild must not resurrect a deleted record. The tombstone is in the log
/// like everything else, and replaying in HLC order has to land it.
#[tokio::test]
async fn a_rebuild_keeps_deletes_deleted() {
    let (e, _organ) = cell().await;
    let uid = plain(&e, "doomed", "Doomed").await;
    e.write_record_text(&uid, None, Some("text"))
        .await
        .expect("write");
    store::records::mark_deleted(&e.store.pool, &uid)
        .await
        .expect("delete");

    e.rebuild_read_model().await.expect("rebuild");

    assert!(
        store::records::get(&e.store.pool, &uid)
            .await
            .expect("get")
            .is_none(),
        "the delete survived the rebuild"
    );
}

/// Quantity is the fact chain's business. A rebuild must leave it exactly
/// alone — re-folding a chain that is already correct would corrupt the very
/// thing the chain exists to protect.
#[tokio::test]
async fn a_rebuild_does_not_touch_quantity() {
    let (e, _organ) = cell().await;
    let uid = plain(&e, "counter", "Counter").await;
    e.append_user(&uid, 7.0).await.expect("bump");
    let before = store::records::get(&e.store.pool, &uid)
        .await
        .expect("get")
        .expect("record")
        .quantity_f64();
    assert_eq!(before, 7.0);

    let report = e.rebuild_read_model().await.expect("rebuild");
    assert!(report.skipped > 0, "fact ops were skipped, not replayed");

    let after = store::records::get(&e.store.pool, &uid)
        .await
        .expect("get")
        .expect("record")
        .quantity_f64();
    assert_eq!(after, before, "the Ledger is untouched");
}

/// A rebuild replays; it never truncates. A row the log says nothing about
/// must survive, or the repair becomes the thing that loses data.
#[tokio::test]
async fn a_rebuild_does_not_delete_what_the_log_does_not_mention() {
    let (e, _organ) = cell().await;
    let uid = plain(&e, "apples", "Apples").await;
    store::sqlx::query("DELETE FROM sync_op WHERE uid = ?")
        .bind(&uid)
        .execute(&e.store.pool)
        .await
        .expect("wipe ops");

    e.rebuild_read_model().await.expect("rebuild");

    assert!(
        store::records::get(&e.store.pool, &uid)
            .await
            .expect("get")
            .is_some(),
        "the record survived a log that no longer describes it"
    );
}

/// Quantity is ADDED on import, never assigned, so a record's opening value
/// and its fact deltas compose whichever order they arrive in.
///
/// The bug this pins: a peer that received the facts first held the sum of the
/// deltas, and the creation op then overwrote it with the opening value —
/// silently discarding every change the record had ever seen. Reachable by any
/// peer catching up from zero, because a catch-up feed is served in local seq
/// order and nothing guarantees creation precedes the facts on the wire.
#[tokio::test]
async fn a_late_creation_op_does_not_reset_a_folded_quantity() {
    let (a, a_organ) = cell().await;
    let uid = store::records::create(
        &a.store.pool,
        NewRecord {
            slug: Some("stock"),
            kind: RecordKind::Plain,
            head: "Stock",
            body: "",
            quantity: store::exact::parse_decimal("5", 0).expect("five"),
        },
    )
    .await
    .map(|r| r.uid)
    .expect("record");
    a.append_user(&uid, 7.0).await.expect("bump");
    assert_eq!(
        store::records::get(&a.store.pool, &uid)
            .await
            .expect("get")
            .expect("record")
            .quantity_f64(),
        12.0,
        "opening 5 plus a delta of 7"
    );

    // B receives the same ops with creation LAST — the adversarial order.
    let (b, _b_organ) = cell().await;
    // Pair properly: an unpaired peer holds no key for A, so its signed facts
    // would be quarantined and the test would pass for the wrong reason.
    let a_intro = a.introduction().await.expect("introduction");
    b.adopt_introduction(&a_intro, 1).await.expect("adopt");
    let (feed, _head) = a.ops_after(0, 10_000).await.expect("feed");
    let (quantity_ops, rest): (Vec<_>, Vec<_>) = feed
        .into_iter()
        .partition(|op| op.tbl == "record" && op.field == "quantity");
    for batch in [rest, quantity_ops] {
        b.import_op_batch(&engine::sync::OpBatch {
            from_organ: a_organ.clone(),
            ops: batch,
        })
        .await
        .expect("import");
    }

    assert_eq!(
        store::records::get(&b.store.pool, &uid)
            .await
            .expect("get")
            .expect("record")
            .quantity_f64(),
        12.0,
        "the peer agrees, despite receiving creation after the facts"
    );
}

/// Two peers importing ops for the SAME field at once must leave the read
/// model agreeing with the log.
///
/// `wire.rs` serves every connection on its own task, so this interleaving is
/// the normal case rather than a race someone has to contrive. Each importer
/// reads the stored stamp, each decides it wins, and the later writer
/// materialises second — leaving the log with the correct winner and the read
/// model with the loser. It does not self-heal.
#[tokio::test]
async fn concurrent_imports_leave_the_read_model_agreeing_with_the_log() {
    let (e, _organ) = cell().await;
    let (a, a_organ) = cell().await;
    let (b, b_organ) = cell().await;
    for (peer, intro_from) in [(&a, &a_organ), (&b, &b_organ)] {
        let intro = peer.introduction().await.expect("introduction");
        e.adopt_introduction(&intro, 1).await.expect("adopt");
        let _ = intro_from;
    }

    // Both peers write the same field of the same uid, at different stamps.
    let uid = "r-contested";
    let batch = |organ: &str, cell_uid: &str, head: &str, hlc: i64| engine::sync::OpBatch {
        from_organ: organ.to_string(),
        ops: vec![engine::sync::WireOp {
            tbl: "record".into(),
            uid: uid.into(),
            field: "head".into(),
            kind: "set".into(),
            value: Some(format!("\"{head}\"")),
            hlc,
            actor_cell: cell_uid.into(),
            organ_uid: organ.to_string(),
            fact: None,
        }],
    };
    let early = nucleus::hlc::next();
    let late = nucleus::hlc::next();
    let from_a = batch(&a_organ, "cell-a", "from A", late);
    let from_b = batch(&b_organ, "cell-b", "from B", early);

    let (ra, rb) = tokio::join!(e.import_op_batch(&from_a), e.import_op_batch(&from_b));
    ra.expect("import a");
    rb.expect("import b");

    let audit = e.audit_read_model().await.expect("audit");
    assert!(
        audit.is_clean(),
        "the read model matches the log: {:?}",
        audit.diverged
    );
    let row = store::records::get(&e.store.pool, uid)
        .await
        .expect("get")
        .expect("record");
    assert_eq!(row.head, "from A", "the higher stamp won, in both places");
}
