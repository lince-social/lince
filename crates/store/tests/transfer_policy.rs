use chrono::{DateTime, Utc};
use nucleus::{
    fact::sha256_hex,
    transfer::application::{EffectMode, PrivateEffect},
};
use serde_json::json;
use store::{
    Store, StoreError, concepts, exact,
    sqlx::{self, SqlitePool},
    transfer_accounting::{Binding, PolicyInput, amount, calculate, effective, policy, set_policy},
};

async fn database() -> Result<Store, StoreError> {
    let store = Store::open_memory().await?;
    sqlx::query(
        "INSERT INTO record (uid, kind, organ_uid, created_at, updated_at)
         SELECT 'owner', 'person', uid, '2026-01-01', '2026-01-01'
         FROM record WHERE slug = 'local-organ'",
    )
    .execute(&store.pool)
    .await?;
    Ok(store)
}

async fn stock(pool: &SqlitePool, uid: &str, unit: Option<&str>) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO record (uid, kind, organ_uid, unit_uid, created_at, updated_at)
         SELECT ?1, 'plain', uid, ?2, '2026-01-01', '2026-01-01'
         FROM record WHERE slug = 'local-organ'",
    )
    .bind(uid)
    .bind(unit)
    .execute(pool)
    .await?;
    Ok(())
}

fn input(formula: &str) -> PolicyInput {
    PolicyInput {
        transfer: "transfer".into(),
        exchange: "exchange".into(),
        person: "owner".into(),
        effects: vec![PrivateEffect {
            record: "stock".into(),
            formula: formula.into(),
            mode: EffectMode::Quantity,
        }],
        expected_version: 0,
        request_id: "request".into(),
    }
}

#[tokio::test]
async fn every_policy_destination_must_be_live_but_existing_requests_replay()
-> Result<(), StoreError> {
    let store = database().await?;
    let pool = &store.pool;
    stock(pool, "stock", None).await?;
    let secondary = "stock-\"é\"";
    stock(pool, secondary, None).await?;
    let set_retired = async |at: Option<&str>| {
        sqlx::query("UPDATE record SET deleted_at = ? WHERE uid = ?")
            .bind(at)
            .bind(secondary)
            .execute(pool)
            .await
    };
    let mut input = input("incoming()");
    input.effects.push(PrivateEffect {
        record: secondary.into(),
        formula: "  incoming() * 2  ".into(),
        mode: EffectMode::Quantity,
    });
    let now = DateTime::<Utc>::UNIX_EPOCH;
    let submit = async |input: PolicyInput, signed: bool| {
        set_policy(pool, input, now, "key", "public", |_| {
            signed.then(|| "signature".into())
        })
        .await
    };

    set_retired(Some("2026-01-02")).await?;
    assert!(
        matches!(submit(input.clone(), true).await, Err(StoreError::Protocol(message))
        if message == "private policies require this Organ's Person and Record")
    );
    assert!(
        policy(pool, "transfer", "exchange", "owner")
            .await?
            .is_none()
    );

    set_retired(None).await?;
    let saved = submit(input.clone(), true).await?;
    assert_eq!(saved.record, "stock");
    assert_eq!(saved.effects[1].formula, "incoming() * 2");
    assert_eq!(saved.version, 1);

    set_retired(Some("2026-01-02")).await?;
    let replayed = submit(input, false).await?;
    assert_eq!(replayed, saved);
    Ok(())
}

#[tokio::test]
async fn unit_conversion_preserves_exact_quantity_and_review_hash() -> Result<(), StoreError> {
    let store = database().await?;
    let pool = &store.pool;
    let mass = concepts::create(pool, "mass", &[]).await?;
    let kg = concepts::create(pool, "kg", &[&mass]).await?;
    let grams = concepts::create(pool, "grams", &[&mass]).await?;
    concepts::set_conversion(pool, &kg, &grams, 1000.0).await?;
    stock(pool, "stock", Some(&grams)).await?;
    let policy = set_policy(
        pool,
        input(" -incoming() / 2 "),
        DateTime::<Utc>::UNIX_EPOCH,
        "key",
        "public",
        |_| Some("signature".into()),
    )
    .await?;
    let binding = Binding {
        transfer: "transfer",
        exchange: "exchange",
        occurrence: None,
        person: "owner",
        record: Some("stock"),
        unit: Some(&kg),
        outgoing: true,
    };
    let result = effective(pool, &binding).await?;
    assert_eq!(result.formula, "-(incoming() * 1000 / 1)/2");
    let review = json!({
        "formula": result.formula,
        "policy": policy.hash,
        "record": "stock",
        "canonical_unit": kg,
        "local_unit": grams,
    });
    assert_eq!(
        result.formula_hash,
        sha256_hex(review.to_string().as_bytes())
    );
    assert_eq!(
        calculate(&result.formula, amount(0.3)?, exact::zero())?
            .0
            .to_string(),
        "-150"
    );
    Ok(())
}
