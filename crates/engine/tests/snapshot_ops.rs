use engine::Engine;
use engine::sync::OpBatch;
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

async fn ops_of_kind(e: &Engine, uid: &str, kind: &str) -> Vec<sync_ops::OpRow> {
    sync_ops::for_field(&e.store.pool, "record", uid, "")
        .await
        .expect("ops")
        .into_iter()
        .filter(|op| op.kind == kind)
        .collect()
}

#[tokio::test]
async fn compaction_logs_a_snapshot_op_owned_by_this_cell() {
    let (e, organ) = cell("http://cell-a").await;
    let this_cell = store::cells::local(&e.store.pool)
        .await
        .expect("cell")
        .expect("cell record")
        .uid;
    let uid = plain(&e, "notes").await;
    e.write_record_text(&uid, None, Some("some collaborative text"))
        .await
        .expect("write");

    assert!(e.compact_doc(&uid).await.expect("compact"));

    let snapshots = ops_of_kind(&e, &uid, "snapshot").await;
    assert_eq!(snapshots.len(), 1, "one snapshot op");
    assert_eq!(snapshots[0].actor_cell, this_cell);
    assert_eq!(snapshots[0].organ_uid, organ);
    assert!(snapshots[0].value.is_some(), "the blob travels on the op");
}

#[tokio::test]
async fn a_snapshot_op_is_never_queued_to_the_outbox() {
    let (e, _organ) = cell("http://cell-a").await;
    store::organs::add_contact(&e.store.pool, "organ-b", None, "B", "http://cell-b", 1)
        .await
        .expect("contact");
    store::organs::set_sync_policy(&e.store.pool, "organ-b", true, false)
        .await
        .expect("policy");
    let uid = plain(&e, "notes").await;
    e.write_record_text(&uid, None, Some("text"))
        .await
        .expect("write");
    e.compact_doc(&uid).await.expect("compact");

    let queued = sync_ops::outbox_due(&e.store.pool).await.expect("outbox");
    assert!(
        !queued.iter().any(|row| row.kind == "snapshot"),
        "no snapshot in the queue: {queued:?}"
    );
    assert_eq!(ops_of_kind(&e, &uid, "snapshot").await.len(), 1);
}

#[tokio::test]
async fn crdt_ops_below_a_snapshot_are_prunable_and_the_text_survives() {
    let (e, _organ) = cell("http://cell-a").await;
    store::organs::add_contact(&e.store.pool, "organ-b", None, "B", "http://cell-b", 1)
        .await
        .expect("contact");
    store::organs::set_sync_policy(&e.store.pool, "organ-b", true, false)
        .await
        .expect("policy");

    let uid = plain(&e, "notes").await;
    for line in ["one", "one two", "one two three"] {
        e.write_record_text(&uid, None, Some(line))
            .await
            .expect("write");
    }
    assert_eq!(ops_of_kind(&e, &uid, "crdt").await.len(), 3);
    e.compact_doc(&uid).await.expect("compact");

    let head = sync_ops::max_seq(&e.store.pool).await.expect("max seq");
    store::organs::advance_peer_acked_seq(&e.store.pool, "organ-b", head)
        .await
        .expect("ack");
    sync_ops::outbox_clear_contact(&e.store.pool, "organ-b")
        .await
        .expect("clear");

    let report = e.prune_op_log(false).await.expect("prune");
    assert!(report.removed > 0, "something was pruned: {report:?}");
    assert!(
        ops_of_kind(&e, &uid, "crdt").await.is_empty(),
        "the folded tail is gone"
    );
    assert_eq!(
        ops_of_kind(&e, &uid, "snapshot").await.len(),
        1,
        "the snapshot that folded them is NOT pruned"
    );

    e.close_record_doc(&uid);
    let row = store::records::get(&e.store.pool, &uid)
        .await
        .expect("get")
        .expect("record");
    assert_eq!(row.body, "one two three");
    assert_eq!(
        e.doc_text(&uid).await.expect("text").1,
        "one two three",
        "the doc rebuilds from the snapshot alone"
    );
}

