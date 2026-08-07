//! The Loro record-doc layer (Ontology §11 "Merge"/"Collab", server side):
//! text edits are cumulative `crdt` ops that converge character-wise across
//! Cells; SQLite always holds the materialized current values.

use engine::Engine;
use engine::actions::Action;
use engine::trust::Signer;
use nucleus::RecordKind;
use store::records::NewRecord;
use store::sync_ops;

async fn cell(base_url: &str) -> (Engine, String) {
    let e = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&e.store.pool, base_url)
        .await
        .expect("local organ")
        .uid;
    e.set_signer(Signer::generate(&organ, "k1"))
        .await
        .expect("signer");
    (e, organ)
}

async fn plain(e: &Engine, slug: &str, head: &str, body: &str) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head,
            body,
            quantity: store::exact::zero(),
        },
    )
    .await
    .map(|r| r.uid)
    .expect("record")
}

async fn pair_both_ways(a: &Engine, a_organ: &str, b: &Engine, b_organ: &str) {
    let a_intro = a.introduction().await.unwrap();
    let b_intro = b.introduction().await.unwrap();
    b.adopt_introduction(&a_intro, 1).await.unwrap();
    a.adopt_introduction(&b_intro, 1).await.unwrap();
    store::organs::set_sync_policy(&a.store.pool, b_organ, true, false)
        .await
        .unwrap();
    store::organs::set_sync_policy(&b.store.pool, a_organ, true, false)
        .await
        .unwrap();
}

async fn wire_push(from: &Engine, to: &Engine) -> usize {
    from.drain_outbox(|_contact, root, batch| async move {
        match root {
            Some(root) => to.import_grant_batch(&root, &batch).await,
            None => to.import_op_batch(&batch).await,
        }
        .map(|_| ())
        .map_err(|e| e.to_string())
    })
    .await
    .expect("drain")
}

async fn edit(e: &Engine, uid: &str, head: Option<&str>, body: Option<&str>) {
    e.act(
        Action::EditRecordText {
            target: uid.to_string(),
            head: head.map(str::to_string),
            body: body.map(str::to_string),
        },
        None,
    )
    .await
    .expect("edit");
}

async fn text_of(e: &Engine, uid: &str) -> (String, String) {
    let r = store::records::get(&e.store.pool, uid)
        .await
        .unwrap()
        .expect("record");
    (r.head, r.body)
}

#[tokio::test]
async fn text_edits_are_crdt_ops_not_set_ops() {
    let (e, _) = cell("http://cell-solo").await;
    let uid = plain(&e, "note", "note", "hello world").await;

    edit(&e, &uid, None, Some("hello brave world")).await;

    let crdt_ops = sync_ops::for_field(&e.store.pool, "record", &uid, "")
        .await
        .unwrap()
        .into_iter()
        .filter(|op| op.kind == "crdt")
        .count();
    assert_eq!(crdt_ops, 1, "one cumulative crdt op per edit");
    // Head/body never travel as post-create set ops.
    let head_sets = sync_ops::for_field(&e.store.pool, "record", &uid, "head")
        .await
        .unwrap();
    let body_sets = sync_ops::for_field(&e.store.pool, "record", &uid, "body")
        .await
        .unwrap();
    assert_eq!(head_sets.len(), 1, "only the create op");
    assert_eq!(body_sets.len(), 1, "only the create op");
    // SQLite is materialized.
    assert_eq!(text_of(&e, &uid).await.1, "hello brave world");
}

