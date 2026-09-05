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

async fn contact(e: &Engine, uid: &str) {
    store::organs::add_contact(&e.store.pool, uid, None, uid, "http://peer", 1)
        .await
        .expect("contact");
    store::organs::set_sync_policy(&e.store.pool, uid, true, true)
        .await
        .expect("policy");
}

async fn pull_all(from: &Engine, to: &Engine) {
    let mut after = 0i64;
    loop {
        let (ops, head) = from.ops_after(after, 500).await.expect("ops");
        if ops.is_empty() {
            break;
        }
        let from_organ = store::organs::local(&from.store.pool)
            .await
            .expect("local")
            .expect("organ")
            .uid;
        to.import_op_batch(&engine::sync::OpBatch { from_organ, ops })
            .await
            .expect("import");
        if head <= after {
            break;
        }
        after = head;
    }
}

async fn state(e: &Engine, uids: &[&str]) -> Vec<(String, String, String, Option<String>)> {
    let rows = store::records::list_all(&e.store.pool)
        .await
        .expect("records");
    let mut out: Vec<_> = rows
        .into_iter()
        .filter(|r| uids.contains(&r.uid.as_str()))
        .map(|r| (r.uid, r.head, r.body, r.slug))
        .collect();
    out.sort();
    out
}

#[tokio::test]
async fn a_contact_added_after_a_prune_still_builds_a_complete_replica() {
    let (a, _a_organ) = cell("http://cell-a").await;

    let kept = plain(&a, "kept", "Kept", "original").await;
    let churned = plain(&a, "churned", "v0", "").await;
    let doomed = plain(&a, "doomed", "Doomed", "goes away").await;

    for i in 1..=25 {
        a.act(
            Action::EditRecordText {
                target: churned.clone(),
                head: Some(format!("v{i}")),
                body: None,
            },
            None,
        )
        .await
        .expect("edit");
        store::records::set_extension(
            &a.store.pool,
            &churned,
            "work.tracking",
            &serde_json::json!({ "estimate": i }),
        )
        .await
        .expect("extension");
    }
    a.act(
        Action::EditRecordText {
            target: kept.clone(),
            head: None,
            body: Some("edited text".into()),
        },
        None,
    )
    .await
    .expect("edit");
    a.act(
        Action::DeleteRecord {
            target: doomed.clone(),
        },
        None,
    )
    .await
    .expect("delete");

    contact(&a, "organ-old").await;
    let head = sync_ops::max_seq(&a.store.pool).await.expect("max");
    store::organs::advance_peer_acked_seq(&a.store.pool, "organ-old", head)
        .await
        .expect("advance");
    store::sync_ops::outbox_clear_contact(&a.store.pool, "organ-old")
        .await
        .expect("clear");

    let before = sync_ops::max_seq(&a.store.pool).await.expect("max");
    let report = a.prune_op_log(false).await.expect("prune");
    assert!(
        report.removed > 0,
        "the churned field left superseded ops to drop",
    );
    assert_eq!(
        sync_ops::max_seq(&a.store.pool).await.expect("max"),
        before,
        "pruning drops superseded ops, never the newest one",
    );

    let (c, _c_organ) = cell("http://cell-c").await;
    pull_all(&a, &c).await;

    let under_test = [kept.as_str(), churned.as_str(), doomed.as_str()];
    let source = state(&a, &under_test).await;
    assert_eq!(source.len(), 2, "the deleted one is gone on the source too");
    assert_eq!(
        state(&c, &under_test).await,
        source,
        "a from-zero replica after pruning must equal the source",
    );

    let c_kept = store::records::get(&c.store.pool, &kept)
        .await
        .expect("get")
        .expect("kept exists on the new replica");
    assert_eq!(c_kept.body, "edited text", "collaborative text survived");
    let c_churned = store::records::get(&c.store.pool, &churned)
        .await
        .expect("get")
        .expect("churned exists");
    assert_eq!(c_churned.head, "v25", "the newest value of a churned field");
    assert_eq!(
        store::records::get_extension(&c.store.pool, &churned, "work.tracking")
            .await
            .expect("extension")
            .and_then(|v| v.get("estimate").cloned()),
        Some(serde_json::json!(25)),
        "a churned extension key bootstraps to its newest value",
    );
    assert_eq!(
        store::sync_apply::record_deleted(&c.store.pool, &doomed)
            .await
            .expect("deleted?"),
        Some(true),
        "a deleted record must NOT be resurrected by bootstrap",
    );
}

#[tokio::test]
async fn the_newest_op_for_a_live_field_is_never_pruned() {
    let (e, _organ) = cell("http://cell-tip").await;
    let uid = plain(&e, "one", "first", "").await;
    for i in 1..=5 {
        e.act(
            Action::EditRecordText {
                target: uid.clone(),
                head: Some(format!("head {i}")),
                body: None,
            },
            None,
        )
        .await
        .expect("edit");
    }
    contact(&e, "organ-a").await;
    let head = sync_ops::max_seq(&e.store.pool).await.expect("max");
    store::organs::advance_peer_acked_seq(&e.store.pool, "organ-a", head)
        .await
        .expect("advance");
    store::sync_ops::outbox_clear_contact(&e.store.pool, "organ-a")
        .await
        .expect("clear");

    e.prune_op_log(false).await.expect("prune");

    let organ = store::organs::local(&e.store.pool)
        .await
        .expect("local")
        .expect("organ")
        .uid;
    let live = store::sync_ops::after(&e.store.pool, &organ, 0, 10_000)
        .await
        .expect("ops");
    assert!(
        live.iter()
            .any(|op| op.tbl == "record" && op.uid == uid && op.field == "head"),
        "the head field kept its tip op",
    );
    assert!(
        live.iter()
            .any(|op| op.tbl == "record" && op.uid == uid && op.field == "slug"),
        "a field written once and never again is entirely current state",
    );
}
