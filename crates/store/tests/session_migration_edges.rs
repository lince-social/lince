use store::Store;
use store::session_access;

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

async fn device_revision(store: &Store, person: &str, node: &str) -> i64 {
    let mut connection = store.pool.acquire().await.unwrap();
    session_access::device_on(&mut connection, person, node)
        .await
        .unwrap()
        .unwrap()
        .revision
}

async fn malformed_contact_roundtrip(alias: &str, blob: bool) {
    let store = Store::open_memory().await.unwrap();
    let person = record(&store, nucleus::RecordKind::Person, "Person").await;
    let role = store::auth::ensure_role(&store.pool, "operator")
        .await
        .unwrap();
    store::auth::create_credential(&store.pool, &person, "operator", "opaque-test-hash", role)
        .await
        .unwrap();
    let canonical = "ab".repeat(32);
    let unrelated_node = "cd".repeat(32);
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let auth = session_access::password_on(&mut tx, "operator")
        .await
        .unwrap()
        .unwrap()
        .authentication()
        .clone();
    let captured = session_access::register_device_on(&mut tx, &auth, &canonical)
        .await
        .unwrap();
    let unrelated = session_access::register_device_on(&mut tx, &auth, &unrelated_node)
        .await
        .unwrap();
    assert!(captured.peer_contact().is_none());
    tx.commit().await.unwrap();
    let organ = record(&store, nucleus::RecordKind::Organ, "Malformed contact").await;
    let query = store::sqlx::query("INSERT INTO organ_contact (record_uid, node_id) VALUES (?, ?)")
        .bind(&organ);
    if blob {
        query
            .bind(alias.as_bytes())
            .execute(&store.pool)
            .await
            .unwrap();
    } else {
        query.bind(alias).execute(&store.pool).await.unwrap();
    }
    let mut expected = captured.device().revision + 1;
    assert_eq!(device_revision(&store, &person, &canonical).await, expected);
    let mut connection = store.pool.acquire().await.unwrap();
    assert!(
        session_access::peer_contact_on(&mut connection, &canonical)
            .await
            .is_err()
    );
    assert!(
        session_access::require_admission_on(&mut connection, &captured)
            .await
            .is_err()
    );
    let epoch = store::sqlx::query_scalar::<_, i64>(
        "SELECT generation FROM organ_login_generation WHERE organ_uid = ?",
    )
    .bind(&organ)
    .fetch_one(&mut *connection)
    .await
    .unwrap();
    drop(connection);
    store::sqlx::query(
        "UPDATE organ_contact SET node_id = node_id, trust = trust, proximity = proximity + 1
         WHERE record_uid = ?",
    )
    .bind(&organ)
    .execute(&store.pool)
    .await
    .unwrap();
    store::sqlx::query("UPDATE record SET kind = kind, deleted_at = deleted_at WHERE uid = ?")
        .bind(&organ)
        .execute(&store.pool)
        .await
        .unwrap();
    assert_eq!(device_revision(&store, &person, &canonical).await, expected);
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT generation FROM organ_login_generation WHERE organ_uid = ?",
        )
        .bind(&organ)
        .fetch_one(&store.pool)
        .await
        .unwrap(),
        epoch
    );
    store::sqlx::query("UPDATE organ_contact SET node_id = ? WHERE record_uid = ?")
        .bind(&canonical)
        .bind(&organ)
        .execute(&store.pool)
        .await
        .unwrap();
    expected += 1;
    assert_eq!(device_revision(&store, &person, &canonical).await, expected);
    let query = store::sqlx::query("UPDATE organ_contact SET node_id = ? WHERE record_uid = ?");
    if blob {
        query
            .bind(alias.as_bytes())
            .bind(&organ)
            .execute(&store.pool)
            .await
            .unwrap();
    } else {
        query
            .bind(alias)
            .bind(&organ)
            .execute(&store.pool)
            .await
            .unwrap();
    }
    expected += 1;
    assert_eq!(device_revision(&store, &person, &canonical).await, expected);
    for kind in ["plain", "organ"] {
        store::sqlx::query("UPDATE record SET kind = ? WHERE uid = ?")
            .bind(kind)
            .bind(&organ)
            .execute(&store.pool)
            .await
            .unwrap();
        expected += 1;
        assert_eq!(device_revision(&store, &person, &canonical).await, expected);
    }
    store::sqlx::query("DELETE FROM organ_contact WHERE record_uid = ?")
        .bind(&organ)
        .execute(&store.pool)
        .await
        .unwrap();
    expected += 1;
    assert_eq!(device_revision(&store, &person, &canonical).await, expected);
    let mut connection = store.pool.acquire().await.unwrap();
    assert!(
        session_access::peer_contact_on(&mut connection, &canonical)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        session_access::require_authentication_on(&mut connection, &auth)
            .await
            .is_ok()
    );
    assert!(
        session_access::require_admission_on(&mut connection, &captured)
            .await
            .is_err()
    );
    assert!(
        session_access::require_admission_on(&mut connection, &unrelated)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn uppercase_contact_roundtrip_never_revives_an_unknown_admission() {
    malformed_contact_roundtrip(&"AB".repeat(32), false).await;
}

#[tokio::test]
async fn padded_contact_roundtrip_never_revives_an_unknown_admission() {
    malformed_contact_roundtrip(&format!(" {} ", "ab".repeat(32)), false).await;
}

#[tokio::test]
async fn blob_contact_roundtrip_never_revives_an_unknown_admission() {
    malformed_contact_roundtrip(&"ab".repeat(32), true).await;
}
