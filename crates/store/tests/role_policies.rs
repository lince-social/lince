use serde_json::json;
use store::Store;

async fn role(store: &Store, name: &str) -> i64 {
    store::auth::ensure_role(&store.pool, name).await.unwrap()
}

#[tokio::test]
async fn the_real_migration_has_a_strict_role_policy_home() {
    let store = Store::open_memory().await.unwrap();
    let columns = store::sqlx::query_as::<_, (String, String, i64, Option<String>, i64, i64)>(
        "SELECT name, type, \"notnull\", dflt_value, pk, hidden
           FROM pragma_table_xinfo('role_policy')
          ORDER BY cid",
    )
    .fetch_all(&store.pool)
    .await
    .unwrap();
    let foreign_keys = store::sqlx::query_as::<_, (String, String, String)>(
        "SELECT \"table\", \"from\", \"to\"
           FROM pragma_foreign_key_list('role_policy')",
    )
    .fetch_all(&store.pool)
    .await
    .unwrap();
    let strict = store::sqlx::query_scalar::<_, i64>(
        "SELECT strict FROM pragma_table_list WHERE name = 'role_policy'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();

    assert_eq!(
        columns,
        vec![
            ("role_id".into(), "INTEGER".into(), 1, None, 1, 0),
            ("policy".into(), "TEXT".into(), 0, None, 0, 0),
            (
                "revision".into(),
                "INTEGER".into(),
                1,
                Some("1".into()),
                0,
                0
            ),
        ]
    );
    assert_eq!(
        foreign_keys,
        vec![("role".into(), "role_id".into(), "id".into())]
    );
    assert_eq!(strict, 1);
}

#[tokio::test]
async fn absent_and_cleared_policies_grant_no_stored_policy() {
    let store = Store::open_memory().await.unwrap();
    let role = role(&store, "empty").await;

    assert_eq!(
        store::role_policies::get(&store.pool, role).await.unwrap(),
        None
    );
    assert!(
        store::role_policies::all(&store.pool)
            .await
            .unwrap()
            .is_empty()
    );

    let cleared = store::role_policies::clear(&store.pool, role, 0)
        .await
        .unwrap();
    assert_eq!(cleared.policy, None);
    assert_eq!(cleared.revision, 1);
    assert_eq!(
        store::role_policies::get(&store.pool, role).await.unwrap(),
        Some(cleared)
    );
}

#[tokio::test]
async fn set_clear_and_recreate_never_repeat_a_revision() {
    let store = Store::open_memory().await.unwrap();
    let role = role(&store, "editor").await;
    let first = json!({"read": {"kind": "plain"}});
    let replacement = json!({"read": {"kind": "person"}});

    let created = store::role_policies::set(&store.pool, role, &first, 0)
        .await
        .unwrap();
    assert_eq!(created.policy.as_ref(), Some(&first));
    assert_eq!(created.revision, 1);
    let cleared = store::role_policies::clear(&store.pool, role, 1)
        .await
        .unwrap();
    assert_eq!(cleared.policy, None);
    assert_eq!(cleared.revision, 2);

    let stale = store::role_policies::set(&store.pool, role, &replacement, 1)
        .await
        .unwrap_err();
    assert!(stale.to_string().contains("revision conflict"));
    assert!(
        store::role_policies::set(&store.pool, role, &replacement, 0)
            .await
            .unwrap_err()
            .to_string()
            .contains("revision conflict")
    );

    let recreated = store::role_policies::set(&store.pool, role, &replacement, 2)
        .await
        .unwrap();
    assert_eq!(recreated.policy.as_ref(), Some(&replacement));
    assert_eq!(recreated.revision, 3);
}

#[tokio::test]
async fn missing_roles_and_non_object_writes_are_refused() {
    let store = Store::open_memory().await.unwrap();
    let role = role(&store, "shape").await;

    assert!(
        store::role_policies::set(&store.pool, i64::MAX, &json!({}), 0)
            .await
            .is_err()
    );
    for invalid in [json!(null), json!([]), json!("policy"), json!(7)] {
        assert!(
            store::role_policies::set(&store.pool, role, &invalid, 0)
                .await
                .is_err()
        );
    }
    assert_eq!(
        store::role_policies::get(&store.pool, role).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn role_foreign_keys_remove_obsolete_policy_rows() {
    let store = Store::open_memory().await.unwrap();
    let role = role(&store, "temporary").await;
    store::role_policies::set(&store.pool, role, &json!({}), 0)
        .await
        .unwrap();

    store::sqlx::query("DELETE FROM role WHERE id = ?")
        .bind(role)
        .execute(&store.pool)
        .await
        .unwrap();

    assert_eq!(
        store::role_policies::get(&store.pool, role).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn malformed_non_object_and_oversized_storage_is_refused() {
    let store = Store::open_memory().await.unwrap();
    let malformed = role(&store, "malformed").await;
    let non_object = role(&store, "non-object").await;
    let oversized = role(&store, "oversized").await;
    let oversized_policy = format!(
        "{{\"text\":\"{}\"}}",
        "x".repeat(store::role_policies::MAX_POLICY_BYTES)
    );

    assert!(
        store::sqlx::query(
            "INSERT INTO role_policy (role_id, policy, revision) VALUES (?, '[]', 1)",
        )
        .bind(non_object)
        .execute(&store.pool)
        .await
        .is_err()
    );

    store::sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&store.pool)
        .await
        .unwrap();
    for (role, policy) in [(malformed, "{"), (non_object, "[]")] {
        store::sqlx::query("INSERT INTO role_policy (role_id, policy, revision) VALUES (?, ?, 1)")
            .bind(role)
            .bind(policy)
            .execute(&store.pool)
            .await
            .unwrap();
    }
    store::sqlx::query("PRAGMA ignore_check_constraints = OFF")
        .execute(&store.pool)
        .await
        .unwrap();

    assert!(
        store::role_policies::get(&store.pool, malformed)
            .await
            .unwrap_err()
            .to_string()
            .contains("valid JSON")
    );
    assert!(
        store::role_policies::get(&store.pool, non_object)
            .await
            .unwrap_err()
            .to_string()
            .contains("JSON object")
    );
    assert!(store::role_policies::all(&store.pool).await.is_err());

    store::sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("INSERT INTO role_policy (role_id, policy, revision) VALUES (?, ?, 1)")
        .bind(oversized)
        .bind(oversized_policy)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("PRAGMA ignore_check_constraints = OFF")
        .execute(&store.pool)
        .await
        .unwrap();

    assert!(
        store::role_policies::get(&store.pool, oversized)
            .await
            .unwrap_err()
            .to_string()
            .contains("byte limit")
    );
    assert!(store::role_policies::all(&store.pool).await.is_err());
}

#[tokio::test]
async fn write_and_complete_list_byte_limits_fail_closed() {
    let store = Store::open_memory().await.unwrap();
    let oversized_role = role(&store, "write-oversized").await;
    let oversized = json!({"text": "x".repeat(store::role_policies::MAX_POLICY_BYTES)});
    assert!(
        store::role_policies::set(&store.pool, oversized_role, &oversized, 0)
            .await
            .is_err()
    );
    assert_eq!(
        store::role_policies::get(&store.pool, oversized_role)
            .await
            .unwrap(),
        None
    );

    let chunk_length = store::role_policies::MAX_POLICY_BYTES / 2;
    let chunk = json!({"text": "x".repeat(chunk_length)});
    let needed = store::role_policies::MAX_TOTAL_POLICY_BYTES / chunk_length + 1;
    for index in 0..needed {
        let role = role(&store, &format!("bounded-{index}")).await;
        store::role_policies::set(&store.pool, role, &chunk, 0)
            .await
            .unwrap();
    }
    assert!(
        store::role_policies::all(&store.pool)
            .await
            .unwrap_err()
            .to_string()
            .contains("total byte limit")
    );
}

#[tokio::test]
async fn complete_list_row_overflow_is_refused() {
    let store = Store::open_memory().await.unwrap();
    store::sqlx::query(
        "WITH digits(value) AS (
             VALUES (0), (1), (2), (3), (4), (5), (6), (7), (8), (9)
         )
         INSERT INTO role (name)
         SELECT 'row-bound-' || (a.value + 10 * b.value + 100 * c.value + 1000 * d.value)
           FROM digits a, digits b, digits c, digits d
          WHERE a.value + 10 * b.value + 100 * c.value + 1000 * d.value <= ?",
    )
    .bind(i64::try_from(store::role_policies::MAX_POLICY_ROWS).unwrap())
    .execute(&store.pool)
    .await
    .unwrap();
    store::sqlx::query(
        "INSERT INTO role_policy (role_id, policy, revision)
         SELECT id, NULL, 1 FROM role WHERE name LIKE 'row-bound-%'",
    )
    .execute(&store.pool)
    .await
    .unwrap();

    assert!(
        store::role_policies::all(&store.pool)
            .await
            .unwrap_err()
            .to_string()
            .contains("row limit")
    );
}

#[tokio::test]
async fn corrupt_and_overflowing_revisions_are_refused() {
    let store = Store::open_memory().await.unwrap();
    let corrupt = role(&store, "corrupt-revision").await;
    let overflow = role(&store, "overflow-revision").await;
    store::role_policies::set(&store.pool, corrupt, &json!({}), 0)
        .await
        .unwrap();
    store::role_policies::set(&store.pool, overflow, &json!({}), 0)
        .await
        .unwrap();

    store::sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE role_policy SET revision = 0 WHERE role_id = ?")
        .bind(corrupt)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("PRAGMA ignore_check_constraints = OFF")
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE role_policy SET revision = ? WHERE role_id = ?")
        .bind(i64::MAX)
        .bind(overflow)
        .execute(&store.pool)
        .await
        .unwrap();

    assert!(
        store::role_policies::get(&store.pool, corrupt)
            .await
            .is_err()
    );
    assert!(
        store::role_policies::set(&store.pool, overflow, &json!({"next": true}), i64::MAX)
            .await
            .unwrap_err()
            .to_string()
            .contains("overflow")
    );
    assert!(
        store::role_policies::set(&store.pool, overflow, &json!({}), -1)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn caller_transaction_policy_changes_roll_back() {
    let store = Store::open_memory().await.unwrap();
    let role = role(&store, "rollback").await;
    let policy = json!({"read": {"kind": "plain"}});
    let mut tx = store::write_tx(&store.pool).await.unwrap();

    let pending = store::role_policies::set_on(&mut tx, role, &policy, 0)
        .await
        .unwrap();
    assert_eq!(
        store::role_policies::get_on(&mut tx, role).await.unwrap(),
        Some(pending.clone())
    );
    assert_eq!(
        store::role_policies::all_on(&mut tx).await.unwrap(),
        vec![pending]
    );
    tx.rollback().await.unwrap();

    assert_eq!(
        store::role_policies::get(&store.pool, role).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn a_second_writer_observes_busy_then_a_stale_revision() {
    let dir = std::env::temp_dir().join(nucleus::new_uid("role-policy-race"));
    std::fs::create_dir_all(&dir).unwrap();
    let url = format!("sqlite://{}", dir.join("lince.db").display());
    let store = Store::open(&url).await.unwrap();
    let role = role(&store, "concurrent").await;
    let first = json!({"source": "first"});
    let second = json!({"source": "second"});
    let mut second_connection = store.pool.acquire().await.unwrap();
    store::sqlx::query("PRAGMA busy_timeout = 0")
        .execute(&mut *second_connection)
        .await
        .unwrap();
    let mut first_tx = store::write_tx(&store.pool).await.unwrap();
    let created = store::role_policies::set_on(&mut first_tx, role, &first, 0)
        .await
        .unwrap();
    let busy = store::sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *second_connection)
        .await
        .unwrap_err();
    assert_eq!(
        busy.as_database_error().unwrap().code().as_deref(),
        Some("5")
    );
    first_tx.commit().await.unwrap();
    drop(second_connection);

    assert!(
        store::role_policies::set(&store.pool, role, &second, 0)
            .await
            .unwrap_err()
            .to_string()
            .contains("revision conflict")
    );
    assert_eq!(
        store::role_policies::get(&store.pool, role).await.unwrap(),
        Some(created)
    );
    store.pool.close().await;
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn policy_persists_across_store_restart() {
    let dir = std::env::temp_dir().join(nucleus::new_uid("role-policy-restart"));
    std::fs::create_dir_all(&dir).unwrap();
    let url = format!("sqlite://{}", dir.join("lince.db").display());
    let store = Store::open(&url).await.unwrap();
    let role = role(&store, "persistent").await;
    let policy = json!({"read": {"kind": "person"}, "grants": []});
    let saved = store::role_policies::set(&store.pool, role, &policy, 0)
        .await
        .unwrap();
    store.pool.close().await;

    let reopened = Store::open(&url).await.unwrap();
    assert_eq!(
        store::role_policies::get(&reopened.pool, role)
            .await
            .unwrap(),
        Some(saved)
    );
    reopened.pool.close().await;
    std::fs::remove_dir_all(dir).unwrap();
}
