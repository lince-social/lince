#![cfg(test)]

use store::{
    Store,
    concepts::{
        add_parent, ancestors_including, convert, create, descendants_including,
        nearest_ancestor_in, set_conversion,
    },
};

async fn seed_units(store: &Store) -> (String, String) {
    let mass = create(&store.pool, "mass", &[]).await.unwrap();
    let kg = create(&store.pool, "kg", &[mass.as_str()]).await.unwrap();
    let g = create(&store.pool, "g", &[mass.as_str()]).await.unwrap();
    (kg, g)
}

#[tokio::test]
async fn conversion_direct_inverse_and_identity() {
    let store = Store::open_memory().await.unwrap();
    let (kg, g) = seed_units(&store).await;
    set_conversion(&store.pool, &kg, &g, 1000.0).await.unwrap();

    assert_eq!(
        convert(&store.pool, &kg, &g, 2.0).await.unwrap(),
        Some(2000.0)
    );
    assert_eq!(
        convert(&store.pool, &g, &kg, 500.0).await.unwrap(),
        Some(0.5)
    );
    assert_eq!(
        convert(&store.pool, &kg, &kg, 3.25).await.unwrap(),
        Some(3.25)
    );
}

#[tokio::test]
async fn conversion_upsert_replaces_and_reverse_row_is_removed() {
    let store = Store::open_memory().await.unwrap();
    let (kg, g) = seed_units(&store).await;
    set_conversion(&store.pool, &kg, &g, 900.0).await.unwrap();
    set_conversion(&store.pool, &g, &kg, 0.001).await.unwrap();
    assert_eq!(
        convert(&store.pool, &kg, &g, 1.0).await.unwrap(),
        Some(1000.0)
    );
}

#[tokio::test]
async fn conversion_requires_a_shared_dimension() {
    let store = Store::open_memory().await.unwrap();
    let (kg, _) = seed_units(&store).await;
    let count = create(&store.pool, "count", &[]).await.unwrap();
    set_conversion(&store.pool, &kg, &count, 12.0)
        .await
        .unwrap();
    assert_eq!(convert(&store.pool, &kg, &count, 1.0).await.unwrap(), None);
}

#[tokio::test]
async fn conversion_unknown_pair_is_none_and_bad_factor_rejected() {
    let store = Store::open_memory().await.unwrap();
    let (kg, g) = seed_units(&store).await;
    assert_eq!(convert(&store.pool, &kg, &g, 1.0).await.unwrap(), None);
    assert!(set_conversion(&store.pool, &kg, &g, 0.0).await.is_err());
    assert!(
        set_conversion(&store.pool, &kg, &g, f64::NAN)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn nearest_ancestor_falls_back_up_the_dag() {
    let store = Store::open_memory().await.unwrap();
    let blocks = create(&store.pool, "blocks", &[]).await.unwrap();
    let precedes = create(&store.pool, "precedes", &[]).await.unwrap();
    let blocks_softly = create(&store.pool, "blocks-softly", &[blocks.as_str()])
        .await
        .unwrap();
    let blocks_gently = create(&store.pool, "blocks-gently", &[blocks_softly.as_str()])
        .await
        .unwrap();

    let known = vec![blocks.clone(), precedes.clone()];
    assert_eq!(
        nearest_ancestor_in(&store.pool, &blocks, &known)
            .await
            .unwrap()
            .as_deref(),
        Some(blocks.as_str())
    );
    assert_eq!(
        nearest_ancestor_in(&store.pool, &blocks_softly, &known)
            .await
            .unwrap()
            .as_deref(),
        Some(blocks.as_str())
    );
    assert_eq!(
        nearest_ancestor_in(&store.pool, &blocks_gently, &known)
            .await
            .unwrap()
            .as_deref(),
        Some(blocks.as_str())
    );
    assert_eq!(
        nearest_ancestor_in(&store.pool, &precedes, &[blocks_softly])
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn diamond_hierarchy_is_breadth_first_and_rejects_cycles() {
    let store = Store::open_memory().await.unwrap();
    let pool = &store.pool;
    let root = create(pool, "root", &[]).await.unwrap();
    let left = create(pool, "left", &[&root]).await.unwrap();
    let right = create(pool, "right", &[&root]).await.unwrap();
    let leaf = create(pool, "leaf", &[&left, &right]).await.unwrap();
    let mut middle = [left.as_str(), right.as_str()];
    middle.sort_unstable();
    let [first, second] = middle;

    assert!(add_parent(pool, &root, &leaf).await.is_err());
    assert_eq!(
        descendants_including(pool, &root).await.unwrap(),
        [root.as_str(), first, second, leaf.as_str()]
    );
    assert_eq!(
        ancestors_including(pool, &leaf).await.unwrap(),
        [leaf.as_str(), first, second, root.as_str()]
    );
}
