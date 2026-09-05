use engine::Engine;
use engine::actions::Action;

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine opens")
}

async fn privileged_actor(e: &Engine, permission: &str) -> String {
    let role_id = store::auth::ensure_role(&e.store.pool, "wields-it")
        .await
        .unwrap();
    let (subject, action) = permission.split_once(':').unwrap();
    let perm_id = store::auth::ensure_permission(&e.store.pool, subject, action)
        .await
        .unwrap();
    store::auth::grant(&e.store.pool, role_id, perm_id)
        .await
        .unwrap();
    store::auth::create_person_login(&e.store.pool, "Wields It", "wields-it", "hash", role_id)
        .await
        .unwrap()
}

#[tokio::test]
async fn local_no_auth_mode_is_unrestricted_for_every_auth_action() {
    let e = engine().await;
    e.act(
        Action::CreateRole {
            name: "support".into(),
        },
        None,
    )
    .await
    .expect("local mode creates roles freely");
    e.act(
        Action::CreateUser {
            username: "amy".into(),
            name: "Amy".into(),
            password: "hunter2".into(),
            role: "support".into(),
        },
        None,
    )
    .await
    .expect("local mode creates users freely");
}

#[tokio::test]
async fn each_auth_action_denies_an_actor_without_its_permission() {
    let e = engine().await;
    let bystander = store::auth::ensure_role(&e.store.pool, "bystander-role")
        .await
        .unwrap();
    let bystander = store::auth::create_person_login(
        &e.store.pool,
        "Bystander",
        "bystander",
        "hash",
        bystander,
    )
    .await
    .unwrap();
    store::auth::ensure_role(&e.store.pool, "existing")
        .await
        .unwrap();

    let err = e
        .act(
            Action::CreateRole {
                name: "new-role".into(),
            },
            Some(bystander.to_string()),
        )
        .await
        .expect_err("no role:create grant");
    assert!(err.to_string().contains("forbidden"));

    let err = e
        .act(
            Action::CreateUser {
                username: "x".into(),
                name: "X".into(),
                password: "p".into(),
                role: "existing".into(),
            },
            Some(bystander.to_string()),
        )
        .await
        .expect_err("no user:create grant");
    assert!(err.to_string().contains("forbidden"));

    let err = e
        .act(
            Action::AssignRole {
                user: bystander.to_string(),
                role: "existing".into(),
            },
            Some(bystander.to_string()),
        )
        .await
        .expect_err("no user:assign_role grant");
    assert!(err.to_string().contains("forbidden"));

    let err = e
        .act(
            Action::GrantPermission {
                role: "existing".into(),
                permission: "record:read".into(),
            },
            Some(bystander.to_string()),
        )
        .await
        .expect_err("no permission:assign grant");
    assert!(err.to_string().contains("forbidden"));

    let err = e
        .act(
            Action::RevokePermission {
                role: "existing".into(),
                permission: "record:read".into(),
            },
            Some(bystander.to_string()),
        )
        .await
        .expect_err("no permission:assign grant");
    assert!(err.to_string().contains("forbidden"));
}

#[tokio::test]
async fn create_role_then_create_user_wires_a_working_login() {
    let e = engine().await;
    let admin = privileged_actor(&e, "role:create").await;
    let role_id = store::auth::ensure_role(&e.store.pool, "user-creator")
        .await
        .unwrap();
    let perm_id = store::auth::ensure_permission(&e.store.pool, "user", "create")
        .await
        .unwrap();
    store::auth::grant(&e.store.pool, role_id, perm_id)
        .await
        .unwrap();
    let creator =
        store::auth::create_person_login(&e.store.pool, "Creator", "creator", "hash", role_id)
            .await
            .unwrap();

    let outcome = e
        .act(
            Action::CreateRole {
                name: "support".into(),
            },
            Some(admin.to_string()),
        )
        .await
        .expect("role:create is granted");
    assert!(outcome.created.is_some());

    let outcome = e
        .act(
            Action::CreateUser {
                username: "amy".into(),
                name: "Amy".into(),
                password: "hunter2".into(),
                role: "support".into(),
            },
            Some(creator.to_string()),
        )
        .await
        .expect("user:create is granted");
    let user_id = outcome.created.expect("created a user");

    let user = store::auth::user_by_uid(&e.store.pool, &user_id)
        .await
        .unwrap()
        .expect("the new user exists");
    assert_eq!(user.username, "amy");
    assert_eq!(user.role, "support");
    assert!(
        utils::auth::verify_password("hunter2", &user.password_hash).unwrap(),
        "the stored hash must verify the plaintext password used to create it"
    );
    assert!(
        !utils::auth::verify_password("wrong-password", &user.password_hash).unwrap(),
        "a different password must not verify"
    );
}

