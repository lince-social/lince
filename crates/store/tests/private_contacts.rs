use std::time::Duration;

use nucleus::RecordKind;
use store::private_contacts::{self, ContactBinding};
use store::session_access::{self, AuthenticationState, ContactTrust};
use store::{Store, sqlx};

struct Fixture {
    store: Store,
    hosted: String,
    contact: String,
    person: String,
    other: String,
    node: String,
}

async fn create_record(store: &Store, hosted: &str, kind: RecordKind) -> String {
    let uid = nucleus::new_uid("r");
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::records::create_with_uid_on(
        &mut tx,
        &uid,
        store::records::NewRecord {
            slug: None,
            kind,
            head: "Fixture",
            body: "",
            quantity: store::exact::zero(),
        },
        hosted,
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    uid
}

async fn fixture_from(store: Store) -> Fixture {
    let hosted = store::organs::local(&store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let contact = create_record(&store, &hosted, RecordKind::Organ).await;
    let person = create_record(&store, &hosted, RecordKind::Person).await;
    let other = create_record(&store, &hosted, RecordKind::Person).await;
    Fixture {
        store,
        hosted,
        contact,
        person,
        other,
        node: format!("{:064x}", 0xabcd),
    }
}

async fn fixture() -> Fixture {
    fixture_from(Store::open_memory().await.unwrap()).await
}

async fn created(f: &Fixture) -> ContactBinding {
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let result = private_contacts::create_on(&mut tx, &f.hosted, &f.contact, &f.node)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    result
}

async fn read(f: &Fixture) -> ContactBinding {
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let result = private_contacts::get_on(&mut tx, &f.contact).await.unwrap();
    tx.rollback().await.unwrap();
    result
}

async fn granted(f: &Fixture) -> (ContactBinding, AuthenticationState) {
    let current = created(f).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let result =
        private_contacts::grant_on(&mut tx, &f.contact, current.peer.generation, &f.person)
            .await
            .unwrap();
    let auth = session_access::granted_login_on(&mut tx, &f.contact, &f.node)
        .await
        .unwrap()
        .unwrap();
    tx.commit().await.unwrap();
    (result, auth)
}

async fn generation(connection: &mut sqlx::SqliteConnection, uid: &str) -> i64 {
    sqlx::query_scalar("SELECT generation FROM organ_login_generation WHERE organ_uid = ?")
        .bind(uid)
        .fetch_one(connection)
        .await
        .unwrap()
}

async fn contact_count(connection: &mut sqlx::SqliteConnection, uid: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM organ_contact WHERE CAST(record_uid AS TEXT) = ?")
        .bind(uid)
        .fetch_one(connection)
        .await
        .unwrap()
}

async fn security_counts(
    connection: &mut sqlx::SqliteConnection,
) -> (i64, i64, i64, i64, i64, i64, i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM person_credential),
                (SELECT COUNT(*) FROM person_device),
                (SELECT COUNT(*) FROM record WHERE kind = 'device'),
                (SELECT COUNT(*) FROM replica_grant),
                (SELECT COUNT(*) FROM visibility_rule),
                (SELECT COUNT(*) FROM sync_op),
                (SELECT COUNT(*) FROM fact),
                (SELECT COUNT(*) FROM person_access)",
    )
    .fetch_one(connection)
    .await
    .unwrap()
}

