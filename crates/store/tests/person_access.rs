use std::borrow::Cow;
use std::str::FromStr;
use std::sync::Arc;

use store::Store;

async fn record(store: &Store, kind: nucleus::RecordKind, head: &str) -> String {
    store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind,
            head,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid
}

async fn person(store: &Store, head: &str) -> String {
    record(store, nucleus::RecordKind::Person, head).await
}

#[tokio::test]
async fn a_credential_free_person_can_receive_authority() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "Credential free").await;
    let role = store::auth::ensure_role(&store.pool, "worker")
        .await
        .unwrap();

    assert!(
        !store::auth::has_credential(&store.pool, &uid)
            .await
            .unwrap()
    );
    assert_eq!(
        store::auth::person_access(&store.pool, &uid).await.unwrap(),
        None
    );

    let access = store::auth::compare_and_set_role(&store.pool, &uid, Some(role), 0)
        .await
        .unwrap();
    assert_eq!(access.role_id, Some(role));
    assert_eq!(access.read_filter, None);
    assert_eq!(access.revision, 1);
    assert!(
        !store::auth::has_credential(&store.pool, &uid)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn a_credential_free_admin_prevents_bootstrap_replacement() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "Credential-free admin").await;
    let admin = store::auth::ensure_role(&store.pool, store::auth::ADMIN_ROLE)
        .await
        .unwrap();
    store::auth::compare_and_set_role(&store.pool, &uid, Some(admin), 0)
        .await
        .unwrap();

    assert!(store::auth::admin_exists(&store.pool).await.unwrap());
    assert_eq!(
        store::auth::admins(&store.pool).await.unwrap(),
        vec![uid.clone()]
    );
    assert!(
        !store::auth::has_credential(&store.pool, &uid)
            .await
            .unwrap()
    );
    assert!(
        store::auth::list_users(&store.pool)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn the_migrated_schema_has_one_authority_home() {
    let store = Store::open_memory().await.unwrap();
    let credential_columns = store::sqlx::query_scalar::<_, String>(
        "SELECT name FROM pragma_table_info('person_credential') ORDER BY cid",
    )
    .fetch_all(&store.pool)
    .await
    .unwrap();
    let access_columns = store::sqlx::query_scalar::<_, String>(
        "SELECT name FROM pragma_table_info('person_access') ORDER BY cid",
    )
    .fetch_all(&store.pool)
    .await
    .unwrap();

    assert!(!credential_columns.contains(&"role_id".to_string()));
    assert!(!credential_columns.contains(&"read_filter".to_string()));
    assert_eq!(
        access_columns,
        vec!["person_uid", "role_id", "read_filter", "revision"]
    );
}

#[tokio::test]
async fn migration_backfills_only_valid_person_authority_and_preserves_credentials() {
    let all = store::sqlx::migrate!("./migrations");
    let before_access = store::sqlx::migrate::Migrator {
        migrations: Cow::Owned(
            all.iter()
                .filter(|migration| migration.version <= 76)
                .cloned()
                .collect(),
        ),
        ..store::sqlx::migrate::Migrator::DEFAULT
    };
    let options = store::sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true);
    let pool = store::sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    before_access.run(&pool).await.unwrap();
    let store = Store { pool };
    store::organs::ensure_local(&store.pool, "").await.unwrap();
    let role = store::auth::ensure_role(&store.pool, "migrated")
        .await
        .unwrap();
    let valid = person(&store, "Valid legacy Person").await;
    let deleted = person(&store, "Deleted legacy Person").await;
    let plain = record(&store, nucleus::RecordKind::Plain, "Legacy plain").await;
    store::records::mark_deleted(&store.pool, &deleted)
        .await
        .unwrap();

    for (uid, username, filter) in [
        (&valid, "valid", "kind:plain"),
        (&deleted, "deleted", "kind:person"),
        (&plain, "plain", "kind:organ"),
    ] {
        store::sqlx::query(
            "INSERT INTO person_credential
                (person_uid, username, password_hash, role_id, read_filter)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(uid)
        .bind(username)
        .bind(format!("{username}-hash"))
        .bind(role)
        .bind(filter)
        .execute(&store.pool)
        .await
        .unwrap();
    }

    all.run(&store.pool).await.unwrap();

    let access = store::auth::person_access(&store.pool, &valid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(access.role_id, Some(role));
    assert_eq!(access.read_filter.as_deref(), Some("kind:plain"));
    assert_eq!(access.revision, 1);
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM person_access WHERE person_uid IN (?, ?)",
        )
        .bind(&deleted)
        .bind(&plain)
        .fetch_one(&store.pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        store::auth::person_access(&store.pool, &deleted)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        store::auth::person_access(&store.pool, &plain)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, String>(
            "SELECT name FROM pragma_table_info('person_credential') ORDER BY cid",
        )
        .fetch_all(&store.pool)
        .await
        .unwrap(),
        vec![
            "person_uid",
            "username",
            "password_hash",
            "created_at",
            "updated_at"
        ]
    );
    assert_eq!(
        store::sqlx::query_as::<_, (String, String, String)>(
            "SELECT person_uid, username, password_hash
               FROM person_credential
              ORDER BY username",
        )
        .fetch_all(&store.pool)
        .await
        .unwrap(),
        vec![
            (deleted, "deleted".into(), "deleted-hash".into()),
            (plain, "plain".into(), "plain-hash".into()),
            (valid, "valid".into(), "valid-hash".into()),
        ]
    );
}

#[tokio::test]
async fn a_deactivated_person_can_receive_an_administrative_assignment() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "Inactive").await;
    let role = store::auth::ensure_role(&store.pool, "worker")
        .await
        .unwrap();
    store::people::deactivate(&store.pool, &uid, "2026-09-06T00:00:00Z", None)
        .await
        .unwrap();

    assert!(!store::people::is_active(&store.pool, &uid).await.unwrap());
    assert_eq!(
        store::auth::compare_and_set_role(&store.pool, &uid, Some(role), 0)
            .await
            .unwrap()
            .role_id,
        Some(role)
    );
}