#[tokio::test]
async fn a_tombstone_is_not_superseded_by_a_collab_op() {
    let (e, _organ) = cell("http://cell-a").await;
    store::organs::add_contact(&e.store.pool, "organ-b", None, "B", "http://cell-b", 1)
        .await
        .expect("contact");
    store::organs::set_sync_policy(&e.store.pool, "organ-b", true, false)
        .await
        .expect("policy");

    let uid = plain(&e, "doomed").await;
    e.write_record_text(&uid, None, Some("text"))
        .await
        .expect("write");
    e.compact_doc(&uid).await.expect("compact");
    let snapshot = ops_of_kind(&e, &uid, "snapshot").await;
    store::records::mark_deleted(&e.store.pool, &uid)
        .await
        .expect("delete");

    let (peer, peer_organ) = cell("http://cell-c").await;
    store::organs::add_contact(&e.store.pool, &peer_organ, None, "C", "http://cell-c", 1)
        .await
        .expect("contact");
    let mut wire = e.hydrate_ops(snapshot).await.expect("hydrate");
    for op in &mut wire {
        op.organ_uid = peer_organ.clone();
        op.actor_cell = format!("{peer_organ}-cell");
        op.hlc = nucleus::hlc::next();
    }
    let _ = peer;
    e.import_op_batch(&OpBatch {
        from_organ: peer_organ,
        ops: wire,
    })
    .await
    .expect("import");
    assert!(
        store::records::get(&e.store.pool, &uid)
            .await
            .expect("get")
            .is_none(),
        "the tombstone freeze refused the snapshot: the record stays deleted"
    );

    let head = sync_ops::max_seq(&e.store.pool).await.expect("max seq");
    store::organs::advance_peer_acked_seq(&e.store.pool, "organ-b", head)
        .await
        .expect("ack");
    sync_ops::outbox_clear_contact(&e.store.pool, "organ-b")
        .await
        .expect("clear");
    e.prune_op_log(false).await.expect("prune");

    assert_eq!(
        ops_of_kind(&e, &uid, "tombstone").await.len(),
        1,
        "the delete survives pruning"
    );
}

#[tokio::test]
async fn the_sweep_compacts_a_doc_nobody_has_open() {
    let (e, _organ) = cell("http://cell-a").await;
    let uid = plain(&e, "abandoned").await;
    for n in 0..3 {
        e.write_record_text(&uid, None, Some(&format!("line {n}")))
            .await
            .expect("write");
    }
    e.close_record_doc(&uid);

    let swept = e.compact_stale_docs().await.expect("sweep");
    assert_eq!(swept, 0, "below the threshold, nothing to do");

    assert!(
        e.compact_doc(&uid).await.expect("compact"),
        "a doc nobody has open still compacts"
    );
    assert_eq!(ops_of_kind(&e, &uid, "snapshot").await.len(), 1);
}

#[tokio::test]
async fn a_contact_added_after_a_prune_still_receives_the_text() {
    let (a, a_organ) = cell("http://cell-a").await;
    store::organs::add_contact(&a.store.pool, "organ-old", None, "Old", "http://old", 1)
        .await
        .expect("contact");
    store::organs::set_sync_policy(&a.store.pool, "organ-old", true, false)
        .await
        .expect("policy");

    let uid = plain(&a, "shared").await;
    for line in ["draft", "draft revised", "final text"] {
        a.write_record_text(&uid, None, Some(line))
            .await
            .expect("write");
    }
    a.compact_doc(&uid).await.expect("compact");

    let head = sync_ops::max_seq(&a.store.pool).await.expect("max seq");
    store::organs::advance_peer_acked_seq(&a.store.pool, "organ-old", head)
        .await
        .expect("ack");
    sync_ops::outbox_clear_contact(&a.store.pool, "organ-old")
        .await
        .expect("clear");
    a.prune_op_log(false).await.expect("prune");
    assert!(
        ops_of_kind(&a, &uid, "crdt").await.is_empty(),
        "the tail really is gone, so this test means something"
    );

    let (b, _b_organ) = cell("http://cell-b").await;
    store::organs::add_contact(&b.store.pool, &a_organ, None, "A", "http://cell-a", 1)
        .await
        .expect("contact");
    let (feed, _head) = a.ops_after(0, 10_000).await.expect("feed");
    b.import_op_batch(&OpBatch {
        from_organ: a_organ,
        ops: feed,
    })
    .await
    .expect("import");

    assert_eq!(
        b.doc_text(&uid).await.expect("text").1,
        "final text",
        "a from-zero peer rebuilds the text from the snapshot in the feed"
    );
    let row = store::records::get(&b.store.pool, &uid)
        .await
        .expect("get")
        .expect("record");
    assert_eq!(row.body, "final text", "and it is materialized");
}

#[tokio::test]
async fn importing_a_snapshot_does_not_emit_one_back() {
    let (a, a_organ) = cell("http://cell-a").await;
    let (b, _b_organ) = cell("http://cell-b").await;
    store::organs::add_contact(&b.store.pool, &a_organ, None, "A", "http://cell-a", 1)
        .await
        .expect("contact");

    let uid = plain(&a, "shared").await;
    a.write_record_text(&uid, None, Some("from a"))
        .await
        .expect("write");
    a.compact_doc(&uid).await.expect("compact");

    let snapshot = ops_of_kind(&a, &uid, "snapshot").await;
    let wire = a.hydrate_ops(snapshot).await.expect("hydrate");
    b.import_op_batch(&OpBatch {
        from_organ: a_organ.clone(),
        ops: wire,
    })
    .await
    .expect("import");

    assert_eq!(
        b.doc_text(&uid).await.expect("text").1,
        "from a",
        "the snapshot alone carried the text"
    );
    assert!(
        ops_of_kind(&b, &uid, "snapshot").await.len() == 1,
        "only the imported one — B did not answer with its own"
    );
}

