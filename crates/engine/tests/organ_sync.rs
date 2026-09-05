use engine::Engine;
use engine::actions::{Action, ConceptSeed};
use engine::sync::Delivery;
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

async fn wire_push(from: &Engine, to: &Engine) -> usize {
    from.drain_outbox(|_contact, root, batch| async move {
        match root {
            Some(root) => to.import_grant_batch(&root, &batch).await,
            None => to.import_op_batch(&batch).await,
        }
        .map_or_else(|e| Delivery::Failed(e.to_string()), |_| Delivery::Sent)
    })
    .await
    .expect("drain")
}

async fn wire_push_to(from: &Engine, to: &Engine, to_organ: &str) -> usize {
    from.drain_outbox(|contact, root, batch| {
        let addressed = contact.record_uid == to_organ;
        async move {
            if !addressed {
                return Delivery::Failed("not this contact".into());
            }
            match root {
                Some(root) => to.import_grant_batch(&root, &batch).await,
                None => to.import_op_batch(&batch).await,
            }
            .map_or_else(|e| Delivery::Failed(e.to_string()), |_| Delivery::Sent)
        }
    })
    .await
    .expect("drain")
}

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

    let apple = store::concepts::create(&a.store.pool, "apple", &[])
        .await
        .unwrap();
    let apples = plain(&a, "apples.stock", 10.0).await;
    store::assertions::set_identity(&a.store.pool, &apples, Some(&apple), None)
        .await
        .unwrap();

    assert_eq!(wire_push(&a, &b).await, 1, "one batch for one contact");

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

    let (ops, _) = a.ops_after(0, 100_000).await.unwrap();
    let batch = OpBatch {
        from_organ: a_organ.clone(),
        ops,
    };
    b.import_op_batch(&batch).await.unwrap();
    let replay = b.import_op_batch(&batch).await.unwrap();
    assert_eq!(replay, 0, "duplicate op identity is a no-op");
    assert_eq!(
        store::records::quantity(&b.store.pool, &apples)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(10.0)
    );

    let ana = person(&a, "ana").await;
    store::visibility::grant(&a.store.pool, "organ", Some(&b_organ), &apples)
        .await
        .unwrap();
    store::misc::insert_promise(
        &a.store.pool,
        store::misc::NewPromise {
            record_uid: Some(apples.clone()),
            delta: 5.0,
            party_uid: Some(ana),
            state: Some(PromiseState::Open),
            ..Default::default()
        },
    )
    .await
    .unwrap();

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

    let fetched = a.open_promise_export(&b_organ).await.unwrap();
    assert_eq!(fetched.len(), 1, "only what visibility allows travels");
    b.refresh_discovery(&a_organ, fetched).await.unwrap();

    let drafts = b.senses_pass().await.unwrap();
    assert_eq!(drafts.len(), 1, "the offer meets the Need in the queue");

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
    tampered.fact.as_mut().unwrap().delta = store::exact::from_f64(500.0);

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

    let stale = engine::sync::WireOp {
        tbl: "record".into(),
        uid: note.clone(),
        field: "head".into(),
        kind: "set".into(),
        value: Some("\"stale\"".into()),
        hlc: 1,
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
        hlc: 2,
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
    wire_push(&a, &b).await;
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

#[tokio::test]
async fn imported_ops_are_not_pushed_onward() {
    let (a, a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
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

    let checkpoint = store::organs::contact(&b.store.pool, &a_organ)
        .await
        .unwrap()
        .unwrap()
        .last_synced_seq;
    let (ops, head) = a.ops_after(checkpoint, 100_000).await.unwrap();
    assert!(ops.is_empty(), "converged means an empty answer");
    assert_eq!(head, checkpoint);
}

#[tokio::test]
async fn a_narrowed_contact_is_narrowed_on_the_push_path_too() {
    let (a, _a_organ) = cell("http://cell-a").await;
    let (b, b_organ) = cell("http://cell-b").await;
    pair_push(&a, &b, &b_organ).await;

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
    assert_eq!(
        store::records::quantity(&b.store.pool, &note)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(7.0),
        "the one column the scope names travels"
    );

    assert!(
        sync_ops::outbox_due(&a.store.pool)
            .await
            .unwrap()
            .iter()
            .all(|row| row.contact_organ != b_organ),
        "nothing stays queued for a contact that can never receive it"
    );
}

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
    a.append_user(&counted, 42.0).await.expect("bump");
    wire_push(&a, &b).await;

    assert!(
        store::records::get(&b.store.pool, &counted)
            .await
            .unwrap()
            .is_none(),
        "the record never arrives"
    );
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
    assert_eq!(
        store::records::quantity(&b.store.pool, &held)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(12.0),
        "the fact chain replays exactly once"
    );
}

#[tokio::test]
async fn widening_a_scope_replays_the_columns_it_adds() {
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
    assert_eq!(
        landed.head, "the headline",
        "the newly-named column arrives"
    );
    assert_eq!(landed.body, "the body");
    assert_eq!(
        store::records::quantity(&b.store.pool, &note)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(3.0),
        "a re-sent fact is deduped by op identity, not added again"
    );
}

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
    assert!(
        sync_ops::outbox_due(&a.store.pool)
            .await
            .unwrap()
            .iter()
            .all(|row| !(row.contact_organ == b_organ && row.uid == note)),
        "a narrowing queues nothing about the records it narrows"
    );
}

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
