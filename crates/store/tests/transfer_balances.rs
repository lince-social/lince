use chrono::{DateTime, Utc};
use nucleus::transfer::application::{EffectMode, PrivateEffect};
use serde_json::{Value, json, to_value};
use store::{
    Store, StoreError,
    sqlx::{self, SqlitePool},
    transfer_accounting::{PolicyInput, set_policy},
    transfer_balances::{read, read_many},
};

mod transfer_occurrence_fixture;

async fn stock(pool: &SqlitePool, uid: &str, amount: &str) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO record (uid, kind, organ_uid, quantity_mantissa, created_at, updated_at)
         SELECT ?1, 'plain', uid, ?2, 'fixture', 'fixture'
         FROM record WHERE slug = 'local-organ'",
    )
    .bind(uid)
    .bind(amount)
    .execute(pool)
    .await?;
    Ok(())
}

async fn delivery(
    pool: &SqlitePool,
    uid: &str,
    revision: i64,
    state: &str,
    projection: Value,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO transfer_remote_reference
            (uid, origin_organ_uid, transfer_uid, delivery_policy_uid, recipient_person_uid,
             recipient_organ_uid, last_transfer_revision, state, projection, created_at, updated_at)
         SELECT ?1, 'origin', 'remote', 'policy', ?1, uid, ?2, ?3, ?4, 'fixture', 'fixture'
         FROM record WHERE slug = 'local-organ'",
    )
    .bind(uid)
    .bind(revision)
    .bind(state)
    .bind(projection.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn balance_prefers_delivered_revision_and_the_promises_own_path()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    stock(pool, "stock", "30").await?;
    stock(pool, "unrelated", "2").await?;
    sqlx::query(
        "INSERT INTO promise
            (uid, record_uid, delta, transfer_uid, party_uid, created_at, updated_at, reserve_from)
         VALUES ('p', 'stock', -19, 'remote', 'owner', 'fixture', 'fixture', 'proposed')",
    )
    .execute(pool)
    .await?;
    delivery(
        pool,
        "older",
        2,
        "active",
        json!({
            "uid": "remote", "revision": 2,
            "promises": [{
                "uid": "p", "exchange": "e", "party": "owner", "record": "stock",
                "title": "Older", "delta": -10, "state": "agreed", "reserve_from": "proposed"
            }],
        }),
    )
    .await?;
    delivery(pool, "current", 3, "revoked", json!({
        "uid": "remote", "revision": 3,
        "promises": [{
            "uid": "p", "exchange": "e", "party": "owner", "record": "stock",
            "title": "Current", "delta": -5, "state": "active", "reserve_from": "proposed"
        }],
        "occurrences": [
            { "uid": "sibling", "promise": "s", "exchange": "e", "exchange_path": "a", "quantity": 29 },
            {
                "uid": "own", "promise": "p", "exchange": "e", "exchange_path": "a",
                "quantity": 5, "window_end": "2026-02-03",
                "settlement_progress": { "cancelled_quantity_exact": "0.5", "cancelled_quantity": 99 }
            },
            {
                "uid": "fallback", "promise": "f", "exchange": "e", "quantity": 3,
                "settlement_progress": { "cancelled_quantity": 1.25 }
            }
        ]
    })).await?;

    let current = read(pool, "stock").await?;
    let terms: Vec<_> = current
        .commitments
        .iter()
        .map(|terms| {
            (
                terms.title.as_str(),
                terms.revision,
                terms.occurrence.as_deref(),
                terms.canonical_remaining.to_f64(),
                terms.due.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        terms,
        [
            ("Current", 3, Some("own"), 4.5, Some("2026-02-03")),
            ("Current", 3, Some("fallback"), 1.75, None),
        ]
    );
    assert_eq!(current.reserved.to_f64(), 6.25);
    assert_eq!(current.can_offer.to_f64(), 23.75);
    assert_eq!(
        current.incomplete,
        ["remote: delivery access is unavailable"]
    );

    let batch = read_many(pool, &["unrelated", "stock", "stock"]).await?;
    assert_eq!(batch.len(), 2);
    assert_eq!(to_value(&batch["stock"])?, to_value(current)?);
    assert_eq!(batch["unrelated"].actual.to_f64(), 2.0);
    assert!(matches!(
        read_many(pool, &["unrelated", "missing"]).await,
        Err(StoreError::RowNotFound)
    ));
    Ok(())
}

#[tokio::test]
async fn current_policy_maps_an_activated_promise_to_its_private_records() -> Result<(), StoreError>
{
    let store = transfer_occurrence_fixture::activated_transfer().await?;
    let pool = &store.pool;
    sqlx::query(
        "UPDATE record SET organ_uid = (SELECT uid FROM record WHERE slug = 'local-organ')
         WHERE uid = 'giver'",
    )
    .execute(pool)
    .await?;
    for record in ["stock", "primary", "secondary"] {
        stock(pool, record, "30").await?;
    }
    sqlx::query(
        "UPDATE promise SET record_uid = 'stock', party_uid = 'giver', state = 'active', delta = -1
         WHERE uid = 'promise'",
    )
    .execute(pool)
    .await?;
    let submit = async |transfer: &str, exchange: &str, effects| {
        set_policy(
            pool,
            PolicyInput {
                transfer: transfer.into(),
                exchange: exchange.into(),
                person: "giver".into(),
                effects,
                expected_version: 0,
                request_id: transfer.into(),
            },
            DateTime::<Utc>::UNIX_EPOCH,
            "key",
            "public",
            |_| Some("signature".into()),
        )
        .await
    };
    let effect = |record: &str, formula: &str| PrivateEffect {
        record: record.into(),
        formula: formula.into(),
        mode: EffectMode::Quantity,
    };
    submit(
        "transfer",
        "promise",
        vec![
            effect("primary", "-incoming()"),
            effect("secondary", "-incoming() / 2"),
        ],
    )
    .await?;
    submit(
        "unlisted",
        "unlisted-exchange",
        vec![effect("secondary", "-incoming()")],
    )
    .await?;

    let batch = read_many(pool, &["stock", "primary", "secondary"]).await?;
    assert!(batch["stock"].commitments.is_empty());
    for (record, reserved) in [("primary", 1.0), ("secondary", 0.5)] {
        let balance = &batch[record];
        assert_eq!(balance.reserved.to_f64(), reserved, "{record}");
        let [commitment] = balance.commitments.as_slice() else {
            panic!("expected one commitment for {record}");
        };
        assert_eq!(commitment.application_record, "primary");
        assert_eq!(commitment.occurrence.as_deref(), Some("occurrence"));
    }
    assert_eq!(
        batch["secondary"].incomplete,
        ["unlisted: a private commitment has no available current terms",]
    );
    Ok(())
}