#[tokio::test]
async fn replacing_a_password_preserves_established_authority() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "Reset").await;
    let established = store::auth::ensure_role(&store.pool, "established")
        .await
        .unwrap();
    let replacement_argument = store::auth::ensure_role(&store.pool, "replacement-argument")
        .await
        .unwrap();

    store::auth::create_credential(&store.pool, &uid, "reset", "old-hash", established)
        .await
        .unwrap();
    let access = store::read_filter::compare_and_set(&store.pool, &uid, Some("kind:plain"), 1)
        .await
        .unwrap();
    store::auth::create_credential(&store.pool, &uid, "reset", "new-hash", replacement_argument)
        .await
        .unwrap();

    assert_eq!(
        store::auth::person_access(&store.pool, &uid)
            .await
            .unwrap()
            .unwrap(),
        access
    );
    let user = store::auth::user_by_username(&store.pool, "reset")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(user.password_hash, "new-hash");
    assert_eq!(user.role_id, established);
}

#[tokio::test]
async fn a_failed_credential_insert_rolls_back_its_initial_assignment() {
    let store = Store::open_memory().await.unwrap();
    let role = store::auth::ensure_role(&store.pool, "worker")
        .await
        .unwrap();
    let first = person(&store, "First").await;
    let refused = person(&store, "Refused").await;
    store::auth::create_credential(&store.pool, &first, "taken", "hash", role)
        .await
        .unwrap();

    assert!(
        store::auth::create_credential(&store.pool, &refused, "taken", "hash", role)
            .await
            .is_err()
    );
    assert_eq!(
        store::auth::person_access(&store.pool, &refused)
            .await
            .unwrap(),
        None
    );
    assert!(
        !store::auth::has_credential(&store.pool, &refused)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn invalid_person_and_role_assignments_are_refused() {
    let store = Store::open_memory().await.unwrap();
    let role = store::auth::ensure_role(&store.pool, "worker")
        .await
        .unwrap();
    let deleted = person(&store, "Deleted").await;
    let plain = record(&store, nucleus::RecordKind::Plain, "Plain").await;
    store::records::mark_deleted(&store.pool, &deleted)
        .await
        .unwrap();

    for uid in [nucleus::new_uid("r"), deleted, plain] {
        assert!(
            store::auth::compare_and_set_role(&store.pool, &uid, Some(role), 0)
                .await
                .is_err(),
            "invalid Person `{uid}` received access"
        );
        assert_eq!(
            store::auth::person_access(&store.pool, &uid).await.unwrap(),
            None
        );
    }

    let valid = person(&store, "Valid").await;
    assert!(
        store::auth::compare_and_set_role(&store.pool, &valid, Some(i64::MAX), 0)
            .await
            .is_err()
    );
    assert_eq!(
        store::auth::person_access(&store.pool, &valid)
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn an_assigned_role_cannot_leave_a_dangling_foreign_key() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "Assigned").await;
    let role = store::auth::ensure_role(&store.pool, "assigned")
        .await
        .unwrap();
    store::auth::compare_and_set_role(&store.pool, &uid, Some(role), 0)
        .await
        .unwrap();

    assert!(
        store::sqlx::query("DELETE FROM role WHERE id = ?")
            .bind(role)
            .execute(&store.pool)
            .await
            .is_err()
    );
    assert_eq!(
        store::auth::person_access(&store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .role_id,
        Some(role)
    );
}

#[tokio::test]
async fn deleting_a_person_hides_their_existing_authority() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "Former").await;
    let role = store::auth::ensure_role(&store.pool, "former")
        .await
        .unwrap();
    store::auth::compare_and_set_role(&store.pool, &uid, Some(role), 0)
        .await
        .unwrap();

    store::records::mark_deleted(&store.pool, &uid)
        .await
        .unwrap();

    assert_eq!(
        store::auth::person_access(&store.pool, &uid).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn a_person_without_a_role_has_no_operation_permissions() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "No role").await;
    let role = store::auth::ensure_role(&store.pool, "temporary")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&store.pool, role, permission)
        .await
        .unwrap();
    store::auth::create_credential(&store.pool, &uid, "no-role", "hash", role)
        .await
        .unwrap();
    store::auth::compare_and_set_role(&store.pool, &uid, None, 1)
        .await
        .unwrap();

    let user = store::auth::user_by_uid(&store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(user.role_id, 0);
    assert_eq!(user.role, "");
    assert!(user.permissions.is_empty());
}

#[tokio::test]
async fn narrowing_filters_are_independent_of_credentials_and_each_other() {
    let store = Store::open_memory().await.unwrap();
    let first = person(&store, "First").await;
    let second = person(&store, "Second").await;

    let first_access =
        store::read_filter::compare_and_set(&store.pool, &first, Some("kind:plain"), 0)
            .await
            .unwrap();
    let second_access =
        store::read_filter::compare_and_set(&store.pool, &second, Some("kind:person"), 0)
            .await
            .unwrap();

    assert_eq!(first_access.role_id, None);
    assert_eq!(second_access.role_id, None);
    assert_eq!(
        store::read_filter::get(&store.pool, &first)
            .await
            .unwrap()
            .as_deref(),
        Some("kind:plain")
    );
    assert_eq!(
        store::read_filter::get(&store.pool, &second)
            .await
            .unwrap()
            .as_deref(),
        Some("kind:person")
    );
    assert_eq!(
        store::read_filter::everyone(&store.pool)
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn stale_access_revisions_conflict_without_changing_authority() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "Concurrent").await;
    let first = store::auth::ensure_role(&store.pool, "first")
        .await
        .unwrap();
    let second = store::auth::ensure_role(&store.pool, "second")
        .await
        .unwrap();
    store::auth::compare_and_set_role(&store.pool, &uid, Some(first), 0)
        .await
        .unwrap();
    let narrowed = store::read_filter::compare_and_set(&store.pool, &uid, Some("kind:plain"), 1)
        .await
        .unwrap();

    let error = store::auth::compare_and_set_role(&store.pool, &uid, Some(second), 1)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("revision conflict"));
    assert_eq!(
        store::auth::person_access(&store.pool, &uid)
            .await
            .unwrap()
            .unwrap(),
        narrowed
    );
}

