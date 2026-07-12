//! Stage 8b: the Cell's native app tables — configuration singleton + the
//! permission/role/user workflow, seeded idempotently.

use store::Store;

const PERMS: &[(&str, &str)] = &[
    ("record", "create"),
    ("record", "read"),
    ("configuration", "update"),
    ("user", "create"),
];

#[tokio::test]
async fn seed_is_idempotent_and_grants_admin_everything() {
    let store = Store::open_memory().await.unwrap();

    // seeding twice must not error or duplicate.
    store::seed::seed(&store.pool, PERMS).await.unwrap();
    store::seed::seed(&store.pool, PERMS).await.unwrap();

    // configuration singleton exists with the column-DEFAULT policy.
    let config = store::config::get(&store.pool).await.unwrap().unwrap();
    assert_eq!(config.language, "en");
    assert_eq!(config.style, "catppuccin_macchiato");
    assert_eq!(config.delete_confirmation, true);

    // admin role holds EVERY seeded permission, exactly once.
    let mut admin_perms = store::auth::role_permission_keys(&store.pool, "admin")
        .await
        .unwrap();
    admin_perms.sort();
    assert_eq!(
        admin_perms,
        vec![
            "configuration:update".to_string(),
            "record:create".to_string(),
            "record:read".to_string(),
            "user:create".to_string(),
        ]
    );

    // the `lince` role exists but was granted nothing.
    assert!(
        store::auth::role_permission_keys(&store.pool, "lince")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn admin_bootstrap_flips_admin_exists() {
    let store = Store::open_memory().await.unwrap();
    store::seed::seed(&store.pool, PERMS).await.unwrap();

    // fresh store: no admin user yet (first run would prompt).
    assert!(!store::auth::admin_exists(&store.pool).await.unwrap());

    let admin_role = store::auth::ensure_role(&store.pool, store::auth::ADMIN_ROLE)
        .await
        .unwrap();
    store::auth::create_user(&store.pool, "Root", "root", "hash:xyz", admin_role)
        .await
        .unwrap();

    assert!(store::auth::admin_exists(&store.pool).await.unwrap());
}
