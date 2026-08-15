//! Part XV acceptance over the op-batch wire (Ontology §11): introduction,
//! reactive deltas through the bounded outbox, catch-up by checkpoint,
//! hardened fact import, per-field LWW convergence, tombstones that cannot
//! resurrect, and the discovery feed closing the loop into the Decision Queue.

use engine::Engine;
use engine::actions::{Action, ConceptSeed};
use engine::sync::OpBatch;
use engine::trust::Signer;
use nucleus::{PromiseState, RecordKind};
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

async fn plain(e: &Engine, slug: &str, quantity: f64) -> String {
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
    .pipe_bump(e, quantity)
    .await
}

async fn person(e: &Engine, slug: &str) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Person,
            head: slug,
            body: "",
            quantity: store::exact::one(),
        },
    )
    .await
    .expect("person")
    .uid
}

trait PipeBump {
    async fn pipe_bump(self, e: &Engine, quantity: f64) -> String;
}
impl PipeBump for String {
    async fn pipe_bump(self, e: &Engine, quantity: f64) -> String {
        if quantity != 0.0 {
            e.append_user(&self, quantity).await.expect("bump");
        }
        self
    }
}

/// The reactive wire: drain `from`'s bounded outbox into `to` — what the HTTP
/// boundary does in production. Returns batches delivered.
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

/// The same wire, but ADDRESSED: only batches destined for `to_organ` are
/// delivered, and a batch for anyone else is dropped as if that contact were
/// offline. `wire_push` above ignores the contact, which is fine while a Cell
/// has one contact and silently wrong the moment it has two — it hands every
/// contact's batch to the same receiver. Any test about per-contact policy
/// must use this one or it proves nothing.
async fn wire_push_to(from: &Engine, to: &Engine, to_organ: &str) -> usize {
    from.drain_outbox(|contact, root, batch| {
        let addressed = contact.record_uid == to_organ;
        async move {
            if !addressed {
                return Err("not this contact".to_string());
            }
            match root {
                Some(root) => to.import_grant_batch(&root, &batch).await,
                None => to.import_op_batch(&batch).await,
            }
            .map(|_| ())
            .map_err(|e| e.to_string())
        }
    })
    .await
    .expect("drain")
}

/// The catch-up wire: `to` pulls everything past its checkpoint for
/// `from_organ` and advances it — what the 30s cycle does in production.
async fn wire_catch_up(from: &Engine, to: &Engine, from_organ: &str) -> usize {
    let checkpoint = store::organs::contact(&to.store.pool, from_organ)
        .await
        .expect("contact query")
        .map(|c| c.last_synced_seq)
        .unwrap_or(0);
    let (ops, head) = from.ops_after(checkpoint, 100_000).await.expect("serve");
    let applied = to
        .import_op_batch(&OpBatch {
            from_organ: from_organ.to_string(),
            ops,
        })
        .await
        .expect("import");
    store::organs::set_last_synced_seq(&to.store.pool, from_organ, head)
        .await
        .expect("checkpoint");
    applied
}

/// Introduce two cells to each other and let `a` push to `b`.
async fn pair_push(a: &Engine, b: &Engine, b_organ: &str) {
    let a_intro = a.introduction().await.unwrap();
    let b_intro = b.introduction().await.unwrap();
    b.adopt_introduction(&a_intro, 1).await.unwrap();
    a.adopt_introduction(&b_intro, 1).await.unwrap();
    store::organs::set_sync_policy(&a.store.pool, b_organ, true, false)
        .await
        .unwrap();
}

