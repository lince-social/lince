use store::{Store, config};

#[tokio::test]
async fn deletion_preferences_default_to_confirmation_and_are_independent() {
    let store = Store::open_memory().await.unwrap();
    assert_eq!(
        config::deletion_confirmations(&store.pool).await.unwrap(),
        (true, true)
    );
    assert_eq!(
        config::set_deletion_confirmation(&store.pool, false, false)
            .await
            .unwrap(),
        (false, true)
    );
    assert_eq!(
        config::set_deletion_confirmation(&store.pool, true, false)
            .await
            .unwrap(),
        (false, false)
    );
    assert_eq!(
        config::set_deletion_confirmation(&store.pool, false, true)
            .await
            .unwrap(),
        (true, false)
    );
    let saved = config::get(&store.pool).await.unwrap().unwrap();
    assert!(saved.sand_delete_confirmation);
    assert!(!saved.delete_confirmation);
}
