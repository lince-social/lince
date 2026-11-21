use nucleus::RecordKind;
use store::{
    Store, StoreError,
    assertions::{AssertionRole, NewAssertion},
    concepts, organs,
    records::{NewRecord, create},
    sqlx::query,
    sync_ops::{self, OpKind, VectorEntry},
    write_tx,
};

async fn incoming(
    store: &Store,
    uid: &str,
    field: &str,
    hlc: i64,
    actor: &str,
    organ: &str,
    root: Option<&str>,
) -> Result<i64, StoreError> {
    Ok(sync_ops::append(
        &store.pool,
        "record",
        uid,
        field,
        OpKind::Set,
        None,
        hlc,
        actor,
        organ,
        Some("peer"),
        root,
    )
    .await?
    .expect("each fixture operation has a distinct actor and stamp"))
}

#[tokio::test]
async fn vectors_select_missing_sequences_within_each_scope() -> Result<(), StoreError> {
    let store = Store::open_memory().await?;
    let root = Some("root");
    let first = incoming(&store, "a", "head", 10, "a", "organ", None).await?;
    let private = incoming(&store, "b", "head", 11, "a", "organ", root).await?;
    incoming(&store, "b", "head", 12, "a", "other-organ", None).await?;
    let second = incoming(&store, "a", "head", 10, "b", "organ", None).await?;
    let last = incoming(&store, "a", "head", 20, "a", "organ", None).await?;
    let vector = [
        VectorEntry {
            actor_cell: "a".into(),
            max_hlc: 10,
        },
        VectorEntry {
            actor_cell: "b".into(),
            max_hlc: 9,
        },
    ];
    let missing = sync_ops::ops_missing_from_vector(&store.pool, "organ", &vector, 10).await?;
    assert_eq!(
        missing.into_iter().map(|op| op.seq).collect::<Vec<_>>(),
        [second, last]
    );
    let siblings =
        sync_ops::sibling_ops_missing_from_vector(&store.pool, "organ", &vector, 1).await?;
    assert_eq!(
        siblings.into_iter().map(|op| op.seq).collect::<Vec<_>>(),
        [second]
    );
    let rooted =
        sync_ops::ops_missing_from_vector_in_root(&store.pool, "root", &vector, 10).await?;
    assert_eq!(
        rooted.into_iter().map(|op| op.seq).collect::<Vec<_>>(),
        [private]
    );
    assert_eq!(
        sync_ops::seq_covered_by_vector(&store.pool, "organ", &vector).await?,
        second - 1
    );
    assert_eq!(
        sync_ops::seq_covered_by_vector(&store.pool, "organ", &[]).await?,
        first - 1
    );

    let full = sync_ops::version_vector_for_organ(&store.pool, "organ").await?;
    assert_eq!(
        sync_ops::seq_covered_by_vector(&store.pool, "organ", &full).await?,
        last
    );
    assert!(
        sync_ops::ops_missing_from_vector(&store.pool, "organ", &[], -1)
            .await?
            .is_empty()
    );
    assert!(sync_ops::outbox_due(&store.pool).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn pruning_preserves_pending_operations_and_root_boundaries() -> Result<(), StoreError> {
    let store = Store::open_memory().await?;
    let peer = "r_THEMTHEMTHEMTHEMTHEMTHEMTH";
    organs::add_contact(&store.pool, peer, None, "peer", "http://peer.test", 1).await?;
    organs::set_sync_policy(&store.pool, peer, true, false).await?;

    let replaced = incoming(&store, "a", "head", 1, "a", "organ", None).await?;
    let private = incoming(&store, "a", "head", 2, "a", "organ", Some("root")).await?;
    let pending = incoming(&store, "a", "body", 3, "a", "organ", None).await?;
    let opening = incoming(&store, "a", "property:quantity", 4, "a", "organ", None).await?;
    let newest = incoming(&store, "a", "head", 5, "a", "organ", None).await?;
    incoming(&store, "a", "body", 6, "a", "organ", None).await?;
    let floor = incoming(&store, "a", "property:quantity", 7, "a", "organ", None).await?;
    query(
        "INSERT INTO sync_outbox (contact_organ, tbl, uid, field, kind, seq, queued_at)
         VALUES (?, 'record', 'a', 'body', 'set', ?, '')",
    )
    .bind(peer)
    .bind(pending)
    .execute(&store.pool)
    .await?;
    organs::advance_peer_acked_seq(&store.pool, peer, floor).await?;

    let preview = sync_ops::prune(&store.pool, true).await?;
    assert_eq!(preview.floor, floor);
    assert!(sync_ops::get_by_seq(&store.pool, replaced).await?.is_some());
    assert_eq!(sync_ops::prune(&store.pool, false).await?, preview);
    assert!(sync_ops::get_by_seq(&store.pool, replaced).await?.is_none());
    for seq in [private, pending, opening, newest, floor] {
        assert!(
            sync_ops::get_by_seq(&store.pool, seq).await?.is_some(),
            "retained seq {seq}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_local_private_append_restamps_and_enqueues_only_for_its_grant() -> Result<(), StoreError>
{
    let store = Store::open_memory().await?;
    let allowed = "r_THEMTHEMTHEMTHEMTHEMTHEMTH";
    let outside = "r_ELSEELSEELSEELSEELSEELSEEL";
    for peer in [allowed, outside] {
        organs::add_contact(&store.pool, peer, None, peer, "http://peer.test", 1).await?;
        organs::set_sync_policy(&store.pool, peer, true, false).await?;
    }
    query(
        "INSERT INTO replica_grant (root_record, contact_organ, state, created_at)
         VALUES ('root', ?, 'accepted', '')",
    )
    .bind(allowed)
    .execute(&store.pool)
    .await?;

    let first = incoming(&store, "private", "head", 1, "cell", "organ", Some("root")).await?;
    let local = sync_ops::append(
        &store.pool,
        "record",
        "private",
        "head",
        OpKind::Set,
        Some("updated"),
        1,
        "cell",
        "organ",
        None,
        Some("root"),
    )
    .await?
    .expect("a local append retries a conflicting hybrid clock");
    assert!(local > first);
    let rows = sync_ops::outbox_due(&store.pool).await?;
    let [pending] = rows.as_slice() else {
        panic!("expected only the accepted grant's delivery, got {rows:?}");
    };
    assert_eq!(pending.contact_organ, allowed);
    assert_eq!(pending.seq, local);
    Ok(())
}

#[tokio::test]
async fn assertion_logging_refuses_non_object_payloads_without_writes() -> Result<(), StoreError> {
    let store = Store::open_memory().await?;
    let record = create(
        &store.pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "Subject",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await?;
    let predicate = concepts::ensure(&store.pool, "log-shape").await?;
    let assertion = store::assertions::assert(
        &store.pool,
        NewAssertion {
            subject_uid: &record.uid,
            predicate_uid: &predicate,
            object_uid: None,
            role: AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await?;
    let before = sync_ops::max_seq(&store.pool).await?;

    for payload in ["[]", "true", "5", "\"value\""] {
        let result = sync_ops::log_local(
            &store.pool,
            "record_assertion",
            &assertion,
            "",
            OpKind::Set,
            Some(payload.into()),
        )
        .await;
        assert!(matches!(result, Err(StoreError::Protocol(_))), "{result:?}");

        let mut tx = write_tx(&store.pool).await?;
        let result = sync_ops::log_local_tx(
            &mut tx,
            "record_assertion",
            &assertion,
            "",
            OpKind::Set,
            Some(payload.into()),
        )
        .await;
        assert!(matches!(result, Err(StoreError::Protocol(_))), "{result:?}");
        tx.commit().await?;
    }
    assert_eq!(sync_ops::max_seq(&store.pool).await?, before);
    Ok(())
}

#[tokio::test]
async fn an_invalid_stored_clock_returns_a_decode_error() -> Result<(), StoreError> {
    let store = Store::open_memory().await?;
    let seq = incoming(&store, "a", "head", 1, "cell", "organ", None).await?;
    query("UPDATE sync_op SET hlc = 'invalid' WHERE seq = ?")
        .bind(seq)
        .execute(&store.pool)
        .await?;
    let result = sync_ops::get_by_seq(&store.pool, seq).await;
    assert!(
        matches!(result, Err(StoreError::ColumnDecode { .. })),
        "{result:?}"
    );
    Ok(())
}