#[tokio::test]
async fn caller_transaction_reads_and_writes_roll_back_together() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "Rollback").await;
    let role = store::auth::ensure_role(&store.pool, "worker")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&store.pool, role, permission)
        .await
        .unwrap();

    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let access = store::auth::compare_and_set_role_on(&mut tx, &uid, Some(role), 0)
        .await
        .unwrap();
    assert_eq!(
        store::auth::person_access_on(&mut tx, &uid).await.unwrap(),
        Some(access)
    );
    assert_eq!(
        store::auth::role_permission_keys_by_id_on(&mut tx, role)
            .await
            .unwrap(),
        vec!["record:read"]
    );
    assert!(store::people::is_active_on(&mut tx, &uid).await.unwrap());
    tx.rollback().await.unwrap();

    assert_eq!(
        store::auth::person_access(&store.pool, &uid).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn caller_transaction_active_check_refuses_corrupt_standing() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "Corrupt").await;
    store::records::set_extension_raw(
        &store.pool,
        &uid,
        store::people::NAMESPACE,
        &serde_json::json!({ store::people::STANDING_KEY: "invalid" }),
    )
    .await
    .unwrap();

    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(store::people::is_active_on(&mut tx, &uid).await.is_err());
    tx.rollback().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_connections_serialize_access_compare_and_set() {
    let dir = std::env::temp_dir().join(nucleus::new_uid("person-access"));
    std::fs::create_dir_all(&dir).unwrap();
    let url = format!("sqlite://{}", dir.join("lince.db").display());
    let store = Arc::new(Store::open(&url).await.unwrap());
    let uid = person(&store, "Serialized").await;
    let first = store::auth::ensure_role(&store.pool, "first")
        .await
        .unwrap();
    let second = store::auth::ensure_role(&store.pool, "second")
        .await
        .unwrap();
    store::auth::compare_and_set_role(&store.pool, &uid, Some(first), 0)
        .await
        .unwrap();

    let mut first_tx = store::write_tx(&store.pool).await.unwrap();
    store::read_filter::compare_and_set_on(&mut first_tx, &uid, Some("kind:plain"), 1)
        .await
        .unwrap();
    let second_store = Arc::clone(&store);
    let second_uid = uid.clone();
    let competing = tokio::spawn(async move {
        store::auth::compare_and_set_role(&second_store.pool, &second_uid, Some(second), 1).await
    });
    tokio::task::yield_now().await;
    first_tx.commit().await.unwrap();

    let error = competing.await.unwrap().unwrap_err();
    assert!(error.to_string().contains("revision conflict"));
    let access = store::auth::person_access(&store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(access.role_id, Some(first));
    assert_eq!(access.read_filter.as_deref(), Some("kind:plain"));
    assert_eq!(access.revision, 2);

    store.pool.close().await;
    std::fs::remove_dir_all(dir).unwrap();
}
