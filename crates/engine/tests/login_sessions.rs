use engine::{Engine, actions::Action, private_password::PasswordInput};

async fn setup() -> (Engine, String) {
    let engine = Engine::open_memory().await.unwrap();
    let permissions: Vec<_> = utils::auth::ALL_PERMISSIONS
        .iter()
        .map(|key| (key.subject, key.action))
        .collect();
    store::seed::seed(&engine.store.pool, &permissions)
        .await
        .unwrap();
    let person = engine
        .act(
            Action::CreateUser {
                username: "owner".into(),
                name: "Owner".into(),
                password: "test-password".into(),
                role: "admin".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    (engine, person)
}

fn password(value: &str) -> PasswordInput {
    PasswordInput::new(value.as_bytes().to_vec()).unwrap()
}

#[tokio::test]
async fn login_rejects_wrong_password_and_revokes_all_clones() {
    let (engine, person) = setup().await;
    assert!(
        engine
            .login_password("owner", password("wrong"), None)
            .await
            .is_err()
    );
    assert!(
        engine
            .login_password("missing", password("wrong"), None)
            .await
            .is_err()
    );
    let login = engine
        .login_password("owner", password("test-password"), None)
        .await
        .unwrap();
    assert_eq!(login.person_uid(), person);
    let copy = login.clone();
    let mut notice = copy.watch_revocation();
    login.revoke();
    notice.changed().await.unwrap();
    assert!(copy.require(&engine).await.is_err());
}

#[tokio::test]
async fn credential_replacement_and_restoration_never_restore_an_old_session() {
    let (engine, person) = setup().await;
    let login = engine
        .login_password("owner", password("test-password"), None)
        .await
        .unwrap();
    for value in ["replacement", "test-password"] {
        engine
            .act(
                Action::UpdateUser {
                    user: person.clone(),
                    username: "owner".into(),
                    name: "Owner".into(),
                    password: value.into(),
                },
                None,
            )
            .await
            .unwrap();
        assert!(login.require(&engine).await.is_err());
    }
    assert!(
        engine
            .login_password("owner", password("test-password"), None)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn peer_device_revocation_is_checked_even_after_it_is_restored() {
    let (engine, person) = setup().await;
    let node = "a".repeat(64);
    let login = engine
        .login_password("owner", password("test-password"), Some(&node))
        .await
        .unwrap();
    let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
    let revoked =
        store::session_access::compare_and_set_revoked_on(&mut tx, &person, &node, 1, true)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    assert!(login.require(&engine).await.is_err());
    let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
    store::session_access::compare_and_set_revoked_on(
        &mut tx,
        &person,
        &node,
        revoked.revision,
        false,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert!(login.require(&engine).await.is_err());
}

#[tokio::test]
async fn account_managers_cannot_grant_permissions_they_do_not_hold() {
    let (engine, owner) = setup().await;
    let role = store::auth::ensure_role(&engine.store.pool, "manager")
        .await
        .unwrap();
    for (subject, action) in [
        ("user", "create"),
        ("user", "update"),
        ("user", "assign_role"),
        ("permission", "assign"),
    ] {
        let permission = store::auth::ensure_permission(&engine.store.pool, subject, action)
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, permission)
            .await
            .unwrap();
    }
    let manager = engine
        .act(
            Action::CreateUser {
                username: "manager".into(),
                name: "Manager".into(),
                password: "password".into(),
                role: "manager".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    for action in [
        Action::GrantPermission {
            role: "manager".into(),
            permission: "record:delete".into(),
        },
        Action::CreateUser {
            username: "elevated".into(),
            name: "Elevated".into(),
            password: "password".into(),
            role: "admin".into(),
        },
        Action::AssignRole {
            user: manager.clone(),
            role: "admin".into(),
        },
    ] {
        assert!(engine.act(action, Some(manager.clone())).await.is_err());
    }
    assert!(
        store::auth::user_by_username(&engine.store.pool, "elevated")
            .await
            .unwrap()
            .is_none()
    );
    store::people::deactivate(&engine.store.pool, &owner, "2026-09-12T12:00:00Z", None)
        .await
        .unwrap();
    assert!(
        engine
            .act(
                Action::SetPersonStanding {
                    person: owner.clone(),
                    active: true,
                    note: None
                },
                Some(manager)
            )
            .await
            .is_err()
    );
    assert!(
        !store::people::is_active(&engine.store.pool, &owner)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn login_is_bound_to_its_hosted_organ() {
    let (engine, _) = setup().await;
    let login = engine
        .login_password("owner", password("test-password"), None)
        .await
        .unwrap();
    store::records::set_slug(&engine.store.pool, login.organ_uid(), Some("former-organ"))
        .await
        .unwrap();
    assert!(login.require(&engine).await.is_err());
}

#[tokio::test]
async fn shared_throttle_bounds_repeated_password_work() {
    let (engine, _) = setup().await;
    for _ in 0..10 {
        assert!(
            engine
                .login_password("owner", password("wrong"), None)
                .await
                .is_err()
        );
    }
    let result = engine
        .login_password("owner", password("test-password"), None)
        .await;
    assert_eq!(result.err().unwrap().code(), Some("login_throttled"));
}

#[tokio::test]
async fn work_waiting_for_an_access_change_rechecks_its_login_before_running() {
    let (engine, person) = setup().await;
    let engine = std::sync::Arc::new(engine);
    let login = engine
        .login_password("owner", password("test-password"), None)
        .await
        .unwrap();
    let (entered, ready) = tokio::sync::oneshot::channel();
    let (release, released) = tokio::sync::oneshot::channel();
    let changing = {
        let engine = engine.clone();
        tokio::spawn(async move {
            engine
                .access_scope(true, async {
                    entered.send(()).unwrap();
                    released.await.unwrap();
                    engine
                        .act(
                            Action::UpdateUser {
                                user: person,
                                username: "owner".into(),
                                name: "Owner".into(),
                                password: "replacement".into(),
                            },
                            None,
                        )
                        .await?;
                    Ok(())
                })
                .await
                .unwrap();
        })
    };
    ready.await.unwrap();
    let ran = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let waiting = {
        let engine = engine.clone();
        let ran = ran.clone();
        tokio::spawn(async move {
            login
                .run(&engine, false, async {
                    ran.store(true, std::sync::atomic::Ordering::SeqCst);
                    Ok(())
                })
                .await
        })
    };
    tokio::task::yield_now().await;
    assert!(!waiting.is_finished());
    release.send(()).unwrap();
    changing.await.unwrap();
    assert!(waiting.await.unwrap().is_err());
    assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
}

#[tokio::test]
async fn document_updates_cannot_bypass_person_management_permissions() {
    let (engine, owner) = setup().await;
    let role = store::auth::ensure_role(&engine.store.pool, "editor")
        .await
        .unwrap();
    for action in ["read", "update"] {
        let permission = store::auth::ensure_permission(&engine.store.pool, "record", action)
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, permission)
            .await
            .unwrap();
    }
    let editor =
        store::auth::create_person_login(&engine.store.pool, "Editor", "editor", "unused", role)
            .await
            .unwrap()
            .to_string();
    store::visibility::grant(&engine.store.pool, "actor", Some(&editor), &owner)
        .await
        .unwrap();
    assert!(engine.may_read_record(Some(&editor), &owner).await.unwrap());
    assert!(matches!(
        engine
            .apply_client_crdt_update_as(&owner, "", Some(&editor))
            .await,
        Err(engine::EngineError::Forbidden(_))
    ));
    assert!(matches!(
        engine
            .act(
                Action::EditRecordText {
                    target: owner,
                    head: Some("Replaced owner".into()),
                    body: None
                },
                Some(editor)
            )
            .await,
        Err(engine::EngineError::Forbidden(_))
    ));
}