#[tokio::test]
async fn create_user_with_unknown_role_is_rejected() {
    let e = engine().await;
    let err = e
        .act(
            Action::CreateUser {
                username: "amy".into(),
                name: "Amy".into(),
                password: "hunter2".into(),
                role: "no-such-role".into(),
            },
            None,
        )
        .await
        .expect_err("the role must already exist");
    assert!(err.to_string().contains("unknown role"));
}

#[tokio::test]
async fn assign_role_moves_a_user_between_roles() {
    let e = engine().await;
    let from_role = store::auth::ensure_role(&e.store.pool, "from-role")
        .await
        .unwrap();
    let to_role = store::auth::ensure_role(&e.store.pool, "to-role")
        .await
        .unwrap();
    let perm_id = store::auth::ensure_permission(&e.store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&e.store.pool, to_role, perm_id)
        .await
        .unwrap();
    let user_id = store::auth::create_person_login(&e.store.pool, "Amy", "amy", "hash", from_role)
        .await
        .unwrap();

    e.act(
        Action::AssignRole {
            user: user_id.to_string(),
            role: "to-role".into(),
        },
        None,
    )
    .await
    .expect("local mode assigns roles freely");

    let user = store::auth::user_by_uid(&e.store.pool, &user_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(user.role, "to-role");
    assert!(user.permissions.iter().any(|p| p == "record:read"));
}

#[tokio::test]
async fn grant_and_revoke_permission_round_trip() {
    let e = engine().await;
    store::auth::ensure_role(&e.store.pool, "toggled")
        .await
        .unwrap();

    e.act(
        Action::GrantPermission {
            role: "toggled".into(),
            permission: "record:delete_own".into(),
        },
        None,
    )
    .await
    .expect("local mode grants freely");
    let keys = store::auth::role_permission_keys(&e.store.pool, "toggled")
        .await
        .unwrap();
    assert!(keys.iter().any(|k| k == "record:delete_own"));

    e.act(
        Action::RevokePermission {
            role: "toggled".into(),
            permission: "record:delete_own".into(),
        },
        None,
    )
    .await
    .expect("local mode revokes freely");
    let keys = store::auth::role_permission_keys(&e.store.pool, "toggled")
        .await
        .unwrap();
    assert!(!keys.iter().any(|k| k == "record:delete_own"));
}

#[tokio::test]
async fn set_person_standing_deactivates_and_restores() {
    let e = engine().await;
    let role = store::auth::ensure_role(&e.store.pool, "staff")
        .await
        .unwrap();
    let person = store::auth::create_person_login(&e.store.pool, "Maria", "maria", "hash", role)
        .await
        .unwrap();

    e.act(
        Action::SetPersonStanding {
            person: person.clone(),
            active: false,
            note: Some("moved out".into()),
        },
        None,
    )
    .await
    .expect("local mode may deactivate");
    assert!(
        !store::people::is_active(&e.store.pool, &person)
            .await
            .unwrap()
    );

    e.act(
        Action::SetPersonStanding {
            person: person.clone(),
            active: true,
            note: None,
        },
        None,
    )
    .await
    .expect("and may restore");
    assert!(
        store::people::is_active(&e.store.pool, &person)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn deactivating_needs_user_update_not_record_update() {
    let e = engine().await;
    let editor_role = store::auth::ensure_role(&e.store.pool, "editor")
        .await
        .unwrap();
    let perm = store::auth::ensure_permission(&e.store.pool, "record", "update")
        .await
        .unwrap();
    store::auth::grant(&e.store.pool, editor_role, perm)
        .await
        .unwrap();
    let editor =
        store::auth::create_person_login(&e.store.pool, "Editor", "editor", "hash", editor_role)
            .await
            .unwrap();
    let target =
        store::auth::create_person_login(&e.store.pool, "Maria", "maria", "hash", editor_role)
            .await
            .unwrap();

    let err = e
        .act(
            Action::SetPersonStanding {
                person: target.clone(),
                active: false,
                note: None,
            },
            Some(editor.clone()),
        )
        .await
        .expect_err("record:update is not enough");
    assert!(err.to_string().contains("forbidden"), "{err}");
    assert!(
        store::people::is_active(&e.store.pool, &target)
            .await
            .unwrap()
    );

    let admin_perm = store::auth::ensure_permission(&e.store.pool, "user", "update")
        .await
        .unwrap();
    store::auth::grant(&e.store.pool, editor_role, admin_perm)
        .await
        .unwrap();
    e.act(
        Action::SetPersonStanding {
            person: target.clone(),
            active: false,
            note: None,
        },
        Some(editor),
    )
    .await
    .expect("user:update is");
    assert!(
        !store::people::is_active(&e.store.pool, &target)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn nobody_can_deactivate_themselves() {
    let e = engine().await;
    let role = store::auth::ensure_role(&e.store.pool, "admin-ish")
        .await
        .unwrap();
    let perm = store::auth::ensure_permission(&e.store.pool, "user", "update")
        .await
        .unwrap();
    store::auth::grant(&e.store.pool, role, perm).await.unwrap();
    let admin = store::auth::create_person_login(&e.store.pool, "Admin", "admin", "hash", role)
        .await
        .unwrap();

    let err = e
        .act(
            Action::SetPersonStanding {
                person: admin.clone(),
                active: false,
                note: None,
            },
            Some(admin.clone()),
        )
        .await
        .expect_err("self-deactivation is refused");
    assert!(
        err.to_string().contains("cannot deactivate yourself"),
        "{err}"
    );
    assert!(
        store::people::is_active(&e.store.pool, &admin)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn standing_is_refused_over_a_record_that_is_not_a_person() {
    let e = engine().await;
    let record = e
        .act(
            Action::CreateRecord {
                slug: Some("the-van".into()),
                kind: nucleus::RecordKind::Plain,
                head: "The van".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .expect("record")
        .created
        .expect("uid");

    let err = e
        .act(
            Action::SetPersonStanding {
                person: record,
                active: false,
                note: None,
            },
            None,
        )
        .await
        .expect_err("a thing has no standing");
    assert!(err.to_string().contains("not a Person"), "{err}");
}

#[tokio::test]
async fn the_last_active_admin_cannot_be_deactivated() {
    let e = engine().await;
    let admin_role = store::auth::ensure_role(&e.store.pool, store::auth::ADMIN_ROLE)
        .await
        .unwrap();
    let first =
        store::auth::create_person_login(&e.store.pool, "First", "first", "hash", admin_role)
            .await
            .unwrap();
    let second =
        store::auth::create_person_login(&e.store.pool, "Second", "second", "hash", admin_role)
            .await
            .unwrap();

    e.act(
        Action::SetPersonStanding {
            person: second.clone(),
            active: false,
            note: None,
        },
        None,
    )
    .await
    .expect("one of two admins may go");

    let err = e
        .act(
            Action::SetPersonStanding {
                person: first.clone(),
                active: false,
                note: None,
            },
            None,
        )
        .await
        .expect_err("the last active admin stays");
    assert!(err.to_string().contains("last active admin"), "{err}");
    assert!(
        store::people::is_active(&e.store.pool, &first)
            .await
            .unwrap()
    );

    e.act(
        Action::SetPersonStanding {
            person: second,
            active: true,
            note: None,
        },
        None,
    )
    .await
    .expect("restore");
    e.act(
        Action::SetPersonStanding {
            person: first.clone(),
            active: false,
            note: None,
        },
        None,
    )
    .await
    .expect("with a second active admin, the first may go");
    assert!(
        !store::people::is_active(&e.store.pool, &first)
            .await
            .unwrap()
    );
}
