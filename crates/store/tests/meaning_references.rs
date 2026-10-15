use store::Store;

#[tokio::test]
async fn historical_meaning_references_survive_new_definitions_without_equivalence_by_label() {
    let store = Store::open_memory().await.unwrap();
    let vocabulary = store::linguas::create(&store.pool, "Food", None, "private")
        .await
        .unwrap();
    let other = store::linguas::create(&store.pool, "Also food", None, "private")
        .await
        .unwrap();
    let concept = store::concepts::create(&store.pool, "portion", &[])
        .await
        .unwrap();
    store::linguas::adopt(&store.pool, &vocabulary, &concept)
        .await
        .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let original = store::vocabulary::meaning_reference_on(&mut tx, &vocabulary, &concept, 1)
        .await
        .unwrap();
    store::vocabulary::revise_definition_on(&mut tx, &concept, 1, "One measured food portion")
        .await
        .unwrap();
    let previous = store::vocabulary::meaning_reference_on(&mut tx, &vocabulary, &concept, 1)
        .await
        .unwrap();
    let current = store::vocabulary::meaning_reference_on(&mut tx, &vocabulary, &concept, 2)
        .await
        .unwrap();
    assert_eq!(original, previous);
    assert_ne!(original.definition_hash, current.definition_hash);
    assert!(
        store::vocabulary::meaning_reference_on(&mut tx, &other, &concept, 1)
            .await
            .is_err()
    );
    tx.commit().await.unwrap();
}
