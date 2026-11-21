use chrono::{DateTime, TimeDelta, Utc};
use nucleus::{Cause, CauseKind, NewFact, RecordKind, fact::seal};
use sqlx::{SqlitePool, query};

use super::{
    get, get_in_transaction, insert, instant, ledger_since, level, map_fact_with_origin,
    retain_origin, sum_window, sum_window_lagged,
};
use crate::{
    Store,
    exact::{integer, zero},
    records::{NewRecord, create},
};

async fn record(pool: &SqlitePool) -> String {
    create(
        pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "Fact history",
            body: "",
            quantity: zero(),
        },
    )
    .await
    .unwrap()
    .uid
}

#[tokio::test]
async fn original_and_local_fact_reads_preserve_their_distinct_evidence() {
    let store = Store::open_memory().await.unwrap();
    let pool = &store.pool;
    let record_uid = record(pool).await;
    let at = DateTime::from_timestamp(4_102_444_800, 0).unwrap();
    let local = seal(
        NewFact {
            actor_uid: Some("actor-private".into()),
            payload: Some("local-private".into()),
            ..NewFact::quantity(
                &record_uid,
                integer(2),
                Cause {
                    kind: CauseKind::Sync,
                    uid: Some("cause-private".into()),
                },
            )
        },
        "genesis",
        at,
    );
    let original = seal(
        NewFact {
            uid: Some(local.uid.clone()),
            payload: Some("original evidence".into()),
            ..NewFact::quantity(&record_uid, local.delta, Cause::user_edit())
        },
        "remote-chain",
        at,
    );
    let mut tx = pool.begin().await.unwrap();
    insert(&mut tx, &local).await.unwrap();
    retain_origin(&mut tx, &original, "organ", "cell")
        .await
        .unwrap();
    assert_eq!(
        get_in_transaction(&mut tx, &local.uid)
            .await
            .unwrap()
            .as_ref(),
        Some(&local)
    );
    tx.commit().await.unwrap();

    assert_eq!(
        get(pool, &local.uid).await.unwrap().as_ref(),
        Some(&original)
    );
    let mapped = query(
        "SELECT f.*, o.payload AS origin_payload FROM fact f
         LEFT JOIN fact_origin o ON o.fact_uid = f.uid WHERE f.uid = ?",
    )
    .bind(&local.uid)
    .try_map(map_fact_with_origin)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(mapped, original);

    let since = instant(at);
    let projection = async |include_details| {
        ledger_since(pool, Some(&since), 10, include_details)
            .await
            .unwrap()
            .into_iter()
            .find(|fact| fact.uid == local.uid)
            .unwrap()
    };
    let details = projection(true).await;
    let aggregate = projection(false).await;
    assert_eq!(details.actor_uid, local.actor_uid);
    assert_eq!(details.cause, local.cause);
    assert_eq!(details.payload, local.payload);
    assert_eq!(aggregate.delta, details.delta);
    assert_eq!(aggregate.cause.kind, details.cause.kind);
    assert!(
        aggregate.actor_uid.is_none()
            && aggregate.cause.uid.is_none()
            && aggregate.payload.is_none()
    );
    assert!(!format!("{details:?}").contains("-private"));
}

#[tokio::test]
async fn checkpoint_base_and_exact_window_boundaries_preserve_quantity() {
    let store = Store::open_memory().await.unwrap();
    let pool = &store.pool;
    let record_uid = record(pool).await;
    let start = DateTime::<Utc>::from_timestamp(1_600_000_000, 0).unwrap();
    let end = start + TimeDelta::seconds(10);
    let checkpoint = NewFact {
        payload: Some(r#"{"level":"7","quantity_offset":"2"}"#.into()),
        ..NewFact::quantity(
            &record_uid,
            zero(),
            Cause {
                kind: CauseKind::Checkpoint,
                uid: None,
            },
        )
    };
    let mut tx = pool.begin().await.unwrap();
    let prior = seal(
        NewFact::quantity(&record_uid, integer(50), Cause::user_edit()),
        "genesis",
        start,
    );
    let anchor = seal(checkpoint, &prior.hash, start + TimeDelta::seconds(5));
    let later = seal(
        NewFact::quantity(&record_uid, integer(2), Cause::user_edit()),
        &anchor.hash,
        end,
    );
    for fact in [&prior, &anchor, &later] {
        insert(&mut tx, fact).await.unwrap();
    }
    tx.commit().await.unwrap();

    assert_eq!(level(pool, &record_uid).await.unwrap(), integer(7));
    assert_eq!(
        sum_window_lagged(pool, &record_uid, 10, 0, end)
            .await
            .unwrap(),
        integer(50)
    );
    assert_eq!(
        sum_window(pool, &record_uid, 0, end).await.unwrap(),
        integer(2)
    );
    assert!(sum_window(pool, &record_uid, i64::MAX, end).await.is_err());
}
