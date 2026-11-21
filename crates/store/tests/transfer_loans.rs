use nucleus::{
    execution::Execution,
    transfer::{
        disclosure::TransferItem,
        exchange::ExchangeRoute,
        loans::{Interval, Reference},
    },
};
use serde_json::{Value, json};
use store::{
    Store, StoreError, exact,
    sqlx::{self, SqlitePool},
    transfer_loans::{Extension, accepted, adjustments, extend, terms, validate_link},
};

async fn deliver(pool: &SqlitePool, uid: &str, projection: Value) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO transfer_remote_reference
            (uid, origin_organ_uid, transfer_uid, delivery_policy_uid, recipient_person_uid,
             recipient_organ_uid, projection, created_at, updated_at)
         SELECT ?1, 'origin', ?1, 'policy', 'borrower', uid, ?2, 'fixture', 'fixture'
         FROM record WHERE slug = 'local-organ'",
    )
    .bind(uid)
    .bind(projection.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

#[tokio::test]
async fn simulated_loan_keeps_exact_receipt_totals_and_linked_physical_returns()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    sqlx::raw_sql(
        "INSERT INTO record (uid, kind, organ_uid, quantity_mantissa, created_at, updated_at)
         SELECT 'stock', 'plain', uid, '5', 'fixture', 'fixture' FROM record WHERE slug = 'local-organ';
         CREATE TABLE simulation_assumed_transfer_effect (
             transfer_uid TEXT, exchange_uid TEXT, person_uid TEXT, occurrence_uid TEXT,
             record_uid TEXT, local_mantissa TEXT, local_scale INTEGER,
             cumulative_mantissa TEXT, cumulative_scale INTEGER, mode TEXT, unit_uid TEXT
         );
         CREATE TABLE simulation_loan_timing (
             origin TEXT, transfer_uid TEXT, exchange_uid TEXT, person_uid TEXT,
             until_ms INTEGER, returned_mantissa TEXT, returned_scale INTEGER
         );
         INSERT INTO simulation_assumed_transfer_effect VALUES
             ('loan', 'loan-exchange', 'borrower', 'first', 'stock', '2', 0, '1', 0, 'quantity', NULL),
             ('loan', 'loan-exchange', 'borrower', 'first', 'stock', '1', 0, '2', 0, 'quantity', NULL),
             ('loan', 'loan-exchange', 'borrower', 'second', 'stock', '3', 0, '1', 0, 'quantity', NULL),
             ('loan', 'loan-exchange', 'lender', 'first', 'stock', '0', 0, '4', 0, 'quantity', NULL),
             ('return', 'return-exchange', 'borrower', 'return', 'stock', '-1', 0, '1', 0, 'quantity', NULL);
         INSERT INTO simulation_loan_timing VALUES ('origin', 'loan', 'loan-exchange', 'borrower', 4000, '2', 0);",
    )
    .execute(pool)
    .await?;
    deliver(pool, "loan", json!({
        "promises": [{"uid":"loan-promise","exchange":"loan-exchange","giver":"lender","receiver":"borrower",
            "loan":{"from":"1970-01-01T00:00:01Z","until":"1970-01-01T00:00:03Z"}}],
    })).await?;
    deliver(pool, "return", json!({
        "promises": [{"uid":"return-promise","exchange":"return-exchange","giver":"borrower","receiver":"lender",
            "return_of":{"origin":"origin","transfer":"loan","exchange":"loan-exchange"}}],
        "occurrences": [
            {"promise":"return-promise","settlement_progress":{"settled_quantity":1}},
            {"promise":"return-promise","disputed":true,"settlement_progress":{"settled_quantity":400}},
            {"promise":"unlinked","settlement_progress":{"settled_quantity":400}}
        ],
    })).await?;
    assert!(adjustments(pool, 2000).await?.is_empty());
    let scope = Execution::new([7; 32], 2000)?;
    for (at, expected, next) in [
        (999, -5, Some(1000)),
        (1000, -2, Some(4000)),
        (3999, -2, Some(4000)),
        (4000, -5, None),
    ] {
        let rows = scope.scope(adjustments(pool, at)).await?;
        let [stock] = rows.as_slice() else {
            panic!("expected a single stock adjustment at {at}");
        };
        assert_eq!(stock.record, "stock");
        assert!(
            stock
                .delta
                .exact_numeric_cmp(exact::integer(expected))
                .is_eq(),
            "{stock:?} at {at}"
        );
        assert_eq!(stock.next_ms, next);
        assert!(!stock.unit_changed);
    }
    let unit = store::concepts::create(pool, "changed-unit", &[]).await?;
    sqlx::query("UPDATE record SET unit_uid = ? WHERE uid = 'stock'")
        .bind(unit)
        .execute(pool)
        .await?;
    let changed = scope.scope(adjustments(pool, 2000)).await?;
    let [stock] = changed.as_slice() else {
        panic!("expected changed stock adjustment");
    };
    assert!(stock.unit_changed && stock.delta.is_zero());

    let delivered = terms(pool, false).await?;
    let linked = delivered
        .iter()
        .find(|term| term.transfer == "return")
        .expect("delivered return");
    assert!(linked.settled.exact_numeric_cmp(exact::integer(1)).is_eq());
    sqlx::query("UPDATE transfer_remote_reference SET state = 'revoked' WHERE uid = 'return'")
        .execute(pool)
        .await?;
    assert!(
        !terms(pool, false)
            .await?
            .iter()
            .any(|term| term.transfer == "return")
    );
    assert!(
        terms(pool, true)
            .await?
            .iter()
            .any(|term| term.transfer == "return" && term.retained)
    );
    Ok(())
}

