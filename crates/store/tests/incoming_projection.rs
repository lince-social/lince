use nucleus::RecordKind;
use serde_json::json;
use store::{
    Store, StoreError, assertions, concepts, organs,
    records::{self, NewRecord},
    sqlx::query,
    sync_apply::{self, Stamp},
    sync_ops::{self, OpKind},
};

async fn record(store: &Store, slug: Option<&str>) -> Result<String, StoreError> {
    Ok(records::create(
        &store.pool,
        NewRecord {
            slug,
            kind: RecordKind::Plain,
            head: "original",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await?
    .uid)
}

fn stamp<'a>(tbl: &'a str, uid: &'a str, field: &'a str, hlc: i64) -> Stamp<'a> {
    Stamp {
        tbl,
        uid,
        field,
        hlc,
    }
}

async fn incoming(store: &Store, stamp: Stamp<'_>, kind: OpKind) -> Result<(), StoreError> {
    let seq = sync_ops::append(
        &store.pool,
        stamp.tbl,
        stamp.uid,
        stamp.field,
        kind,
        None,
        stamp.hlc,
        "peer-cell",
        "peer-organ",
        Some("peer-organ"),
        None,
    )
    .await?;
    assert!(seq.is_some(), "fixture operations have distinct stamps");
    Ok(())
}

#[tokio::test]
async fn record_replay_preserves_clock_guards_slug_conflicts_and_exact_quantities()
-> Result<(), StoreError> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    record(&store, Some("taken")).await?;
    let organ = organs::local(pool).await?.expect("Store has a local Organ");
    let uid = nucleus::new_uid("r");
    sync_apply::ensure_record_stub(pool, &uid, "", &organ.uid, None, Some(7)).await?;
    let slug = stamp("record", &uid, "slug", 20);
    assert!(
        sync_apply::set_record_field(pool, &uid, "slug", &json!("available"), false, slug).await?
    );
    assert!(sync_apply::set_record_field(pool, &uid, "slug", &json!("taken"), false, slug).await?);
    let row = records::get(pool, &uid)
        .await?
        .expect("incoming stub exists");
    assert_eq!(row.kind, "plain");
    assert_eq!(row.created_hlc, Some(7));
    assert_eq!(row.slug, None);

    let latest = stamp("record", &uid, "head", 30);
    incoming(&store, latest, OpKind::Set).await?;
    sync_apply::tombstone_record(pool, &uid, stamp("record", &uid, "", 40)).await?;
    let stale = stamp("record", &uid, "head", 29);
    assert!(!sync_apply::set_record_field(pool, &uid, "head", &json!("stale"), true, stale).await?);
    assert_eq!(sync_apply::record_deleted(pool, &uid).await?, Some(true));
    assert!(
        sync_apply::set_record_field(pool, &uid, "head", &json!("current"), true, latest).await?
    );
    assert_eq!(sync_apply::record_deleted(pool, &uid).await?, Some(false));
    assert_eq!(records::head(pool, &uid).await?.as_deref(), Some("current"));

    for quantity in ["250", "-25"] {
        assert!(
            sync_apply::set_record_field(
                pool,
                &uid,
                "quantity",
                &json!({"mantissa": quantity, "scale": 2}),
                false,
                stamp("record", &uid, "quantity", 50),
            )
            .await?
        );
    }
    let expected = nucleus::DecimalValue::from_mantissa(2, 225).expect("fixture decimal");
    assert_eq!(records::quantity(pool, &uid).await?, Some(expected));

    query(
        "INSERT INTO transfer_stock_limit (record_uid, writer_cell_uid, person_uid,
            minimum_mantissa, minimum_scale, version, event_uid)
         VALUES (?1, ?2, ?2, '0', 0, 1, 'fixture')",
    )
    .bind(&uid)
    .bind(&organ.uid)
    .execute(pool)
    .await?;
    let opening = stamp("record", &uid, "quantity", 60);
    let guarded = sync_apply::set_record_field(
        pool,
        &uid,
        "quantity",
        &json!({"mantissa": "1", "scale": 0}),
        false,
        opening,
    )
    .await;
    assert!(matches!(guarded, Err(StoreError::Protocol(message)) if message.contains("guarded")));
    assert!(
        sync_apply::set_record_field(
            pool,
            &uid,
            "quantity",
            &json!({"mantissa": "0"}),
            false,
            opening
        )
        .await?
    );
    assert_eq!(records::quantity(pool, &uid).await?, Some(expected));
    Ok(())
}

#[tokio::test]
async fn extension_replay_preserves_latest_fields_and_upgrades_legacy_objects()
-> Result<(), StoreError> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    let uid = record(&store, None).await?;
    sync_apply::set_extension_whole(
        pool,
        &uid,
        "sample",
        &json!([1, 2]),
        stamp("record_extension", &uid, "sample", 10),
    )
    .await?;
    let latest = stamp("record_extension", &uid, "sample.a", 30);
    incoming(&store, latest, OpKind::Set).await?;
    sync_apply::set_extension_key(pool, &uid, "sample", "a", json!(1), latest).await?;
    sync_apply::tombstone_extension_key(
        pool,
        &uid,
        "sample",
        "a",
        stamp("record_extension", &uid, "sample.a", 29),
    )
    .await?;
    assert_eq!(
        records::get_extension(pool, &uid, "sample").await?,
        Some(json!({"a":1}))
    );
    sync_apply::tombstone_extension_key(pool, &uid, "sample", "a", latest).await?;
    assert_eq!(
        records::get_extension(pool, &uid, "sample").await?,
        Some(json!({}))
    );
    Ok(())
}

#[tokio::test]
async fn ordinary_assertion_replay_reinstates_the_next_winner_after_tombstoning()
-> Result<(), StoreError> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    let subject = record(&store, None).await?;
    let predicate = concepts::ensure(pool, "replica-edges").await?;
    let object = record(&store, None).await?;
    let value = json!({"subject_uid": subject, "predicate_uid": predicate, "object_uid": object});
    let lower = "a_00000000000000000000000001";
    let higher = "a_00000000000000000000000002";
    for (uid, hlc) in [(higher, 10), (lower, 11)] {
        sync_apply::upsert_assertion(pool, uid, &value, stamp("record_assertion", uid, "", hlc))
            .await?;
    }
    let before = assertions::get(pool, higher)
        .await?
        .expect("winning Assertion");
    assert!(before.retracted_at.is_none());
    let shadowed = assertions::get(pool, lower)
        .await?
        .expect("shadowed Assertion");
    assert!(shadowed.retracted_at.is_some());

    let tombstone = stamp("record_assertion", higher, "", 20);
    incoming(&store, tombstone, OpKind::Tombstone).await?;
    sync_apply::retract_assertion(pool, higher, tombstone).await?;
    let active = assertions::incident_to(pool, &[&subject]).await?;
    assert_eq!(
        active.into_iter().map(|row| row.uid).collect::<Vec<_>>(),
        [lower]
    );
    Ok(())
}
