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

async fn generation(store: &Store, person_uid: &str) -> i64 {
    let mut connection = store.pool.acquire().await.unwrap();
    store::auth::credential_generation_on(&mut connection, person_uid)
        .await
        .unwrap()
}

async fn stored_generation(store: &Store, person_uid: &str) -> i64 {
    store::sqlx::query_scalar("SELECT generation FROM person_auth_generation WHERE person_uid = ?")
        .bind(person_uid)
        .fetch_one(&store.pool)
        .await
        .unwrap()
}

async fn credential_values(store: &Store, person_uid: &str) -> Option<(String, String)> {
    store::sqlx::query_as(
        "SELECT username, password_hash FROM person_credential WHERE person_uid = ?",
    )
    .bind(person_uid)
    .fetch_optional(&store.pool)
    .await
    .unwrap()
}

async fn create_credential(
    store: &Store,
    person_uid: &str,
    username: &str,
    password_hash: &str,
) -> i64 {
    let expected = generation(store, person_uid).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let next =
        store::auth::create_credential_on(&mut tx, person_uid, username, password_hash, expected)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    next
}

#[tokio::test]
async fn credential_lifecycle_advances_generation_without_changing_authority() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "Credential lifecycle").await;
    let role = store::auth::ensure_role(&store.pool, "worker")
        .await
        .unwrap();
    let access = store::auth::compare_and_set_role(&store.pool, &uid, Some(role), 0)
        .await
        .unwrap();
    let access = store::auth::compare_and_set_read_filter(
        &store.pool,
        &uid,
        Some("kind:plain"),
        access.revision,
    )
    .await
    .unwrap();
    let initial = generation(&store, &uid).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let created =
        store::auth::create_credential_on(&mut tx, &uid, "worker", "opaque-hash", initial)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    assert!(created > initial);

    let old_authentication = {
        let mut connection = store.pool.acquire().await.unwrap();
        store::session_access::password_on(&mut connection, "worker")
            .await
            .unwrap()
            .unwrap()
            .authentication()
            .clone()
    };
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let replaced =
        store::auth::replace_credential_on(&mut tx, &uid, "worker", "opaque-hash", created)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    assert!(replaced > created);
    let mut connection = store.pool.acquire().await.unwrap();
    assert!(
        store::session_access::require_authentication_on(&mut connection, &old_authentication)
            .await
            .is_err()
    );
    drop(connection);

    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let removed = store::auth::remove_credential_on(&mut tx, &uid, replaced)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(removed > replaced);
    assert_eq!(credential_values(&store, &uid).await, None);
    assert_eq!(
        store::auth::person_access(&store.pool, &uid).await.unwrap(),
        Some(access)
    );

    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        store::auth::replace_credential_on(&mut tx, &uid, "worker", "new", removed)
            .await
            .unwrap_err()
            .to_string()
            .contains("missing")
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn presence_stale_generation_and_username_conflicts_do_not_change_state() {
    let store = Store::open_memory().await.unwrap();
    let first = person(&store, "First").await;
    let second = person(&store, "Second").await;
    let first_generation = create_credential(&store, &first, "shared", "first-hash").await;
    let second_generation = generation(&store, &second).await;
    assert_eq!(
        store::auth::person_access(&store.pool, &first)
            .await
            .unwrap(),
        None
    );

    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        store::auth::create_credential_on(&mut tx, &first, "again", "other-hash", first_generation)
            .await
            .unwrap_err()
            .to_string()
            .contains("already exists")
    );
    tx.rollback().await.unwrap();

    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        store::auth::replace_credential_on(
            &mut tx,
            &first,
            "shared",
            "second-hash",
            first_generation - 1
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("generation conflict")
    );
    tx.rollback().await.unwrap();

    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        store::auth::create_credential_on(
            &mut tx,
            &second,
            "shared",
            "second-hash",
            second_generation
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
    assert_eq!(generation(&store, &first).await, first_generation);
    assert_eq!(generation(&store, &second).await, second_generation);
    assert_eq!(
        credential_values(&store, &first).await,
        Some(("shared".into(), "first-hash".into()))
    );
    assert_eq!(credential_values(&store, &second).await, None);
}

