use store::{
    Store,
    config::{self, InterfaceStorage},
};

#[tokio::test]
async fn interface_storage_defaults_updates_and_invalid_values() {
    let store = Store::open_memory().await.unwrap();
    assert_eq!(
        config::interface_storage(&store.pool).await.unwrap(),
        InterfaceStorage::default()
    );
    let settings = InterfaceStorage {
        snapshot_seconds: 60,
        history_seconds: 900,
        history_count: 4,
    };
    config::set_interface_storage(&store.pool, settings)
        .await
        .unwrap();
    assert_eq!(
        config::interface_storage(&store.pool).await.unwrap(),
        settings
    );
    assert_eq!(
        config::get(&store.pool)
            .await
            .unwrap()
            .unwrap()
            .interface_storage,
        settings
    );
    assert!(config::interface_close_suspends(&store.pool).await.unwrap());
    config::set_interface_close_suspends(&store.pool, false)
        .await
        .unwrap();
    assert!(!config::interface_close_suspends(&store.pool).await.unwrap());
    assert!(
        !config::get(&store.pool)
            .await
            .unwrap()
            .unwrap()
            .interface_close_suspends
    );
    for invalid in [
        InterfaceStorage {
            snapshot_seconds: 0,
            ..settings
        },
        InterfaceStorage {
            history_seconds: 59,
            ..settings
        },
        InterfaceStorage {
            history_seconds: u64::MAX,
            ..settings
        },
        InterfaceStorage {
            history_count: 0,
            ..settings
        },
        InterfaceStorage {
            history_count: 101,
            ..settings
        },
    ] {
        assert!(
            config::set_interface_storage(&store.pool, invalid)
                .await
                .is_err()
        );
        assert_eq!(
            config::interface_storage(&store.pool).await.unwrap(),
            settings
        );
    }
    assert!(
        sqlx::query("UPDATE configuration SET interface_snapshot_seconds = 0 WHERE id = 1")
            .execute(&store.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE configuration SET interface_snapshot_seconds = 901 WHERE id = 1")
            .execute(&store.pool)
            .await
            .is_err()
    );
}
