use chrono::{DateTime, Utc};
use nucleus::RecordKind;
use serde_json::{Value, json};
use store::Store;
use store::people::{self, MAX_PERSON_EXTENSION_BYTES, MAX_STANDING_NOTE_BYTES, StandingChange};
use store::session_access::{self, DeviceAdmission};
use store::sqlx::SqliteConnection;

fn instant() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-09-07T12:34:56.123456789-03:00")
        .unwrap()
        .with_timezone(&Utc)
}

fn deactivate(note: Option<&str>) -> StandingChange {
    StandingChange::Deactivate {
        at: instant(),
        note: note.map(str::to_owned),
    }
}

fn standing_value(note: Option<&str>) -> Value {
    let mut standing = json!({
        "active": false,
        "at": "2026-09-07T15:34:56.123456789Z"
    });
    if let Some(note) = note {
        standing["note"] = json!(note);
    }
    standing
}

async fn record(store: &Store, kind: RecordKind) -> String {
    store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind,
            head: "Standing fixture",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid
}

async fn fixture() -> (Store, String) {
    let store = Store::open_memory().await.unwrap();
    let person = record(&store, RecordKind::Person).await;
    (store, person)
}

async fn revision(store: &Store, uid: &str) -> i64 {
    let mut connection = store.pool.acquire().await.unwrap();
    store::record_revisions::get_on(&mut connection, uid)
        .await
        .unwrap()
        .revision
}

async fn fields_on(connection: &mut SqliteConnection, uid: &str) -> Option<Value> {
    store::sqlx::query_scalar::<_, String>(
        "SELECT fds FROM record_extension WHERE record_uid = ? AND namespace = 'lince.person'",
    )
    .bind(uid)
    .fetch_optional(connection)
    .await
    .unwrap()
    .map(|raw| serde_json::from_str(&raw).unwrap())
}

async fn generation_on(connection: &mut SqliteConnection, uid: &str) -> i64 {
    store::sqlx::query_scalar("SELECT generation FROM person_auth_generation WHERE person_uid = ?")
        .bind(uid)
        .fetch_one(connection)
        .await
        .unwrap()
}

#[derive(Debug, PartialEq, Eq)]
struct StoredState {
    extension: Vec<(String, String, String, String)>,
    revision: i64,
    generation: i64,
    head: String,
    sync: i64,
}

async fn state_on(connection: &mut SqliteConnection, uid: &str) -> StoredState {
    let extension = store::sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT quote(record_uid), quote(namespace), quote(fds), quote(version)
           FROM record_extension WHERE CAST(record_uid AS TEXT) = ?
            AND CAST(namespace AS TEXT) = 'lince.person'
          ORDER BY quote(record_uid), quote(namespace)",
    )
    .bind(uid)
    .fetch_all(&mut *connection)
    .await
    .unwrap();
    let revision = store::record_revisions::get_on(connection, uid)
        .await
        .unwrap()
        .revision;
    let generation = generation_on(connection, uid).await;
    let head = store::sqlx::query_scalar("SELECT head FROM record WHERE uid = ?")
        .bind(uid)
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    let sync = store::sqlx::query_scalar("SELECT COUNT(*) FROM sync_op")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    StoredState {
        extension,
        revision,
        generation,
        head,
        sync,
    }
}

async fn state(store: &Store, uid: &str) -> StoredState {
    state_on(&mut store.pool.acquire().await.unwrap(), uid).await
}

async fn set_fields(store: &Store, uid: &str, fields: &Value) {
    store::records::set_extension(&store.pool, uid, people::NAMESPACE, fields)
        .await
        .unwrap();
}