#[tokio::test]
async fn donation_flows_between_two_cells_and_feeds_the_decision_queue() {
    let (a, a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    pair_push(&a, &b, &b_organ).await;

    // Ana's Cell: 10 apples, tagged @apple.
    let apple = store::concepts::create(&a.store.pool, "apple", &[])
        .await
        .unwrap();
    let apples = plain(&a, "apples.stock", 10.0).await;
    store::assertions::set_identity(&a.store.pool, &apples, Some(&apple), None)
        .await
        .unwrap();

    // DONATION over the wire: the bounded outbox drains through the boundary.
    assert_eq!(wire_push(&a, &b).await, 1, "one batch for one contact");

    // B has the record by the SAME uid, deltas applied, signatures verified.
    let imported = store::records::get(&b.store.pool, &apples).await.unwrap();
    assert!(imported.is_some(), "identity replicates by uid");
    assert_eq!(
        store::records::quantity(&b.store.pool, &apples)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(10.0)
    );
    assert_eq!(
        store::organs::quarantine_count(&b.store.pool)
            .await
            .unwrap(),
        0,
        "adopted keys verify the origin signatures"
    );

    // Idempotent: importing the same ops twice — the second pass is a no-op.
    let (ops, _) = a.ops_after(0, 100_000).await.unwrap();
    let batch = OpBatch {
        from_organ: a_organ.clone(),
        ops,
    };
    b.import_op_batch(&batch).await.unwrap(); // catches pre-pairing ops
    let replay = b.import_op_batch(&batch).await.unwrap();
    assert_eq!(replay, 0, "duplicate op identity is a no-op");
    assert_eq!(
        store::records::quantity(&b.store.pool, &apples)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(10.0)
    );

    // ---- the discovery loop: A's open offer meets B's Need as a decision
    let ana = person(&a, "ana").await;
    store::visibility::grant(&a.store.pool, "organ", Some(&b_organ), &apples)
        .await
        .unwrap();
    store::misc::insert_promise(
        &a.store.pool,
        store::misc::NewPromise {
            record_uid: Some(apples.clone()),
            delta: 5.0, // an open Contribution
            party_uid: Some(ana),
            state: Some(PromiseState::Open),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // B adopts A's concept (uid + lineage) and has a complementary Need.
    b.act(
        Action::AdoptConcepts {
            concepts: vec![ConceptSeed {
                uid: apple.clone(),
                name: "apple".into(),
                origin: Some(a_organ.clone()),
                parents: vec![],
            }],
        },
        None,
    )
    .await
    .unwrap();
    let my_apples = plain(&b, "my.apples", -3.0).await;
    let bia = person(&b, "bia").await;
    store::assertions::set_identity(&b.store.pool, &my_apples, Some(&apple), None)
        .await
        .unwrap();
    store::misc::insert_promise(
        &b.store.pool,
        store::misc::NewPromise {
            record_uid: Some(my_apples),
            delta: -3.0,
            party_uid: Some(bia),
            state: Some(PromiseState::Open),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    b.act(
        Action::CreateMatchRule {
            slug: "senses.nearby".into(),
            head: "Nearby".into(),
            watch_concept: None,
            max_proximity: 1,
            min_confidence: 0.0,
            auto: "draft_only".into(),
        },
        None,
    )
    .await
    .unwrap();

    // B pulls A's visible open promises into its discovery cache (the wire's
    // pull side; proximity stamped from B's own contact row).
    let fetched = a.open_promise_export(&b_organ).await.unwrap();
    assert_eq!(fetched.len(), 1, "only what visibility allows travels");
    b.refresh_discovery(&a_organ, fetched).await.unwrap();

    let drafts = b.senses_pass().await.unwrap();
    assert_eq!(drafts.len(), 1, "the offer meets the Need in the queue");

    // ---- blocked rejects everything everywhere
    store::organs::set_trust(&b.store.pool, &a_organ, "blocked")
        .await
        .unwrap();
    let (ops, _) = a.ops_after(0, 10).await.unwrap();
    assert!(
        b.import_op_batch(&OpBatch {
            from_organ: a_organ.clone(),
            ops
        })
        .await
        .is_err()
    );
    assert!(b.refresh_discovery(&a_organ, vec![]).await.is_err());
}

#[tokio::test]
async fn tampered_facts_are_quarantined_on_import() {
    let (a, a_organ) = cell("http://cell-a").await;
    let (b, _b_organ) = cell("http://cell-b").await;
    b.adopt_introduction(&a.introduction().await.unwrap(), 1)
        .await
        .unwrap();

    let apples = plain(&a, "apples.stock", 10.0).await;
    let (mut ops, _) = a.ops_after(0, 100_000).await.unwrap();
    let tampered = ops
        .iter_mut()
        .find(|op| op.fact.as_ref().is_some_and(|f| !f.delta.is_zero()))
        .expect("a quantity fact travels");
    tampered.fact.as_mut().unwrap().delta = store::exact::from_f64(500.0); // the tamper

    b.import_op_batch(&OpBatch {
        from_organ: a_organ.clone(),
        ops,
    })
    .await
    .unwrap();
    assert!(
        store::organs::quarantine_count(&b.store.pool)
            .await
            .unwrap()
            >= 1,
        "the tampered fact is remembered in the quarantine list"
    );
    assert_eq!(
        store::records::quantity(&b.store.pool, &apples)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(0.0),
        "…and never lands in the fold"
    );
}

/// E0.0: a Fact's declared precision must survive the sync wire, not just its
/// value — the preimage carries the scale, so any f64 round-trip would land
/// the chain in quarantine.
#[tokio::test]
async fn declared_precision_survives_the_sync_wire() {
    let (a, _a_organ) = cell("http://cell-precise-a").await;
    let (b, b_organ) = cell("http://cell-precise-b").await;
    pair_push(&a, &b, &b_organ).await;

    let grams = plain(&a, "flour.grams", 0.0).await;
    let precise = nucleus::DecimalValue::parse_canonical(2, "1.50").unwrap();
    a.append(
        nucleus::NewFact::quantity(grams.clone(), precise, nucleus::Cause::user_edit()),
        chrono::Utc::now(),
    )
    .await
    .expect("append exact fact");

    assert_eq!(wire_push(&a, &b).await, 1);

    assert_eq!(
        store::organs::quarantine_count(&b.store.pool)
            .await
            .unwrap(),
        0,
        "an exact delta's scale survives the wire, so signatures still verify"
    );
    let landed = store::records::quantity(&b.store.pool, &grams)
        .await
        .unwrap()
        .expect("record replicated");
    assert_eq!(landed.canonical(), "1.50", "trailing zero is not dropped");
    assert_eq!(landed.scale(), 2);
}

#[tokio::test]
async fn different_field_edits_converge_both_ways() {
    let (a, a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    pair_push(&a, &b, &b_organ).await;
    store::organs::set_sync_policy(&b.store.pool, &a_organ, true, false)
        .await
        .unwrap();

    let note = plain(&a, "shared.note", 0.0).await;
    assert!(wire_push(&a, &b).await >= 1);

    // Concurrent edits to DIFFERENT fields of the same record.
    a.act(
        Action::EditRecordText {
            target: note.clone(),
            head: Some("A's headline".into()),
            body: None,
        },
        None,
    )
    .await
    .unwrap();
    b.act(
        Action::EditRecordText {
            target: note.clone(),
            head: None,
            body: Some("B's body".into()),
        },
        None,
    )
    .await
    .unwrap();
    wire_push(&a, &b).await;
    wire_push(&b, &a).await;
    // Text now merges through the record-doc: settle the echo round so both
    // cells hold the merged doc.
    wire_push(&a, &b).await;

    for e in [&a, &b] {
        let r = store::records::get(&e.store.pool, &note)
            .await
            .unwrap()
            .expect("record");
        assert_eq!(r.head, "A's headline", "head survives on both");
        assert_eq!(r.body, "B's body", "body survives on both");
    }
}

#[tokio::test]
async fn older_ops_lose_lww_and_tombstones_cannot_resurrect() {
    let (a, a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    pair_push(&a, &b, &b_organ).await;

    let note = plain(&a, "the.note", 0.0).await;
    a.act(
        Action::EditRecordText {
            target: note.clone(),
            head: Some("current".into()),
            body: None,
        },
        None,
    )
    .await
    .unwrap();
    wire_push(&a, &b).await;

    // (b) an op with an OLDER HLC than the stored field is logged but ignored.
    let stale = engine::sync::WireOp {
        tbl: "record".into(),
        uid: note.clone(),
        field: "head".into(),
        kind: "set".into(),
        value: Some("\"stale\"".into()),
        hlc: 1, // ancient
        actor_cell: a_organ.clone(),
        organ_uid: a_organ.clone(),
        fact: None,
    };
    b.import_op_batch(&OpBatch {
        from_organ: a_organ.clone(),
        ops: vec![stale],
    })
    .await
    .unwrap();
    assert_eq!(
        store::records::get(&b.store.pool, &note)
            .await
            .unwrap()
            .unwrap()
            .head,
        "current",
        "older set loses LWW"
    );

    // (c) delete, then a LATE older set arrives — the record stays deleted.
    a.act(
        Action::DeleteRecord {
            target: note.clone(),
        },
        None,
    )
    .await
    .unwrap();
    wire_push(&a, &b).await;
    assert!(
        store::records::get(&b.store.pool, &note)
            .await
            .unwrap()
            .is_none(),
        "tombstone replicated"
    );
    let late = engine::sync::WireOp {
        tbl: "record".into(),
        uid: note.clone(),
        field: "head".into(),
        kind: "set".into(),
        value: Some("\"zombie\"".into()),
        hlc: 2, // older than the tombstone
        actor_cell: a_organ.clone(),
        organ_uid: a_organ.clone(),
        fact: None,
    };
    b.import_op_batch(&OpBatch {
        from_organ: a_organ.clone(),
        ops: vec![late],
    })
    .await
    .unwrap();
    assert!(
        store::records::get(&b.store.pool, &note)
            .await
            .unwrap()
            .is_none(),
        "a late older set cannot resurrect a deleted record"
    );

    // A set NEWER than the tombstone undeletes — undelete is a newer write.
    // The record was collab-edited, so the record-doc owns its text: the op's
    // undelete power applies, its text payload is log-only.
    let revive = engine::sync::WireOp {
        tbl: "record".into(),
        uid: note.clone(),
        field: "head".into(),
        kind: "set".into(),
        value: Some("\"reborn\"".into()),
        hlc: nucleus::hlc::next(),
        actor_cell: a_organ.clone(),
        organ_uid: a_organ.clone(),
        fact: None,
    };
    b.import_op_batch(&OpBatch {
        from_organ: a_organ.clone(),
        ops: vec![revive],
    })
    .await
    .unwrap();
    let revived = store::records::get(&b.store.pool, &note).await.unwrap();
    assert_eq!(
        revived.map(|r| r.head).as_deref(),
        Some("current"),
        "a newer set undeletes; doc-owned text stays"
    );
}

#[tokio::test]
async fn outbox_is_bounded_per_contact_and_field() {
    let (a, _a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    pair_push(&a, &b, &b_organ).await;

    let note = plain(&a, "busy.note", 0.0).await;
    wire_push(&a, &b).await; // flush creation ops
    for i in 0..50 {
        a.act(
            Action::EditRecordText {
                target: note.clone(),
                head: Some(format!("draft {i}")),
                body: None,
            },
            None,
        )
        .await
        .unwrap();
    }
    // Text edits are cumulative crdt ops keyed (record, uid, "") — the burst
    // coalesces to ONE queued op whose tail carries every edit.
    let queued: Vec<_> = sync_ops::outbox_due(&a.store.pool)
        .await
        .unwrap()
        .into_iter()
        .filter(|row| row.uid == note && row.tbl == "record" && row.field.is_empty())
        .collect();
    assert_eq!(
        queued.len(),
        1,
        "a burst of typing queues ONE op, not fifty"
    );
    wire_push(&a, &b).await;
    assert_eq!(
        store::records::get(&b.store.pool, &note)
            .await
            .unwrap()
            .unwrap()
            .head,
        "draft 49",
        "…and it is the latest value"
    );
}

/// Relaying is OFF (Ontology §11 "Op authenticity"), and this test used to
/// assert the opposite.
///
/// An imported op is stored, so it still rides a catch-up feed to a peer that
/// asks us for our log — but it is never pushed onward. That is what makes
/// `op.organ_uid == batch.from_organ` free: with relay on, the Organ on the
/// other end is a carrier rather than the author, and the receiver cannot tell
/// a forged attribution from a relayed one without signatures that do not
/// exist yet.
#[tokio::test]
async fn imported_ops_are_not_pushed_onward() {
    let (a, a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    // B pushes to A (so A's outbox has a contact to enqueue for), and B also
    // has a second contact C to relay to.
    pair_push(&a, &b, &b_organ).await;
    store::organs::set_sync_policy(&b.store.pool, &a_organ, true, false)
        .await
        .unwrap();
    store::organs::add_contact(&b.store.pool, "organ-c", None, "C", "http://cell-c", 1)
        .await
        .unwrap();
    store::organs::set_sync_policy(&b.store.pool, "organ-c", true, false)
        .await
        .unwrap();

    let note = plain(&a, "travel.note", 0.0).await;
    wire_push(&a, &b).await;

    let queued = sync_ops::outbox_due(&b.store.pool).await.unwrap();
    let for_a: Vec<_> = queued
        .iter()
        .filter(|row| row.contact_organ == a_organ && row.uid == note)
        .collect();
    let for_c: Vec<_> = queued
        .iter()
        .filter(|row| row.contact_organ == "organ-c" && row.uid == note)
        .collect();
    assert!(for_a.is_empty(), "no echo back to the source");
    assert!(
        for_c.is_empty(),
        "…and no push onward to other contacts either"
    );
    // Still in the log, so a peer asking B for its feed receives it.
    let stored = sync_ops::for_field(&b.store.pool, "record", &note, "head")
        .await
        .unwrap();
    assert!(!stored.is_empty(), "the op is kept, just not forwarded");
}

#[tokio::test]
async fn extension_keys_merge_across_cells() {
    let (a, a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    pair_push(&a, &b, &b_organ).await;
    store::organs::set_sync_policy(&b.store.pool, &a_organ, true, false)
        .await
        .unwrap();

    let task = plain(&a, "the.task", 0.0).await;
    store::records::set_extension(
        &a.store.pool,
        &task,
        "work.tracking",
        &serde_json::json!({ "estimate": 3 }),
    )
    .await
    .unwrap();
    wire_push(&a, &b).await;

    // A bumps the estimate on a laptop; B logs an owner on a phone.
    store::records::set_extension(
        &a.store.pool,
        &task,
        "work.tracking",
        &serde_json::json!({ "estimate": 5 }),
    )
    .await
    .unwrap();
    store::records::set_extension(
        &b.store.pool,
        &task,
        "work.tracking",
        &serde_json::json!({ "estimate": 3, "owner": "bia" }),
    )
    .await
    .unwrap();
    wire_push(&a, &b).await;
    wire_push(&b, &a).await;

    for e in [&a, &b] {
        let fds = store::records::get_extension(&e.store.pool, &task, "work.tracking")
            .await
            .unwrap()
            .expect("namespace");
        assert_eq!(fds["estimate"], 5, "A's estimate survives");
        assert_eq!(fds["owner"], "bia", "B's owner survives");
    }
}

#[tokio::test]
async fn catch_up_finds_ops_written_before_the_pairing() {
    let (a, a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;

    // The record exists BEFORE the contacts pair: the reactive path never saw
    // it, so only catch-up can deliver it.
    let old = plain(&a, "old.note", 0.0).await;
    pair_push(&a, &b, &b_organ).await;
    assert!(
        store::records::get(&b.store.pool, &old)
            .await
            .unwrap()
            .is_none()
    );

    let applied = wire_catch_up(&a, &b, &a_organ).await;
    assert!(applied > 0, "checkpoint pull applies the missed ops");
    assert!(
        store::records::get(&b.store.pool, &old)
            .await
            .unwrap()
            .is_some(),
        "the pre-pairing record lands"
    );

    // Converged: the next cycle is one empty answer.
    let checkpoint = store::organs::contact(&b.store.pool, &a_organ)
        .await
        .unwrap()
        .unwrap()
        .last_synced_seq;
    let (ops, head) = a.ops_after(checkpoint, 100_000).await.unwrap();
    assert!(ops.is_empty(), "converged means an empty answer");
    assert_eq!(head, checkpoint);
}

/// The per-contact scope must hold on the PUSH path, not only on the pull one.
///
/// It was built into `FetchOpsSince` first, which is the path a peer takes
/// when it asks us — while `drain_outbox`, the path the sync runner actually
/// drives, sent every column regardless. A narrowing that covers one of two
/// delivery paths is not a narrowing, so this test drives the outbox rather
/// than the fetch.
#[tokio::test]
async fn a_narrowed_contact_is_narrowed_on_the_push_path_too() {
    let (a, _a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    pair_push(&a, &b, &b_organ).await;

    // B may see how much of a thing there is, and not what it is called.
    store::organs::set_contact_scope(&a.store.pool, &b_organ, Some(&["quantity".to_string()]))
        .await
        .unwrap();

    let note = plain(&a, "counted.thing", 7.0).await;
    a.act(
        Action::EditRecordText {
            target: note.clone(),
            head: Some("the secret headline".into()),
            body: Some("and its body".into()),
        },
        None,
    )
    .await
    .unwrap();
    wire_push(&a, &b).await;
    wire_push(&a, &b).await;

    let landed = store::records::get(&b.store.pool, &note)
        .await
        .unwrap()
        .expect("the record itself still arrives — narrowing hides columns, not rows");
    assert_eq!(landed.head, "", "the headline was outside the scope");
    assert_eq!(landed.body, "", "the body rode with it and is outside too");
    // And the named column DID arrive — without this the test would pass just
    // as well if the scope had dropped everything.
    assert_eq!(
        store::records::quantity(&b.store.pool, &note)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(7.0),
        "the one column the scope names travels"
    );

    // And the withheld ops are not left queued forever: an op this contact
    // will never be sent is not owed, so nothing holds the outbox open.
    assert!(
        sync_ops::outbox_due(&a.store.pool)
            .await
            .unwrap()
            .iter()
            .all(|row| row.contact_organ != b_organ),
        "nothing stays queued for a contact that can never receive it"
    );
}

/// Per-record hiding: WHOLE rows kept out of one contact's feed, where the
/// scope keeps out columns. Both filters run on both delivery paths.
#[tokio::test]
async fn a_hidden_record_stays_out_of_that_contacts_feed_only() {
    let (a, _a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    let (c, c_organ) = cell("http://cell-c").await;
    pair_push(&a, &b, &b_organ).await;
    pair_push(&a, &c, &c_organ).await;

    let secret = plain(&a, "the.secret", 0.0).await;
    let ordinary = plain(&a, "the.ordinary", 0.0).await;
    a.act(
        Action::HideRecordFromContact {
            target: b_organ.clone(),
            record: "the.secret".into(),
            hidden: true,
        },
        None,
    )
    .await
    .expect("hide by slug, which is what a person knows it by");
    wire_push_to(&a, &b, &b_organ).await;
    wire_push_to(&a, &c, &c_organ).await;

    assert!(
        store::records::get(&b.store.pool, &secret)
            .await
            .unwrap()
            .is_none(),
        "hidden from B"
    );
    assert!(
        store::records::get(&b.store.pool, &ordinary)
            .await
            .unwrap()
            .is_some(),
        "and only that one — B's feed is otherwise untouched"
    );
    assert!(
        store::records::get(&c.store.pool, &secret)
            .await
            .unwrap()
            .is_some(),
        "per CONTACT: C was never told to hide anything"
    );

    // Unhiding is the same act back, and the feed reopens.
    a.act(
        Action::HideRecordFromContact {
            target: b_organ.clone(),
            record: secret.clone(),
            hidden: false,
        },
        None,
    )
    .await
    .unwrap();
    a.act(
        Action::EditRecordText {
            target: secret.clone(),
            head: Some("now shared".into()),
            body: None,
        },
        None,
    )
    .await
    .unwrap();
    wire_push(&a, &b).await;
    wire_push(&a, &b).await;
    assert!(
        store::records::get(&b.store.pool, &secret)
            .await
            .unwrap()
            .is_some(),
        "unhiding lets what comes NEXT through"
    );
}

/// A hidden Record's facts and links must not ride either. An op that names a
/// different table still belongs to a Record, and only that mapping decides
/// whose policy governs it.
#[tokio::test]
async fn hiding_a_record_hides_the_facts_hanging_off_it() {
    let (a, a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    pair_push(&a, &b, &b_organ).await;

    let counted = plain(&a, "counted.secret", 0.0).await;
    a.act(
        Action::HideRecordFromContact {
            target: b_organ.clone(),
            record: counted.clone(),
            hidden: true,
        },
        None,
    )
    .await
    .unwrap();
    // A quantity fact is its own op on its own table.
    a.append_user(&counted, 42.0).await.expect("bump");
    wire_push(&a, &b).await;

    assert!(
        store::records::get(&b.store.pool, &counted)
            .await
            .unwrap()
            .is_none(),
        "the record never arrives"
    );
    // Nothing about it reached B, so B cannot have a quantity for a record it
    // does not hold — the check that would fail if fact ops rode alone.
    assert_eq!(
        store::records::quantity(&b.store.pool, &counted)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        None,
        "and neither does the fact hanging off it"
    );
    let _ = a_organ;
}

/// Hiding refuses a Record that does not exist rather than storing a rule that
/// hides nothing while reading as applied.
#[tokio::test]
async fn hiding_refuses_a_record_and_a_contact_it_cannot_find() {
    let (a, a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    pair_push(&a, &b, &b_organ).await;

    let missing = a
        .act(
            Action::HideRecordFromContact {
                target: b_organ.clone(),
                record: "no.such.record".into(),
                hidden: true,
            },
            None,
        )
        .await;
    assert!(missing.is_err(), "a record that is not there is refused");

    let plainly = plain(&a, "real.one", 0.0).await;
    let local = a
        .act(
            Action::HideRecordFromContact {
                target: a_organ.clone(),
                record: plainly,
                hidden: true,
            },
            None,
        )
        .await;
    assert!(
        local.is_err(),
        "our own Organ is not a contact and has no feed to hide from"
    );
}

/// Unhiding is a GRANT, and a grant that does not reach back grants nothing.
///
/// The ops this contact missed sit below their version vector, so ordinary
/// catch-up will never offer them again — without a replay the Record stays
/// permanently absent while the panel reads as shared. Note there is no edit
/// after the unhide: an earlier test proved the NEXT change arrives, which is
/// a much weaker claim and passes even with no replay at all.
#[tokio::test]
async fn unhiding_replays_the_record_the_contact_never_received() {
    let (a, _a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    pair_push(&a, &b, &b_organ).await;

    let held = plain(&a, "held.back", 12.0).await;
    a.act(
        Action::HideRecordFromContact {
            target: b_organ.clone(),
            record: held.clone(),
            hidden: true,
        },
        None,
    )
    .await
    .unwrap();
    a.act(
        Action::EditRecordText {
            target: held.clone(),
            head: Some("written while hidden".into()),
            body: None,
        },
        None,
    )
    .await
    .unwrap();
    wire_push_to(&a, &b, &b_organ).await;
    assert!(
        store::records::get(&b.store.pool, &held)
            .await
            .unwrap()
            .is_none(),
        "nothing reached B while it was hidden"
    );

    a.act(
        Action::HideRecordFromContact {
            target: b_organ.clone(),
            record: held.clone(),
            hidden: false,
        },
        None,
    )
    .await
    .unwrap();
    // Two passes: the record-doc settles its text on the echo round, exactly
    // as it does for an ordinary edit.
    wire_push_to(&a, &b, &b_organ).await;
    wire_push_to(&a, &b, &b_organ).await;

    let landed = store::records::get(&b.store.pool, &held)
        .await
        .unwrap()
        .expect("the whole record arrives, with no new edit to carry it");
    assert_eq!(
        landed.head, "written while hidden",
        "including what changed while they could not see it"
    );
    // The quantity is a fact chain, and facts are the reason a re-grant cannot
    // be a snapshot of current state: `quantity` is ADDED on apply, so a
    // synthesized 'current value' op would have made this 24.
    assert_eq!(
        store::records::quantity(&b.store.pool, &held)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(12.0),
        "the fact chain replays exactly once"
    );
}

/// Widening a scope has to reach back, for the same reason unhiding does: the
/// ops for a newly-named column are already below the contact's version
/// vector, so catch-up will never offer them again.
#[tokio::test]
async fn widening_a_scope_replays_the_columns_it_adds() {
    let (a, _a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    pair_push(&a, &b, &b_organ).await;

    // B may see the quantity and nothing else.
    a.act(
        Action::SetContactScope {
            target: b_organ.clone(),
            fields: Some(vec!["quantity".into()]),
        },
        None,
    )
    .await
    .unwrap();
    let note = plain(&a, "widened.note", 3.0).await;
    a.act(
        Action::EditRecordText {
            target: note.clone(),
            head: Some("the headline".into()),
            body: Some("the body".into()),
        },
        None,
    )
    .await
    .unwrap();
    wire_push_to(&a, &b, &b_organ).await;
    wire_push_to(&a, &b, &b_organ).await;
    assert_eq!(
        store::records::get(&b.store.pool, &note)
            .await
            .unwrap()
            .expect("the record itself arrives")
            .head,
        "",
        "the text was outside the scope"
    );

    // Widen. Nothing is edited afterwards — an edit would carry the text on
    // its own and the test would prove nothing about the repair.
    a.act(
        Action::SetContactScope {
            target: b_organ.clone(),
            fields: Some(vec!["quantity".into(), "head".into(), "body".into()]),
        },
        None,
    )
    .await
    .unwrap();
    wire_push_to(&a, &b, &b_organ).await;
    wire_push_to(&a, &b, &b_organ).await;

    let landed = store::records::get(&b.store.pool, &note)
        .await
        .unwrap()
        .expect("record");
    assert_eq!(landed.head, "the headline", "the newly-named column arrives");
    assert_eq!(landed.body, "the body");
    // The replay walks the whole feed, so the column that was ALREADY shared
    // rides again — and must not be counted twice.
    assert_eq!(
        store::records::quantity(&b.store.pool, &note)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(3.0),
        "a re-sent fact is deduped by op identity, not added again"
    );
}

/// Narrowing needs no repair and must not trigger one: it stops sending, it
/// does not reach back and retract.
#[tokio::test]
async fn narrowing_a_scope_queues_nothing() {
    let (a, _a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    pair_push(&a, &b, &b_organ).await;

    let note = plain(&a, "settled.note", 1.0).await;
    wire_push_to(&a, &b, &b_organ).await;
    wire_push_to(&a, &b, &b_organ).await;

    a.act(
        Action::SetContactScope {
            target: b_organ.clone(),
            fields: Some(vec!["quantity".into()]),
        },
        None,
    )
    .await
    .unwrap();
    // Checked against the RECORD rather than against the whole outbox: every
    // contact action annotates the contact's own Record, which is an ordinary
    // logged write and queues an op of its own. An assertion that the outbox
    // is empty would be failing on that, not on a replay.
    assert!(
        sync_ops::outbox_due(&a.store.pool)
            .await
            .unwrap()
            .iter()
            .all(|row| !(row.contact_organ == b_organ && row.uid == note)),
        "a narrowing queues nothing about the records it narrows"
    );
}

/// A widening queues the DIFFERENCE, not the feed. The repair asks the same
/// predicate the drain will ask, twice — once with the old scope and once with
/// the new — so what gets repaired and what gets served cannot disagree.
#[tokio::test]
async fn widening_queues_only_what_it_newly_permits() {
    let (a, _a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    pair_push(&a, &b, &b_organ).await;

    a.act(
        Action::SetContactScope {
            target: b_organ.clone(),
            fields: Some(vec!["quantity".into()]),
        },
        None,
    )
    .await
    .unwrap();
    let note = plain(&a, "diffed.note", 5.0).await;
    a.act(
        Action::EditRecordText {
            target: note.clone(),
            head: Some("text".into()),
            body: None,
        },
        None,
    )
    .await
    .unwrap();
    wire_push_to(&a, &b, &b_organ).await;
    wire_push_to(&a, &b, &b_organ).await;

    a.act(
        Action::SetContactScope {
            target: b_organ.clone(),
            fields: Some(vec!["quantity".into(), "head".into(), "body".into()]),
        },
        None,
    )
    .await
    .unwrap();

    let queued = sync_ops::outbox_due(&a.store.pool)
        .await
        .unwrap()
        .into_iter()
        .filter(|row| row.contact_organ == b_organ && row.uid == note)
        .collect::<Vec<_>>();
    assert!(
        queued
            .iter()
            .any(|row| matches!(row.kind.as_str(), "crdt" | "snapshot")),
        "the collaborative document is queued: {queued:?}"
    );
    // Everything queued is TEXT: the creation-time `head`/`body` sets and the
    // record-doc ops. Nothing about quantity, which was already in scope — a
    // widening owes only what it newly permits, and re-sending the rest is
    // the O(current state) churn the diff exists to avoid.
    assert!(
        queued.iter().all(|row| row.tbl == "record"
            && (row.field == "head" || row.field == "body" || row.field.is_empty())),
        "and nothing the contact could already receive: {queued:?}"
    );
    assert!(
        queued.iter().all(|row| row.tbl != "fact"),
        "least of all the facts, which were in scope the whole time: {queued:?}"
    );
}

/// A fact logs under an empty field, and an empty field is not a wildcard. A
/// contact scoped to the text must not receive our quantity changes.
#[tokio::test]
async fn a_scope_without_quantity_does_not_receive_the_facts() {
    let (a, _a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    pair_push(&a, &b, &b_organ).await;

    store::organs::set_contact_scope(
        &a.store.pool,
        &b_organ,
        Some(&["head".to_string(), "body".to_string()]),
    )
    .await
    .unwrap();

    let note = plain(&a, "textual.note", 99.0).await;
    a.act(
        Action::EditRecordText {
            target: note.clone(),
            head: Some("they may read this".into()),
            body: None,
        },
        None,
    )
    .await
    .unwrap();
    wire_push_to(&a, &b, &b_organ).await;
    wire_push_to(&a, &b, &b_organ).await;

    assert_eq!(
        store::records::get(&b.store.pool, &note)
            .await
            .unwrap()
            .expect("the record arrives")
            .head,
        "they may read this",
        "the scope works, or this test proves nothing"
    );
    assert_eq!(
        store::records::quantity(&b.store.pool, &note)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(0.0),
        "and the quantity they were never given stays at nothing"
    );
}