#[tokio::test]
async fn credential_inputs_and_targets_are_strictly_validated() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "Valid Person").await;
    let plain = record(&store, nucleus::RecordKind::Plain, "Plain").await;
    let deleted = person(&store, "Deleted Person").await;
    store::records::mark_deleted(&store.pool, &deleted)
        .await
        .unwrap();
    let missing = nucleus::new_uid("r");
    let username = "u".repeat(store::session_access::MAX_USERNAME_BYTES);
    let password_hash = "h".repeat(store::session_access::MAX_PASSWORD_HASH_BYTES);
    let expected = generation(&store, &uid).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let next =
        store::auth::create_credential_on(&mut tx, &uid, &username, &password_hash, expected)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    assert!(next > 0);

    let invalid_inputs = vec![
        (String::new(), "hash".to_string()),
        ("   ".to_string(), "hash".to_string()),
        (username.clone() + "u", "hash".to_string()),
        ("user".to_string(), String::new()),
        ("user".to_string(), "   ".to_string()),
        ("user".to_string(), password_hash.clone() + "h"),
    ];
    for (candidate_username, candidate_hash) in invalid_inputs {
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        assert!(
            store::auth::replace_credential_on(
                &mut tx,
                &uid,
                &candidate_username,
                &candidate_hash,
                next
            )
            .await
            .is_err()
        );
        tx.rollback().await.unwrap();
    }
    for (target, expected) in [
        (&plain, 1),
        (&deleted, stored_generation(&store, &deleted).await),
        (&missing, 1),
    ] {
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        assert!(
            store::auth::create_credential_on(&mut tx, target, "target", "hash", expected)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
    }
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        store::auth::create_credential_on(&mut tx, "not-a-uid", "target", "hash", 1)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn a_disabled_person_can_replace_a_credential_without_reactivation() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "Disabled Person").await;
    create_credential(&store, &uid, "disabled", "old-hash").await;
    store::people::deactivate(&store.pool, &uid, "2026-09-07", None)
        .await
        .unwrap();
    let expected = generation(&store, &uid).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let next =
        store::auth::replace_credential_on(&mut tx, &uid, "disabled", "replacement-hash", expected)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    assert!(next > expected);
    assert_eq!(
        credential_values(&store, &uid).await,
        Some(("disabled".into(), "replacement-hash".into()))
    );
    assert!(!store::people::is_active(&store.pool, &uid).await.unwrap());
}