async fn captured(store: &Store, uid: &str) -> DeviceAdmission {
    let role = store::auth::ensure_role(&store.pool, "standing-fixture")
        .await
        .unwrap();
    store::auth::create_credential(&store.pool, uid, "standing-person", "fixture-hash", role)
        .await
        .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let password = session_access::password_on(&mut tx, "standing-person")
        .await
        .unwrap()
        .unwrap();
    let admission = session_access::register_device_on(
        &mut tx,
        password.authentication(),
        &format!("{:064x}", 1),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    admission
}

#[tokio::test]
async fn standing_transactions_deactivation_and_reactivation_preserve_every_sibling() {
    let (store, uid) = fixture().await;
    let siblings = json!({
        "alias": "Ana",
        "nested": {"values": [null, true, 1, "one"]},
        "integer": u64::MAX,
        "decimal": 1.25,
        "optional": null
    });
    set_fields(&store, &uid, &siblings).await;
    let before = state(&store, &uid).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let result = people::compare_and_set_standing_on(
        &mut tx,
        &uid,
        before.revision,
        deactivate(Some(" Away \n")),
    )
    .await
    .unwrap();
    let mut expected = siblings.clone();
    expected[people::STANDING_KEY] = standing_value(Some(" Away \n"));
    assert_eq!(fields_on(&mut tx, &uid).await, Some(expected));
    assert_eq!(result.revision, before.revision + 1);
    assert_eq!(result.record_uid, uid);
    assert!(!people::is_active_on(&mut tx, &uid).await.unwrap());
    assert_eq!(generation_on(&mut tx, &uid).await, before.generation + 1);
    let changes = store::sqlx::query_as::<_, (String, String, String)>(
        "SELECT field, kind, value FROM sync_op WHERE uid = ? AND tbl = 'record_extension'
          ORDER BY seq DESC LIMIT 1",
    )
    .bind(&uid)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(changes.0, "lince.person.standing");
    assert_eq!(changes.1, "set");
    assert_eq!(
        serde_json::from_str::<Value>(&changes.2).unwrap(),
        standing_value(Some(" Away \n"))
    );
    tx.commit().await.unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let restored = people::compare_and_set_standing_on(
        &mut tx,
        &uid,
        result.revision,
        StandingChange::Reactivate,
    )
    .await
    .unwrap();
    assert_eq!(restored.revision, result.revision + 1);
    assert_eq!(fields_on(&mut tx, &uid).await, Some(siblings));
    assert!(people::is_active_on(&mut tx, &uid).await.unwrap());
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn standing_transactions_already_disabled_person_can_change_note_without_credentials() {
    let (store, uid) = fixture().await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let expected = store::record_revisions::get_on(&mut tx, &uid)
        .await
        .unwrap()
        .revision;
    let first = people::compare_and_set_standing_on(&mut tx, &uid, expected, deactivate(None))
        .await
        .unwrap();
    let second = people::compare_and_set_standing_on(
        &mut tx,
        &uid,
        first.revision,
        deactivate(Some("updated reason")),
    )
    .await
    .unwrap();
    assert_eq!(second.revision, first.revision + 1);
    assert_eq!(
        fields_on(&mut tx, &uid).await,
        Some(json!({"standing": standing_value(Some("updated reason"))}))
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM person_credential WHERE person_uid = ?"
        )
        .bind(&uid)
        .fetch_one(&mut *tx)
        .await
        .unwrap(),
        0
    );
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn standing_transactions_exact_noops_leave_revisions_generations_and_sync_unchanged() {
    for fields in [None, Some(json!({"sibling": true})), Some(json!({}))] {
        let (store, uid) = fixture().await;
        if let Some(fields) = fields {
            store::records::set_extension_raw(&store.pool, &uid, people::NAMESPACE, &fields)
                .await
                .unwrap();
        }
        let before = state(&store, &uid).await;
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        let unchanged = people::compare_and_set_standing_on(
            &mut tx,
            &uid,
            before.revision,
            StandingChange::Reactivate,
        )
        .await
        .unwrap();
        assert_eq!(unchanged.revision, before.revision);
        assert_eq!(state_on(&mut tx, &uid).await, before);
        tx.commit().await.unwrap();
    }
    let (store, uid) = fixture().await;
    set_fields(
        &store,
        &uid,
        &json!({"standing": standing_value(Some("same"))}),
    )
    .await;
    let before = state(&store, &uid).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    people::compare_and_set_standing_on(&mut tx, &uid, before.revision, deactivate(Some("same")))
        .await
        .unwrap();
    assert_eq!(state_on(&mut tx, &uid).await, before);
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn standing_transactions_reactivation_removes_null_or_explicit_active_standing_only() {
    for standing in [Value::Null, json!({"active": true, "note": "previous"})] {
        let (store, uid) = fixture().await;
        set_fields(&store, &uid, &json!({"standing": standing, "sibling": 4})).await;
        let before = state(&store, &uid).await;
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        let after = people::compare_and_set_standing_on(
            &mut tx,
            &uid,
            before.revision,
            StandingChange::Reactivate,
        )
        .await
        .unwrap();
        assert_eq!(after.revision, before.revision + 1);
        assert_eq!(fields_on(&mut tx, &uid).await, Some(json!({"sibling": 4})));
        tx.commit().await.unwrap();
    }
}

#[tokio::test]
async fn standing_transactions_rollback_restores_composed_writes_and_captured_admission() {
    let (store, uid) = fixture().await;
    set_fields(&store, &uid, &json!({"sibling": "retained"})).await;
    let admission = captured(&store, &uid).await;
    let before = state(&store, &uid).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    session_access::require_admission_on(&mut tx, &admission)
        .await
        .unwrap();
    let standing = people::compare_and_set_standing_on(
        &mut tx,
        &uid,
        before.revision,
        deactivate(Some("temporary")),
    )
    .await
    .unwrap();
    store::records::set_authoring_text_on(&mut tx, &uid, Some("Changed too"), None)
        .await
        .unwrap();
    assert!(
        session_access::require_admission_on(&mut tx, &admission)
            .await
            .is_err()
    );
    let staged = state_on(&mut tx, &uid).await;
    assert_eq!(staged.revision, standing.revision + 1);
    assert_ne!(staged, before);
    tx.rollback().await.unwrap();
    assert_eq!(state(&store, &uid).await, before);
    let mut connection = store.pool.acquire().await.unwrap();
    session_access::require_admission_on(&mut connection, &admission)
        .await
        .unwrap();
}

#[tokio::test]
async fn standing_transactions_committed_roundtrip_revokes_old_admission_without_erasing_access() {
    let (store, uid) = fixture().await;
    let admission = captured(&store, &uid).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let original_access = store::auth::person_access_on(&mut tx, &uid)
        .await
        .unwrap()
        .unwrap();
    let access = store::auth::compare_and_set_read_filter_on(
        &mut tx,
        &uid,
        Some("{}"),
        original_access.revision,
    )
    .await
    .unwrap();
    let other_node = format!("{:064x}", 2);
    let other =
        session_access::register_device_on(&mut tx, admission.authentication(), &other_node)
            .await
            .unwrap();
    let revoked = session_access::compare_and_set_revoked_on(
        &mut tx,
        &uid,
        &other_node,
        other.device().revision,
        true,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let old_revision = revision(&store, &uid).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let disabled =
        people::compare_and_set_standing_on(&mut tx, &uid, old_revision, deactivate(None))
            .await
            .unwrap();
    tx.commit().await.unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        session_access::require_admission_on(&mut tx, &admission)
            .await
            .is_err()
    );
    people::compare_and_set_standing_on(
        &mut tx,
        &uid,
        disabled.revision,
        StandingChange::Reactivate,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        session_access::require_admission_on(&mut tx, &admission)
            .await
            .is_err()
    );
    assert_eq!(
        store::auth::person_access_on(&mut tx, &uid).await.unwrap(),
        Some(access)
    );
    assert_eq!(
        session_access::device_on(&mut tx, &uid, &other_node)
            .await
            .unwrap(),
        Some(revoked)
    );
    assert_eq!(
        session_access::device_on(&mut tx, &uid, &admission.device().node_id)
            .await
            .unwrap()
            .as_ref(),
        Some(admission.device())
    );
    let password = session_access::password_on(&mut tx, "standing-person")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(password.password_hash(), "fixture-hash");
    assert!(password.authentication().generation() > admission.authentication().generation());
    assert!(
        session_access::register_device_on(&mut tx, password.authentication(), &other_node)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn standing_transactions_invalid_identity_missing_deleted_or_wrong_kind_refuses() {
    let (store, person) = fixture().await;
    let plain = record(&store, RecordKind::Plain).await;
    let deleted = record(&store, RecordKind::Person).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::records::mark_deleted_on(&mut tx, &deleted)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let targets = [
        ("".to_owned(), 1),
        ("person-slug".to_owned(), 1),
        (nucleus::new_uid("p"), 1),
        (format!(" {person}"), 1),
        (nucleus::new_uid("r"), 1),
        (plain.clone(), revision(&store, &plain).await),
        (deleted.clone(), revision(&store, &deleted).await),
    ];
    for (uid, expected) in targets {
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        assert!(
            people::compare_and_set_standing_on(&mut tx, &uid, expected, deactivate(None))
                .await
                .is_err()
        );
        assert!(
            people::compare_and_set_standing_on(
                &mut tx,
                &uid,
                expected,
                StandingChange::Reactivate
            )
            .await
            .is_err()
        );
        tx.rollback().await.unwrap();
    }
    assert!(people::is_active(&store.pool, &person).await.unwrap());
}

#[tokio::test]
async fn standing_transactions_expected_revision_is_mandatory_and_checks_other_record_fields() {
    let (store, uid) = fixture().await;
    let original = revision(&store, &uid).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::records::set_authoring_text_on(&mut tx, &uid, Some("Concurrent change"), None)
        .await
        .unwrap();
    let before = state_on(&mut tx, &uid).await;
    for expected in [i64::MIN, -1, 0, original, before.revision + 1] {
        assert!(
            people::compare_and_set_standing_on(&mut tx, &uid, expected, deactivate(None))
                .await
                .is_err()
        );
        assert_eq!(state_on(&mut tx, &uid).await, before);
    }
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn standing_transactions_delete_recreate_extension_does_not_revive_old_cas() {
    let (store, uid) = fixture().await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let expected = store::record_revisions::get_on(&mut tx, &uid)
        .await
        .unwrap()
        .revision;
    let first = people::compare_and_set_standing_on(&mut tx, &uid, expected, deactivate(None))
        .await
        .unwrap();
    let cleared = people::compare_and_set_standing_on(
        &mut tx,
        &uid,
        first.revision,
        StandingChange::Reactivate,
    )
    .await
    .unwrap();
    assert_eq!(fields_on(&mut tx, &uid).await, None);
    let recreated =
        people::compare_and_set_standing_on(&mut tx, &uid, cleared.revision, deactivate(None))
            .await
            .unwrap();
    assert!(recreated.revision > first.revision);
    assert!(
        people::compare_and_set_standing_on(
            &mut tx,
            &uid,
            first.revision,
            StandingChange::Reactivate
        )
        .await
        .is_err()
    );
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn standing_transactions_notes_are_bounded_by_bytes_without_trimming() {
    for note in ["x".repeat(MAX_STANDING_NOTE_BYTES), "🦀".repeat(4096)] {
        let (store, uid) = fixture().await;
        let expected = revision(&store, &uid).await;
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        people::compare_and_set_standing_on(&mut tx, &uid, expected, deactivate(Some(&note)))
            .await
            .unwrap();
        assert_eq!(
            people::standing_on(&mut tx, &uid)
                .await
                .unwrap()
                .unwrap()
                .note,
            Some(note)
        );
        tx.commit().await.unwrap();
    }
    for note in ["x".repeat(MAX_STANDING_NOTE_BYTES + 1), "🦀".repeat(4097)] {
        let (store, uid) = fixture().await;
        let before = state(&store, &uid).await;
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        assert!(
            people::compare_and_set_standing_on(
                &mut tx,
                &uid,
                before.revision,
                deactivate(Some(&note))
            )
            .await
            .is_err()
        );
        assert_eq!(state_on(&mut tx, &uid).await, before);
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn standing_transactions_complete_proposed_extension_byte_limit_is_exact() {
    for over in [0, 1] {
        let (store, uid) = fixture().await;
        let mut proposed = json!({"sibling": "", "standing": standing_value(None)});
        let overhead = serde_json::to_string(&proposed).unwrap().len();
        let sibling = "x".repeat(MAX_PERSON_EXTENSION_BYTES - overhead + over);
        proposed["sibling"] = json!(sibling);
        assert_eq!(
            serde_json::to_string(&proposed).unwrap().len(),
            MAX_PERSON_EXTENSION_BYTES + over
        );
        set_fields(&store, &uid, &json!({"sibling": proposed["sibling"]})).await;
        let before = state(&store, &uid).await;
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        let result =
            people::compare_and_set_standing_on(&mut tx, &uid, before.revision, deactivate(None))
                .await;
        if over == 0 {
            result.unwrap();
            assert_eq!(fields_on(&mut tx, &uid).await, Some(proposed));
            tx.commit().await.unwrap();
        } else {
            assert!(result.is_err());
            assert_eq!(state_on(&mut tx, &uid).await, before);
            tx.rollback().await.unwrap();
        }
    }
}

#[tokio::test]
async fn standing_transactions_serialized_note_expansion_counts_toward_complete_bound() {
    let (store, uid) = fixture().await;
    let before = state(&store, &uid).await;
    let note = "\0".repeat(MAX_STANDING_NOTE_BYTES);
    assert_eq!(note.len(), MAX_STANDING_NOTE_BYTES);
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        people::compare_and_set_standing_on(
            &mut tx,
            &uid,
            before.revision,
            deactivate(Some(&note))
        )
        .await
        .is_err()
    );
    assert_eq!(state_on(&mut tx, &uid).await, before);
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn standing_transactions_existing_extension_size_is_checked_even_for_clear_or_noop() {
    for over in [0, 1] {
        let (store, uid) = fixture().await;
        let empty = json!({"sibling": ""});
        let overhead = serde_json::to_string(&empty).unwrap().len();
        let fields = json!({"sibling": "x".repeat(MAX_PERSON_EXTENSION_BYTES - overhead + over)});
        set_fields(&store, &uid, &fields).await;
        let before = state(&store, &uid).await;
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        let result = people::compare_and_set_standing_on(
            &mut tx,
            &uid,
            before.revision,
            StandingChange::Reactivate,
        )
        .await;
        if over == 0 {
            assert_eq!(result.unwrap().revision, before.revision);
        } else {
            assert!(result.is_err());
        }
        assert_eq!(state_on(&mut tx, &uid).await, before);
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn standing_transactions_corrupt_json_nonobject_or_standing_shape_never_repairs() {
    for raw in [
        "{broken",
        "[]",
        "null",
        "true",
        "1",
        r#"{"standing":{}}"#,
        r#"{"standing":false}"#,
        r#"{"standing":{"active":"false"}}"#,
        r#"{"standing":{"active":false,"at":3}}"#,
        r#"{"standing":{"active":false,"note":[]}}"#,
    ] {
        let (store, uid) = fixture().await;
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        store::sqlx::query("PRAGMA ignore_check_constraints = ON")
            .execute(&mut *tx)
            .await
            .unwrap();
        store::sqlx::query(
            "INSERT INTO record_extension (record_uid, namespace, fds)
             VALUES (?, 'lince.person', ?)",
        )
        .bind(&uid)
        .bind(raw)
        .execute(&mut *tx)
        .await
        .unwrap();
        store::sqlx::query("PRAGMA ignore_check_constraints = OFF")
            .execute(&mut *tx)
            .await
            .unwrap();
        let before = state_on(&mut tx, &uid).await;
        for change in [deactivate(None), StandingChange::Reactivate] {
            assert!(
                people::compare_and_set_standing_on(&mut tx, &uid, before.revision, change)
                    .await
                    .is_err()
            );
            assert_eq!(state_on(&mut tx, &uid).await, before);
        }
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn standing_transactions_invalid_extension_storage_types_refuse_before_rewriting() {
    for sql in [
        "UPDATE record_extension SET fds = CAST(fds AS BLOB) WHERE record_uid = ?",
        "UPDATE record_extension SET namespace = CAST(namespace AS BLOB) WHERE record_uid = ?",
        "UPDATE record_extension SET version = 'corrupt' WHERE record_uid = ?",
        "UPDATE record_extension SET version = 1.5 WHERE record_uid = ?",
        "UPDATE record_extension SET version = 0 WHERE record_uid = ?",
    ] {
        let (store, uid) = fixture().await;
        set_fields(&store, &uid, &json!({"sibling": true})).await;
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        store::sqlx::query(sql)
            .bind(&uid)
            .execute(&mut *tx)
            .await
            .unwrap();
        let before = state_on(&mut tx, &uid).await;
        assert!(
            people::compare_and_set_standing_on(
                &mut tx,
                &uid,
                before.revision,
                StandingChange::Reactivate
            )
            .await
            .is_err()
        );
        assert_eq!(state_on(&mut tx, &uid).await, before);
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn standing_transactions_ambiguous_text_and_blob_namespace_rows_refuse() {
    let (store, uid) = fixture().await;
    set_fields(&store, &uid, &json!({"first": 1})).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::sqlx::query(
        "INSERT INTO record_extension (record_uid, namespace, fds)
         VALUES (?, CAST('lince.person' AS BLOB), '{\"second\":2}')",
    )
    .bind(&uid)
    .execute(&mut *tx)
    .await
    .unwrap();
    let before = state_on(&mut tx, &uid).await;
    assert_eq!(before.extension.len(), 2);
    assert!(
        people::compare_and_set_standing_on(&mut tx, &uid, before.revision, deactivate(None))
            .await
            .is_err()
    );
    assert_eq!(state_on(&mut tx, &uid).await, before);
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn standing_transactions_record_revision_exhaustion_aborts_real_changes() {
    for change in [deactivate(None), StandingChange::Reactivate] {
        let (store, uid) = fixture().await;
        if matches!(&change, StandingChange::Reactivate) {
            set_fields(&store, &uid, &json!({"standing": standing_value(None)})).await;
        }
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        store::sqlx::query("UPDATE record_revision SET revision = ? WHERE record_uid = ?")
            .bind(i64::MAX)
            .bind(&uid)
            .execute(&mut *tx)
            .await
            .unwrap();
        let before = state_on(&mut tx, &uid).await;
        assert!(
            people::compare_and_set_standing_on(&mut tx, &uid, i64::MAX, change)
                .await
                .is_err()
        );
        assert_eq!(state_on(&mut tx, &uid).await, before);
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn standing_transactions_authentication_generation_exhaustion_aborts_real_changes() {
    for change in [deactivate(None), StandingChange::Reactivate] {
        let (store, uid) = fixture().await;
        if matches!(&change, StandingChange::Reactivate) {
            set_fields(&store, &uid, &json!({"standing": standing_value(None)})).await;
        }
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        store::sqlx::query("UPDATE person_auth_generation SET generation = ? WHERE person_uid = ?")
            .bind(i64::MAX)
            .bind(&uid)
            .execute(&mut *tx)
            .await
            .unwrap();
        let before = state_on(&mut tx, &uid).await;
        assert!(
            people::compare_and_set_standing_on(&mut tx, &uid, before.revision, change)
                .await
                .is_err()
        );
        assert_eq!(state_on(&mut tx, &uid).await, before);
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn standing_transactions_extension_version_exhaustion_refuses_incrementing_writes() {
    for change in [deactivate(None), StandingChange::Reactivate] {
        let (store, uid) = fixture().await;
        let mut fields = json!({"sibling": true});
        if matches!(&change, StandingChange::Reactivate) {
            fields["standing"] = standing_value(None);
        }
        set_fields(&store, &uid, &fields).await;
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        store::sqlx::query("UPDATE record_extension SET version = ? WHERE record_uid = ?")
            .bind(i64::MAX)
            .bind(&uid)
            .execute(&mut *tx)
            .await
            .unwrap();
        let before = state_on(&mut tx, &uid).await;
        assert!(
            people::compare_and_set_standing_on(&mut tx, &uid, before.revision, change)
                .await
                .is_err()
        );
        assert_eq!(state_on(&mut tx, &uid).await, before);
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn standing_transactions_deleting_sole_standing_does_not_increment_removed_extension() {
    let (store, uid) = fixture().await;
    set_fields(&store, &uid, &json!({"standing": standing_value(None)})).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::sqlx::query("UPDATE record_extension SET version = ? WHERE record_uid = ?")
        .bind(i64::MAX)
        .bind(&uid)
        .execute(&mut *tx)
        .await
        .unwrap();
    let before = state_on(&mut tx, &uid).await;
    let after = people::compare_and_set_standing_on(
        &mut tx,
        &uid,
        before.revision,
        StandingChange::Reactivate,
    )
    .await
    .unwrap();
    assert_eq!(after.revision, before.revision + 1);
    assert_eq!(generation_on(&mut tx, &uid).await, before.generation + 1);
    assert_eq!(fields_on(&mut tx, &uid).await, None);
    let tombstone = store::sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT field, kind, value FROM sync_op WHERE uid = ? ORDER BY seq DESC LIMIT 1",
    )
    .bind(&uid)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        tombstone,
        ("lince.person.standing".into(), "tombstone".into(), None)
    );
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn standing_transactions_exact_noop_is_allowed_at_all_counter_limits() {
    let (store, uid) = fixture().await;
    set_fields(&store, &uid, &json!({"standing": standing_value(None)})).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::sqlx::query("UPDATE record_extension SET version = ? WHERE record_uid = ?")
        .bind(i64::MAX)
        .bind(&uid)
        .execute(&mut *tx)
        .await
        .unwrap();
    store::sqlx::query("UPDATE record_revision SET revision = ? WHERE record_uid = ?")
        .bind(i64::MAX)
        .bind(&uid)
        .execute(&mut *tx)
        .await
        .unwrap();
    store::sqlx::query("UPDATE person_auth_generation SET generation = ? WHERE person_uid = ?")
        .bind(i64::MAX)
        .bind(&uid)
        .execute(&mut *tx)
        .await
        .unwrap();
    let before = state_on(&mut tx, &uid).await;
    let result = people::compare_and_set_standing_on(&mut tx, &uid, i64::MAX, deactivate(None))
        .await
        .unwrap();
    assert_eq!(result.revision, i64::MAX);
    assert_eq!(state_on(&mut tx, &uid).await, before);
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn standing_transactions_missing_or_corrupt_record_revision_is_not_repaired() {
    for sql in [
        "DELETE FROM record_revision WHERE record_uid = ?",
        "UPDATE record_revision SET revision = 0 WHERE record_uid = ?",
    ] {
        let (store, uid) = fixture().await;
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        store::sqlx::query("PRAGMA ignore_check_constraints = ON")
            .execute(&mut *tx)
            .await
            .unwrap();
        store::sqlx::query(sql)
            .bind(&uid)
            .execute(&mut *tx)
            .await
            .unwrap();
        store::sqlx::query("PRAGMA ignore_check_constraints = OFF")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            people::compare_and_set_standing_on(&mut tx, &uid, 1, deactivate(None))
                .await
                .is_err()
        );
        assert_eq!(fields_on(&mut tx, &uid).await, None);
        tx.rollback().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn standing_transactions_other_connections_see_only_commit_and_stale_writer_loses() {
    let directory = std::env::temp_dir().join(nucleus::new_uid("standing-transactions"));
    std::fs::create_dir(&directory).unwrap();
    let url = format!("sqlite://{}", directory.join("lince.db").display());
    let store = std::sync::Arc::new(Store::open(&url).await.unwrap());
    let uid = record(&store, RecordKind::Person).await;
    let admission = captured(&store, &uid).await;
    let before = state(&store, &uid).await;
    let mut observer = store.pool.acquire().await.unwrap();
    let mut first = store::write_tx(&store.pool).await.unwrap();
    let accepted = people::compare_and_set_standing_on(
        &mut first,
        &uid,
        before.revision,
        deactivate(Some("committed")),
    )
    .await
    .unwrap();
    assert_eq!(state_on(&mut observer, &uid).await, before);
    session_access::require_admission_on(&mut observer, &admission)
        .await
        .unwrap();
    let competing_store = store.clone();
    let competing_uid = uid.clone();
    let expected = before.revision;
    let (attempted, started) = tokio::sync::oneshot::channel();
    let competing = tokio::spawn(async move {
        attempted.send(()).unwrap();
        let mut tx = store::write_tx(&competing_store.pool).await.unwrap();
        let result = people::compare_and_set_standing_on(
            &mut tx,
            &competing_uid,
            expected,
            StandingChange::Reactivate,
        )
        .await;
        tx.rollback().await.unwrap();
        result
    });
    started.await.unwrap();
    first.commit().await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(10), competing)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    let after = state_on(&mut observer, &uid).await;
    assert_eq!(after.revision, accepted.revision);
    assert!(after.generation > before.generation);
    assert!(after.sync > before.sync);
    assert!(
        session_access::require_admission_on(&mut observer, &admission)
            .await
            .is_err()
    );
    drop(observer);
    store.pool.close().await;
    let reopened = Store::open(&url).await.unwrap();
    assert_eq!(state(&reopened, &uid).await, after);
    let mut connection = reopened.pool.acquire().await.unwrap();
    assert!(
        session_access::require_admission_on(&mut connection, &admission)
            .await
            .is_err()
    );
    drop(connection);
    reopened.pool.close().await;
    std::fs::remove_dir_all(directory).unwrap();
}
