use nucleus::RecordKind;
use store::Store;
use store::records::NewRecord;
use store::sync_ops;

async fn cell() -> Store {
    Store::open_memory().await.expect("in-memory store")
}

/// The guard `outbox_delete` carries, tested directly rather than only through
/// the drain that relies on it.
///
/// The bounded outbox keeps ONE row per (contact, tbl, uid, field): a second
/// edit to the same field replaces the queued row's seq rather than adding a
/// row. So a delete that matched on identity alone would, on a drain that
/// raced an edit, remove a row pointing at an op the peer never received —
/// losing that edit silently and forever, because nothing re-queues it.
#[tokio::test]
async fn an_op_superseded_while_in_flight_is_not_dropped_by_the_delete() {
    let store = cell().await;
    let them = "r_THEMTHEMTHEMTHEMTHEMTHEMTH";
    store::organs::add_contact(&store.pool, them, None, "them", "http://them.test", 1)
        .await
        .expect("contact");
    store::organs::set_sync_policy(&store.pool, them, true, false)
        .await
        .expect("policy");

    let uid = store::records::create(
        &store.pool,
        NewRecord {
            slug: Some("in-flight"),
            kind: RecordKind::Plain,
            head: "in flight",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid;

    let queued = sync_ops::outbox_due(&store.pool).await.expect("due");
    let in_flight = queued
        .iter()
        .find(|row| row.uid == uid && row.field == "slug")
        .expect("the create queued a slug op")
        .clone();

    // The drain is now "in flight" holding `in_flight`. A second edit lands
    // before it finishes and REPLACES that queued row with a newer seq.
    store::records::set_slug(&store.pool, &uid, Some("edited-mid-flight"))
        .await
        .expect("edit");
    let superseding = sync_ops::outbox_due(&store.pool)
        .await
        .expect("due")
        .into_iter()
        .find(|row| row.uid == uid && row.field == "slug")
        .expect("still queued");
    assert!(
        superseding.seq > in_flight.seq,
        "the edit replaced the queued row rather than adding one"
    );

    // The drain returns and deletes what IT sent.
    sync_ops::outbox_delete(&store.pool, &in_flight)
        .await
        .expect("delete");

    let left = sync_ops::outbox_due(&store.pool).await.expect("due");
    let survivor = left
        .iter()
        .find(|row| row.uid == uid && row.field == "slug")
        .expect("the newer op is still queued and will be sent next drain");
    assert_eq!(survivor.seq, superseding.seq);
}

/// Deleting what WAS sent, unraced, must still work — otherwise the guard
/// would be indistinguishable from a delete that never fires.
#[tokio::test]
async fn an_unraced_delivered_row_is_deleted() {
    let store = cell().await;
    let them = "r_THEMTHEMTHEMTHEMTHEMTHEMTH";
    store::organs::add_contact(&store.pool, them, None, "them", "http://them.test", 1)
        .await
        .expect("contact");
    store::organs::set_sync_policy(&store.pool, them, true, false)
        .await
        .expect("policy");
    let uid = store::records::create(
        &store.pool,
        NewRecord {
            slug: Some("plain-send"),
            kind: RecordKind::Plain,
            head: "plain send",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid;

    for row in sync_ops::outbox_due(&store.pool).await.expect("due") {
        sync_ops::outbox_delete(&store.pool, &row)
            .await
            .expect("delete");
    }
    assert!(
        sync_ops::outbox_due(&store.pool)
            .await
            .expect("due")
            .iter()
            .all(|row| row.uid != uid),
        "an unraced drain empties the queue"
    );
}
