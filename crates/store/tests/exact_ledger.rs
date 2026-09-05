use chrono::{TimeDelta, Utc};
use nucleus::{Cause, DecimalValue, NewFact};
use store::Store;
use store::exact::{from_f64, zero};

async fn record(store: &Store, slug: &str) -> String {
    store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: Some(slug),
            kind: nucleus::RecordKind::Plain,
            head: slug,
            body: "",
            quantity: zero(),
        },
    )
    .await
    .expect("record")
    .uid
}

async fn append(store: &Store, record_uid: &str, delta: DecimalValue) {
    let now = Utc::now();
    let mut tx = store.pool.begin().await.expect("begin");
    let prev = store::facts::last_hash(&mut tx).await.expect("head");
    let fact = nucleus::fact::seal(
        NewFact::quantity(record_uid, delta, Cause::user_edit()),
        &prev,
        now,
    );
    store::facts::insert(&mut tx, &fact).await.expect("insert");
    store::records::bump_quantity(&mut tx, record_uid, delta, &now.to_rfc3339())
        .await
        .expect("bump");
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn float_drift_does_not_survive_the_ledger() {
    let store = Store::open_memory().await.expect("store");
    let uid = record(&store, "drift").await;

    append(&store, &uid, from_f64(0.1)).await;
    append(&store, &uid, from_f64(0.2)).await;

    let level = store::records::quantity(&store.pool, &uid)
        .await
        .expect("query")
        .expect("record exists");
    assert_eq!(level.canonical(), "0.3", "0.1 + 0.2 is exactly 0.3");
    assert_ne!(
        0.1_f64 + 0.2_f64,
        0.3_f64,
        "the float this replaces really does drift"
    );
}

#[tokio::test]
async fn a_thousand_cent_additions_reach_exactly_ten() {
    let store = Store::open_memory().await.expect("store");
    let uid = record(&store, "cents").await;

    let cent = DecimalValue::parse_canonical(2, "0.01").expect("0.01");
    for _ in 0..1000 {
        append(&store, &uid, cent).await;
    }

    let level = store::records::quantity(&store.pool, &uid)
        .await
        .expect("query")
        .expect("record exists");
    assert_eq!(level.canonical(), "10.00", "1000 x 0.01 is exactly 10");

    let summed = store::facts::sum_window(&store.pool, &uid, 3600, Utc::now())
        .await
        .expect("sum");
    assert_eq!(summed.canonical(), "10.00");

    let mut float_total = 0.0_f64;
    for _ in 0..1000 {
        float_total += 0.01;
    }
    assert_ne!(
        float_total, 10.0,
        "the float this replaces really does drift"
    );
}

#[tokio::test]
async fn mixed_scales_fold_at_the_finer_scale() {
    let store = Store::open_memory().await.expect("store");
    let uid = record(&store, "scales").await;

    append(&store, &uid, DecimalValue::parse_canonical(0, "5").unwrap()).await;
    append(
        &store,
        &uid,
        DecimalValue::parse_canonical(4, "-0.0001").unwrap(),
    )
    .await;

    let level = store::records::quantity(&store.pool, &uid)
        .await
        .expect("query")
        .expect("record exists");
    assert_eq!(level.canonical(), "4.9999");
    assert_eq!(level.scale(), 4, "the cache takes the finer scale");
}

#[tokio::test]
async fn sign_filtered_windows_stay_exact() {
    let store = Store::open_memory().await.expect("store");
    let uid = record(&store, "signs").await;
    let now = Utc::now();

    for value in ["0.1", "-0.3", "0.2"] {
        append(
            &store,
            &uid,
            DecimalValue::parse_canonical(1, value).unwrap(),
        )
        .await;
    }

    let window = TimeDelta::hours(1).num_seconds();
    let net = store::facts::sum_window(&store.pool, &uid, window, now)
        .await
        .expect("sum");
    let gains = store::facts::sum_pos_window(&store.pool, &uid, window, now)
        .await
        .expect("sum_pos");
    let losses = store::facts::sum_neg_window(&store.pool, &uid, window, now)
        .await
        .expect("sum_neg");

    assert_eq!(net.canonical(), "0.0");
    assert_eq!(gains.canonical(), "0.3");
    assert_eq!(losses.canonical(), "-0.3");
}

#[tokio::test]
async fn no_real_quantity_column_survives() {
    use sqlx::Row;

    let store = Store::open_memory().await.expect("store");
    for (table, columns) in [
        ("fact", vec!["delta_mantissa", "delta_scale"]),
        ("record", vec!["quantity_mantissa", "quantity_scale"]),
    ] {
        let rows = sqlx::query(&format!(
            "SELECT name, type FROM pragma_table_info('{table}')"
        ))
        .fetch_all(&store.pool)
        .await
        .expect("pragma");
        let found: Vec<(String, String)> = rows
            .iter()
            .map(|r| (r.get::<String, _>("name"), r.get::<String, _>("type")))
            .collect();

        for column in columns {
            assert!(
                found.iter().any(|(name, _)| name == column),
                "{table}.{column} missing; found {found:?}"
            );
        }
        assert!(
            !found.iter().any(|(name, ty)| {
                (name == "delta" || name == "quantity") && ty.eq_ignore_ascii_case("REAL")
            }),
            "a REAL quantity column survives on {table}: {found:?}"
        );
    }
}

#[test]
fn the_exact_pair_is_inside_the_hash_preimage() {
    let now = Utc::now();
    let fact = nucleus::fact::seal(
        NewFact::quantity(
            "r_A",
            DecimalValue::parse_canonical(2, "1.50").unwrap(),
            Cause::user_edit(),
        ),
        "genesis",
        now,
    );
    assert!(nucleus::fact::verify_chain_step(&fact));

    let mut mantissa_tampered = fact.clone();
    mantissa_tampered.delta = DecimalValue::parse_canonical(2, "9.99").unwrap();
    assert!(!nucleus::fact::verify_chain_step(&mantissa_tampered));

    let mut scale_tampered = fact.clone();
    scale_tampered.delta = DecimalValue::parse_canonical(1, "1.5").unwrap();
    assert!(!nucleus::fact::verify_chain_step(&scale_tampered));
}
