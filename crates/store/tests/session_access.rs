use std::borrow::Cow;
use std::str::FromStr;
use std::sync::Arc;

use store::Store;
use store::session_access::{
    self, AuthenticationSource, AuthenticationState, ContactTrust, DeviceAdmission,
};

fn node(number: u64) -> String {
    format!("{number:064x}")
}

async fn person(store: &Store, name: &str) -> String {
    store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: name,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid
}

async fn credential(store: &Store, person_uid: &str, username: &str, hash: &str) {
    let role = store::auth::ensure_role(&store.pool, "worker")
        .await
        .unwrap();
    store::auth::create_credential(&store.pool, person_uid, username, hash, role)
        .await
        .unwrap();
}

async fn password(store: &Store, username: &str) -> AuthenticationState {
    let mut connection = store.pool.acquire().await.unwrap();
    session_access::password_on(&mut connection, username)
        .await
        .unwrap()
        .unwrap()
        .authentication()
        .clone()
}

async fn fixture() -> (Store, String, AuthenticationState) {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "Worker").await;
    credential(&store, &uid, "worker", "opaque-password-hash").await;
    let auth = password(&store, "worker").await;
    (store, uid, auth)
}

async fn peer(store: &Store, node_id: &str) -> String {
    let uid = nucleus::new_uid("r");
    store::organs::add_contact(&store.pool, &uid, None, "Remote Organ", "", 1)
        .await
        .unwrap();
    store::organs::set_node_id(&store.pool, &uid, Some(node_id))
        .await
        .unwrap();
    uid
}