async fn all_refuse(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, f: &Fixture, epoch: i64) {
    assert!(private_contacts::get_on(tx, &f.contact).await.is_err());
    assert!(
        private_contacts::grant_on(tx, &f.contact, epoch, &f.other)
            .await
            .is_err()
    );
    assert!(
        private_contacts::replace_on(tx, &f.contact, epoch, &f.other)
            .await
            .is_err()
    );
    assert!(
        private_contacts::revoke_on(tx, &f.contact, epoch)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn private_contacts_creation_is_closed_company_origin_and_not_admission() {
    let f = fixture().await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let before = security_counts(&mut tx).await;
    let record_before: (String, String, String, String, Option<String>) = sqlx::query_as(
        "SELECT kind, organ_uid, head, body, replica_root FROM record WHERE uid = ?",
    )
    .bind(&f.contact)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let epoch = generation(&mut tx, &f.contact).await;
    let result = private_contacts::create_on(&mut tx, &f.hosted, &f.contact, &f.node)
        .await
        .unwrap();
    assert_eq!(result.peer.organ_uid, f.contact);
    assert_eq!(result.peer.node_id, f.node);
    assert_eq!(result.peer.trust, ContactTrust::Unknown);
    assert!(result.peer.generation > epoch);
    assert_eq!(result.person_uid, None);
    assert_eq!(
        session_access::peer_contact_on(&mut tx, &f.node)
            .await
            .unwrap(),
        Some(result.peer)
    );
    assert!(
        session_access::granted_login_on(&mut tx, &f.contact, &f.node)
            .await
            .unwrap()
            .is_none()
    );
    let closed: i64 = sqlx::query_scalar(
        "SELECT sync_out = 0 AND sync_in = 0 AND closed_by_default = 1
             AND trust = 'unknown' AND scope_fields = '[]' AND accept_fields = '[]'
             AND scope_version = 0 AND accept_version = 0 AND pending_introduction = 0
             AND mode = 'direct' AND proximity = 1 AND catchup_interval_secs = 30
             AND last_synced_seq = 0 AND peer_acked_seq = 0 AND share_protein IS NULL
             AND share_seen_seq IS NULL AND unreachable_since IS NULL
             AND mailed_at IS NULL AND awaiting_roster_since IS NULL
           FROM organ_contact WHERE record_uid = ?",
    )
    .bind(&f.contact)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(closed, 1);
    let record_after = sqlx::query_as::<_, (String, String, String, String, Option<String>)>(
        "SELECT kind, organ_uid, head, body, replica_root FROM record WHERE uid = ?",
    )
    .bind(&f.contact)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(record_after, record_before);
    assert_eq!(record_after.1, f.hosted);
    assert_eq!(security_counts(&mut tx).await, before);
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn private_contacts_creation_requires_exact_absence() {
    let f = fixture().await;
    let current = created(&f).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert!(
        private_contacts::create_on(&mut tx, &f.hosted, &f.contact, &f.node)
            .await
            .is_err()
    );
    assert!(
        private_contacts::create_on(&mut tx, &f.hosted, &f.contact, &format!("{:064x}", 42))
            .await
            .is_err()
    );
    assert_eq!(
        private_contacts::get_on(&mut tx, &f.contact).await.unwrap(),
        current
    );
    assert_eq!(contact_count(&mut tx, &f.contact).await, 1);
}

#[tokio::test]
async fn private_contacts_creation_refuses_missing_deleted_wrong_kind_and_foreign_records() {
    let f = fixture().await;
    for mutation in [
        "UPDATE record SET deleted_at = '2026-09-07T00:00:00Z' WHERE uid = ?",
        "UPDATE record SET kind = 'plain' WHERE uid = ?",
        "UPDATE record SET organ_uid = uid WHERE uid = ?",
        "UPDATE record SET organ_uid = CAST(organ_uid AS BLOB) WHERE uid = ?",
    ] {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        sqlx::query(mutation)
            .bind(&f.contact)
            .execute(&mut *tx)
            .await
            .unwrap();
        let epoch = generation(&mut tx, &f.contact).await;
        assert!(
            private_contacts::create_on(&mut tx, &f.hosted, &f.contact, &f.node)
                .await
                .is_err()
        );
        assert_eq!(contact_count(&mut tx, &f.contact).await, 0);
        assert_eq!(generation(&mut tx, &f.contact).await, epoch);
        tx.rollback().await.unwrap();
    }
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    for target in [
        nucleus::new_uid("r"),
        f.person.clone(),
        f.hosted.clone(),
        "invalid".into(),
    ] {
        assert!(
            private_contacts::create_on(&mut tx, &f.hosted, &target, &f.node)
                .await
                .is_err()
        );
    }
    for hosted in [nucleus::new_uid("r"), f.person.clone(), "invalid".into()] {
        assert!(
            private_contacts::create_on(&mut tx, &hosted, &f.contact, &f.node)
                .await
                .is_err()
        );
    }
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn private_contacts_creation_requires_a_live_hosted_organ() {
    let f = fixture().await;
    for mutation in [
        "UPDATE record SET kind = 'plain' WHERE uid = ?",
        "UPDATE record SET deleted_at = '2026-09-07T00:00:00Z' WHERE uid = ?",
    ] {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        sqlx::query(mutation)
            .bind(&f.hosted)
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            private_contacts::create_on(&mut tx, &f.hosted, &f.contact, &f.node)
                .await
                .is_err()
        );
        assert_eq!(contact_count(&mut tx, &f.contact).await, 0);
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn private_contacts_node_input_is_exact_canonical_transport_identity() {
    let f = fixture().await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    for node in [
        String::new(),
        "a".repeat(63),
        "a".repeat(65),
        "g".repeat(64),
        f.node.to_uppercase(),
        format!(" {}", f.node),
        format!("{} ", f.node),
        "é".repeat(32),
    ] {
        assert!(
            private_contacts::create_on(&mut tx, &f.hosted, &f.contact, &node)
                .await
                .is_err()
        );
    }
    assert_eq!(contact_count(&mut tx, &f.contact).await, 0);
}

#[tokio::test]
async fn private_contacts_node_collision_including_malformed_aliases_refuses() {
    let f = fixture().await;
    let other_contact = create_record(&f.store, &f.hosted, RecordKind::Organ).await;
    for node in [
        f.node.clone(),
        f.node.to_uppercase(),
        format!(" {} ", f.node),
    ] {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        sqlx::query("INSERT INTO organ_contact (record_uid, node_id) VALUES (?, ?)")
            .bind(&other_contact)
            .bind(node)
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            private_contacts::create_on(&mut tx, &f.hosted, &f.contact, &f.node)
                .await
                .is_err()
        );
        assert_eq!(contact_count(&mut tx, &f.contact).await, 0);
        tx.rollback().await.unwrap();
    }
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    sqlx::query("INSERT INTO organ_contact (record_uid, node_id) VALUES (?, CAST(? AS BLOB))")
        .bind(&other_contact)
        .bind(&f.node)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(
        private_contacts::create_on(&mut tx, &f.hosted, &f.contact, &f.node)
            .await
            .is_err()
    );
    assert_eq!(contact_count(&mut tx, &f.contact).await, 0);
}

#[tokio::test]
async fn private_contacts_ambiguous_current_node_refuses_all_operations() {
    let f = fixture().await;
    let current = created(&f).await;
    let other_contact = create_record(&f.store, &f.hosted, RecordKind::Organ).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    sqlx::query("INSERT INTO organ_contact (record_uid, node_id) VALUES (?, ?)")
        .bind(&other_contact)
        .bind(f.node.to_uppercase())
        .execute(&mut *tx)
        .await
        .unwrap();
    all_refuse(&mut tx, &f, current.peer.generation).await;
}

#[tokio::test]
async fn private_contacts_grant_credential_free_person_without_other_authority_writes() {
    let f = fixture().await;
    let current = created(&f).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let before = security_counts(&mut tx).await;
    let credentials: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM person_credential WHERE person_uid = ?")
            .bind(&f.person)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(credentials, 0);
    let result =
        private_contacts::grant_on(&mut tx, &f.contact, current.peer.generation, &f.person)
            .await
            .unwrap();
    assert!(result.peer.generation > current.peer.generation);
    assert_eq!(result.person_uid.as_deref(), Some(f.person.as_str()));
    let auth = session_access::granted_login_on(&mut tx, &f.contact, &f.node)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(auth.person_uid(), f.person);
    session_access::require_authentication_on(&mut tx, &auth)
        .await
        .unwrap();
    assert_eq!(
        session_access::peer_contact_on(&mut tx, &f.node)
            .await
            .unwrap(),
        Some(result.peer)
    );
    assert_eq!(security_counts(&mut tx).await, before);
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn private_contacts_login_modes_have_distinct_presence_preconditions() {
    let f = fixture().await;
    let current = created(&f).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert!(
        private_contacts::replace_on(&mut tx, &f.contact, current.peer.generation, &f.person)
            .await
            .is_err()
    );
    assert!(
        private_contacts::revoke_on(&mut tx, &f.contact, current.peer.generation)
            .await
            .is_err()
    );
    let granted =
        private_contacts::grant_on(&mut tx, &f.contact, current.peer.generation, &f.person)
            .await
            .unwrap();
    assert!(
        private_contacts::grant_on(&mut tx, &f.contact, granted.peer.generation, &f.person)
            .await
            .is_err()
    );
    assert!(
        private_contacts::grant_on(&mut tx, &f.contact, granted.peer.generation, &f.other)
            .await
            .is_err()
    );
    assert_eq!(
        private_contacts::get_on(&mut tx, &f.contact).await.unwrap(),
        granted
    );
}

#[tokio::test]
async fn private_contacts_replace_revoke_regrant_invalidate_actual_n12_authentication() {
    let f = fixture().await;
    let (current, old_auth) = granted(&f).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let replaced =
        private_contacts::replace_on(&mut tx, &f.contact, current.peer.generation, &f.other)
            .await
            .unwrap();
    assert!(replaced.peer.generation > current.peer.generation);
    assert!(
        session_access::require_authentication_on(&mut tx, &old_auth)
            .await
            .is_err()
    );
    let replacement_auth = session_access::granted_login_on(&mut tx, &f.contact, &f.node)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(replacement_auth.person_uid(), f.other);
    let revoked = private_contacts::revoke_on(&mut tx, &f.contact, replaced.peer.generation)
        .await
        .unwrap();
    assert!(revoked.peer.generation > replaced.peer.generation);
    assert_eq!(revoked.person_uid, None);
    assert!(
        session_access::granted_login_on(&mut tx, &f.contact, &f.node)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        session_access::require_authentication_on(&mut tx, &replacement_auth)
            .await
            .is_err()
    );
    let restored =
        private_contacts::grant_on(&mut tx, &f.contact, revoked.peer.generation, &f.person)
            .await
            .unwrap();
    assert_eq!(restored.person_uid, current.person_uid);
    assert!(restored.peer.generation > revoked.peer.generation);
    assert!(
        session_access::require_authentication_on(&mut tx, &old_auth)
            .await
            .is_err()
    );
    let fresh = session_access::granted_login_on(&mut tx, &f.contact, &f.node)
        .await
        .unwrap()
        .unwrap();
    session_access::require_authentication_on(&mut tx, &fresh)
        .await
        .unwrap();
    assert_eq!(
        session_access::peer_contact_on(&mut tx, &f.node)
            .await
            .unwrap(),
        Some(restored.peer)
    );
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn private_contacts_expected_generation_is_positive_exact_and_not_presence_only() {
    let f = fixture().await;
    let current = created(&f).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    for epoch in [
        0,
        -1,
        current.peer.generation - 1,
        current.peer.generation + 1,
    ] {
        assert!(
            private_contacts::grant_on(&mut tx, &f.contact, epoch, &f.person)
                .await
                .is_err()
        );
    }
    let granted =
        private_contacts::grant_on(&mut tx, &f.contact, current.peer.generation, &f.person)
            .await
            .unwrap();
    for epoch in [0, -1, current.peer.generation, granted.peer.generation + 1] {
        assert!(
            private_contacts::replace_on(&mut tx, &f.contact, epoch, &f.other)
                .await
                .is_err()
        );
        assert!(
            private_contacts::revoke_on(&mut tx, &f.contact, epoch)
                .await
                .is_err()
        );
    }
    assert_eq!(
        private_contacts::get_on(&mut tx, &f.contact).await.unwrap(),
        granted
    );
}

#[tokio::test]
async fn private_contacts_disabled_former_grantee_can_be_revoked_or_replaced() {
    let f = fixture().await;
    let (current, auth) = granted(&f).await;
    store::people::deactivate(&f.store.pool, &f.person, "2026-09-07T00:00:00Z", None)
        .await
        .unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert!(
        session_access::require_authentication_on(&mut tx, &auth)
            .await
            .is_err()
    );
    assert!(
        private_contacts::replace_on(&mut tx, &f.contact, current.peer.generation, &f.person)
            .await
            .is_err()
    );
    let revoked = private_contacts::revoke_on(&mut tx, &f.contact, current.peer.generation)
        .await
        .unwrap();
    assert_eq!(revoked.person_uid, None);
    tx.rollback().await.unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let replaced =
        private_contacts::replace_on(&mut tx, &f.contact, current.peer.generation, &f.other)
            .await
            .unwrap();
    assert_eq!(replaced.person_uid.as_deref(), Some(f.other.as_str()));
}

#[tokio::test]
async fn private_contacts_soft_deleted_former_person_can_be_revoked_without_content() {
    let f = fixture().await;
    let (current, _) = granted(&f).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    sqlx::query("UPDATE record SET deleted_at = '2026-09-07T00:00:00Z' WHERE uid = ?")
        .bind(&f.person)
        .execute(&mut *tx)
        .await
        .unwrap();
    let revoked = private_contacts::revoke_on(&mut tx, &f.contact, current.peer.generation)
        .await
        .unwrap();
    assert_eq!(revoked.person_uid, None);
}

#[tokio::test]
async fn private_contacts_proposed_person_must_be_active_live_valid_and_bounded() {
    let f = fixture().await;
    let current = created(&f).await;
    for mutation in [
        "UPDATE record SET kind = 'plain' WHERE uid = ?",
        "UPDATE record SET deleted_at = '2026-09-07T00:00:00Z' WHERE uid = ?",
        "INSERT INTO record_extension (record_uid, namespace, fds) VALUES (?, 'lince.person', '{')",
        "INSERT INTO record_extension (record_uid, namespace, fds) VALUES (?, 'lince.person', '{\"standing\":{\"active\":false}}')",
        "INSERT INTO record_extension (record_uid, namespace, fds) VALUES (?, 'lince.person', CAST(zeroblob(65537) AS TEXT))",
    ] {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        sqlx::query("PRAGMA ignore_check_constraints = ON")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(mutation)
            .bind(&f.person)
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            private_contacts::grant_on(&mut tx, &f.contact, current.peer.generation, &f.person)
                .await
                .is_err()
        );
        let granted =
            private_contacts::grant_on(&mut tx, &f.contact, current.peer.generation, &f.other)
                .await
                .unwrap();
        assert!(
            private_contacts::replace_on(&mut tx, &f.contact, granted.peer.generation, &f.person)
                .await
                .is_err()
        );
        assert_eq!(
            private_contacts::get_on(&mut tx, &f.contact).await.unwrap(),
            granted
        );
        tx.rollback().await.unwrap();
    }
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    for person in [
        nucleus::new_uid("r"),
        f.contact.clone(),
        "p_invalid".into(),
        "x".repeat(100_000),
    ] {
        assert!(
            private_contacts::grant_on(&mut tx, &f.contact, current.peer.generation, &person)
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn private_contacts_blocked_corrupt_or_deleted_current_peer_is_not_mutable() {
    let f = fixture().await;
    let (current, _) = granted(&f).await;
    for mutation in [
        "UPDATE organ_contact SET trust = 'blocked' WHERE record_uid = ?",
        "UPDATE organ_contact SET trust = 'trusted' WHERE record_uid = ?",
        "UPDATE organ_contact SET trust = CAST('known' AS BLOB) WHERE record_uid = ?",
        "UPDATE organ_contact SET node_id = upper(node_id) WHERE record_uid = ?",
        "UPDATE organ_contact SET node_id = CAST(node_id AS BLOB) WHERE record_uid = ?",
        "UPDATE organ_contact SET node_id = CAST(zeroblob(100000) AS TEXT) WHERE record_uid = ?",
        "UPDATE record SET kind = 'plain' WHERE uid = ?",
        "UPDATE record SET deleted_at = '2026-09-07T00:00:00Z' WHERE uid = ?",
        "DELETE FROM organ_contact WHERE record_uid = ?",
    ] {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        sqlx::query(mutation)
            .bind(&f.contact)
            .execute(&mut *tx)
            .await
            .unwrap();
        let epoch = generation(&mut tx, &f.contact).await;
        assert!(epoch >= current.peer.generation);
        all_refuse(&mut tx, &f, epoch).await;
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn private_contacts_known_peer_is_supported_but_trust_roundtrip_stales_epoch() {
    let f = fixture().await;
    let current = created(&f).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    sqlx::query("UPDATE organ_contact SET trust = 'known' WHERE record_uid = ?")
        .bind(&f.contact)
        .execute(&mut *tx)
        .await
        .unwrap();
    let known = private_contacts::get_on(&mut tx, &f.contact).await.unwrap();
    assert_eq!(known.peer.trust, ContactTrust::Known);
    assert!(
        private_contacts::grant_on(&mut tx, &f.contact, current.peer.generation, &f.person)
            .await
            .is_err()
    );
    let granted = private_contacts::grant_on(&mut tx, &f.contact, known.peer.generation, &f.person)
        .await
        .unwrap();
    assert_eq!(granted.peer.trust, ContactTrust::Known);
    let auth = session_access::granted_login_on(&mut tx, &f.contact, &f.node)
        .await
        .unwrap()
        .unwrap();
    for trust in ["blocked", "known"] {
        sqlx::query("UPDATE organ_contact SET trust = ? WHERE record_uid = ?")
            .bind(trust)
            .bind(&f.contact)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    assert!(
        session_access::require_authentication_on(&mut tx, &auth)
            .await
            .is_err()
    );
    assert!(
        private_contacts::revoke_on(&mut tx, &f.contact, granted.peer.generation)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn private_contacts_exact_unchanged_replace_is_noop_even_at_maximum_generation() {
    let f = fixture().await;
    let (current, auth) = granted(&f).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let unchanged =
        private_contacts::replace_on(&mut tx, &f.contact, current.peer.generation, &f.person)
            .await
            .unwrap();
    assert_eq!(unchanged, current);
    session_access::require_authentication_on(&mut tx, &auth)
        .await
        .unwrap();
    sqlx::query("UPDATE organ_login_generation SET generation = ? WHERE organ_uid = ?")
        .bind(i64::MAX)
        .bind(&f.contact)
        .execute(&mut *tx)
        .await
        .unwrap();
    let maximum = private_contacts::get_on(&mut tx, &f.contact).await.unwrap();
    assert_eq!(
        private_contacts::replace_on(&mut tx, &f.contact, i64::MAX, &f.person)
            .await
            .unwrap(),
        maximum
    );
    assert!(
        private_contacts::replace_on(&mut tx, &f.contact, i64::MAX, &f.other)
            .await
            .is_err()
    );
    assert!(
        private_contacts::revoke_on(&mut tx, &f.contact, i64::MAX)
            .await
            .is_err()
    );
    assert_eq!(
        private_contacts::get_on(&mut tx, &f.contact).await.unwrap(),
        maximum
    );
}

#[tokio::test]
async fn private_contacts_creation_and_grant_refuse_exhausted_generation() {
    let f = fixture().await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    sqlx::query("UPDATE organ_login_generation SET generation = ? WHERE organ_uid = ?")
        .bind(i64::MAX)
        .bind(&f.contact)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(
        private_contacts::create_on(&mut tx, &f.hosted, &f.contact, &f.node)
            .await
            .is_err()
    );
    assert_eq!(contact_count(&mut tx, &f.contact).await, 0);
    tx.rollback().await.unwrap();
    created(&f).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    sqlx::query("UPDATE organ_login_generation SET generation = ? WHERE organ_uid = ?")
        .bind(i64::MAX)
        .bind(&f.contact)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(
        private_contacts::grant_on(&mut tx, &f.contact, i64::MAX, &f.person)
            .await
            .is_err()
    );
    assert_eq!(
        private_contacts::get_on(&mut tx, &f.contact)
            .await
            .unwrap()
            .person_uid,
        None
    );
}

#[tokio::test]
async fn private_contacts_missing_and_corrupt_generation_never_default_or_repair() {
    let f = fixture().await;
    for damage in [
        "DELETE FROM organ_login_generation WHERE organ_uid = ?",
        "UPDATE organ_login_generation SET generation = 0 WHERE organ_uid = ?",
    ] {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        sqlx::query("DROP TRIGGER organ_login_generation_immutable_delete")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("DROP TRIGGER organ_login_generation_monotonic_update")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("PRAGMA ignore_check_constraints = ON")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(damage)
            .bind(&f.contact)
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            private_contacts::create_on(&mut tx, &f.hosted, &f.contact, &f.node)
                .await
                .is_err()
        );
        assert_eq!(contact_count(&mut tx, &f.contact).await, 0);
        tx.rollback().await.unwrap();
    }
    created(&f).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    sqlx::query("DROP TRIGGER organ_login_generation_immutable_delete")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("DELETE FROM organ_login_generation WHERE organ_uid = ?")
        .bind(&f.contact)
        .execute(&mut *tx)
        .await
        .unwrap();
    all_refuse(&mut tx, &f, 1).await;
}

#[tokio::test]
async fn private_contacts_login_timestamp_is_bounded_typed_and_preserved_on_replace() {
    let f = fixture().await;
    let (current, _) = granted(&f).await;
    for timestamp in [
        String::new(),
        "not a timestamp".into(),
        "x".repeat(private_contacts::MAX_LOGIN_TIMESTAMP_BYTES + 1),
        "x".repeat(100_000),
    ] {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        sqlx::query("UPDATE organ_login SET created_at = ? WHERE organ_uid = ?")
            .bind(timestamp)
            .bind(&f.contact)
            .execute(&mut *tx)
            .await
            .unwrap();
        let epoch = generation(&mut tx, &f.contact).await;
        all_refuse(&mut tx, &f, epoch).await;
        tx.rollback().await.unwrap();
    }
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let timestamp: String =
        sqlx::query_scalar("SELECT created_at FROM organ_login WHERE organ_uid = ?")
            .bind(&f.contact)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    private_contacts::replace_on(&mut tx, &f.contact, current.peer.generation, &f.other)
        .await
        .unwrap();
    let after: String =
        sqlx::query_scalar("SELECT created_at FROM organ_login WHERE organ_uid = ?")
            .bind(&f.contact)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(after, timestamp);
}

#[tokio::test]
async fn private_contacts_corrupt_login_types_and_wrong_kind_reference_refuse() {
    let f = fixture().await;
    granted(&f).await;
    for mutation in [
        "UPDATE organ_login SET created_at = CAST(created_at AS BLOB) WHERE organ_uid = ?",
        "UPDATE organ_login SET person_uid = organ_uid WHERE organ_uid = ?",
    ] {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        sqlx::query(mutation)
            .bind(&f.contact)
            .execute(&mut *tx)
            .await
            .unwrap();
        let epoch = generation(&mut tx, &f.contact).await;
        all_refuse(&mut tx, &f, epoch).await;
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn private_contacts_second_organ_cannot_steal_unique_person_login() {
    let f = fixture().await;
    let (original, auth) = granted(&f).await;
    let second = create_record(&f.store, &f.hosted, RecordKind::Organ).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let empty = private_contacts::create_on(&mut tx, &f.hosted, &second, &format!("{:064x}", 99))
        .await
        .unwrap();
    store::records::set_authoring_text_on(&mut tx, &second, Some("caller write"), None)
        .await
        .unwrap();
    assert!(
        private_contacts::grant_on(&mut tx, &second, empty.peer.generation, &f.person)
            .await
            .is_err()
    );
    assert_eq!(
        private_contacts::get_on(&mut tx, &second).await.unwrap(),
        empty
    );
    assert_eq!(
        private_contacts::get_on(&mut tx, &f.contact).await.unwrap(),
        original
    );
    session_access::require_authentication_on(&mut tx, &auth)
        .await
        .unwrap();
    let other = private_contacts::grant_on(&mut tx, &second, empty.peer.generation, &f.other)
        .await
        .unwrap();
    assert!(
        private_contacts::replace_on(&mut tx, &second, other.peer.generation, &f.person)
            .await
            .is_err()
    );
    assert_eq!(
        private_contacts::get_on(&mut tx, &second).await.unwrap(),
        other
    );
    tx.commit().await.unwrap();
    let head: String = sqlx::query_scalar("SELECT head FROM record WHERE uid = ?")
        .bind(second)
        .fetch_one(&f.store.pool)
        .await
        .unwrap();
    assert_eq!(head, "caller write");
}

#[tokio::test]
async fn private_contacts_existing_login_without_contact_is_not_adopted_by_creation() {
    let f = fixture().await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    sqlx::query("INSERT INTO organ_login (organ_uid, person_uid, created_at) VALUES (?, ?, '2026-09-07T00:00:00Z')")
        .bind(&f.contact).bind(&f.person).execute(&mut *tx).await.unwrap();
    let epoch = generation(&mut tx, &f.contact).await;
    assert!(
        private_contacts::create_on(&mut tx, &f.hosted, &f.contact, &f.node)
            .await
            .is_err()
    );
    assert_eq!(contact_count(&mut tx, &f.contact).await, 0);
    assert_eq!(generation(&mut tx, &f.contact).await, epoch);
}

#[tokio::test]
async fn private_contacts_removal_and_recreation_keep_the_retained_binding_epoch() {
    let f = fixture().await;
    let current = created(&f).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    sqlx::query("DELETE FROM organ_contact WHERE record_uid = ?")
        .bind(&f.contact)
        .execute(&mut *tx)
        .await
        .unwrap();
    let removed = generation(&mut tx, &f.contact).await;
    assert!(removed > current.peer.generation);
    let recreated = private_contacts::create_on(&mut tx, &f.hosted, &f.contact, &f.node)
        .await
        .unwrap();
    assert!(recreated.peer.generation > removed);
    assert!(
        private_contacts::grant_on(&mut tx, &f.contact, current.peer.generation, &f.person)
            .await
            .is_err()
    );
    assert_eq!(
        session_access::peer_contact_on(&mut tx, &f.node)
            .await
            .unwrap(),
        Some(recreated.peer)
    );
}

#[tokio::test]
async fn private_contacts_malformed_login_identity_storage_refuses_before_projection() {
    let f = fixture().await;
    granted(&f).await;
    for mutation in [
        "UPDATE organ_login SET person_uid = CAST(person_uid AS BLOB) WHERE organ_uid = ?",
        "UPDATE organ_login SET person_uid = CAST(zeroblob(100000) AS TEXT) WHERE organ_uid = ?",
        "UPDATE organ_login SET organ_uid = CAST(organ_uid AS BLOB) WHERE organ_uid = ?",
        "UPDATE organ_login SET person_uid = 'r_01ARZ3NDEKTSV4RRFFQ69G5FAV' WHERE organ_uid = ?",
    ] {
        let mut tx = store::write_tx(&f.store.pool).await.unwrap();
        sqlx::query("PRAGMA defer_foreign_keys = ON")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(mutation)
            .bind(&f.contact)
            .execute(&mut *tx)
            .await
            .unwrap();
        let epoch = generation(&mut tx, &f.contact).await;
        all_refuse(&mut tx, &f, epoch).await;
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn private_contacts_malformed_contact_identity_is_not_an_absent_contact() {
    let f = fixture().await;
    created(&f).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    sqlx::query("PRAGMA defer_foreign_keys = ON")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE organ_contact SET record_uid = CAST(record_uid AS BLOB) WHERE record_uid = ?",
    )
    .bind(&f.contact)
    .execute(&mut *tx)
    .await
    .unwrap();
    let epoch = generation(&mut tx, &f.contact).await;
    all_refuse(&mut tx, &f, epoch).await;
    assert!(
        private_contacts::create_on(&mut tx, &f.hosted, &f.contact, &f.node)
            .await
            .is_err()
    );
    assert_eq!(contact_count(&mut tx, &f.contact).await, 1);
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn private_contacts_creation_postvalidation_failure_rolls_back_only_its_savepoint() {
    let f = fixture().await;
    sqlx::query("CREATE TRIGGER private_contacts_bad_metadata AFTER INSERT ON organ_contact BEGIN UPDATE organ_contact SET sync_out = 1 WHERE record_uid = NEW.record_uid; END")
        .execute(&f.store.pool).await.unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    store::records::set_authoring_text_on(&mut tx, &f.contact, Some("earlier caller write"), None)
        .await
        .unwrap();
    let before = generation(&mut tx, &f.contact).await;
    assert!(
        private_contacts::create_on(&mut tx, &f.hosted, &f.contact, &f.node)
            .await
            .is_err()
    );
    assert_eq!(contact_count(&mut tx, &f.contact).await, 0);
    assert_eq!(generation(&mut tx, &f.contact).await, before);
    assert!(
        session_access::peer_contact_on(&mut tx, &f.node)
            .await
            .unwrap()
            .is_none()
    );
    tx.commit().await.unwrap();
    let head: String = sqlx::query_scalar("SELECT head FROM record WHERE uid = ?")
        .bind(&f.contact)
        .fetch_one(&f.store.pool)
        .await
        .unwrap();
    assert_eq!(head, "earlier caller write");
}

#[tokio::test]
async fn private_contacts_replacement_postvalidation_failure_restores_login_and_epoch() {
    let f = fixture().await;
    let (current, auth) = granted(&f).await;
    sqlx::query("CREATE TRIGGER private_contacts_bad_timestamp AFTER UPDATE OF person_uid ON organ_login BEGIN UPDATE organ_login SET created_at = 'invalid' WHERE organ_uid = NEW.organ_uid; END")
        .execute(&f.store.pool).await.unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    store::records::set_authoring_text_on(&mut tx, &f.contact, Some("caller write survives"), None)
        .await
        .unwrap();
    assert!(
        private_contacts::replace_on(&mut tx, &f.contact, current.peer.generation, &f.other)
            .await
            .is_err()
    );
    assert_eq!(
        private_contacts::get_on(&mut tx, &f.contact).await.unwrap(),
        current
    );
    session_access::require_authentication_on(&mut tx, &auth)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(read(&f).await, current);
    let head: String = sqlx::query_scalar("SELECT head FROM record WHERE uid = ?")
        .bind(&f.contact)
        .fetch_one(&f.store.pool)
        .await
        .unwrap();
    assert_eq!(head, "caller write survives");
}

#[tokio::test]
async fn private_contacts_revoke_sql_failure_preserves_binding_and_prior_caller_work() {
    let f = fixture().await;
    let (current, auth) = granted(&f).await;
    sqlx::query("CREATE TRIGGER private_contacts_revoke_failure BEFORE DELETE ON organ_login BEGIN SELECT RAISE(ABORT, 'injected refusal'); END")
        .execute(&f.store.pool).await.unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    store::records::set_authoring_text_on(&mut tx, &f.contact, Some("retained caller write"), None)
        .await
        .unwrap();
    assert!(
        private_contacts::revoke_on(&mut tx, &f.contact, current.peer.generation)
            .await
            .is_err()
    );
    assert_eq!(
        private_contacts::get_on(&mut tx, &f.contact).await.unwrap(),
        current
    );
    session_access::require_authentication_on(&mut tx, &auth)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let head: String = sqlx::query_scalar("SELECT head FROM record WHERE uid = ?")
        .bind(&f.contact)
        .fetch_one(&f.store.pool)
        .await
        .unwrap();
    assert_eq!(head, "retained caller write");
}

#[tokio::test]
async fn private_contacts_cancellation_after_insert_restores_only_inner_savepoint() {
    let f = fixture().await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    store::records::set_authoring_text_on(&mut tx, &f.contact, Some("before cancellation"), None)
        .await
        .unwrap();
    let before = generation(&mut tx, &f.contact).await;
    let (reached_sender, reached_receiver) = tokio::sync::oneshot::channel();
    let (release_sender, release_receiver) = std::sync::mpsc::channel();
    let mut reached_sender = Some(reached_sender);
    tx.lock_handle()
        .await
        .unwrap()
        .set_update_hook(move |update| {
            if update.table == "organ_contact"
                && matches!(update.operation, sqlx::sqlite::SqliteOperation::Insert)
                && let Some(sender) = reached_sender.take()
            {
                let _ = sender.send(());
                let _ = release_receiver.recv_timeout(Duration::from_secs(5));
            }
        });
    let mut creating = Box::pin(private_contacts::create_on(
        &mut tx, &f.hosted, &f.contact, &f.node,
    ));
    let reached = tokio::select! {
        result = &mut creating => panic!("creation completed before cancellation gate: {result:?}"),
        result = tokio::time::timeout(Duration::from_secs(3), reached_receiver) => result,
    };
    drop(creating);
    let released = release_sender.send(());
    reached.unwrap().unwrap();
    released.unwrap();
    assert_eq!(contact_count(&mut tx, &f.contact).await, 0);
    assert_eq!(generation(&mut tx, &f.contact).await, before);
    tx.lock_handle().await.unwrap().remove_update_hook();
    tx.commit().await.unwrap();
    let head: String = sqlx::query_scalar("SELECT head FROM record WHERE uid = ?")
        .bind(&f.contact)
        .fetch_one(&f.store.pool)
        .await
        .unwrap();
    assert_eq!(head, "before cancellation");
    created(&f).await;
}

#[tokio::test]
async fn private_contacts_cancellation_after_replacement_restores_login_and_epoch() {
    let f = fixture().await;
    let (current, auth) = granted(&f).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    store::records::set_authoring_text_on(
        &mut tx,
        &f.contact,
        Some("before replacement cancellation"),
        None,
    )
    .await
    .unwrap();
    let (reached_sender, reached_receiver) = tokio::sync::oneshot::channel();
    let (release_sender, release_receiver) = std::sync::mpsc::channel();
    let mut reached_sender = Some(reached_sender);
    tx.lock_handle()
        .await
        .unwrap()
        .set_update_hook(move |update| {
            if update.table == "organ_login"
                && matches!(update.operation, sqlx::sqlite::SqliteOperation::Update)
                && let Some(sender) = reached_sender.take()
            {
                let _ = sender.send(());
                let _ = release_receiver.recv_timeout(Duration::from_secs(5));
            }
        });
    let mut replacing = Box::pin(private_contacts::replace_on(
        &mut tx,
        &f.contact,
        current.peer.generation,
        &f.other,
    ));
    let reached = tokio::select! {
        result = &mut replacing => panic!("replacement completed before cancellation gate: {result:?}"),
        result = tokio::time::timeout(Duration::from_secs(3), reached_receiver) => result,
    };
    drop(replacing);
    let released = release_sender.send(());
    reached.unwrap().unwrap();
    released.unwrap();
    assert_eq!(
        private_contacts::get_on(&mut tx, &f.contact).await.unwrap(),
        current
    );
    session_access::require_authentication_on(&mut tx, &auth)
        .await
        .unwrap();
    tx.lock_handle().await.unwrap().remove_update_hook();
    tx.commit().await.unwrap();
    let head: String = sqlx::query_scalar("SELECT head FROM record WHERE uid = ?")
        .bind(&f.contact)
        .fetch_one(&f.store.pool)
        .await
        .unwrap();
    assert_eq!(head, "before replacement cancellation");
}

#[tokio::test]
async fn private_contacts_outer_rollback_restores_complete_contact_and_login_state() {
    let f = fixture().await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let before = generation(&mut tx, &f.contact).await;
    let created = private_contacts::create_on(&mut tx, &f.hosted, &f.contact, &f.node)
        .await
        .unwrap();
    let granted =
        private_contacts::grant_on(&mut tx, &f.contact, created.peer.generation, &f.person)
            .await
            .unwrap();
    assert!(granted.peer.generation > created.peer.generation);
    tx.rollback().await.unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    assert_eq!(contact_count(&mut tx, &f.contact).await, 0);
    assert_eq!(generation(&mut tx, &f.contact).await, before);
    assert!(
        session_access::granted_login_on(&mut tx, &f.contact, &f.node)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn private_contacts_creation_invalidates_existing_unknown_peer_device_without_enrollment() {
    let f = fixture().await;
    let role = store::auth::ensure_role(&f.store.pool, "fixture role")
        .await
        .unwrap();
    store::auth::create_credential(
        &f.store.pool,
        &f.person,
        "fixture-person",
        "fixture-not-a-real-hash",
        role,
    )
    .await
    .unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let credential = session_access::password_on(&mut tx, "fixture-person")
        .await
        .unwrap()
        .unwrap();
    let admitted =
        session_access::register_device_on(&mut tx, credential.authentication(), &f.node)
            .await
            .unwrap();
    assert_eq!(admitted.peer_contact(), None);
    let before = security_counts(&mut tx).await;
    private_contacts::create_on(&mut tx, &f.hosted, &f.contact, &f.node)
        .await
        .unwrap();
    assert!(
        session_access::require_admission_on(&mut tx, &admitted)
            .await
            .is_err()
    );
    let device = session_access::device_on(&mut tx, &f.person, &f.node)
        .await
        .unwrap()
        .unwrap();
    assert!(device.revision > admitted.device().revision);
    assert!(!device.revoked);
    assert_eq!(security_counts(&mut tx).await, before);
    sqlx::query("DELETE FROM organ_contact WHERE record_uid = ?")
        .bind(&f.contact)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(
        session_access::peer_contact_on(&mut tx, &f.node)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        session_access::require_admission_on(&mut tx, &admitted)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn private_contacts_device_revision_overflow_aborts_contact_without_losing_caller_write() {
    let f = fixture().await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    sqlx::query(
        "INSERT INTO person_device (person_uid, node_id, revoked, revision) VALUES (?, ?, 1, ?)",
    )
    .bind(&f.person)
    .bind(&f.node)
    .bind(i64::MAX)
    .execute(&mut *tx)
    .await
    .unwrap();
    store::records::set_authoring_text_on(
        &mut tx,
        &f.contact,
        Some("kept at device overflow"),
        None,
    )
    .await
    .unwrap();
    let before = generation(&mut tx, &f.contact).await;
    assert!(
        private_contacts::create_on(&mut tx, &f.hosted, &f.contact, &f.node)
            .await
            .is_err()
    );
    assert_eq!(contact_count(&mut tx, &f.contact).await, 0);
    assert_eq!(generation(&mut tx, &f.contact).await, before);
    let device = session_access::device_on(&mut tx, &f.person, &f.node)
        .await
        .unwrap()
        .unwrap();
    assert!(device.revoked);
    assert_eq!(device.revision, i64::MAX);
    tx.commit().await.unwrap();
    let head: String = sqlx::query_scalar("SELECT head FROM record WHERE uid = ?")
        .bind(&f.contact)
        .fetch_one(&f.store.pool)
        .await
        .unwrap();
    assert_eq!(head, "kept at device overflow");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn private_contacts_two_connections_observe_only_commit_and_restart_retains_epoch() {
    let dir = std::env::temp_dir().join(nucleus::new_uid("private-contacts"));
    std::fs::create_dir(&dir).unwrap();
    let url = format!("sqlite://{}", dir.join("lince.db").display());
    let f = fixture_from(Store::open(&url).await.unwrap()).await;
    let current = created(&f).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let granted =
        private_contacts::grant_on(&mut tx, &f.contact, current.peer.generation, &f.person)
            .await
            .unwrap();
    let mut observer = f.store.pool.acquire().await.unwrap();
    assert!(
        session_access::granted_login_on(&mut observer, &f.contact, &f.node)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        generation(&mut observer, &f.contact).await,
        current.peer.generation
    );
    tx.commit().await.unwrap();
    let auth = session_access::granted_login_on(&mut observer, &f.contact, &f.node)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(auth.person_uid(), f.person);
    assert_eq!(
        generation(&mut observer, &f.contact).await,
        granted.peer.generation
    );
    drop(observer);
    let mut stale = store::write_tx(&f.store.pool).await.unwrap();
    assert!(
        private_contacts::revoke_on(&mut stale, &f.contact, current.peer.generation)
            .await
            .is_err()
    );
    stale.rollback().await.unwrap();
    f.store.pool.close().await;
    let reopened = Store::open(&url).await.unwrap();
    let mut tx = store::write_tx(&reopened.pool).await.unwrap();
    assert_eq!(
        private_contacts::get_on(&mut tx, &f.contact).await.unwrap(),
        granted
    );
    session_access::require_authentication_on(&mut tx, &auth)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    reopened.pool.close().await;
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn private_contacts_timestamp_byte_boundary_accepts_exactly_bounded_valid_data() {
    let f = fixture().await;
    granted(&f).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let exact = format!("2026-09-07T00:00:00.{}Z", "0".repeat(43));
    assert_eq!(exact.len(), private_contacts::MAX_LOGIN_TIMESTAMP_BYTES);
    assert!(chrono::DateTime::parse_from_rfc3339(&exact).is_ok());
    sqlx::query("UPDATE organ_login SET created_at = ? WHERE organ_uid = ?")
        .bind(&exact)
        .bind(&f.contact)
        .execute(&mut *tx)
        .await
        .unwrap();
    let current = private_contacts::get_on(&mut tx, &f.contact).await.unwrap();
    assert_eq!(
        private_contacts::replace_on(&mut tx, &f.contact, current.peer.generation, &f.person)
            .await
            .unwrap(),
        current
    );
    let over = format!("2026-09-07T00:00:00.{}Z", "0".repeat(44));
    assert_eq!(over.len(), private_contacts::MAX_LOGIN_TIMESTAMP_BYTES + 1);
    assert!(chrono::DateTime::parse_from_rfc3339(&over).is_ok());
    sqlx::query("UPDATE organ_login SET created_at = ? WHERE organ_uid = ?")
        .bind(over)
        .bind(&f.contact)
        .execute(&mut *tx)
        .await
        .unwrap();
    let epoch = generation(&mut tx, &f.contact).await;
    all_refuse(&mut tx, &f, epoch).await;
}

#[tokio::test]
async fn private_contacts_uncommitted_n13_record_and_contact_share_callers_transaction() {
    let f = fixture().await;
    let contact = nucleus::new_uid("r");
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    store::records::create_with_uid_on(
        &mut tx,
        &contact,
        store::records::NewRecord {
            slug: None,
            kind: RecordKind::Organ,
            head: "Uncommitted company contact",
            body: "",
            quantity: store::exact::zero(),
        },
        &f.hosted,
        None,
    )
    .await
    .unwrap();
    let binding = private_contacts::create_on(&mut tx, &f.hosted, &contact, &f.node)
        .await
        .unwrap();
    let granted = private_contacts::grant_on(&mut tx, &contact, binding.peer.generation, &f.person)
        .await
        .unwrap();
    assert_eq!(
        session_access::peer_contact_on(&mut tx, &f.node)
            .await
            .unwrap(),
        Some(granted.peer)
    );
    let auth = session_access::granted_login_on(&mut tx, &contact, &f.node)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(auth.person_uid(), f.person);
    tx.rollback().await.unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    let records: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM record WHERE uid = ?")
        .bind(&contact)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(records, 0);
    assert_eq!(contact_count(&mut tx, &contact).await, 0);
    let epochs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM organ_login_generation WHERE organ_uid = ?")
            .bind(&contact)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(epochs, 0);
    assert!(
        session_access::granted_login_on(&mut tx, &contact, &f.node)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn private_contacts_grant_rechecks_actual_person_after_insert_and_rolls_back_refusal() {
    let f = fixture().await;
    let current = created(&f).await;
    sqlx::query("CREATE TRIGGER private_contacts_disable_during_grant AFTER INSERT ON organ_login BEGIN INSERT INTO record_extension (record_uid, namespace, fds) VALUES (NEW.person_uid, 'lince.person', '{\"standing\":{\"active\":false}}'); END")
        .execute(&f.store.pool).await.unwrap();
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    store::records::set_authoring_text_on(&mut tx, &f.contact, Some("before grant refusal"), None)
        .await
        .unwrap();
    let person_epoch = session_access::person_generation_on(&mut tx, &f.person)
        .await
        .unwrap();
    assert!(
        private_contacts::grant_on(&mut tx, &f.contact, current.peer.generation, &f.person)
            .await
            .is_err()
    );
    assert_eq!(
        private_contacts::get_on(&mut tx, &f.contact).await.unwrap(),
        current
    );
    assert_eq!(
        session_access::person_generation_on(&mut tx, &f.person)
            .await
            .unwrap(),
        person_epoch
    );
    assert!(
        store::people::standing_on(&mut tx, &f.person)
            .await
            .unwrap()
            .is_none()
    );
    tx.commit().await.unwrap();
    let head: String = sqlx::query_scalar("SELECT head FROM record WHERE uid = ?")
        .bind(&f.contact)
        .fetch_one(&f.store.pool)
        .await
        .unwrap();
    assert_eq!(head, "before grant refusal");
}

#[tokio::test]
async fn private_contacts_database_failure_is_not_missing_state_or_success() {
    let f = fixture().await;
    let current = created(&f).await;
    let mut tx = store::write_tx(&f.store.pool).await.unwrap();
    sqlx::query("ALTER TABLE organ_login RENAME TO unavailable_organ_login")
        .execute(&mut *tx)
        .await
        .unwrap();
    all_refuse(&mut tx, &f, current.peer.generation).await;
    assert!(
        private_contacts::create_on(&mut tx, &f.hosted, &f.contact, &f.node)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    assert_eq!(read(&f).await, current);
}
