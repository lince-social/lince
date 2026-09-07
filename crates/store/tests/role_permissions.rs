use std::time::Duration;

use serde_json::json;
use store::Store;
use store::role_permissions::{self, PermissionIdentity, RolePermissionSet};
use store::sqlx::{Sqlite, Transaction};

async fn role(store: &Store, name: &str) -> i64 {
    store::auth::ensure_role(&store.pool, name).await.unwrap()
}

async fn permission(store: &Store, subject: &str, action: &str) -> PermissionIdentity {
    PermissionIdentity {
        id: store::auth::ensure_permission(&store.pool, subject, action)
            .await
            .unwrap(),
        subject: subject.into(),
        action: action.into(),
    }
}

async fn read(store: &Store, role_id: i64) -> RolePermissionSet {
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let result = role_permissions::get_on(&mut tx, role_id).await.unwrap();
    tx.commit().await.unwrap();
    result
}

async fn replace(
    store: &Store,
    role_id: i64,
    revision: i64,
    permissions: &[PermissionIdentity],
) -> RolePermissionSet {
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let result = role_permissions::compare_and_set_on(&mut tx, role_id, revision, permissions)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    result
}

async fn fixture() -> (Store, i64, PermissionIdentity, PermissionIdentity) {
    let store = Store::open_memory().await.unwrap();
    let role_id = role(&store, "worker").await;
    let first = permission(&store, "record", "read").await;
    let second = permission(&store, "record", "update").await;
    store::auth::grant(&store.pool, role_id, first.id)
        .await
        .unwrap();
    (store, role_id, first, second)
}

async fn epoch_on(tx: &mut Transaction<'_, Sqlite>, role_id: i64) -> i64 {
    store::sqlx::query_scalar("SELECT revision FROM role_permission_revision WHERE role_id = ?")
        .bind(role_id)
        .fetch_one(&mut **tx)
        .await
        .unwrap()
}

async fn corrupt_revision_guards(store: &Store) {
    let names = store::sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_schema WHERE type = 'trigger'
            AND tbl_name = 'role_permission_revision'",
    )
    .fetch_all(&store.pool)
    .await
    .unwrap();
    assert!(!names.is_empty());
    for name in names {
        assert!(
            name.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        );
        store::sqlx::query(&format!("DROP TRIGGER {name}"))
            .execute(&store.pool)
            .await
            .unwrap();
    }
}

async fn many_permissions(store: &Store, count: usize, bytes: usize) -> Vec<PermissionIdentity> {
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let mut result = Vec::new();
    for number in 0..count {
        let subject = format!("p{number:04}{}", "x".repeat(bytes - 7));
        let id = store::auth::ensure_permission_on(&mut tx, &subject, "r")
            .await
            .unwrap();
        result.push(PermissionIdentity {
            id,
            subject,
            action: "r".into(),
        });
    }
    tx.commit().await.unwrap();
    result
}

