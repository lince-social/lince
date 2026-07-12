//! Lingua Part III backend items: unit conversion within a shared dimension
//! and dialect fallback via the parent DAG.

use store::Store;

/// mass dimension: kg and g under @mass; unrelated @count stands alone.
async fn seed_units(store: &Store) -> (String, String, String) {
    let mass = store::concepts::create(&store.pool, "mass", &[])
        .await
        .unwrap();
    let kg = store::concepts::create(&store.pool, "kg", &[mass.as_str()])
        .await
        .unwrap();
    let g = store::concepts::create(&store.pool, "g", &[mass.as_str()])
        .await
        .unwrap();
    (mass, kg, g)
}

#[tokio::test]
async fn conversion_direct_inverse_and_identity() {
    let store = Store::open_memory().await.unwrap();
    let (_mass, kg, g) = seed_units(&store).await;
    store::concepts::set_conversion(&store.pool, &kg, &g, 1000.0)
        .await
        .unwrap();

    // direct: 2 kg -> 2000 g
    assert_eq!(
        store::concepts::convert(&store.pool, &kg, &g, 2.0)
            .await
            .unwrap(),
        Some(2000.0)
    );
    // derived inverse: 500 g -> 0.5 kg
    assert_eq!(
        store::concepts::convert(&store.pool, &g, &kg, 500.0)
            .await
            .unwrap(),
        Some(0.5)
    );
    // identity
    assert_eq!(
        store::concepts::convert(&store.pool, &kg, &kg, 3.25)
            .await
            .unwrap(),
        Some(3.25)
    );
}

#[tokio::test]
async fn conversion_upsert_replaces_and_reverse_row_is_removed() {
    let store = Store::open_memory().await.unwrap();
    let (_mass, kg, g) = seed_units(&store).await;
    store::concepts::set_conversion(&store.pool, &kg, &g, 900.0)
        .await
        .unwrap();
    // declaring the opposite direction becomes the single authoritative row
    store::concepts::set_conversion(&store.pool, &g, &kg, 0.001)
        .await
        .unwrap();
    assert_eq!(
        store::concepts::convert(&store.pool, &kg, &g, 1.0)
            .await
            .unwrap(),
        Some(1000.0)
    );
}

#[tokio::test]
async fn conversion_requires_a_shared_dimension() {
    let store = Store::open_memory().await.unwrap();
    let (_mass, kg, _g) = seed_units(&store).await;
    let count = store::concepts::create(&store.pool, "count", &[])
        .await
        .unwrap();
    // a declared factor across dimensions is still refused
    store::concepts::set_conversion(&store.pool, &kg, &count, 12.0)
        .await
        .unwrap();
    assert_eq!(
        store::concepts::convert(&store.pool, &kg, &count, 1.0)
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn conversion_unknown_pair_is_none_and_bad_factor_rejected() {
    let store = Store::open_memory().await.unwrap();
    let (_mass, kg, g) = seed_units(&store).await;
    assert_eq!(
        store::concepts::convert(&store.pool, &kg, &g, 1.0)
            .await
            .unwrap(),
        None
    );
    assert!(
        store::concepts::set_conversion(&store.pool, &kg, &g, 0.0)
            .await
            .is_err()
    );
    assert!(
        store::concepts::set_conversion(&store.pool, &kg, &g, f64::NAN)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn nearest_ancestor_falls_back_up_the_dag() {
    let store = Store::open_memory().await.unwrap();
    let blocks = store::concepts::create(&store.pool, "blocks", &[])
        .await
        .unwrap();
    let precedes = store::concepts::create(&store.pool, "precedes", &[])
        .await
        .unwrap();
    let blocks_softly = store::concepts::create(&store.pool, "blocks-softly", &[blocks.as_str()])
        .await
        .unwrap();
    let blocks_gently =
        store::concepts::create(&store.pool, "blocks-gently", &[blocks_softly.as_str()])
            .await
            .unwrap();

    let known = vec![blocks.clone(), precedes.clone()];
    // itself when already known
    assert_eq!(
        store::concepts::nearest_ancestor_in(&store.pool, &blocks, &known)
            .await
            .unwrap(),
        Some(blocks.clone())
    );
    // parent
    assert_eq!(
        store::concepts::nearest_ancestor_in(&store.pool, &blocks_softly, &known)
            .await
            .unwrap(),
        Some(blocks.clone())
    );
    // grandparent
    assert_eq!(
        store::concepts::nearest_ancestor_in(&store.pool, &blocks_gently, &known)
            .await
            .unwrap(),
        Some(blocks.clone())
    );
    // miss: no ancestor in the known set
    assert_eq!(
        store::concepts::nearest_ancestor_in(&store.pool, &precedes, &[blocks_softly.clone()])
            .await
            .unwrap(),
        None
    );
}
