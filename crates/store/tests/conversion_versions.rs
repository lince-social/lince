use store::Store;

async fn convert_one(store: &Store, from: &str, to: &str) -> Option<nucleus::DecimalValue> {
    store::concepts::convert_exact(
        &store.pool,
        from,
        to,
        nucleus::DecimalValue::from_mantissa(0, 1).unwrap(),
        0,
        nucleus::karma::Rounding::TowardZero,
    )
    .await
    .unwrap()
    .map(|result| result.value)
}

#[tokio::test]
async fn conversion_history_pins_units_ratios_and_withdrawals_without_rewriting_receipts() {
    let store = Store::open_memory().await.unwrap();
    let vocabulary = store::linguas::create(&store.pool, "Units", None, "private")
        .await
        .unwrap();
    let gram = store::concepts::create_in(&store.pool, &vocabulary, "gram", &[])
        .await
        .unwrap();
    let kilo = store::concepts::create_in(&store.pool, &vocabulary, "kilogram", &[&gram])
        .await
        .unwrap();
    store::concepts::set_conversion_exact(&store.pool, &kilo, &gram, 1000, 1)
        .await
        .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let original = store::vocabulary::conversion_reference_on(
        &mut tx,
        &vocabulary,
        &kilo,
        &vocabulary,
        &gram,
        1,
    )
    .await
    .unwrap();
    store::vocabulary::revise_definition_on(&mut tx, &gram, 1, "A gram of a measured substance")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(convert_one(&store, &kilo, &gram).await.is_none());
    store::concepts::set_conversion_exact(&store.pool, &kilo, &gram, 999, 1)
        .await
        .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let previous = store::vocabulary::conversion_reference_on(
        &mut tx,
        &vocabulary,
        &kilo,
        &vocabulary,
        &gram,
        1,
    )
    .await
    .unwrap();
    let changed = store::vocabulary::conversion_reference_on(
        &mut tx,
        &vocabulary,
        &kilo,
        &vocabulary,
        &gram,
        2,
    )
    .await
    .unwrap();
    assert_eq!(original, previous);
    assert_eq!(previous.to.revision, 1);
    assert_eq!(changed.to.revision, 2);
    assert_eq!(changed.numerator, "999");
    assert_ne!(previous.hash, changed.hash);
    original.validate().unwrap();
    assert!(store::sqlx::query("UPDATE concept_conversion_version SET numerator='1' WHERE a_uid=? AND b_uid=? AND revision=1").bind(&kilo).bind(&gram).execute(&mut *tx).await.is_err());
    assert!(
        store::sqlx::query("DELETE FROM concept_conversion_version WHERE a_uid=? AND b_uid=?")
            .bind(&kilo)
            .bind(&gram)
            .execute(&mut *tx)
            .await
            .is_err()
    );
    tx.commit().await.unwrap();
    store::concepts::set_conversion_exact(&store.pool, &gram, &kilo, 1, 1000)
        .await
        .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let withdrawn = store::vocabulary::conversion_reference_on(
        &mut tx,
        &vocabulary,
        &kilo,
        &vocabulary,
        &gram,
        3,
    )
    .await
    .unwrap();
    assert!(withdrawn.withdrawn);
    assert_eq!(withdrawn.numerator, "999");
    let mut tampered = original.clone();
    tampered.numerator = "1".into();
    assert!(tampered.validate().is_err());
    let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM concept_conversion_version")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    store::concepts::set_conversion_exact(&store.pool, &gram, &kilo, 1, 1000)
        .await
        .unwrap();
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM concept_conversion_version")
            .fetch_one(&store.pool)
            .await
            .unwrap(),
        count
    );
}

#[tokio::test]
async fn reapproving_the_same_ratio_binds_new_definitions_and_keys_cannot_be_renamed() {
    let store = Store::open_memory().await.unwrap();
    let vocabulary = store::linguas::create(&store.pool, "Versioned units", None, "private")
        .await
        .unwrap();
    let small = store::concepts::create_in(&store.pool, &vocabulary, "small", &[])
        .await
        .unwrap();
    let large = store::concepts::create_in(&store.pool, &vocabulary, "large", &[&small])
        .await
        .unwrap();
    store::concepts::set_conversion_exact(&store.pool, &large, &small, 1000, 1)
        .await
        .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::vocabulary::revise_definition_on(&mut tx, &small, 1, "The revised unit")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(convert_one(&store, &large, &small).await.is_none());
    store::concepts::set_conversion_exact(&store.pool, &large, &small, 1000, 1)
        .await
        .unwrap();
    assert_eq!(
        convert_one(&store, &large, &small).await,
        Some(nucleus::DecimalValue::from_mantissa(0, 1000).unwrap())
    );
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let reference = store::vocabulary::conversion_reference_on(
        &mut tx,
        &vocabulary,
        &large,
        &vocabulary,
        &small,
        2,
    )
    .await
    .unwrap();
    assert_eq!(reference.to.revision, 2);
    assert!(
        store::sqlx::query(
            "UPDATE concept_conversion SET a_uid=b_uid,b_uid=a_uid WHERE a_uid=? AND b_uid=?"
        )
        .bind(&large)
        .bind(&small)
        .execute(&mut *tx)
        .await
        .is_err()
    );
    tx.commit().await.unwrap();
}
