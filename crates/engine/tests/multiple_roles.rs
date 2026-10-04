use engine::{Engine, actions::Action};

async fn fixture() -> (Engine, String, i64, i64) {
    let engine = Engine::open_memory().await.unwrap();
    let person = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Worker",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    let a = store::auth::ensure_role(&engine.store.pool, "A")
        .await
        .unwrap();
    let b = store::auth::ensure_role(&engine.store.pool, "B")
        .await
        .unwrap();
    for (role, action) in [(a, "read"), (a, "update"), (b, "create")] {
        let permission = store::auth::ensure_permission(&engine.store.pool, "record", action)
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, permission)
            .await
            .unwrap();
    }
    (engine, person, a, b)
}

#[tokio::test]
async fn membership_is_additive_revisioned_and_shown_for_credential_free_actors() {
    let (engine, person, a, b) = fixture().await;
    engine
        .act(
            Action::AssignRoles {
                person: person.clone(),
                roles: vec!["A".into(), "B".into()],
                expected_revision: 0,
            },
            None,
        )
        .await
        .unwrap();
    let principal = store::auth::principal(&engine.store.pool, &person)
        .await
        .unwrap()
        .unwrap();
    assert!(principal.permits("record:read"));
    assert!(principal.permits("record:update"));
    assert!(principal.permits("record:create"));
    assert_eq!(
        store::person_roles::ids(&engine.store.pool, &person)
            .await
            .unwrap(),
        vec![a, b]
    );
    assert!(
        engine
            .act(
                Action::AssignRoles {
                    person: person.clone(),
                    roles: vec!["A".into()],
                    expected_revision: 0
                },
                None
            )
            .await
            .is_err()
    );
    let revision = store::auth::person_access(&engine.store.pool, &person)
        .await
        .unwrap()
        .unwrap()
        .revision;
    engine
        .act(
            Action::AssignRoles {
                person: person.clone(),
                roles: vec!["B".into()],
                expected_revision: revision,
            },
            None,
        )
        .await
        .unwrap();
    let principal = store::auth::principal(&engine.store.pool, &person)
        .await
        .unwrap()
        .unwrap();
    assert!(!principal.permits("record:read"));
    assert!(principal.permits("record:create"));
    let query = serde_json::from_value(serde_json::json!({"source":"auth"})).unwrap();
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    assert!(
        rows.iter()
            .any(|row| row["id"] == person && row["roles"] == serde_json::json!(["B"]))
    );
}

#[tokio::test]
async fn renamed_management_role_preserves_recovery_without_reseeding_its_name() {
    let (engine, person, _, _) = fixture().await;
    let permissions: Vec<_> = utils::auth::ALL_PERMISSIONS
        .iter()
        .map(|key| (key.subject, key.action))
        .collect();
    store::seed::seed(&engine.store.pool, &permissions)
        .await
        .unwrap();
    engine
        .act(
            Action::AssignRoles {
                person: person.clone(),
                roles: vec!["admin".into()],
                expected_revision: 0,
            },
            None,
        )
        .await
        .unwrap();
    let role = store::auth::role_by_name(&engine.store.pool, "admin")
        .await
        .unwrap()
        .unwrap();
    let revision = store::roles::catalog(&engine.store.pool, true)
        .await
        .unwrap()
        .into_iter()
        .find(|row| row.id == role)
        .unwrap()
        .revision;
    engine
        .act(
            Action::RenameRole {
                role,
                expected_revision: revision,
                name: "Maintainers".into(),
            },
            None,
        )
        .await
        .unwrap();
    store::seed::seed(&engine.store.pool, &permissions)
        .await
        .unwrap();
    assert!(
        store::auth::role_by_name(&engine.store.pool, "admin")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store::auth::admins(&engine.store.pool).await.unwrap(),
        vec![person.clone()]
    );
    let revision = store::auth::person_access(&engine.store.pool, &person)
        .await
        .unwrap()
        .unwrap()
        .revision;
    assert!(
        engine
            .act(
                Action::AssignRoles {
                    person: person.clone(),
                    roles: vec![],
                    expected_revision: revision
                },
                None
            )
            .await
            .is_err()
    );
    assert!(
        store::auth::principal(&engine.store.pool, &person)
            .await
            .unwrap()
            .unwrap()
            .permits("permission:assign")
    );
}