#[tokio::test]
async fn concurrent_text_edits_converge_on_both_cells() {
    let (a, a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    pair_both_ways(&a, &a_organ, &b, &b_organ).await;

    let note = plain(&a, "shared", "shared", "hello world").await;
    wire_push(&a, &b).await;

    // Concurrent CHARACTER edits to the SAME field.
    edit(&a, &note, None, Some("hello brave world")).await;
    edit(&b, &note, None, Some("hello world!")).await;
    wire_push(&a, &b).await;
    wire_push(&b, &a).await;
    // A's merge result produced a new op for B; settle the echo round.
    wire_push(&a, &b).await;

    let (_, a_body) = text_of(&a, &note).await;
    let (_, b_body) = text_of(&b, &note).await;
    assert!(
        a_body.contains("brave") && a_body.contains('!'),
        "both edits survive: {a_body:?}"
    );
    assert_eq!(a_body, b_body, "cells converge to identical text");
    assert_eq!(
        store::organs::quarantine_count(&b.store.pool)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn offline_burst_queues_one_cumulative_op() {
    let (a, a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    pair_both_ways(&a, &a_organ, &b, &b_organ).await;

    let note = plain(&a, "burst", "burst", "v0").await;
    wire_push(&a, &b).await;

    for i in 1..=10 {
        edit(&a, &note, None, Some(&format!("v{i}"))).await;
    }
    let queued: Vec<_> = sync_ops::outbox_due(&a.store.pool)
        .await
        .unwrap()
        .into_iter()
        .filter(|row| row.uid == note && row.tbl == "record" && row.field.is_empty())
        .collect();
    assert_eq!(queued.len(), 1, "ten edits, one queued cumulative op");

    wire_push(&a, &b).await;
    assert_eq!(text_of(&b, &note).await.1, "v10", "the tail is lossless");
}

#[tokio::test]
async fn deleted_record_freezes_its_doc() {
    let (a, a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    pair_both_ways(&a, &a_organ, &b, &b_organ).await;

    let note = plain(&a, "doomed", "doomed", "original").await;
    edit(&a, &note, None, Some("edited")).await;
    wire_push(&a, &b).await;

    // B deletes; the tombstone reaches A.
    b.act(
        Action::DeleteRecord {
            target: note.clone(),
        },
        None,
    )
    .await
    .unwrap();
    wire_push(&b, &a).await;
    assert!(
        store::records::get(&a.store.pool, &note)
            .await
            .unwrap()
            .is_none(),
        "tombstone landed on A"
    );

    // A crdt op arriving at B AFTER the delete: log-only, doc frozen.
    let before = sync_ops::for_field(&b.store.pool, "record", &note, "")
        .await
        .unwrap()
        .len();
    let crdt_op = sync_ops::for_field(&a.store.pool, "record", &note, "")
        .await
        .unwrap()
        .into_iter()
        .find(|op| op.kind == "crdt")
        .expect("A logged a crdt op");
    b.import_op_batch(&engine::sync::OpBatch {
        from_organ: a_organ.clone(),
        ops: vec![engine::sync::WireOp {
            tbl: "record".into(),
            uid: note.clone(),
            field: "".into(),
            kind: "crdt".into(),
            value: crdt_op.value.clone(),
            hlc: nucleus::hlc::next(),
            actor_organ: a_organ.clone(),
            fact: None,
        }],
    })
    .await
    .unwrap();
    let after = sync_ops::for_field(&b.store.pool, "record", &note, "")
        .await
        .unwrap()
        .len();
    assert_eq!(after, before + 1, "the op stays relayable in the log");
    assert!(
        store::records::get(&b.store.pool, &note)
            .await
            .unwrap()
            .is_none(),
        "the record stays deleted"
    );
}

#[tokio::test]
async fn compaction_preserves_text_across_reload() {
    let (e, _) = cell("http://cell-compact").await;
    let uid = plain(&e, "long", "long", "start").await;

    // Push past the 100-op compaction threshold.
    for i in 0..110 {
        edit(&e, &uid, None, Some(&format!("edit number {i}"))).await;
    }
    let doc_row = store::record_docs::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .expect("compaction stored a snapshot");
    assert!(doc_row.through_seq > 0);
    let expected = text_of(&e, &uid).await;

    // Reload from cold: evict the open doc, then edit again — the doc comes
    // back from snapshot + tail with identical text.
    e.close_record_doc(&uid);
    edit(&e, &uid, None, Some("after reload")).await;
    assert_eq!(text_of(&e, &uid).await.1, "after reload");

    // A fresh Engine on the same store sees the same materialized text.
    let e2 = Engine::new(e.store.clone()).await.unwrap();
    assert_eq!(text_of(&e2, &uid).await.0, expected.0);
}

#[tokio::test]
async fn first_collab_write_preserves_preexisting_text() {
    let (e, _) = cell("http://cell-seed").await;
    // Created with materialized text but NO crdt history (create logs set ops).
    let uid = plain(&e, "old-era", "Old Title", "old body text").await;

    // First collab write touches only the head; the body must survive the
    // seeding of the doc from materialized columns.
    edit(&e, &uid, Some("New Title"), None).await;
    let (head, body) = text_of(&e, &uid).await;
    assert_eq!(head, "New Title");
    assert_eq!(body, "old body text", "seeding preserved the body");
}

/// NO `crdt` op is ever pruned, even one a snapshot has already absorbed and
/// even when the whole log sits below the retention floor.
///
/// Each crdt op is the cumulative tail since the last stored SNAPSHOT, and
/// that snapshot lives in `record_doc` — local state, not in the log. A peer
/// replaying from zero holds no snapshot, so dropping any crdt op would lose
/// the text written before it with no way to recover: the materialized columns
/// would survive on the sender, and the new replica would simply never see
/// them. Pruning them safely requires serving `record_doc.snapshot` as part of
/// a bootstrap, which needs a synthesized op identity — and `(actor_organ,
/// hlc)` is the unique index import dedupes on, so that is not free.
#[tokio::test]
async fn no_crdt_op_is_ever_pruned() {
    let (e, _) = cell("http://cell-prune-crdt").await;
    let uid = plain(&e, "doc", "doc", "start").await;
    edit(&e, &uid, None, Some("some live text")).await;

    store::organs::add_contact(&e.store.pool, "organ-p", None, "P", "http://p", 1)
        .await
        .expect("contact");
    store::organs::set_sync_policy(&e.store.pool, "organ-p", true, true)
        .await
        .expect("policy");

    let head = store::sync_ops::max_seq(&e.store.pool).await.expect("max");
    // Clear the outbox so the OUTBOX guard cannot be what saves these ops —
    // this test is about the compaction guard specifically.
    store::sync_ops::outbox_clear_contact(&e.store.pool, "organ-p")
        .await
        .expect("clear");
    store::organs::advance_peer_acked_seq(&e.store.pool, "organ-p", head)
        .await
        .expect("advance");

    let before: Vec<i64> = store::sync_ops::after(&e.store.pool, 0, 10_000)
        .await
        .expect("ops")
        .into_iter()
        .filter(|o| o.kind == "crdt")
        .map(|o| o.seq)
        .collect();
    assert!(!before.is_empty(), "the text edit produced crdt ops");

    e.prune_op_log(false).await.expect("prune");

    let after: Vec<i64> = store::sync_ops::after(&e.store.pool, 0, 10_000)
        .await
        .expect("ops")
        .into_iter()
        .filter(|o| o.kind == "crdt")
        .map(|o| o.seq)
        .collect();
    assert_eq!(before, after, "every crdt op survived pruning");

    // And the text is still readable, which is the point of all of it.
    assert_eq!(text_of(&e, &uid).await.1, "some live text");
}
