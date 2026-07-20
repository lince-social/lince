//! The permission/role/user system, driven through Actions (2026-07-18) — the
//! first brick of "the WS is guarded by Protein + Actions + Permissions":
//! `create-role`, `create-user`, `assign-role`, `grant-permission`,
//! `revoke-permission`. Each is gated on the matching key already in
//! `utils::auth::ALL_PERMISSIONS`; `actor: None` (local Cell) is unrestricted,
//! same convention as `delete-record` (crates/engine/tests/record_edits.rs).

use engine::Engine;
use engine::actions::Action;

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine opens")
}

async fn privileged_actor(e: &Engine, permission: &str) -> i64 {
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
    store::auth::create_user(&e.store.pool, "Wields It", "wields-it", "hash", role_id)
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
    let bystander =
        store::auth::create_user(&e.store.pool, "Bystander", "bystander", "hash", bystander)
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
    // role:create alone can't create-user too — a second, separately-granted actor.
    let role_id = store::auth::ensure_role(&e.store.pool, "user-creator")
        .await
        .unwrap();
    let perm_id = store::auth::ensure_permission(&e.store.pool, "user", "create")
        .await
        .unwrap();
    store::auth::grant(&e.store.pool, role_id, perm_id)
        .await
        .unwrap();
    let creator = store::auth::create_user(&e.store.pool, "Creator", "creator", "hash", role_id)
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
    let user_id: i64 = outcome.created.expect("created a user").parse().unwrap();

    let user = store::auth::user_by_id(&e.store.pool, user_id)
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
    let user_id = store::auth::create_user(&e.store.pool, "Amy", "amy", "hash", from_role)
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

    let user = store::auth::user_by_id(&e.store.pool, user_id)
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