#[tokio::test]
async fn role_permissions_real_schema_is_strict_retained_and_positive() {
    let store = Store::open_memory().await.unwrap();
    let role_id = role(&store, "new").await;
    let initial = read(&store, role_id).await;
    assert_eq!(initial.revision, 1);
    assert!(initial.permissions.is_empty());
    let strict: i64 = store::sqlx::query_scalar(
        "SELECT strict FROM pragma_table_list WHERE name = 'role_permission_revision'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(strict, 1);
    let foreign_keys: i64 = store::sqlx::query_scalar(
        "SELECT count(*) FROM pragma_foreign_key_list('role_permission_revision')",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(foreign_keys, 0);
    for statement in [
        "DELETE FROM role_permission_revision WHERE role_id = ?",
        "UPDATE role_permission_revision SET role_id = role_id + 100 WHERE role_id = ?",
        "UPDATE role_permission_revision SET revision = revision WHERE role_id = ?",
        "UPDATE role_permission_revision SET revision = 0 WHERE role_id = ?",
        "UPDATE role_permission_revision SET revision = 1.5 WHERE role_id = ?",
        "UPDATE role_permission_revision SET revision = NULL WHERE role_id = ?",
        "UPDATE role_permission_revision SET revision = X'31' WHERE role_id = ?",
    ] {
        assert!(
            store::sqlx::query(statement)
                .bind(role_id)
                .execute(&store.pool)
                .await
                .is_err()
        );
    }
    assert_eq!(read(&store, role_id).await, initial);
}

#[tokio::test]
async fn role_permissions_revision_replace_cannot_erase_tombstone_without_recursive_triggers() {
    let store = Store::open_memory().await.unwrap();
    let role_id = role(&store, "retained").await;
    store::sqlx::query("UPDATE role_permission_revision SET revision = 10 WHERE role_id = ?")
        .bind(role_id)
        .execute(&store.pool)
        .await
        .unwrap();
    let before = read(&store, role_id).await;
    store::sqlx::query("PRAGMA recursive_triggers = OFF")
        .execute(&store.pool)
        .await
        .unwrap();
    assert!(
        store::sqlx::query(
            "INSERT OR REPLACE INTO role_permission_revision (role_id, revision) VALUES (?, 1)"
        )
        .bind(role_id)
        .execute(&store.pool)
        .await
        .is_err()
    );
    assert_eq!(read(&store, role_id).await, before);
}

#[tokio::test]
async fn role_permissions_replacement_returns_complete_sorted_actual_set() {
    let (store, role_id, first, second) = fixture().await;
    let before = read(&store, role_id).await;
    let expanded = replace(
        &store,
        role_id,
        before.revision,
        &[second.clone(), first.clone()],
    )
    .await;
    assert_eq!(expanded.permissions, vec![first.clone(), second.clone()]);
    assert!(expanded.revision > before.revision);
    let unchanged = replace(&store, role_id, expanded.revision, &[second.clone(), first]).await;
    assert_eq!(unchanged, expanded);
    let reduced = replace(
        &store,
        role_id,
        unchanged.revision,
        std::slice::from_ref(&second),
    )
    .await;
    assert_eq!(reduced.permissions, vec![second]);
    assert!(reduced.revision > expanded.revision);
    let empty = replace(&store, role_id, reduced.revision, &[]).await;
    assert!(empty.permissions.is_empty());
    assert!(empty.revision > reduced.revision);
    assert_eq!(replace(&store, role_id, empty.revision, &[]).await, empty);
}

#[tokio::test]
async fn role_permissions_expected_revision_is_mandatory_even_for_noop() {
    let (store, role_id, first, _) = fixture().await;
    let before = read(&store, role_id).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    for expected in [-1, 0, before.revision - 1, before.revision + 1] {
        assert!(
            role_permissions::compare_and_set_on(
                &mut tx,
                role_id,
                expected,
                std::slice::from_ref(&first)
            )
            .await
            .is_err()
        );
        assert_eq!(
            role_permissions::get_on(&mut tx, role_id).await.unwrap(),
            before
        );
    }
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn role_permissions_reject_unknown_mismatched_duplicate_and_invalid_identities() {
    let (store, role_id, first, second) = fixture().await;
    let before = read(&store, role_id).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let unknown = PermissionIdentity {
        id: i64::MAX,
        ..first.clone()
    };
    let mismatch = PermissionIdentity {
        id: second.id,
        ..first.clone()
    };
    for proposed in [
        vec![unknown],
        vec![mismatch],
        vec![first.clone(), first.clone()],
        vec![PermissionIdentity {
            id: 0,
            ..first.clone()
        }],
        vec![PermissionIdentity {
            id: -1,
            ..first.clone()
        }],
        vec![PermissionIdentity {
            subject: " ".into(),
            ..first.clone()
        }],
        vec![PermissionIdentity {
            action: "\n".into(),
            ..first.clone()
        }],
        vec![PermissionIdentity {
            action: "Read".into(),
            ..first.clone()
        }],
    ] {
        assert!(
            role_permissions::compare_and_set_on(&mut tx, role_id, before.revision, &proposed)
                .await
                .is_err()
        );
        assert_eq!(
            role_permissions::get_on(&mut tx, role_id).await.unwrap(),
            before
        );
    }
}

#[tokio::test]
async fn role_permissions_do_not_resolve_by_colliding_display_strings() {
    let store = Store::open_memory().await.unwrap();
    let role_id = role(&store, "ambiguous").await;
    let left = permission(&store, "a:b", "c").await;
    let right = permission(&store, "a", "b:c").await;
    let initial = read(&store, role_id).await;
    let saved = replace(
        &store,
        role_id,
        initial.revision,
        std::slice::from_ref(&left),
    )
    .await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        role_permissions::compare_and_set_on(
            &mut tx,
            role_id,
            saved.revision,
            &[left.clone(), right.clone()]
        )
        .await
        .is_err()
    );
    let impostor = PermissionIdentity {
        id: left.id,
        ..right.clone()
    };
    assert!(
        role_permissions::compare_and_set_on(&mut tx, role_id, saved.revision, &[impostor])
            .await
            .is_err()
    );
    assert_eq!(
        role_permissions::get_on(&mut tx, role_id).await.unwrap(),
        saved
    );
    store::auth::grant_on(&mut tx, role_id, right.id)
        .await
        .unwrap();
    assert!(role_permissions::get_on(&mut tx, role_id).await.is_err());
    tx.rollback().await.unwrap();
    let replaced = replace(
        &store,
        role_id,
        saved.revision,
        std::slice::from_ref(&right),
    )
    .await;
    assert_eq!(replaced.permissions, vec![right]);
    assert!(replaced.revision > saved.revision);
}

#[tokio::test]
async fn role_permissions_reject_missing_or_invalid_role_and_revision() {
    let store = Store::open_memory().await.unwrap();
    let role_id = role(&store, "missing-version").await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    for invalid in [0, -1, i64::MAX] {
        assert!(role_permissions::get_on(&mut tx, invalid).await.is_err());
        assert!(
            role_permissions::compare_and_set_on(&mut tx, invalid, 1, &[])
                .await
                .is_err()
        );
    }
    tx.rollback().await.unwrap();
    let known = permission(&store, "record", "read").await;
    corrupt_revision_guards(&store).await;
    store::sqlx::query("DELETE FROM role_permission_revision WHERE role_id = ?")
        .bind(role_id)
        .execute(&store.pool)
        .await
        .unwrap();
    assert_eq!(role(&store, "missing-version").await, role_id);
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(role_permissions::get_on(&mut tx, role_id).await.is_err());
    assert!(
        role_permissions::compare_and_set_on(&mut tx, role_id, 1, &[])
            .await
            .is_err()
    );
    assert!(
        store::auth::grant_on(&mut tx, role_id, known.id)
            .await
            .is_err()
    );
    assert!(
        store::sqlx::query("DELETE FROM role WHERE id = ?")
            .bind(role_id)
            .execute(&mut *tx)
            .await
            .is_err()
    );
    let count: i64 =
        store::sqlx::query_scalar("SELECT count(*) FROM role_permission WHERE role_id = ?")
            .bind(role_id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn role_permissions_corrupt_revision_refuses_read_and_legacy_writes() {
    let (store, role_id, first, second) = fixture().await;
    corrupt_revision_guards(&store).await;
    store::sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE role_permission_revision SET revision = 0 WHERE role_id = ?")
        .bind(role_id)
        .execute(&store.pool)
        .await
        .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(role_permissions::get_on(&mut tx, role_id).await.is_err());
    assert!(
        role_permissions::compare_and_set_on(&mut tx, role_id, 1, &[])
            .await
            .is_err()
    );
    assert!(
        store::auth::grant_on(&mut tx, role_id, second.id)
            .await
            .is_err()
    );
    assert!(
        store::sqlx::query("DELETE FROM role_permission WHERE role_id = ? AND permission_id = ?")
            .bind(role_id)
            .bind(first.id)
            .execute(&mut *tx)
            .await
            .is_err()
    );
    let ids = store::sqlx::query_scalar::<_, i64>(
        "SELECT permission_id FROM role_permission WHERE role_id = ?",
    )
    .bind(role_id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(ids, vec![first.id]);
}

#[tokio::test]
async fn role_permissions_noop_at_max_but_overflow_precedes_compound_mutation() {
    let (store, role_id, first, second) = fixture().await;
    store::sqlx::query("UPDATE role_permission_revision SET revision = ? WHERE role_id = ?")
        .bind(i64::MAX - 1)
        .bind(role_id)
        .execute(&store.pool)
        .await
        .unwrap();
    let before = read(&store, role_id).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        role_permissions::compare_and_set_on(
            &mut tx,
            role_id,
            before.revision,
            std::slice::from_ref(&second)
        )
        .await
        .is_err()
    );
    assert_eq!(
        role_permissions::get_on(&mut tx, role_id).await.unwrap(),
        before
    );
    store::sqlx::query("UPDATE role_permission_revision SET revision = ? WHERE role_id = ?")
        .bind(i64::MAX)
        .bind(role_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    let maximum = role_permissions::get_on(&mut tx, role_id).await.unwrap();
    assert_eq!(
        role_permissions::compare_and_set_on(
            &mut tx,
            role_id,
            i64::MAX,
            std::slice::from_ref(&first)
        )
        .await
        .unwrap(),
        maximum
    );
    assert!(
        role_permissions::compare_and_set_on(&mut tx, role_id, i64::MAX, &[])
            .await
            .is_err()
    );
    assert!(
        store::auth::grant_on(&mut tx, role_id, second.id)
            .await
            .is_err()
    );
    assert_eq!(
        role_permissions::get_on(&mut tx, role_id).await.unwrap(),
        maximum
    );
}

#[tokio::test]
async fn role_permissions_savepoint_failure_preserves_set_and_earlier_caller_write() {
    let (store, role_id, _, second) = fixture().await;
    let before = read(&store, role_id).await;
    store::sqlx::query("CREATE TRIGGER role_permissions_injected_failure BEFORE INSERT ON role_permission BEGIN SELECT RAISE(ABORT, 'injected insert refusal'); END")
        .execute(&store.pool).await.unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::sqlx::query("UPDATE role SET name = 'earlier caller write' WHERE id = ?")
        .bind(role_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(
        role_permissions::compare_and_set_on(&mut tx, role_id, before.revision, &[second])
            .await
            .is_err()
    );
    assert_eq!(
        role_permissions::get_on(&mut tx, role_id).await.unwrap(),
        before
    );
    tx.commit().await.unwrap();
    assert_eq!(read(&store, role_id).await, before);
    assert_eq!(
        store::auth::role_by_name(&store.pool, "earlier caller write")
            .await
            .unwrap(),
        Some(role_id)
    );
}

#[tokio::test]
async fn role_permissions_cancellation_after_delete_restores_savepoint_only() {
    let (store, role_id, _, second) = fixture().await;
    let before = read(&store, role_id).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::sqlx::query("UPDATE role SET name = 'before cancelled replacement' WHERE id = ?")
        .bind(role_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    let (deleted_sender, deleted_receiver) = tokio::sync::oneshot::channel();
    let (release_sender, release_receiver) = std::sync::mpsc::channel();
    let mut deleted_sender = Some(deleted_sender);
    tx.lock_handle()
        .await
        .unwrap()
        .set_update_hook(move |update| {
            if update.table == "role_permission"
                && matches!(
                    update.operation,
                    store::sqlx::sqlite::SqliteOperation::Delete
                )
                && let Some(sender) = deleted_sender.take()
            {
                let _ = sender.send(());
                let _ = release_receiver.recv_timeout(Duration::from_secs(5));
            }
        });
    let proposed = [second];
    let mut replacing = Box::pin(role_permissions::compare_and_set_on(
        &mut tx,
        role_id,
        before.revision,
        &proposed,
    ));
    let reached_delete = tokio::select! {
        result = &mut replacing => panic!("replacement completed before cancellation gate: {result:?}"),
        result = tokio::time::timeout(Duration::from_secs(3), deleted_receiver) => result,
    };
    drop(replacing);
    let released = release_sender.send(());
    reached_delete.unwrap().unwrap();
    released.unwrap();
    assert_eq!(
        role_permissions::get_on(&mut tx, role_id).await.unwrap(),
        before
    );
    tx.lock_handle().await.unwrap().remove_update_hook();
    tx.commit().await.unwrap();
    assert_eq!(read(&store, role_id).await, before);
    assert_eq!(
        store::auth::role_by_name(&store.pool, "before cancelled replacement")
            .await
            .unwrap(),
        Some(role_id)
    );
}

#[tokio::test]
async fn role_permissions_outer_rollback_restores_other_writes_and_revision() {
    let (store, role_id, _, second) = fixture().await;
    let before = read(&store, role_id).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::sqlx::query("UPDATE role SET name = 'rolled back' WHERE id = ?")
        .bind(role_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    role_permissions::compare_and_set_on(&mut tx, role_id, before.revision, &[second])
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(read(&store, role_id).await, before);
    assert_eq!(
        store::auth::role_by_name(&store.pool, "worker")
            .await
            .unwrap(),
        Some(role_id)
    );
}

#[tokio::test]
async fn role_permissions_legacy_grant_revoke_and_clear_regrant_invalidate_cas() {
    let (store, role_id, first, second) = fixture().await;
    let initial = read(&store, role_id).await;
    store::auth::grant(&store.pool, role_id, first.id)
        .await
        .unwrap();
    assert_eq!(read(&store, role_id).await, initial);
    store::auth::revoke(&store.pool, role_id, second.id)
        .await
        .unwrap();
    assert_eq!(read(&store, role_id).await, initial);
    store::auth::revoke(&store.pool, role_id, first.id)
        .await
        .unwrap();
    let empty = read(&store, role_id).await;
    assert!(empty.revision > initial.revision);
    store::auth::grant(&store.pool, role_id, first.id)
        .await
        .unwrap();
    let restored = read(&store, role_id).await;
    assert_eq!(restored.permissions, initial.permissions);
    assert!(restored.revision > empty.revision);
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        role_permissions::compare_and_set_on(&mut tx, role_id, initial.revision, &[second])
            .await
            .is_err()
    );
}

#[tokio::test]
async fn role_permissions_membership_move_invalidates_both_roles_not_unrelated() {
    let (store, first_role, first, second) = fixture().await;
    let second_role = role(&store, "second").await;
    let unrelated_role = role(&store, "unrelated").await;
    let old_first = read(&store, first_role).await;
    let old_second = read(&store, second_role).await;
    let unrelated = read(&store, unrelated_role).await;
    store::sqlx::query("UPDATE role_permission SET role_id = ?, permission_id = ? WHERE role_id = ? AND permission_id = ?")
        .bind(second_role).bind(second.id).bind(first_role).bind(first.id).execute(&store.pool).await.unwrap();
    assert!(read(&store, first_role).await.revision > old_first.revision);
    let moved = read(&store, second_role).await;
    assert!(moved.revision > old_second.revision);
    assert_eq!(moved.permissions, vec![second.clone()]);
    assert_eq!(read(&store, unrelated_role).await, unrelated);
    store::sqlx::query("UPDATE role_permission SET role_id = role_id, permission_id = permission_id WHERE role_id = ?")
        .bind(second_role).execute(&store.pool).await.unwrap();
    assert_eq!(read(&store, second_role).await, moved);
    store::sqlx::query("UPDATE role_permission SET permission_id = ? WHERE role_id = ?")
        .bind(first.id)
        .bind(second_role)
        .execute(&store.pool)
        .await
        .unwrap();
    assert!(read(&store, second_role).await.revision > moved.revision);
}

#[tokio::test]
async fn role_permissions_permission_key_edits_invalidate_all_using_roles() {
    let (store, first_role, first, _) = fixture().await;
    let second_role = role(&store, "second").await;
    let unrelated_role = role(&store, "unrelated").await;
    store::auth::grant(&store.pool, second_role, first.id)
        .await
        .unwrap();
    let before_first = read(&store, first_role).await;
    let before_second = read(&store, second_role).await;
    let unrelated = read(&store, unrelated_role).await;
    store::sqlx::query("UPDATE permission SET description = 'new description', subject = subject, action = action WHERE id = ?")
        .bind(first.id).execute(&store.pool).await.unwrap();
    assert_eq!(read(&store, first_role).await, before_first);
    store::sqlx::query("UPDATE permission SET subject = 'other', action = 'changed' WHERE id = ?")
        .bind(first.id)
        .execute(&store.pool)
        .await
        .unwrap();
    let after_first = read(&store, first_role).await;
    assert!(after_first.revision > before_first.revision);
    assert!(read(&store, second_role).await.revision > before_second.revision);
    assert_eq!(after_first.permissions[0].subject, "other");
    assert_eq!(read(&store, unrelated_role).await, unrelated);
    store::sqlx::query("UPDATE permission SET subject = ?, action = ? WHERE id = ?")
        .bind(&first.subject)
        .bind(&first.action)
        .bind(first.id)
        .execute(&store.pool)
        .await
        .unwrap();
    let restored = read(&store, first_role).await;
    assert!(restored.revision > after_first.revision);
    assert_eq!(restored.permissions, before_first.permissions);
}

#[tokio::test]
async fn role_permissions_key_edit_exhaustion_rolls_back_every_affected_role() {
    let (store, first_role, first, _) = fixture().await;
    let second_role = role(&store, "second").await;
    store::auth::grant(&store.pool, second_role, first.id)
        .await
        .unwrap();
    store::sqlx::query("UPDATE role_permission_revision SET revision = ? WHERE role_id = ?")
        .bind(i64::MAX)
        .bind(second_role)
        .execute(&store.pool)
        .await
        .unwrap();
    let before_first = read(&store, first_role).await;
    let before_second = read(&store, second_role).await;
    assert!(
        store::sqlx::query("UPDATE permission SET action = 'changed' WHERE id = ?")
            .bind(first.id)
            .execute(&store.pool)
            .await
            .is_err()
    );
    assert_eq!(read(&store, first_role).await, before_first);
    assert_eq!(read(&store, second_role).await, before_second);
}

#[tokio::test]
async fn role_permissions_permission_id_move_advances_version_before_reference_repair() {
    let (store, role_id, first, _) = fixture().await;
    let before = read(&store, role_id).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::sqlx::query("PRAGMA defer_foreign_keys = ON")
        .execute(&mut *tx)
        .await
        .unwrap();
    let moved_id = first.id + 1000;
    store::sqlx::query("UPDATE permission SET id = ? WHERE id = ?")
        .bind(moved_id)
        .bind(first.id)
        .execute(&mut *tx)
        .await
        .unwrap();
    let edited_revision = epoch_on(&mut tx, role_id).await;
    assert!(edited_revision > before.revision);
    assert!(role_permissions::get_on(&mut tx, role_id).await.is_err());
    store::sqlx::query(
        "UPDATE role_permission SET permission_id = ? WHERE role_id = ? AND permission_id = ?",
    )
    .bind(moved_id)
    .bind(role_id)
    .bind(first.id)
    .execute(&mut *tx)
    .await
    .unwrap();
    let moved = role_permissions::get_on(&mut tx, role_id).await.unwrap();
    assert!(moved.revision > edited_revision);
    assert_eq!(
        moved.permissions,
        vec![PermissionIdentity {
            id: moved_id,
            ..first
        }]
    );
    tx.commit().await.unwrap();
    assert_eq!(read(&store, role_id).await, moved);
}

#[tokio::test]
async fn role_permissions_permission_delete_recreate_and_regrant_retains_history() {
    let (store, role_id, first, _) = fixture().await;
    let before = read(&store, role_id).await;
    store::sqlx::query("DELETE FROM permission WHERE id = ?")
        .bind(first.id)
        .execute(&store.pool)
        .await
        .unwrap();
    let deleted = read(&store, role_id).await;
    assert!(deleted.permissions.is_empty());
    assert!(deleted.revision > before.revision);
    store::sqlx::query("INSERT INTO permission (id, subject, action) VALUES (?, ?, ?)")
        .bind(first.id)
        .bind(&first.subject)
        .bind(&first.action)
        .execute(&store.pool)
        .await
        .unwrap();
    store::auth::grant(&store.pool, role_id, first.id)
        .await
        .unwrap();
    let recreated = read(&store, role_id).await;
    assert_eq!(recreated.permissions, before.permissions);
    assert!(recreated.revision > deleted.revision);
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        role_permissions::compare_and_set_on(&mut tx, role_id, before.revision, &[])
            .await
            .is_err()
    );
}

#[tokio::test]
async fn role_permissions_role_delete_recreate_never_resets_revision() {
    let (store, role_id, first, _) = fixture().await;
    let before = read(&store, role_id).await;
    store::sqlx::query("DELETE FROM role WHERE id = ?")
        .bind(role_id)
        .execute(&store.pool)
        .await
        .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let deleted_revision = epoch_on(&mut tx, role_id).await;
    assert!(deleted_revision > before.revision);
    assert!(role_permissions::get_on(&mut tx, role_id).await.is_err());
    store::sqlx::query("INSERT INTO role (id, name) VALUES (?, 'recreated')")
        .bind(role_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(epoch_on(&mut tx, role_id).await > deleted_revision);
    store::auth::grant_on(&mut tx, role_id, first.id)
        .await
        .unwrap();
    assert!(
        role_permissions::compare_and_set_on(&mut tx, role_id, before.revision, &[])
            .await
            .is_err()
    );
    assert_eq!(
        role_permissions::get_on(&mut tx, role_id)
            .await
            .unwrap()
            .permissions,
        before.permissions
    );
}

#[tokio::test]
async fn role_permissions_role_id_moves_invalidate_both_retained_identities() {
    let store = Store::open_memory().await.unwrap();
    let role_id = role(&store, "moving").await;
    let original = read(&store, role_id).await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let moved_id = role_id + 1000;
    store::sqlx::query("UPDATE role SET id = ? WHERE id = ?")
        .bind(moved_id)
        .bind(role_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    let old_tombstone = epoch_on(&mut tx, role_id).await;
    assert!(old_tombstone > original.revision);
    let moved = role_permissions::get_on(&mut tx, moved_id).await.unwrap();
    store::sqlx::query("UPDATE role SET id = ? WHERE id = ?")
        .bind(role_id)
        .bind(moved_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(epoch_on(&mut tx, moved_id).await > moved.revision);
    assert!(
        role_permissions::get_on(&mut tx, role_id)
            .await
            .unwrap()
            .revision
            > old_tombstone
    );
    assert!(
        role_permissions::compare_and_set_on(&mut tx, role_id, original.revision, &[])
            .await
            .is_err()
    );
}

#[tokio::test]
async fn role_permissions_cascade_exhaustion_leaves_role_membership_and_revision_intact() {
    let (store, role_id, _, _) = fixture().await;
    store::sqlx::query("UPDATE role_permission_revision SET revision = ? WHERE role_id = ?")
        .bind(i64::MAX - 1)
        .bind(role_id)
        .execute(&store.pool)
        .await
        .unwrap();
    let before = read(&store, role_id).await;
    assert!(
        store::sqlx::query("DELETE FROM role WHERE id = ?")
            .bind(role_id)
            .execute(&store.pool)
            .await
            .is_err()
    );
    assert_eq!(read(&store, role_id).await, before);
}

#[tokio::test]
async fn role_permissions_exhausted_tombstone_refuses_role_recreation() {
    let store = Store::open_memory().await.unwrap();
    let role_id = role(&store, "exhausted").await;
    store::sqlx::query("DELETE FROM role WHERE id = ?")
        .bind(role_id)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE role_permission_revision SET revision = ? WHERE role_id = ?")
        .bind(i64::MAX)
        .bind(role_id)
        .execute(&store.pool)
        .await
        .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        store::sqlx::query("INSERT INTO role (id, name) VALUES (?, 'cannot recreate')")
            .bind(role_id)
            .execute(&mut *tx)
            .await
            .is_err()
    );
    assert!(role_permissions::get_on(&mut tx, role_id).await.is_err());
    assert_eq!(epoch_on(&mut tx, role_id).await, i64::MAX);
    let present: i64 = store::sqlx::query_scalar("SELECT count(*) FROM role WHERE id = ?")
        .bind(role_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(present, 0);
}

#[tokio::test]
async fn role_permissions_count_bounds_apply_to_stored_and_proposed_sets() {
    let store = Store::open_memory().await.unwrap();
    let role_id = role(&store, "bounded").await;
    let permissions = many_permissions(&store, 1025, 8).await;
    let initial = read(&store, role_id).await;
    let maximum = replace(&store, role_id, initial.revision, &permissions[..1024]).await;
    assert_eq!(maximum.permissions.len(), 1024);
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        role_permissions::compare_and_set_on(&mut tx, role_id, maximum.revision, &permissions)
            .await
            .is_err()
    );
    assert_eq!(
        role_permissions::get_on(&mut tx, role_id).await.unwrap(),
        maximum
    );
    store::auth::grant_on(&mut tx, role_id, permissions[1024].id)
        .await
        .unwrap();
    assert!(role_permissions::get_on(&mut tx, role_id).await.is_err());
    assert!(
        role_permissions::compare_and_set_on(&mut tx, role_id, maximum.revision + 1, &[])
            .await
            .is_err()
    );
}

#[tokio::test]
async fn role_permissions_key_bytes_count_utf8_and_reject_stored_oversize() {
    let store = Store::open_memory().await.unwrap();
    let role_id = role(&store, "bytes").await;
    let maximum = permission(&store, &format!("{}x", "é".repeat(126)), "ab").await;
    assert_eq!(maximum.subject.len() + 1 + maximum.action.len(), 256);
    let initial = read(&store, role_id).await;
    let saved = replace(
        &store,
        role_id,
        initial.revision,
        std::slice::from_ref(&maximum),
    )
    .await;
    let oversized = permission(&store, &"x".repeat(255), "r").await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        role_permissions::compare_and_set_on(
            &mut tx,
            role_id,
            saved.revision,
            std::slice::from_ref(&oversized)
        )
        .await
        .is_err()
    );
    assert_eq!(
        role_permissions::get_on(&mut tx, role_id).await.unwrap(),
        saved
    );
    store::auth::grant_on(&mut tx, role_id, oversized.id)
        .await
        .unwrap();
    assert!(role_permissions::get_on(&mut tx, role_id).await.is_err());
}

#[tokio::test]
async fn role_permissions_aggregate_bytes_are_complete_and_bounded_before_projection() {
    let store = Store::open_memory().await.unwrap();
    let role_id = role(&store, "aggregate").await;
    let permissions = many_permissions(&store, 257, 256).await;
    let initial = read(&store, role_id).await;
    let saved = replace(&store, role_id, initial.revision, &permissions[..256]).await;
    assert_eq!(saved.permissions.len(), 256);
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        role_permissions::compare_and_set_on(&mut tx, role_id, saved.revision, &permissions)
            .await
            .is_err()
    );
    assert_eq!(
        role_permissions::get_on(&mut tx, role_id).await.unwrap(),
        saved
    );
    store::auth::grant_on(&mut tx, role_id, permissions[256].id)
        .await
        .unwrap();
    assert!(role_permissions::get_on(&mut tx, role_id).await.is_err());
}

#[tokio::test]
async fn role_permissions_corrupt_key_and_orphan_membership_do_not_disappear() {
    let (store, role_id, first, _) = fixture().await;
    store::sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("UPDATE permission SET action = '' WHERE id = ?")
        .bind(first.id)
        .execute(&store.pool)
        .await
        .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(role_permissions::get_on(&mut tx, role_id).await.is_err());
    tx.rollback().await.unwrap();
    store::sqlx::query("UPDATE permission SET action = ? WHERE id = ?")
        .bind(&first.action)
        .bind(first.id)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("INSERT INTO role_permission (role_id, permission_id) VALUES (?, ?)")
        .bind(role_id)
        .bind(i64::MAX)
        .execute(&store.pool)
        .await
        .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(role_permissions::get_on(&mut tx, role_id).await.is_err());
}

#[tokio::test]
async fn role_permissions_permission_id_with_changed_pair_cannot_be_reused_in_proposal() {
    let store = Store::open_memory().await.unwrap();
    let role_id = role(&store, "identity").await;
    let old = permission(&store, "record", "read").await;
    let before = read(&store, role_id).await;
    store::sqlx::query("DELETE FROM permission WHERE id = ?")
        .bind(old.id)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query(
        "INSERT INTO permission (id, subject, action) VALUES (?, 'permission', 'assign')",
    )
    .bind(old.id)
    .execute(&store.pool)
    .await
    .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        role_permissions::compare_and_set_on(&mut tx, role_id, before.revision, &[old])
            .await
            .is_err()
    );
    assert_eq!(
        role_permissions::get_on(&mut tx, role_id).await.unwrap(),
        before
    );
}

#[tokio::test]
async fn role_permissions_policy_person_access_filter_and_credentials_are_unchanged() {
    let (store, role_id, _, second) = fixture().await;
    let person = store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Person",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap();
    store::auth::create_credential(&store.pool, &person.uid, "person", "opaque hash", role_id)
        .await
        .unwrap();
    let access = store::auth::person_access(&store.pool, &person.uid)
        .await
        .unwrap()
        .unwrap();
    let access = store::auth::compare_and_set_read_filter(
        &store.pool,
        &person.uid,
        Some("{\"kind\":\"plain\"}"),
        access.revision,
    )
    .await
    .unwrap();
    let policy = store::role_policies::set(
        &store.pool,
        role_id,
        &json!({"read": {"kind": "plain"}, "grants": []}),
        0,
    )
    .await
    .unwrap();
    let credential = store::sqlx::query_as::<_, (String, String, String)>(
        "SELECT person_uid, username, password_hash FROM person_credential WHERE person_uid = ?",
    )
    .bind(&person.uid)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    let before = read(&store, role_id).await;
    replace(&store, role_id, before.revision, &[second]).await;
    assert_eq!(
        store::role_policies::get(&store.pool, role_id)
            .await
            .unwrap(),
        Some(policy)
    );
    assert_eq!(
        store::auth::person_access(&store.pool, &person.uid)
            .await
            .unwrap(),
        Some(access)
    );
    assert_eq!(
        store::sqlx::query_as::<_, (String, String, String)>(
            "SELECT person_uid, username, password_hash FROM person_credential WHERE person_uid = ?"
        )
        .bind(&person.uid)
        .fetch_one(&store.pool)
        .await
        .unwrap(),
        credential
    );
    let empty_role = role(&store, "without policy").await;
    let current = read(&store, empty_role).await;
    replace(&store, empty_role, current.revision, &before.permissions).await;
    assert_eq!(
        store::role_policies::get(&store.pool, empty_role)
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn role_permissions_two_connections_observe_atomic_cas_and_restart() {
    let directory = std::env::temp_dir().join(nucleus::new_uid("role-permissions"));
    std::fs::create_dir(&directory).unwrap();
    let url = format!("sqlite://{}", directory.join("store.db").display());
    let store = Store::open(&url).await.unwrap();
    let role_id = role(&store, "concurrent").await;
    let first = permission(&store, "record", "read").await;
    let second = permission(&store, "record", "update").await;
    let initial = read(&store, role_id).await;
    let before = replace(&store, role_id, initial.revision, &[first]).await;
    let mut observer = store.pool.acquire().await.unwrap();
    store::sqlx::query("PRAGMA busy_timeout = 0")
        .execute(&mut *observer)
        .await
        .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let saved = role_permissions::compare_and_set_on(
        &mut tx,
        role_id,
        before.revision,
        std::slice::from_ref(&second),
    )
    .await
    .unwrap();
    let visible = store::sqlx::query_scalar::<_, i64>(
        "SELECT permission_id FROM role_permission WHERE role_id = ?",
    )
    .bind(role_id)
    .fetch_all(&mut *observer)
    .await
    .unwrap();
    assert_eq!(visible, vec![before.permissions[0].id]);
    assert!(
        store::sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *observer)
            .await
            .is_err()
    );
    tx.commit().await.unwrap();
    drop(observer);
    let mut stale = store::write_tx(&store.pool).await.unwrap();
    assert!(
        role_permissions::compare_and_set_on(&mut stale, role_id, before.revision, &[])
            .await
            .is_err()
    );
    stale.rollback().await.unwrap();
    assert_eq!(read(&store, role_id).await, saved);
    store.pool.close().await;
    let reopened = Store::open(&url).await.unwrap();
    assert_eq!(read(&reopened, role_id).await, saved);
    reopened.pool.close().await;
    std::fs::remove_dir_all(directory).unwrap();
}
