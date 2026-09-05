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

    store::seed::seed(&store.pool, PERMS).await.unwrap();
    store::seed::seed(&store.pool, PERMS).await.unwrap();

    let config = store::config::get(&store.pool).await.unwrap().unwrap();
    assert_eq!(config.language, "en");
    assert_eq!(config.style, "catppuccin_macchiato");
    assert_eq!(config.delete_confirmation, true);

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

    assert!(!store::auth::admin_exists(&store.pool).await.unwrap());

    let admin_role = store::auth::ensure_role(&store.pool, store::auth::ADMIN_ROLE)
        .await
        .unwrap();
    store::auth::create_person_login(&store.pool, "Root", "root", "hash:xyz", admin_role)
        .await
        .unwrap();

    assert!(store::auth::admin_exists(&store.pool).await.unwrap());
}

#[tokio::test]
async fn you_can_see_what_you_made_without_sharing_it_with_yourself() {
    let store = store::Store::open("sqlite::memory:").await.unwrap();
    let me = "person-me";
    let someone_else = "person-else";
    let mine = store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Plain,
            head: "My own note",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO fact
            (uid, record_uid, delta_mantissa, delta_scale, at, actor_uid,
             cause_kind, prev_hash, hash)
         VALUES (?, ?, '0', 0, datetime('now'), ?, 'user_edit', '', 'h')",
    )
    .bind("f-mine")
    .bind(&mine.uid)
    .bind(me)
    .execute(&store.pool)
    .await
    .unwrap();

    let visible = store::visibility::visible_targets(&store.pool, me)
        .await
        .unwrap();
    assert!(
        visible.contains(&mine.uid),
        "a creator must see their own record without an explicit grant",
    );

    let theirs = store::visibility::visible_targets(&store.pool, someone_else)
        .await
        .unwrap();
    assert!(
        !theirs.contains(&mine.uid),
        "but only the creator — this must not leak it to everyone",
    );
}