#[tokio::test]
async fn extension_replays_its_fact_without_changing_the_accepted_end()
-> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    let origin: String = sqlx::query_scalar("SELECT uid FROM record WHERE slug = 'local-organ'")
        .fetch_one(pool)
        .await?;
    sqlx::query(
        "INSERT INTO record (uid, kind, organ_uid, created_at, updated_at)
         VALUES ('transfer', 'transfer', ?1, 'fixture', 'fixture'),
                ('giver', 'person', ?1, 'fixture', 'fixture'),
                ('receiver', 'person', ?1, 'fixture', 'fixture')",
    )
    .bind(&origin)
    .execute(pool)
    .await?;
    sqlx::raw_sql(
        "INSERT INTO transfer (record_uid, revision) VALUES ('transfer', 1);
         INSERT INTO transfer_party (uid, transfer_uid, actor_uid)
         VALUES ('giver-party', 'transfer', 'giver'), ('receiver-party', 'transfer', 'receiver');
         INSERT INTO promise (uid, record_uid, delta, transfer_uid, created_at, updated_at)
         VALUES ('promise', 'transfer', 1, 'transfer', 'fixture', 'fixture');",
    )
    .execute(pool)
    .await?;
    let interval = Interval {
        from: "2026-01-01T00:00:00Z".into(),
        until: "2026-01-02T00:00:00Z".into(),
    };
    let (from, until) = interval.bounds()?;
    let item = TransferItem {
        title: "Loan".into(),
        exchange: Some(ExchangeRoute {
            uid: "loan".into(),
            giver: "giver".into(),
            receiver: "receiver".into(),
        }),
        loan: Some(interval),
        ..Default::default()
    };
    sqlx::query("UPDATE promise SET item_json = ? WHERE uid = 'promise'")
        .bind(serde_json::to_string(&item)?)
        .execute(pool)
        .await?;
    sqlx::query(
        "INSERT INTO fact (uid, record_uid, delta_mantissa, delta_scale, at, cause_kind, prev_hash, hash)
         VALUES ('accepted', 'transfer', '0', 0, '2026-01-01T00:00:00Z', 'user_edit', '', 'accepted')",
    )
    .execute(pool).await?;
    sqlx::query(
        "INSERT INTO transfer_loan_agreement
           (transfer_uid, exchange_uid, revision, from_ms, until_ms, agreement_fact_uid, created_at)
         VALUES ('transfer', 'loan', 1, ?, ?, 'accepted', 'fixture')",
    )
    .bind(from)
    .bind(until)
    .execute(pool)
    .await?;
    let linked = TransferItem {
        title: "Return".into(),
        exchange: Some(ExchangeRoute {
            uid: "return".into(),
            giver: "receiver".into(),
            receiver: "giver".into(),
        }),
        return_of: Some(Reference {
            origin,
            transfer: "transfer".into(),
            exchange: "loan".into(),
        }),
        ..Default::default()
    };
    validate_link(pool, &linked, "receiver").await?;

    let input = Extension {
        transfer: "transfer".into(),
        exchange: "loan".into(),
        until: "2026-01-03T00:00:00Z".into(),
        expected_revision: 1,
        person: "receiver".into(),
        request_id: "extend".into(),
        authorization_intent_uid: None,
    };
    let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T01:00:00Z")?.to_utc();
    let outcome = extend(pool, input.clone(), now, |_| Some("signed".into())).await?;
    let replay = extend(pool, input, now, |_| None).await?;
    assert_eq!(outcome.uid, replay.uid);
    assert!(outcome.fact.is_some() && replay.fact.is_none());
    let agreements = accepted(pool, "transfer").await?;
    let [agreement] = agreements.as_slice() else {
        panic!("expected one accepted loan");
    };
    assert_eq!(agreement.until_ms, until);
    Ok(())
}
