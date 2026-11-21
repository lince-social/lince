use std::{
    future::{Future, poll_fn},
    pin::pin,
    slice::from_ref,
    task::Poll,
};

use chrono::DateTime;
use nucleus::DecimalValue;
use sqlx::{query, query_scalar};

use super::{NewEntry, create, history, list_all};
use crate::{Store, StoreError};

#[tokio::test]
async fn concurrent_captures_with_one_request_commit_once() -> Result<(), StoreError> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    query(
        "INSERT INTO record (uid, organ_uid, created_at, updated_at)
         VALUES ('record', 'record', '', '')",
    )
    .execute(pool)
    .await?;
    query(
        "INSERT INTO fact (uid, record_uid, delta_mantissa, delta_scale, at, cause_kind, prev_hash, hash)
         VALUES ('fact', 'record', '0', 0, '', 'test', '', 'test')",
    )
    .execute(pool)
    .await?;
    let amount = DecimalValue::from_mantissa(0, 1)
        .map_err(|error| StoreError::Protocol(error.to_string()))?;
    let now = DateTime::UNIX_EPOCH;
    let input = || NewEntry {
        record_uid: "record",
        amount,
        note: None,
        occurred_at: now,
        fact_uid: "fact",
        request_id: "same request",
        actor_uid: None,
    };

    // Queue both attempts on the sole connection. With replay outside the
    // transaction, both queued reads would finish before either write begins.
    let mut connection = pool.acquire().await?;
    let sequence: i64 = query_scalar("SELECT value FROM commit_sequence WHERE id = 1")
        .fetch_one(&mut *connection)
        .await?;
    let mut first = pin!(create(pool, input(), now));
    let mut second = pin!(create(pool, input(), now));
    poll_fn(|context| {
        assert!(first.as_mut().poll(context).is_pending());
        assert!(second.as_mut().poll(context).is_pending());
        Poll::Ready(())
    })
    .await;
    drop(connection);

    let (first, second) = tokio::try_join!(first, second)?;
    assert_eq!(first.entry(), second.entry());
    assert_ne!(first.was_replayed(), second.was_replayed());
    assert_eq!(
        list_all(pool, -1).await?.as_slice(),
        from_ref(first.entry())
    );
    assert_eq!(history(pool, &first.entry().uid).await?.len(), 1);
    let committed_sequence: i64 = query_scalar("SELECT value FROM commit_sequence WHERE id = 1")
        .fetch_one(pool)
        .await?;
    assert_eq!(committed_sequence, sequence + 1);
    Ok(())
}
