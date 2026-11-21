use store::{
    Store,
    concepts::{create as create_concept, create_in},
    linguas::{
        adopt, concept_uids, concepts_for_linguas, create as create_vocabulary,
        lingua_uids_for_concept, linguas_for_concepts, remove_concept,
    },
    sqlx::query,
    vocabulary::{meaning_reference_on, revise_definition_on},
    write_tx,
};

#[tokio::test]
async fn historical_meaning_references_survive_new_definitions_without_equivalence_by_label() {
    let store = Store::open_memory().await.unwrap();
    let pool = &store.pool;
    let vocabulary = create_vocabulary(pool, "Food", None, "private")
        .await
        .unwrap();
    let other = create_vocabulary(pool, "Also food", None, "private")
        .await
        .unwrap();
    let concept = create_concept(pool, "portion", &[]).await.unwrap();
    adopt(pool, &vocabulary, &concept).await.unwrap();
    let mut tx = write_tx(pool).await.unwrap();
    let original = meaning_reference_on(&mut tx, &vocabulary, &concept, 1)
        .await
        .unwrap();
    revise_definition_on(&mut tx, &concept, 1, "One measured food portion")
        .await
        .unwrap();
    let previous = meaning_reference_on(&mut tx, &vocabulary, &concept, 1)
        .await
        .unwrap();
    let current = meaning_reference_on(&mut tx, &vocabulary, &concept, 2)
        .await
        .unwrap();
    assert_eq!(original, previous);
    assert_ne!(original.definition_hash, current.definition_hash);
    assert!(
        meaning_reference_on(&mut tx, &other, &concept, 1)
            .await
            .is_err()
    );
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn singleton_and_batch_memberships_keep_adoption_order_and_handle_removal() {
    let store = Store::open_memory().await.unwrap();
    let pool = &store.pool;
    let primary = create_vocabulary(pool, "Primary", None, "private")
        .await
        .unwrap();
    let secondary = create_vocabulary(pool, "Secondary", None, "private")
        .await
        .unwrap();
    let first = create_in(pool, &primary, "first term", &[]).await.unwrap();
    let second = create_in(pool, &primary, "second term", &[]).await.unwrap();
    adopt(pool, &secondary, &first).await.unwrap();
    query(
        "UPDATE lingua_concept SET adopted_at = CASE
            WHEN lingua_uid = ?1 AND concept_uid = ?2 THEN '2025-02-01T00:00:00Z'
            ELSE '2025-01-01T00:00:00Z' END WHERE lingua_uid IN (?1, ?3)",
    )
    .bind(&primary)
    .bind(&first)
    .bind(&secondary)
    .execute(pool)
    .await
    .unwrap();

    let concepts = concepts_for_linguas(pool, &[&primary, &primary])
        .await
        .unwrap();
    let expected = [second.as_str(), first.as_str()];
    assert_eq!(concept_uids(pool, &primary).await.unwrap(), expected);
    assert_eq!(concepts.get(&primary).unwrap(), &expected);
    let vocabularies = linguas_for_concepts(pool, &[&first]).await.unwrap();
    let expected = [secondary.as_str(), primary.as_str()];
    assert_eq!(
        lingua_uids_for_concept(pool, &first).await.unwrap(),
        expected
    );
    assert_eq!(vocabularies.get(&first).unwrap(), &expected);

    assert!(remove_concept(pool, &secondary, &first).await.unwrap());
    assert!(concept_uids(pool, &secondary).await.unwrap().is_empty());
    assert!(concepts_for_linguas(pool, &[]).await.unwrap().is_empty());
}
