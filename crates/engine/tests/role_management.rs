use engine::{Engine, EngineError, actions::Action};
use serde_json::json;

async fn role(e: &Engine, name: &str) -> store::roles::Role {
    store::roles::catalog(&e.store.pool, true)
        .await
        .unwrap()
        .into_iter()
        .find(|role| role.name == name)
        .unwrap()
}

async fn create(e: &Engine, name: &str) -> store::roles::Role {
    e.act(Action::CreateRole { name: name.into() }, None)
        .await
        .unwrap();
    role(e, name).await
}

fn rename(role: &store::roles::Role, name: &str) -> Action {
    Action::RenameRole {
        role: role.id,
        expected_revision: role.revision,
        name: name.into(),
    }
}

fn delete(role: &store::roles::Role) -> Action {
    Action::DeleteRole {
        role: role.id,
        expected_revision: role.revision,
    }
}

#[tokio::test]
async fn rename_preserves_identity_permissions_membership_and_policy() {
    let e = Engine::open_memory().await.unwrap();
    let original = create(&e, "staff").await;
    e.act(
        Action::GrantPermission {
            role: "staff".into(),
            permission: "record:read".into(),
        },
        None,
    )
    .await
    .unwrap();
    let original = role(&e, &original.name).await;
    let person =
        store::auth::create_person_login(&e.store.pool, "Person", "person", "hash", original.id)
            .await
            .unwrap();
    let policy = store::role_policies::set(&e.store.pool, original.id, &json!({}), 0)
        .await
        .unwrap();
    e.act(rename(&original, "colleagues"), None).await.unwrap();
    let renamed = role(&e, "colleagues").await;
    assert_eq!(renamed.id, original.id);
    assert_eq!(renamed.permissions, original.permissions);
    assert_eq!(renamed.revision, original.revision + 1);
    assert_eq!(
        store::role_policies::get(&e.store.pool, renamed.id)
            .await
            .unwrap(),
        Some(policy)
    );
    let principal = store::auth::principal(&e.store.pool, &person)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(principal.role, "colleagues");
    assert!(principal.permits("record:read"));
    assert!(matches!(
        e.act(rename(&original, "stale"), None).await,
        Err(EngineError::Conflict {
            code: "role_changed",
            ..
        })
    ));
    assert!(matches!(
        e.act(delete(&renamed), None).await,
        Err(EngineError::Conflict {
            code: "role_assigned",
            ..
        })
    ));
    e.act(
        Action::DeleteUser {
            user: person.clone(),
        },
        None,
    )
    .await
    .unwrap();
    assert!(matches!(
        e.act(delete(&renamed), None).await,
        Err(EngineError::Conflict {
            code: "role_assigned",
            ..
        })
    ));
    e.act(
        Action::SetPersonStanding {
            person,
            active: false,
            note: None,
        },
        None,
    )
    .await
    .unwrap();
    assert!(matches!(
        e.act(delete(&renamed), None).await,
        Err(EngineError::Conflict {
            code: "role_assigned",
            ..
        })
    ));
}

#[tokio::test]
async fn deletion_cascades_and_old_requests_cannot_change_recreated_roles() {
    let e = Engine::open_memory().await.unwrap();
    create(&e, "temporary").await;
    e.act(
        Action::GrantPermission {
            role: "temporary".into(),
            permission: "record:read".into(),
        },
        None,
    )
    .await
    .unwrap();
    let original = role(&e, "temporary").await;
    store::role_policies::set(&e.store.pool, original.id, &json!({}), 0)
        .await
        .unwrap();
    e.act(delete(&original), None).await.unwrap();
    assert!(
        store::auth::role_by_name(&e.store.pool, "temporary")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store::role_policies::get(&e.store.pool, original.id)
            .await
            .unwrap()
            .is_none()
    );
    let recreated = create(&e, "temporary").await;
    assert_eq!(original.id, recreated.id);
    assert!(recreated.revision > original.revision);
    assert!(recreated.permissions.is_empty());
    for stale in [delete(&original), rename(&original, "hijacked")] {
        assert!(matches!(
            e.act(stale, None).await,
            Err(EngineError::Conflict {
                code: "role_changed",
                ..
            })
        ));
    }
    e.act(delete(&recreated), None).await.unwrap();
}