#[tokio::test]
async fn corrupt_oversized_and_exhausted_credentials_refuse_without_secret_output() {
    let store = Store::open_memory().await.unwrap();
    let oversized = person(&store, "Oversized").await;
    let corrupt = person(&store, "Corrupt").await;
    let exhausted = person(&store, "Exhausted").await;
    let role = store::auth::ensure_role(&store.pool, "worker")
        .await
        .unwrap();
    store::auth::create_credential(
        &store.pool,
        &oversized,
        "oversized",
        &"s".repeat(store::session_access::MAX_PASSWORD_HASH_BYTES + 1),
        role,
    )
    .await
    .unwrap();
    create_credential(&store, &corrupt, "corrupt", "protected-secret").await;
    create_credential(&store, &exhausted, "exhausted", "protected-secret").await;
    store::sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE person_credential SET password_hash = '' WHERE person_uid = ?")
        .bind(&corrupt)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE person_auth_generation SET generation = ? WHERE person_uid = ?")
        .bind(i64::MAX)
        .bind(&exhausted)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("PRAGMA ignore_check_constraints = OFF")
        .execute(&store.pool)
        .await
        .unwrap();

    for target in [&oversized, &corrupt] {
        let expected = generation(&store, target).await;
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        let error = store::auth::remove_credential_on(&mut tx, target, expected)
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("invalid or oversized"));
        assert!(!error.contains("protected-secret"));
        tx.rollback().await.unwrap();
    }
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        store::auth::replace_credential_on(
            &mut tx,
            &exhausted,
            "exhausted",
            "protected-secret",
            i64::MAX
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("exhausted")
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn credential_and_access_changes_share_the_callers_rollback() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "Rollback Person").await;
    let role = store::auth::ensure_role(&store.pool, "worker")
        .await
        .unwrap();
    let initial_generation = generation(&store, &uid).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let access = store::auth::compare_and_set_role_on(&mut tx, &uid, Some(role), 0)
        .await
        .unwrap();
    let access = store::auth::compare_and_set_read_filter_on(
        &mut tx,
        &uid,
        Some("private-filter"),
        access.revision,
    )
    .await
    .unwrap();
    let next = store::auth::create_credential_on(
        &mut tx,
        &uid,
        "rollback",
        "rollback-hash",
        initial_generation,
    )
    .await
    .unwrap();
    assert!(next > initial_generation);
    assert_eq!(
        store::auth::person_access_on(&mut tx, &uid).await.unwrap(),
        Some(access)
    );
    tx.rollback().await.unwrap();
    assert_eq!(credential_values(&store, &uid).await, None);
    assert_eq!(generation(&store, &uid).await, initial_generation);
    assert_eq!(
        store::auth::person_access(&store.pool, &uid).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn retained_access_includes_credential_free_disabled_and_deleted_people() {
    let store = Store::open_memory().await.unwrap();
    let active = person(&store, "Active").await;
    let disabled = person(&store, "Disabled").await;
    let deleted = person(&store, "Deleted").await;
    let role = store::auth::ensure_role(&store.pool, "worker")
        .await
        .unwrap();
    for (uid, filter) in [
        (&active, "active-filter"),
        (&disabled, "disabled-filter"),
        (&deleted, "deleted-filter"),
    ] {
        let access = store::auth::compare_and_set_role(&store.pool, uid, Some(role), 0)
            .await
            .unwrap();
        store::auth::compare_and_set_read_filter(&store.pool, uid, Some(filter), access.revision)
            .await
            .unwrap();
    }
    store::people::deactivate(&store.pool, &disabled, "2026-09-07", None)
        .await
        .unwrap();
    store::records::mark_deleted(&store.pool, &deleted)
        .await
        .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    let rows = store::auth::retained_person_access_on(&mut tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .find(|row| row.person_uid == active)
            .unwrap()
            .read_filter
            .as_deref(),
        Some("active-filter")
    );
    assert!(
        !rows
            .iter()
            .find(|row| row.person_uid == active)
            .unwrap()
            .deleted
    );
    assert_eq!(
        rows.iter()
            .find(|row| row.person_uid == disabled)
            .unwrap()
            .read_filter
            .as_deref(),
        Some("disabled-filter")
    );
    assert!(
        !rows
            .iter()
            .find(|row| row.person_uid == disabled)
            .unwrap()
            .deleted
    );
    assert_eq!(
        rows.iter()
            .find(|row| row.person_uid == deleted)
            .unwrap()
            .read_filter
            .as_deref(),
        Some("deleted-filter")
    );
    assert!(
        rows.iter()
            .find(|row| row.person_uid == deleted)
            .unwrap()
            .deleted
    );
    for row in &rows {
        assert!(credential_values(&store, &row.person_uid).await.is_none());
    }
}

#[tokio::test]
async fn retained_access_observes_the_callers_transaction() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "Transaction Person").await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let pending =
        store::auth::compare_and_set_read_filter_on(&mut tx, &uid, Some("pending-filter"), 0)
            .await
            .unwrap();
    assert_eq!(
        store::auth::retained_person_access_on(&mut tx)
            .await
            .unwrap(),
        vec![store::auth::RetainedPersonAccess {
            person_uid: uid.clone(),
            role_id: None,
            read_filter: Some("pending-filter".into()),
            revision: pending.revision,
            deleted: false,
        }]
    );
    tx.rollback().await.unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::auth::retained_person_access_on(&mut tx)
            .await
            .unwrap()
            .is_empty()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn retained_access_refuses_missing_wrong_kind_role_and_revision_corruption() {
    for corruption in [
        "missing-person",
        "wrong-kind",
        "missing-role",
        "bad-revision",
    ] {
        let store = Store::open_memory().await.unwrap();
        let valid = person(&store, "Valid").await;
        let plain = record(&store, nucleus::RecordKind::Plain, "Plain").await;
        let missing = nucleus::new_uid("r");
        let mut connection = store.pool.acquire().await.unwrap();
        store::sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&mut *connection)
            .await
            .unwrap();
        store::sqlx::query("PRAGMA ignore_check_constraints = ON")
            .execute(&mut *connection)
            .await
            .unwrap();
        match corruption {
            "missing-person" => {
                store::sqlx::query(
                    "INSERT INTO person_access (person_uid, revision) VALUES (?, 1)",
                )
                .bind(&missing)
                .execute(&mut *connection)
                .await
                .unwrap();
            }
            "wrong-kind" => {
                store::sqlx::query(
                    "INSERT INTO person_access (person_uid, revision) VALUES (?, 1)",
                )
                .bind(&plain)
                .execute(&mut *connection)
                .await
                .unwrap();
            }
            "missing-role" => {
                store::sqlx::query(
                    "INSERT INTO person_access (person_uid, role_id, revision) VALUES (?, 999999, 1)",
                )
                .bind(&valid)
                .execute(&mut *connection)
                .await
                .unwrap();
            }
            "bad-revision" => {
                store::sqlx::query(
                    "INSERT INTO person_access (person_uid, revision) VALUES (?, 0)",
                )
                .bind(&valid)
                .execute(&mut *connection)
                .await
                .unwrap();
            }
            _ => unreachable!(),
        }
        drop(connection);
        let mut tx = store.pool.begin().await.unwrap();
        assert!(
            store::auth::retained_person_access_on(&mut tx)
                .await
                .unwrap_err()
                .to_string()
                .contains("invalid or oversized")
        );
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn retained_access_refuses_individual_total_and_row_bounds() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "Oversized filter").await;
    store::sqlx::query(
        "INSERT INTO person_access (person_uid, read_filter, revision) VALUES (?, ?, 1)",
    )
    .bind(&uid)
    .bind("x".repeat(store::auth::MAX_READ_FILTER_BYTES + 1))
    .execute(&store.pool)
    .await
    .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::auth::retained_person_access_on(&mut tx)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();

    let store = Store::open_memory().await.unwrap();
    for index in 0..5 {
        let uid = person(&store, &format!("Total {index}")).await;
        store::sqlx::query(
            "INSERT INTO person_access (person_uid, read_filter, revision) VALUES (?, ?, 1)",
        )
        .bind(uid)
        .bind("x".repeat(store::auth::MAX_READ_FILTER_BYTES))
        .execute(&store.pool)
        .await
        .unwrap();
    }
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::auth::retained_person_access_on(&mut tx)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();

    let store = Store::open_memory().await.unwrap();
    let organ_uid = store::organs::local(&store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    store::sqlx::query(
        "WITH digits(value) AS (
             VALUES (0), (1), (2), (3), (4), (5), (6), (7), (8), (9)
         ), numbered(value) AS (
             SELECT a.value + 10 * b.value + 100 * c.value + 1000 * d.value
               FROM digits a, digits b, digits c, digits d
         )
         INSERT INTO record (uid, kind, organ_uid, created_at, updated_at)
         SELECT printf('r_%026X', value), 'person', ?, 'row-bound', 'row-bound'
           FROM numbered WHERE value <= ?",
    )
    .bind(&organ_uid)
    .bind(store::auth::MAX_PERSON_ACCESS_ROWS as i64)
    .execute(&store.pool)
    .await
    .unwrap();
    store::sqlx::query(
        "INSERT INTO person_access (person_uid, revision)
         SELECT uid, 1 FROM record WHERE created_at = 'row-bound'",
    )
    .execute(&store.pool)
    .await
    .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::auth::retained_person_access_on(&mut tx)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn retained_access_refuses_blob_filters_before_returning_rows() {
    let directory = std::env::temp_dir().join(nucleus::new_uid("account-filter-corruption"));
    std::fs::create_dir_all(&directory).unwrap();
    let url = format!("sqlite://{}", directory.join("lince.db").display());
    let store = Store::open(&url).await.unwrap();
    let uid = person(&store, "Blob filter").await;
    let mut connection = store.pool.acquire().await.unwrap();
    store::sqlx::query("PRAGMA writable_schema = ON")
        .execute(&mut *connection)
        .await
        .unwrap();
    store::sqlx::query(
        "UPDATE sqlite_schema SET sql = replace(sql, ') STRICT', ')') WHERE name = 'person_access'",
    )
    .execute(&mut *connection)
    .await
    .unwrap();
    store::sqlx::query("PRAGMA writable_schema = OFF")
        .execute(&mut *connection)
        .await
        .unwrap();
    drop(connection);
    store.pool.close().await;

    let reopened = Store::open(&url).await.unwrap();
    store::sqlx::query(
        "INSERT INTO person_access (person_uid, read_filter, revision) VALUES (?, zeroblob(?), 1)",
    )
    .bind(&uid)
    .bind(store::auth::MAX_READ_FILTER_BYTES as i64 * 2)
    .execute(&reopened.pool)
    .await
    .unwrap();
    let mut tx = reopened.pool.begin().await.unwrap();
    let error = store::auth::retained_person_access_on(&mut tx)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("invalid or oversized stored data"));
    tx.rollback().await.unwrap();
    reopened.pool.close().await;
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn separate_connections_serialize_credentials_and_restart_keeps_the_winner() {
    let directory = std::env::temp_dir().join(nucleus::new_uid("account-transaction"));
    std::fs::create_dir_all(&directory).unwrap();
    let url = format!("sqlite://{}", directory.join("lince.db").display());
    let store = Store::open(&url).await.unwrap();
    let uid = person(&store, "Concurrent Person").await;
    let created = create_credential(&store, &uid, "worker", "original-hash").await;
    let mut second = store.pool.acquire().await.unwrap();
    store::sqlx::query("PRAGMA busy_timeout = 0")
        .execute(&mut *second)
        .await
        .unwrap();
    let mut first = store::write_tx(&store.pool).await.unwrap();
    let replaced =
        store::auth::replace_credential_on(&mut first, &uid, "worker", "winner-hash", created)
            .await
            .unwrap();
    let busy = store::sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *second)
        .await
        .unwrap_err();
    assert_eq!(
        busy.as_database_error().unwrap().code().as_deref(),
        Some("5")
    );
    first.commit().await.unwrap();
    drop(second);

    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        store::auth::replace_credential_on(&mut tx, &uid, "worker", "loser-hash", created)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    assert_eq!(generation(&store, &uid).await, replaced);
    store.pool.close().await;

    let reopened = Store::open(&url).await.unwrap();
    assert_eq!(
        credential_values(&reopened, &uid).await,
        Some(("worker".into(), "winner-hash".into()))
    );
    assert_eq!(generation(&reopened, &uid).await, replaced);
    reopened.pool.close().await;
    std::fs::remove_dir_all(directory).unwrap();
}
