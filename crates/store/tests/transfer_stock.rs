use chrono::{DateTime, Utc};
use nucleus::{DecimalValue, execution::Execution, fact::sha256_hex};
use serde_json::{Value, json};
use store::{
    Store, StoreError, cells, exact,
    misc::{NewPromise, insert_promise},
    records,
    sqlx::{self, SqlitePool},
    transfer_stock::{Input, get, require_current_writer_on, set, version},
    write_tx,
};

async fn stock() -> Result<(Store, String), StoreError> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    let organ: String = sqlx::query_scalar("SELECT uid FROM record WHERE slug = 'local-organ'")
        .fetch_one(pool)
        .await?;
    let cell = cells::ensure_local(pool, &organ, "writer").await?;
    sqlx::query(
        "INSERT INTO record (uid, kind, organ_uid, quantity_mantissa, created_at, updated_at)
         VALUES ('stock', 'plain', ?1, '3', 'fixture', 'fixture'),
                ('owner', 'person', ?1, '0', 'fixture', 'fixture')",
    )
    .bind(organ)
    .execute(pool)
    .await?;
    Ok((store, cell.uid))
}

fn request(id: &str, expected_version: u64, minimum: Option<DecimalValue>) -> Input {
    Input {
        record: "stock".into(),
        person: "owner".into(),
        minimum,
        expected_version,
        request_id: id.into(),
    }
}

fn sign(hash: &str) -> Option<String> {
    Some(format!("signed {hash}"))
}

#[tokio::test]
async fn signed_limit_changes_replay_and_reject_stock_shortfalls_atomically()
-> Result<(), Box<dyn std::error::Error>> {
    let (store, writer) = stock().await?;
    let pool = &store.pool;
    let now = DateTime::<Utc>::UNIX_EPOCH;
    let input = request("first", 0, Some(exact::integer(1)));
    let event = set(pool, input.clone(), now, "key", "public", sign).await?;
    let (payload, signature): (String, String) =
        sqlx::query_as("SELECT payload, signature FROM transfer_stock_limit_event WHERE uid = ?")
            .bind(&event)
            .fetch_one(pool)
            .await?;
    assert_eq!(payload, serde_json::to_string(&input)?);
    assert_eq!(Some(signature), sign(&sha256_hex(payload.as_bytes())));
    let limit = get(pool, "stock").await?.expect("signed limit");
    assert_eq!(limit.writer, writer);
    assert_eq!(limit.version, 1);

    let promise = || NewPromise {
        record_uid: Some("stock".into()),
        party_uid: Some("owner".into()),
        delta: -2.0,
        reserve_from: Some("proposed".into()),
        ..Default::default()
    };
    let (first, second) = tokio::join!(
        insert_promise(pool, promise()),
        insert_promise(pool, promise())
    );
    let rejected = match (first, second) {
        (Ok(_), Err(error)) | (Err(error), Ok(_)) => error,
        other => panic!("one reservation should succeed and one should fail: {other:?}"),
    };
    assert!(
        rejected.to_string().contains("would be below 1"),
        "{rejected}"
    );
    let promises: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM promise")
        .fetch_one(pool)
        .await?;
    assert_eq!(promises, 1);
    assert!(matches!(
        set(pool, request("too-high", 1, Some(exact::integer(2))), now, "key", "public", sign).await,
        Err(StoreError::Protocol(message)) if message.contains("would be below")
    ));
    assert_eq!(version(pool, "stock").await?, 1);

    let mut tx = write_tx(pool).await?;
    assert!(matches!(
        records::bump_imported_quantity(&mut tx, "stock", exact::integer(-1), "fixture", "other-cell").await,
        Err(StoreError::Protocol(message)) if message.contains("without stock authority")
    ));
    tx.rollback().await?;

    set(pool, request("remove", 1, None), now, "key", "public", sign).await?;
    assert_eq!(version(pool, "stock").await?, 2);
    assert!(get(pool, "stock").await?.is_none());
    assert_eq!(
        set(pool, input, now, "key", "public", |_| None).await?,
        event
    );
    assert!(matches!(
        set(pool, request("first", 0, None), now, "key", "public", sign).await,
        Err(StoreError::Protocol(message)) if message.contains("reused with different values")
    ));
    assert!(get(pool, "stock").await?.is_none());
    Ok(())
}

async fn publish(
    pool: &SqlitePool,
    version: i64,
    not_after: &str,
    cells: Value,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO organ_roster (organ_uid, root_key, version, not_after, payload, signature, updated_at)
         SELECT uid, 'root', ?1, ?2, ?3, 'signature', 'fixture'
         FROM record WHERE slug = 'local-organ'
         ON CONFLICT (organ_uid) DO UPDATE SET version = excluded.version,
            not_after = excluded.not_after, payload = excluded.payload",
    )
    .bind(version)
    .bind(not_after)
    .bind(json!({ "cells": cells }).to_string())
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn stock_limits_keep_historical_write_authority_until_expiry()
-> Result<(), Box<dyn std::error::Error>> {
    let (store, cell) = stock().await?;
    let pool = &store.pool;
    let current = json!({"cell_uid": cell, "capabilities": ["write"]});
    let disconnected = json!({"cell_uid": "other", "capabilities": ["write"]});
    let prior_expiry = "2026-02-01T00:00:00+03:00";
    let current_expiry = "2026-03-01T00:00:00Z";
    let expiry = DateTime::parse_from_rfc3339(prior_expiry)?.to_utc();
    let before = Execution::new([1; 32], expiry.timestamp_millis() - 1)?;
    let after = Execution::new([2; 32], expiry.timestamp_millis())?;
    let submit = || {
        set(
            pool,
            request("limit", 0, Some(exact::zero())),
            expiry,
            "key",
            "public",
            sign,
        )
    };

    publish(pool, 1, prior_expiry, json!([current, disconnected])).await?;
    assert!(matches!(
        before.scope(submit()).await,
        Err(StoreError::Protocol(message)) if message.contains("only writer")
    ));
    publish(pool, 2, current_expiry, json!([current])).await?;
    assert!(matches!(
        before.scope(submit()).await,
        Err(StoreError::Protocol(message)) if message.contains("unexpired write authorization")
    ));
    assert_eq!(version(pool, "stock").await?, 0);
    after.scope(submit()).await?;
    assert_eq!(version(pool, "stock").await?, 1);

    let expired = Execution::new(
        [3; 32],
        DateTime::parse_from_rfc3339(current_expiry)?.timestamp_millis(),
    )?;
    let mut connection = pool.acquire().await?;
    assert!(matches!(
        expired.scope(require_current_writer_on(&mut connection)).await,
        Err(StoreError::Protocol(message)) if message.contains("current write authorization")
    ));
    Ok(())
}