#[tokio::test]
async fn names_recovery_role_and_permission_revision_are_checked() {
    let e = Engine::open_memory().await.unwrap();
    let original = create(&e, "staff").await;
    create(&e, "occupied").await;
    let admin_id = store::auth::ensure_role(&e.store.pool, "admin")
        .await
        .unwrap();
    let admin = role(&e, "admin").await;
    assert_eq!(admin.id, admin_id);
    for name in [
        "",
        " spaced",
        "line\nbreak",
        &"a".repeat(101),
        "occupied",
        "admin",
    ] {
        assert!(
            e.act(rename(&original, name), None).await.is_err(),
            "{name:?}"
        );
    }
    assert!(e.act(rename(&admin, "recovery"), None).await.is_err());
    assert!(e.act(delete(&admin), None).await.is_err());
    e.act(
        Action::GrantPermission {
            role: "staff".into(),
            permission: "record:read".into(),
        },
        None,
    )
    .await
    .unwrap();
    assert!(matches!(
        e.act(delete(&original), None).await,
        Err(EngineError::Conflict {
            code: "role_changed",
            ..
        })
    ));
    assert!(matches!(
        e.act(rename(&original, "stale"), None).await,
        Err(EngineError::Conflict {
            code: "role_changed",
            ..
        })
    ));
    assert_eq!(role(&e, "staff").await.permissions, vec!["record:read"]);
}

#[tokio::test]
async fn direct_requests_require_current_permissions_and_cannot_manage_greater_access() {
    let e = Engine::open_memory().await.unwrap();
    let manager_role = create(&e, "manager").await;
    let manager = store::auth::create_person_login(
        &e.store.pool,
        "Manager",
        "manager",
        "hash",
        manager_role.id,
    )
    .await
    .unwrap();
    let target = create(&e, "target").await;
    for action in [rename(&target, "renamed"), delete(&target)] {
        assert!(matches!(
            e.act(action, Some(manager.clone())).await,
            Err(EngineError::Forbidden(_))
        ));
    }
    for permission in ["role:update", "role:delete"] {
        e.act(
            Action::GrantPermission {
                role: "manager".into(),
                permission: permission.into(),
            },
            None,
        )
        .await
        .unwrap();
    }
    e.act(rename(&target, "renamed"), Some(manager.clone()))
        .await
        .unwrap();
    e.act(
        Action::GrantPermission {
            role: "renamed".into(),
            permission: "record:read".into(),
        },
        None,
    )
    .await
    .unwrap();
    let target = role(&e, "renamed").await;
    for action in [rename(&target, "stronger"), delete(&target)] {
        assert!(matches!(
            e.act(action, Some(manager.clone())).await,
            Err(EngineError::Forbidden(_))
        ));
    }
    e.act(
        Action::GrantPermission {
            role: "manager".into(),
            permission: "record:read".into(),
        },
        None,
    )
    .await
    .unwrap();
    e.act(delete(&target), Some(manager.clone())).await.unwrap();
    let target = create(&e, "last").await;
    e.act(
        Action::RevokePermission {
            role: "manager".into(),
            permission: "role:delete".into(),
        },
        None,
    )
    .await
    .unwrap();
    assert!(matches!(
        e.act(delete(&target), Some(manager)).await,
        Err(EngineError::Forbidden(_))
    ));
}

#[tokio::test]
async fn seeding_does_not_recreate_a_renamed_or_deleted_default_role() {
    let e = Engine::open_memory().await.unwrap();
    store::seed::seed(&e.store.pool, &[]).await.unwrap();
    let original = role(&e, "lince").await;
    e.act(rename(&original, "members"), None).await.unwrap();
    store::seed::seed(&e.store.pool, &[]).await.unwrap();
    assert!(
        store::auth::role_by_name(&e.store.pool, "lince")
            .await
            .unwrap()
            .is_none()
    );
    e.act(delete(&role(&e, "members").await), None)
        .await
        .unwrap();
    store::seed::seed(&e.store.pool, &[]).await.unwrap();
    assert!(
        store::auth::role_by_name(&e.store.pool, "members")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store::auth::role_by_name(&e.store.pool, "lince")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn concurrent_renames_have_one_winner() {
    let e = Engine::open_memory().await.unwrap();
    let original = create(&e, "shared").await;
    let (first, second) = tokio::join!(
        e.act(rename(&original, "first"), None),
        e.act(rename(&original, "second"), None),
    );
    assert_ne!(first.is_ok(), second.is_ok());
    let rejected = if first.is_err() { first } else { second };
    assert!(matches!(
        rejected,
        Err(EngineError::Conflict {
            code: "role_changed",
            ..
        })
    ));
}