#[tokio::test]
async fn importing_a_compaction_sized_op_does_not_deadlock() {
    let (a, a_organ) = cell("http://cell-a").await;
    let (b, _b_organ) = cell("http://cell-b").await;
    let a_intro = a.introduction().await.expect("introduction");
    b.adopt_introduction(&a_intro, 1).await.expect("adopt");

    let uid = plain(&a, "big").await;
    let big = "lorem ipsum ".repeat(40_000);
    a.write_record_text(&uid, None, Some(&big))
        .await
        .expect("write");

    let ops = sync_ops::for_field(&a.store.pool, "record", &uid, "")
        .await
        .expect("ops")
        .into_iter()
        .filter(|op| op.kind == "crdt")
        .collect::<Vec<_>>();
    assert!(!ops.is_empty(), "there is a crdt op to import");
    let wire = a.hydrate_ops(ops).await.expect("hydrate");

    let imported = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        b.import_op_batch(&OpBatch {
            from_organ: a_organ,
            ops: wire,
        }),
    )
    .await
    .expect("import must not deadlock")
    .expect("import");
    assert!(imported > 0);
    assert_eq!(b.doc_text(&uid).await.expect("text").1, big);
}

#[tokio::test]
async fn a_version_vector_catch_up_survives_the_servers_prune() {
    let (a, a_organ) = cell("http://cell-a").await;
    let (b, _b_organ) = cell("http://cell-b").await;
    let a_intro = a.introduction().await.expect("introduction");
    b.adopt_introduction(&a_intro, 1).await.expect("adopt");
    store::organs::add_contact(&a.store.pool, "organ-old", None, "Old", "http://old", 1)
        .await
        .expect("contact");
    store::organs::set_sync_policy(&a.store.pool, "organ-old", true, false)
        .await
        .expect("policy");

    let uid = plain(&a, "shared").await;
    for value in ["one", "two", "three"] {
        store::records::set_extension(
            &a.store.pool,
            &uid,
            "test.state",
            &serde_json::json!({ "step": value }),
        )
        .await
        .expect("set extension");
    }
    let head_seq = sync_ops::max_seq(&a.store.pool).await.expect("max seq");
    store::organs::advance_peer_acked_seq(&a.store.pool, "organ-old", head_seq)
        .await
        .expect("ack");
    sync_ops::outbox_clear_contact(&a.store.pool, "organ-old")
        .await
        .expect("clear");
    let report = a.prune_op_log(false).await.expect("prune");
    assert!(report.removed > 0, "A really did prune: {report:?}");

    let mine = store::sync_ops::version_vector_for_organ(&b.store.pool, &a_organ)
        .await
        .expect("vector");
    assert!(mine.is_empty(), "B starts with nothing of A's");
    let missing = store::sync_ops::ops_missing_from_vector(&a.store.pool, &a_organ, &mine, 2000)
        .await
        .expect("missing");
    let wire = a.hydrate_ops(missing).await.expect("hydrate");
    b.import_op_batch(&OpBatch {
        from_organ: a_organ.clone(),
        ops: wire,
    })
    .await
    .expect("import");

    assert_eq!(
        store::records::get_extension(&b.store.pool, &uid, "test.state")
            .await
            .expect("extension")
            .and_then(|fields| fields
                .get("step")
                .and_then(|v| v.as_str().map(str::to_string))),
        Some("three".to_string()),
        "B has the surviving tip despite A having pruned the history"
    );

    let mine = store::sync_ops::version_vector_for_organ(&b.store.pool, &a_organ)
        .await
        .expect("vector");
    let still_missing =
        store::sync_ops::ops_missing_from_vector(&a.store.pool, &a_organ, &mine, 2000)
            .await
            .expect("missing");
    assert!(
        still_missing.is_empty(),
        "a second pass asks for nothing: {still_missing:?}"
    );
}

#[tokio::test]
async fn a_version_vector_never_mentions_a_third_organ() {
    let (us, _our_organ) = cell("http://us").await;
    let (a, a_organ) = cell("http://a").await;
    let (c, c_organ) = cell("http://c").await;
    for peer in [&a, &c] {
        let intro = peer.introduction().await.expect("introduction");
        us.adopt_introduction(&intro, 1).await.expect("adopt");
    }
    for (peer, organ) in [(&a, &a_organ), (&c, &c_organ)] {
        let uid = plain(peer, "note").await;
        let _ = uid;
        let (feed, _head) = peer.ops_after(0, 10_000).await.expect("feed");
        us.import_op_batch(&OpBatch {
            from_organ: organ.clone(),
            ops: feed,
        })
        .await
        .expect("import");
    }

    let for_a = store::sync_ops::version_vector_for_organ(&us.store.pool, &a_organ)
        .await
        .expect("vector");
    assert!(!for_a.is_empty(), "we do hold some of A's ops");
    let c_cells: Vec<String> = store::sync_ops::version_vector_for_organ(&us.store.pool, &c_organ)
        .await
        .expect("vector")
        .into_iter()
        .map(|entry| entry.actor_cell)
        .collect();
    assert!(
        !c_cells.is_empty(),
        "and some of C's, or this proves nothing"
    );
    for entry in &for_a {
        assert!(
            !c_cells.contains(&entry.actor_cell),
            "the vector sent to A must not name any of C's Cells"
        );
    }
}