async fn admission(store: &Store, auth: &AuthenticationState, node_id: &str) -> DeviceAdmission {
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let admitted = session_access::register_device_on(&mut tx, auth, node_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    admitted
}

async fn current_auth(store: &Store, auth: &AuthenticationState) -> bool {
    let mut connection = store.pool.acquire().await.unwrap();
    session_access::require_authentication_on(&mut connection, auth)
        .await
        .is_ok()
}

async fn current_admission(store: &Store, admitted: &DeviceAdmission) -> bool {
    let mut connection = store.pool.acquire().await.unwrap();
    session_access::require_admission_on(&mut connection, admitted)
        .await
        .is_ok()
}

#[tokio::test]
async fn session_access_real_schema_generations_and_redacted_credentials() {
    let (store, uid, auth) = fixture().await;
    assert!(auth.generation() > 0);
    assert_eq!(auth.person_uid(), uid);
    assert_eq!(auth.source(), &AuthenticationSource::Password);
    let mut connection = store.pool.acquire().await.unwrap();
    for table in [
        "person_auth_generation",
        "organ_login_generation",
        "person_device",
    ] {
        let strict: i64 =
            store::sqlx::query_scalar("SELECT strict FROM pragma_table_list WHERE name = ?")
                .bind(table)
                .fetch_one(&mut *connection)
                .await
                .unwrap();
        assert_eq!(strict, 1);
    }
    let snapshot = session_access::password_on(&mut connection, "worker")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.username(), "worker");
    assert_eq!(snapshot.password_hash(), "opaque-password-hash");
    assert!(!format!("{snapshot:?}").contains("opaque-password-hash"));
    assert!(
        session_access::password_on(&mut connection, "missing")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        session_access::password_on(&mut connection, "")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn session_access_password_reset_remove_and_recreate_never_revive_a_capture() {
    let (store, uid, old) = fixture().await;
    credential(&store, &uid, "worker", "replacement-hash").await;
    assert!(!current_auth(&store, &old).await);
    let replaced = password(&store, "worker").await;
    assert!(replaced.generation() > old.generation());
    store::sqlx::query("DELETE FROM person_credential WHERE person_uid = ?")
        .bind(&uid)
        .execute(&store.pool)
        .await
        .unwrap();
    assert!(!current_auth(&store, &replaced).await);
    credential(&store, &uid, "worker", "replacement-hash").await;
    assert!(!current_auth(&store, &replaced).await);
    assert!(password(&store, "worker").await.generation() > replaced.generation());
}

#[tokio::test]
async fn session_access_disable_reactivate_and_kind_roundtrip_advance_generation() {
    let (store, uid, old) = fixture().await;
    store::people::deactivate(&store.pool, &uid, "2026-09-06", None)
        .await
        .unwrap();
    assert!(!current_auth(&store, &old).await);
    store::people::reactivate(&store.pool, &uid).await.unwrap();
    assert!(!current_auth(&store, &old).await);
    let active = password(&store, "worker").await;
    for kind in ["plain", "person"] {
        store::sqlx::query("UPDATE record SET kind = ? WHERE uid = ?")
            .bind(kind)
            .bind(&uid)
            .execute(&store.pool)
            .await
            .unwrap();
        assert!(!current_auth(&store, &active).await);
    }
    assert!(password(&store, "worker").await.generation() > active.generation());
}

#[tokio::test]
async fn session_access_deleted_person_recreation_keeps_epoch_and_device_tombstones() {
    let (store, uid, auth) = fixture().await;
    let admitted = admission(&store, &auth, &node(1)).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let revoked = session_access::compare_and_set_revoked_on(
        &mut tx,
        &uid,
        &node(1),
        admitted.device().revision,
        true,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    store::sqlx::query("DELETE FROM record WHERE uid = ?")
        .bind(&uid)
        .execute(&store.pool)
        .await
        .unwrap();
    assert!(!current_auth(&store, &auth).await);
    store::records::create_with_uid(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Recreated",
            body: "",
            quantity: store::exact::zero(),
        },
        &uid,
    )
    .await
    .unwrap();
    credential(&store, &uid, "worker", "opaque-password-hash").await;
    let fresh = password(&store, "worker").await;
    assert!(fresh.generation() > auth.generation());
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert_eq!(
        session_access::device_on(&mut tx, &uid, &node(1))
            .await
            .unwrap(),
        Some(revoked)
    );
    assert!(
        session_access::register_device_on(&mut tx, &fresh, &node(1))
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn session_access_credentialless_organ_login_is_exact_and_regrant_has_a_new_epoch() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "Without password").await;
    let organ = peer(&store, &node(1)).await;
    store::logins::grant(&store.pool, &organ, &uid)
        .await
        .unwrap();
    let mut connection = store.pool.acquire().await.unwrap();
    let auth = session_access::granted_login_on(&mut connection, &organ, &node(1))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(auth.person_uid(), uid);
    assert!(
        session_access::granted_login_on(&mut connection, &organ, &node(2))
            .await
            .is_err()
    );
    drop(connection);
    assert!(
        !store::auth::has_credential(&store.pool, &uid)
            .await
            .unwrap()
    );
    let admitted = admission(&store, &auth, &node(1)).await;
    store::logins::revoke(&store.pool, &organ).await.unwrap();
    store::logins::grant(&store.pool, &organ, &uid)
        .await
        .unwrap();
    assert!(!current_auth(&store, &auth).await);
    assert!(!current_admission(&store, &admitted).await);
    let mut connection = store.pool.acquire().await.unwrap();
    let fresh = session_access::granted_login_on(&mut connection, &organ, &node(1))
        .await
        .unwrap()
        .unwrap();
    assert_ne!(fresh, auth);
    assert!(
        session_access::register_device_on(&mut connection, &fresh, &node(2))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn session_access_device_registration_never_unrevokes_and_cas_is_monotonic() {
    let (store, uid, auth) = fixture().await;
    let first = admission(&store, &auth, &node(1)).await;
    assert_eq!(admission(&store, &auth, &node(1)).await, first);
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let revoked = session_access::compare_and_set_revoked_on(
        &mut tx,
        &uid,
        &node(1),
        first.device().revision,
        true,
    )
    .await
    .unwrap();
    assert!(
        session_access::register_device_on(&mut tx, &auth, &node(1))
            .await
            .is_err()
    );
    assert!(
        session_access::compare_and_set_revoked_on(
            &mut tx,
            &uid,
            &node(1),
            first.device().revision,
            false
        )
        .await
        .is_err()
    );
    let active = session_access::compare_and_set_revoked_on(
        &mut tx,
        &uid,
        &node(1),
        revoked.revision,
        false,
    )
    .await
    .unwrap();
    assert!(active.revision > revoked.revision);
    let repeated =
        session_access::compare_and_set_revoked_on(&mut tx, &uid, &node(1), active.revision, false)
            .await
            .unwrap();
    assert!(repeated.revision > active.revision);
    tx.commit().await.unwrap();
    assert!(!current_admission(&store, &first).await);
    assert!(current_admission(&store, &admission(&store, &auth, &node(1)).await).await);
}

#[tokio::test]
async fn session_access_device_scope_is_person_and_canonical_node_not_signing_key() {
    let (store, uid, auth) = fixture().await;
    let other = person(&store, "Other").await;
    credential(&store, &other, "other", "hash").await;
    let other_auth = password(&store, "other").await;
    let first = admission(&store, &auth, &node(1)).await;
    let second = admission(&store, &other_auth, &node(1)).await;
    let third = admission(&store, &auth, &node(2)).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    session_access::compare_and_set_revoked_on(
        &mut tx,
        &uid,
        &node(1),
        first.device().revision,
        true,
    )
    .await
    .unwrap();
    for invalid in [
        "person-signing-key".to_string(),
        "f".repeat(63),
        "F".repeat(64),
        "g".repeat(64),
    ] {
        assert!(
            session_access::register_device_on(&mut tx, &auth, &invalid)
                .await
                .is_err()
        );
    }
    tx.commit().await.unwrap();
    assert!(!current_admission(&store, &first).await);
    assert!(current_admission(&store, &second).await);
    assert!(current_admission(&store, &third).await);
}

#[tokio::test]
async fn session_access_known_peer_node_and_trust_roundtrips_invalidate_password_and_grant() {
    let (store, uid, auth) = fixture().await;
    let organ = peer(&store, &node(1)).await;
    store::logins::grant(&store.pool, &organ, &uid)
        .await
        .unwrap();
    let password_admission = admission(&store, &auth, &node(1)).await;
    let mut connection = store.pool.acquire().await.unwrap();
    let grant = session_access::granted_login_on(&mut connection, &organ, &node(1))
        .await
        .unwrap()
        .unwrap();
    drop(connection);
    for value in [node(2), node(1)] {
        store::organs::set_node_id(&store.pool, &organ, Some(&value))
            .await
            .unwrap();
    }
    assert!(!current_auth(&store, &grant).await);
    assert!(!current_admission(&store, &password_admission).await);
    let fresh = admission(&store, &auth, &node(1)).await;
    store::organs::set_trust(&store.pool, &organ, "blocked")
        .await
        .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        session_access::register_device_on(&mut tx, &auth, &node(1))
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    store::organs::set_trust(&store.pool, &organ, "known")
        .await
        .unwrap();
    assert!(!current_admission(&store, &fresh).await);
}

#[tokio::test]
async fn session_access_unknown_peer_contact_roundtrip_is_remembered_by_device_revision() {
    let (store, _, auth) = fixture().await;
    let unknown = admission(&store, &auth, &node(1)).await;
    let unrelated = admission(&store, &auth, &node(2)).await;
    assert!(unknown.peer_contact().is_none());
    let organ = peer(&store, &node(1)).await;
    store::organs::set_trust(&store.pool, &organ, "blocked")
        .await
        .unwrap();
    store::sqlx::query("DELETE FROM organ_contact WHERE record_uid = ?")
        .bind(&organ)
        .execute(&store.pool)
        .await
        .unwrap();
    let mut connection = store.pool.acquire().await.unwrap();
    assert!(
        session_access::peer_contact_on(&mut connection, &node(1))
            .await
            .unwrap()
            .is_none()
    );
    let device = session_access::device_on(&mut connection, auth.person_uid(), &node(1))
        .await
        .unwrap()
        .unwrap();
    assert!(device.revision > unknown.device().revision);
    drop(connection);
    assert!(!current_admission(&store, &unknown).await);
    assert!(current_admission(&store, &unrelated).await);
}

#[tokio::test]
async fn session_access_credential_and_standing_subject_moves_advance_both_epochs() {
    let (store, first, _) = fixture().await;
    let second = person(&store, "Second").await;
    let epochs = |first: String, second: String| {
        let pool = store.pool.clone();
        async move {
            store::sqlx::query_scalar::<_, i64>("SELECT generation FROM person_auth_generation WHERE person_uid IN (?, ?) ORDER BY person_uid")
                .bind(first).bind(second).fetch_all(&pool).await.unwrap()
        }
    };
    let before = epochs(first.clone(), second.clone()).await;
    store::sqlx::query("UPDATE person_credential SET person_uid = ? WHERE person_uid = ?")
        .bind(&second)
        .bind(&first)
        .execute(&store.pool)
        .await
        .unwrap();
    let moved = epochs(first.clone(), second.clone()).await;
    assert!(moved.iter().zip(&before).all(|(new, old)| new > old));
    assert_eq!(password(&store, "worker").await.person_uid(), second);
    store::people::deactivate(&store.pool, &first, "2026-09-06", None)
        .await
        .unwrap();
    let before = epochs(first.clone(), second.clone()).await;
    store::sqlx::query("UPDATE record_extension SET record_uid = ? WHERE record_uid = ? AND namespace = 'lince.person'")
        .bind(&second).bind(&first).execute(&store.pool).await.unwrap();
    let moved = epochs(first.clone(), second.clone()).await;
    assert!(moved.iter().zip(&before).all(|(new, old)| new > old));
    assert!(store::people::is_active(&store.pool, &first).await.unwrap());
    assert!(
        !store::people::is_active(&store.pool, &second)
            .await
            .unwrap()
    );
    let before = moved;
    store::sqlx::query("UPDATE record_extension SET namespace = 'ordinary.extension' WHERE record_uid = ? AND namespace = 'lince.person'")
        .bind(&second).execute(&store.pool).await.unwrap();
    let moved = epochs(first.clone(), second.clone()).await;
    assert_eq!(
        moved
            .iter()
            .zip(before)
            .filter(|(new, old)| **new > *old)
            .count(),
        1
    );
}

#[tokio::test]
async fn session_access_contact_and_organ_identity_changes_invalidate_only_affected_devices() {
    let (store, uid, auth) = fixture().await;
    let organ = peer(&store, &node(1)).await;
    let first = admission(&store, &auth, &node(1)).await;
    let unrelated = admission(&store, &auth, &node(2)).await;
    for kind in ["plain", "organ"] {
        store::sqlx::query("UPDATE record SET kind = ? WHERE uid = ?")
            .bind(kind)
            .bind(&organ)
            .execute(&store.pool)
            .await
            .unwrap();
    }
    assert!(!current_admission(&store, &first).await);
    assert!(current_admission(&store, &unrelated).await);
    let fresh = admission(&store, &auth, &node(1)).await;
    store::sqlx::query("UPDATE record SET deleted_at = '2026-09-06' WHERE uid = ?")
        .bind(&organ)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE record SET deleted_at = NULL WHERE uid = ?")
        .bind(&organ)
        .execute(&store.pool)
        .await
        .unwrap();
    assert!(!current_admission(&store, &fresh).await);
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let device = session_access::device_on(&mut tx, &uid, &node(1))
        .await
        .unwrap()
        .unwrap();
    let revoked =
        session_access::compare_and_set_revoked_on(&mut tx, &uid, &node(1), device.revision, true)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    store::organs::set_trust(&store.pool, &organ, "known")
        .await
        .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let retained = session_access::device_on(&mut tx, &uid, &node(1))
        .await
        .unwrap()
        .unwrap();
    assert!(retained.revoked);
    assert!(retained.revision > revoked.revision);
    assert!(
        session_access::compare_and_set_revoked_on(
            &mut tx,
            &uid,
            &node(1),
            revoked.revision,
            false
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn session_access_absent_corrupt_and_blocked_contacts_are_distinct() {
    let (store, uid, _) = fixture().await;
    let mut connection = store.pool.acquire().await.unwrap();
    assert!(
        session_access::peer_contact_on(&mut connection, &node(1))
            .await
            .unwrap()
            .is_none()
    );
    drop(connection);
    let organ = peer(&store, &node(1)).await;
    store::logins::grant(&store.pool, &organ, &uid)
        .await
        .unwrap();
    store::organs::set_trust(&store.pool, &organ, "blocked")
        .await
        .unwrap();
    let mut connection = store.pool.acquire().await.unwrap();
    assert_eq!(
        session_access::peer_contact_on(&mut connection, &node(1))
            .await
            .unwrap()
            .unwrap()
            .trust,
        ContactTrust::Blocked
    );
    assert!(
        session_access::granted_login_on(&mut connection, &organ, &node(1))
            .await
            .is_err()
    );
    drop(connection);
    for trust in ["unexpected".to_string(), "x".repeat(100_000)] {
        store::organs::set_trust(&store.pool, &organ, &trust)
            .await
            .unwrap();
        let mut connection = store.pool.acquire().await.unwrap();
        assert!(
            session_access::peer_contact_on(&mut connection, &node(1))
                .await
                .is_err()
        );
    }
    store::organs::set_trust(&store.pool, &organ, "known")
        .await
        .unwrap();
    store::sqlx::query(
        "UPDATE organ_contact SET node_id = CAST(node_id AS BLOB) WHERE record_uid = ?",
    )
    .bind(&organ)
    .execute(&store.pool)
    .await
    .unwrap();
    let mut connection = store.pool.acquire().await.unwrap();
    assert!(
        session_access::peer_contact_on(&mut connection, &node(1))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn session_access_password_payload_limits_precede_credential_output() {
    let (store, uid, _) = fixture().await;
    let username = "u".repeat(session_access::MAX_USERNAME_BYTES);
    let hash = "h".repeat(session_access::MAX_PASSWORD_HASH_BYTES);
    credential(&store, &uid, &username, &hash).await;
    let mut connection = store.pool.acquire().await.unwrap();
    assert_eq!(
        session_access::password_on(&mut connection, &username)
            .await
            .unwrap()
            .unwrap()
            .password_hash()
            .len(),
        hash.len()
    );
    assert!(
        session_access::password_on(&mut connection, &(username.clone() + "u"))
            .await
            .is_err()
    );
    drop(connection);
    credential(&store, &uid, &username, &(hash + "h")).await;
    let mut connection = store.pool.acquire().await.unwrap();
    assert!(
        session_access::password_on(&mut connection, &username)
            .await
            .is_err()
    );
    store::sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&mut *connection)
        .await
        .unwrap();
    store::sqlx::query("UPDATE person_credential SET password_hash = '' WHERE person_uid = ?")
        .bind(&uid)
        .execute(&mut *connection)
        .await
        .unwrap();
    assert!(
        session_access::password_on(&mut connection, &username)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn session_access_person_standing_is_byte_bounded_and_storage_type_checked() {
    let (store, uid, _) = fixture().await;
    let empty = serde_json::json!({"padding":""}).to_string();
    let limit = store::people::MAX_PERSON_EXTENSION_BYTES;
    let accepted = serde_json::json!({"padding":"x".repeat(limit - empty.len())}).to_string();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::sqlx::query(
        "INSERT INTO record_extension (record_uid, namespace, fds) VALUES (?, 'lince.person', ?)",
    )
    .bind(&uid)
    .bind(&accepted)
    .execute(&mut *tx)
    .await
    .unwrap();
    assert!(store::people::is_active_on(&mut tx, &uid).await.unwrap());
    store::sqlx::query(
        "UPDATE record_extension SET fds = ? WHERE record_uid = ? AND namespace = 'lince.person'",
    )
    .bind(accepted + " ")
    .bind(&uid)
    .execute(&mut *tx)
    .await
    .unwrap();
    assert!(store::people::is_active_on(&mut tx, &uid).await.is_err());
    store::sqlx::query("UPDATE record_extension SET fds = CAST('{}' AS BLOB) WHERE record_uid = ? AND namespace = 'lince.person'")
        .bind(&uid).execute(&mut *tx).await.unwrap();
    assert!(store::people::standing_on(&mut tx, &uid).await.is_err());
    store::sqlx::query("UPDATE record_extension SET fds = '{}', namespace = CAST(namespace AS BLOB) WHERE record_uid = ?")
        .bind(&uid).execute(&mut *tx).await.unwrap();
    assert!(store::people::is_active_on(&mut tx, &uid).await.is_err());
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn session_access_person_filter_has_exact_input_and_stored_payload_bounds() {
    let (store, uid, _) = fixture().await;
    let filter = "x".repeat(store::auth::MAX_READ_FILTER_BYTES);
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let access = store::auth::person_access_on(&mut tx, &uid)
        .await
        .unwrap()
        .unwrap();
    let changed =
        store::auth::compare_and_set_read_filter_on(&mut tx, &uid, Some(&filter), access.revision)
            .await
            .unwrap();
    assert_eq!(changed.read_filter.as_ref().unwrap().len(), filter.len());
    assert!(
        store::auth::compare_and_set_read_filter_on(
            &mut tx,
            &uid,
            Some(&(filter.clone() + "x")),
            changed.revision
        )
        .await
        .is_err()
    );
    assert_eq!(
        store::auth::person_access_on(&mut tx, &uid)
            .await
            .unwrap()
            .unwrap(),
        changed
    );
    let opaque = store::auth::compare_and_set_read_filter_on(
        &mut tx,
        &uid,
        Some("not Protein JSON"),
        changed.revision,
    )
    .await
    .unwrap();
    assert_eq!(opaque.read_filter.as_deref(), Some("not Protein JSON"));
    store::sqlx::query("UPDATE person_access SET read_filter = ? WHERE person_uid = ?")
        .bind(filter + "x")
        .bind(&uid)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(store::auth::person_access_on(&mut tx, &uid).await.is_err());
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn session_access_person_access_corrupt_revision_and_role_refuse() {
    let (store, uid, _) = fixture().await;
    let mut connection = store.pool.acquire().await.unwrap();
    store::sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&mut *connection)
        .await
        .unwrap();
    store::sqlx::query("UPDATE person_access SET revision = 0 WHERE person_uid = ?")
        .bind(&uid)
        .execute(&mut *connection)
        .await
        .unwrap();
    assert!(
        store::auth::person_access_on(&mut connection, &uid)
            .await
            .is_err()
    );
    store::sqlx::query(
        "UPDATE person_access SET revision = 1, role_id = NULL WHERE person_uid = ?",
    )
    .bind(&uid)
    .execute(&mut *connection)
    .await
    .unwrap();
    assert!(
        store::auth::person_access_on(&mut connection, &uid)
            .await
            .unwrap()
            .unwrap()
            .role_id
            .is_none()
    );
    store::sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *connection)
        .await
        .unwrap();
    store::sqlx::query("UPDATE person_access SET role_id = 999999 WHERE person_uid = ?")
        .bind(&uid)
        .execute(&mut *connection)
        .await
        .unwrap();
    assert!(
        store::auth::person_access_on(&mut connection, &uid)
            .await
            .is_err()
    );
    assert!(
        store::auth::person_access_on(&mut connection, "not-a-person")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn session_access_role_permission_count_and_key_bytes_are_bounded() {
    let store = Store::open_memory().await.unwrap();
    let role = store::auth::ensure_role(&store.pool, "counted")
        .await
        .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::sqlx::query("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM n WHERE x < ?) INSERT INTO permission (subject, action) SELECT 's' || x, 'a' FROM n")
        .bind(store::auth::MAX_ROLE_PERMISSIONS as i64).execute(&mut *tx).await.unwrap();
    store::sqlx::query(
        "INSERT INTO role_permission (role_id, permission_id) SELECT ?, id FROM permission",
    )
    .bind(role)
    .execute(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        store::auth::role_permission_keys_by_id_on(&mut tx, role)
            .await
            .unwrap()
            .len(),
        store::auth::MAX_ROLE_PERMISSIONS
    );
    let extra = store::auth::ensure_permission_on(&mut tx, "extra", "a")
        .await
        .unwrap();
    store::auth::grant_on(&mut tx, role, extra).await.unwrap();
    assert!(
        store::auth::role_permission_keys_by_id_on(&mut tx, role)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let exact = store::auth::ensure_permission_on(&mut tx, &"é".repeat(127), "a")
        .await
        .unwrap();
    store::auth::grant_on(&mut tx, role, exact).await.unwrap();
    assert_eq!(
        store::auth::role_permission_keys_by_id_on(&mut tx, role)
            .await
            .unwrap()[0]
            .len(),
        store::auth::MAX_PERMISSION_KEY_BYTES
    );
    store::sqlx::query("UPDATE permission SET subject = ? WHERE id = ?")
        .bind("é".repeat(128))
        .bind(exact)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(
        store::auth::role_permission_keys_by_id_on(&mut tx, role)
            .await
            .is_err()
    );
    assert!(
        store::auth::role_permission_keys_by_id_on(&mut tx, -1)
            .await
            .is_err()
    );
    assert!(
        store::auth::role_permission_keys_by_id_on(&mut tx, i64::MAX)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn session_access_role_permission_total_bytes_and_corruption_refuse() {
    let store = Store::open_memory().await.unwrap();
    let role = store::auth::ensure_role(&store.pool, "bytes")
        .await
        .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    for number in 0..256 {
        let subject = format!("{number:03}{}", "s".repeat(251));
        let permission = store::auth::ensure_permission_on(&mut tx, &subject, "a")
            .await
            .unwrap();
        store::auth::grant_on(&mut tx, role, permission)
            .await
            .unwrap();
    }
    let keys = store::auth::role_permission_keys_by_id_on(&mut tx, role)
        .await
        .unwrap();
    assert_eq!(
        keys.iter().map(String::len).sum::<usize>(),
        store::auth::MAX_ROLE_PERMISSION_BYTES
    );
    let extra = store::auth::ensure_permission_on(&mut tx, "extra", "a")
        .await
        .unwrap();
    store::auth::grant_on(&mut tx, role, extra).await.unwrap();
    assert!(
        store::auth::role_permission_keys_by_id_on(&mut tx, role)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let mut connection = store.pool.acquire().await.unwrap();
    store::sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *connection)
        .await
        .unwrap();
    store::sqlx::query("INSERT INTO role_permission (role_id, permission_id) VALUES (?, 999999)")
        .bind(role)
        .execute(&mut *connection)
        .await
        .unwrap();
    assert!(
        store::auth::role_permission_keys_by_id_on(&mut connection, role)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn session_access_device_limits_count_retained_revoked_rows() {
    let (store, uid, auth) = fixture().await;
    for number in 0..session_access::MAX_DEVICES_PER_PERSON {
        let admitted = admission(&store, &auth, &node(number as u64)).await;
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        session_access::compare_and_set_revoked_on(
            &mut tx,
            &uid,
            &node(number as u64),
            admitted.device().revision,
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        session_access::register_device_on(&mut tx, &auth, &node(999))
            .await
            .is_err()
    );
    assert!(
        session_access::device_on(&mut tx, &uid, &node(999))
            .await
            .unwrap()
            .is_none()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn session_access_global_device_limit_refuses_without_a_placeholder() {
    let (store, uid, auth) = fixture().await;
    let other = person(&store, "Population fixture").await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::sqlx::query("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM n WHERE x < ?) INSERT INTO person_device (person_uid, node_id, revoked, revision) SELECT ?, printf('%064x', x), 1, 1 FROM n")
        .bind(session_access::MAX_DEVICES as i64).bind(&other).execute(&mut *tx).await.unwrap();
    assert!(
        session_access::register_device_on(&mut tx, &auth, &node(1))
            .await
            .is_err()
    );
    assert!(
        session_access::device_on(&mut tx, &uid, &node(1))
            .await
            .unwrap()
            .is_none()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn session_access_authentication_and_device_changes_roll_back_together() {
    let (store, uid, old) = fixture().await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::sqlx::query(
        "UPDATE person_credential SET password_hash = 'new-hash' WHERE person_uid = ?",
    )
    .bind(&uid)
    .execute(&mut *tx)
    .await
    .unwrap();
    assert!(
        session_access::require_authentication_on(&mut tx, &old)
            .await
            .is_err()
    );
    let temporary = session_access::password_on(&mut tx, "worker")
        .await
        .unwrap()
        .unwrap()
        .authentication()
        .clone();
    let temporary_device = session_access::register_device_on(&mut tx, &temporary, &node(1))
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert!(current_auth(&store, &old).await);
    assert!(!current_auth(&store, &temporary).await);
    assert!(!current_admission(&store, &temporary_device).await);
    let mut connection = store.pool.acquire().await.unwrap();
    assert!(
        session_access::device_on(&mut connection, &uid, &node(1))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn session_access_generation_overflow_rolls_back_existing_credential_and_grant_writes() {
    let (store, uid, _) = fixture().await;
    store::sqlx::query("UPDATE person_auth_generation SET generation = ? WHERE person_uid = ?")
        .bind(i64::MAX)
        .bind(&uid)
        .execute(&store.pool)
        .await
        .unwrap();
    let maximum = password(&store, "worker").await;
    let role = store::auth::ensure_role(&store.pool, "worker")
        .await
        .unwrap();
    assert!(
        store::auth::create_credential(&store.pool, &uid, "worker", "replacement", role)
            .await
            .is_err()
    );
    assert!(
        store::people::deactivate(&store.pool, &uid, "2026-09-06", None)
            .await
            .is_err()
    );
    assert!(current_auth(&store, &maximum).await);
    let organ = peer(&store, &node(1)).await;
    store::logins::grant(&store.pool, &organ, &uid)
        .await
        .unwrap();
    store::sqlx::query("UPDATE organ_login_generation SET generation = ? WHERE organ_uid = ?")
        .bind(i64::MAX)
        .bind(&organ)
        .execute(&store.pool)
        .await
        .unwrap();
    assert!(store::logins::revoke(&store.pool, &organ).await.is_err());
    assert_eq!(
        store::logins::person_for_organ(&store.pool, &organ)
            .await
            .unwrap(),
        Some(uid)
    );
}

#[tokio::test]
async fn session_access_device_overflow_aborts_contact_change_without_unrevoking() {
    let (store, uid, auth) = fixture().await;
    let organ = peer(&store, &node(1)).await;
    admission(&store, &auth, &node(1)).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let before = session_access::peer_contact_on(&mut tx, &node(1))
        .await
        .unwrap();
    store::sqlx::query(
        "UPDATE person_device SET revision = ?, revoked = 1 WHERE person_uid = ? AND node_id = ?",
    )
    .bind(i64::MAX)
    .bind(&uid)
    .bind(node(1))
    .execute(&mut *tx)
    .await
    .unwrap();
    assert!(
        session_access::compare_and_set_revoked_on(&mut tx, &uid, &node(1), i64::MAX, false)
            .await
            .is_err()
    );
    tx.commit().await.unwrap();
    assert!(
        store::organs::set_trust(&store.pool, &organ, "known")
            .await
            .is_err()
    );
    let mut connection = store.pool.acquire().await.unwrap();
    assert_eq!(
        session_access::peer_contact_on(&mut connection, &node(1))
            .await
            .unwrap(),
        before
    );
    assert!(
        session_access::device_on(&mut connection, &uid, &node(1))
            .await
            .unwrap()
            .unwrap()
            .revoked
    );
}

#[tokio::test]
async fn session_access_tombstone_guards_refuse_delete_key_changes_and_decreases() {
    let (store, uid, auth) = fixture().await;
    admission(&store, &auth, &node(1)).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        store::sqlx::query("DELETE FROM person_auth_generation WHERE person_uid = ?")
            .bind(&uid)
            .execute(&mut *tx)
            .await
            .is_err()
    );
    assert!(
        store::sqlx::query(
            "UPDATE person_auth_generation SET generation = generation - 1 WHERE person_uid = ?"
        )
        .bind(&uid)
        .execute(&mut *tx)
        .await
        .is_err()
    );
    assert!(
        store::sqlx::query("UPDATE person_auth_generation SET person_uid = ? WHERE person_uid = ?")
            .bind(nucleus::new_uid("r"))
            .bind(&uid)
            .execute(&mut *tx)
            .await
            .is_err()
    );
    assert!(
        store::sqlx::query("DELETE FROM person_device WHERE person_uid = ?")
            .bind(&uid)
            .execute(&mut *tx)
            .await
            .is_err()
    );
    assert!(
        store::sqlx::query("UPDATE person_device SET node_id = ? WHERE person_uid = ?")
            .bind(node(2))
            .bind(&uid)
            .execute(&mut *tx)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn session_access_out_of_band_epoch_damage_refuses_instead_of_defaulting() {
    let (store, uid, auth) = fixture().await;
    let mut connection = store.pool.acquire().await.unwrap();
    let guards = store::sqlx::query_scalar::<_, String>("SELECT name FROM sqlite_master WHERE type = 'trigger' AND tbl_name = 'person_auth_generation'")
        .fetch_all(&mut *connection).await.unwrap();
    for guard in guards {
        let query = format!("DROP TRIGGER \"{}\"", guard.replace('"', "\"\""));
        store::sqlx::query(&query)
            .execute(&mut *connection)
            .await
            .unwrap();
    }
    store::sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&mut *connection)
        .await
        .unwrap();
    store::sqlx::query("UPDATE person_auth_generation SET generation = 0 WHERE person_uid = ?")
        .bind(&uid)
        .execute(&mut *connection)
        .await
        .unwrap();
    assert!(
        session_access::password_on(&mut connection, "worker")
            .await
            .is_err()
    );
    assert!(
        session_access::require_authentication_on(&mut connection, &auth)
            .await
            .is_err()
    );
    store::sqlx::query("DELETE FROM person_auth_generation WHERE person_uid = ?")
        .bind(&uid)
        .execute(&mut *connection)
        .await
        .unwrap();
    assert!(
        session_access::password_on(&mut connection, "worker")
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_access_separate_connections_serialize_device_cas_and_restart_retains_revocation() {
    let dir = std::env::temp_dir().join(nucleus::new_uid("session-access"));
    std::fs::create_dir_all(&dir).unwrap();
    let url = format!("sqlite://{}", dir.join("lince.db").display());
    let store = Arc::new(Store::open(&url).await.unwrap());
    let uid = person(&store, "Concurrent").await;
    credential(&store, &uid, "worker", "hash").await;
    let auth = password(&store, "worker").await;
    let admitted = admission(&store, &auth, &node(1)).await;
    let mut first = store::write_tx(&store.pool).await.unwrap();
    session_access::compare_and_set_revoked_on(
        &mut first,
        &uid,
        &node(1),
        admitted.device().revision,
        true,
    )
    .await
    .unwrap();
    let second_store = Arc::clone(&store);
    let second_uid = uid.clone();
    let expected = admitted.device().revision;
    let competing = tokio::spawn(async move {
        let mut tx = store::write_tx(&second_store.pool).await.unwrap();
        let result = session_access::compare_and_set_revoked_on(
            &mut tx,
            &second_uid,
            &node(1),
            expected,
            false,
        )
        .await;
        tx.rollback().await.unwrap();
        result
    });
    tokio::task::yield_now().await;
    first.commit().await.unwrap();
    assert!(competing.await.unwrap().is_err());
    assert!(!current_admission(&store, &admitted).await);
    store.pool.close().await;
    let reopened = Store::open(&url).await.unwrap();
    assert!(current_auth(&reopened, &auth).await);
    let mut tx = store::write_tx(&reopened.pool).await.unwrap();
    assert!(
        session_access::register_device_on(&mut tx, &auth, &node(1))
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    reopened.pool.close().await;
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn session_access_database_errors_are_not_an_admission() {
    let (store, _, auth) = fixture().await;
    let mut connection = store.pool.acquire().await.unwrap();
    store::sqlx::query("ALTER TABLE person_auth_generation RENAME TO unavailable_auth_generation")
        .execute(&mut *connection)
        .await
        .unwrap();
    assert!(
        session_access::require_authentication_on(&mut connection, &auth)
            .await
            .is_err()
    );
    assert!(
        session_access::register_device_on(&mut connection, &auth, &node(1))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn session_access_real_migration_backfills_people_grants_and_ungranted_contacts() {
    let all = store::sqlx::migrate!("./migrations");
    let before = store::sqlx::migrate::Migrator {
        migrations: Cow::Owned(
            all.iter()
                .filter(|migration| migration.version < 80)
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
    before.run(&pool).await.unwrap();
    let store = Store { pool };
    store::organs::ensure_local(&store.pool, "").await.unwrap();
    let uid = person(&store, "Existing").await;
    credential(&store, &uid, "worker", "hash").await;
    let deleted = person(&store, "Deleted").await;
    store::records::mark_deleted(&store.pool, &deleted)
        .await
        .unwrap();
    let organ = peer(&store, &node(1)).await;
    let ungranted = peer(&store, &node(2)).await;
    store::logins::grant(&store.pool, &organ, &uid)
        .await
        .unwrap();
    all.run(&store.pool).await.unwrap();
    let mut connection = store.pool.acquire().await.unwrap();
    assert_eq!(
        session_access::password_on(&mut connection, "worker")
            .await
            .unwrap()
            .unwrap()
            .authentication()
            .generation(),
        1
    );
    assert!(
        session_access::granted_login_on(&mut connection, &organ, &node(1))
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        session_access::peer_contact_on(&mut connection, &node(2))
            .await
            .unwrap()
            .unwrap()
            .organ_uid,
        ungranted
    );
    let tombstone: i64 = store::sqlx::query_scalar(
        "SELECT generation FROM person_auth_generation WHERE person_uid = ?",
    )
    .bind(&deleted)
    .fetch_one(&mut *connection)
    .await
    .unwrap();
    assert_eq!(tombstone, 1);
}

#[tokio::test]
async fn session_access_moved_organ_grant_and_contact_keys_advance_both_sides() {
    let (store, uid, auth) = fixture().await;
    let first = peer(&store, &node(1)).await;
    let second = peer(&store, &node(2)).await;
    store::logins::grant(&store.pool, &first, &uid)
        .await
        .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let first_epoch = session_access::peer_contact_on(&mut tx, &node(1))
        .await
        .unwrap()
        .unwrap()
        .generation;
    let second_epoch = session_access::peer_contact_on(&mut tx, &node(2))
        .await
        .unwrap()
        .unwrap()
        .generation;
    let old_auth = session_access::granted_login_on(&mut tx, &first, &node(1))
        .await
        .unwrap()
        .unwrap();
    store::sqlx::query("UPDATE organ_login SET organ_uid = ? WHERE organ_uid = ?")
        .bind(&second)
        .bind(&first)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(
        session_access::peer_contact_on(&mut tx, &node(1))
            .await
            .unwrap()
            .unwrap()
            .generation
            > first_epoch
    );
    assert!(
        session_access::peer_contact_on(&mut tx, &node(2))
            .await
            .unwrap()
            .unwrap()
            .generation
            > second_epoch
    );
    assert!(
        session_access::require_authentication_on(&mut tx, &old_auth)
            .await
            .is_err()
    );
    tx.commit().await.unwrap();
    let admitted = admission(&store, &auth, &node(1)).await;
    store::sqlx::query("DELETE FROM organ_contact WHERE record_uid = ?")
        .bind(&second)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE organ_contact SET record_uid = ? WHERE record_uid = ?")
        .bind(&second)
        .bind(&first)
        .execute(&store.pool)
        .await
        .unwrap();
    assert!(!current_admission(&store, &admitted).await);
    let mut connection = store.pool.acquire().await.unwrap();
    assert_eq!(
        session_access::peer_contact_on(&mut connection, &node(1))
            .await
            .unwrap()
            .unwrap()
            .organ_uid,
        second
    );
}

#[tokio::test]
async fn session_access_contact_noops_preserve_admission_and_same_node_move_bumps_once() {
    let (store, _, auth) = fixture().await;
    let first = peer(&store, &node(1)).await;
    let second = peer(&store, &node(2)).await;
    let admitted = admission(&store, &auth, &node(1)).await;
    store::organs::set_node_id(&store.pool, &first, Some(&node(1)))
        .await
        .unwrap();
    store::organs::set_trust(&store.pool, &first, "unknown")
        .await
        .unwrap();
    store::sqlx::query("UPDATE organ_contact SET proximity = proximity + 1 WHERE record_uid = ?")
        .bind(&first)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE record SET kind = kind, deleted_at = deleted_at WHERE uid = ?")
        .bind(&first)
        .execute(&store.pool)
        .await
        .unwrap();
    assert!(current_admission(&store, &admitted).await);
    store::sqlx::query("DELETE FROM organ_contact WHERE record_uid = ?")
        .bind(&second)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE organ_contact SET record_uid = ? WHERE record_uid = ?")
        .bind(&second)
        .bind(&first)
        .execute(&store.pool)
        .await
        .unwrap();
    let mut connection = store.pool.acquire().await.unwrap();
    assert_eq!(
        session_access::device_on(&mut connection, auth.person_uid(), &node(1))
            .await
            .unwrap()
            .unwrap()
            .revision,
        admitted.device().revision + 1
    );
}
