use super::fixtures::{base, engine, make};
use engine::actions::Action;
use nucleus::RecordKind;
use protein::{Predicate, Source, execute, execute_for};
use serde_json::json;
use store::auth;

#[tokio::test]
async fn visibility_gate_is_the_one_read_boundary() {
    let e = engine().await;
    let public_need = make(&e, "public.apples", RecordKind::Plain, -1.0).await;
    make(&e, "private.diary", RecordKind::Plain, -1.0).await;
    let neighbor = make(&e, "neighbor", RecordKind::Person, 0.0).await;
    let unknown = make(&e, "unknown", RecordKind::Person, 0.0).await;
    let role = auth::ensure_role(&e.store.pool, "neighbor").await.unwrap();
    let read = auth::ensure_permission(&e.store.pool, "record", "read")
        .await
        .unwrap();
    auth::grant(&e.store.pool, role, read).await.unwrap();
    for person in [&neighbor, &unknown] {
        auth::set_user_role(&e.store.pool, person, role)
            .await
            .unwrap();
    }

    e.act(
        Action::GrantVisibility {
            subject_kind: "actor".into(),
            subject: Some(neighbor.clone()),
            target: public_need.clone(),
        },
        None,
    )
    .await
    .unwrap();

    let p = base(
        Source::Record,
        vec![Predicate::QuantityLt(store::exact::zero())],
    );
    assert_eq!(execute(&e.store, &p).await.unwrap().len(), 2);
    let seen = execute_for(&e.store, &p, Some(&neighbor)).await.unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0]["slug"], "public.apples");
    assert!(
        execute_for(&e.store, &p, Some(&unknown))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn auth_source_lists_roles_users_and_the_permission_catalog() {
    let e = engine().await;
    e.act(
        Action::CreateRole {
            name: "support".into(),
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::GrantPermission {
            role: "support".into(),
            permission: "record:delete_own".into(),
        },
        None,
    )
    .await
    .unwrap();
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
    .unwrap();

    let rows = execute(&e.store, &base(Source::Auth, vec![]))
        .await
        .unwrap();
    let role = rows
        .iter()
        .find(|r| r["kind"] == "role" && r["name"] == "support")
        .expect("the new role is listed");
    assert!(
        role["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p == "record:delete_own")
    );
    let user = rows
        .iter()
        .find(|r| r["kind"] == "user" && r["username"] == "amy")
        .expect("the new user is listed");
    assert_eq!(user["role"], "support");
    assert!(user.get("password_hash").is_none(), "never expose the hash");
    assert!(
        rows.iter().any(|r| r["kind"] == "permission_catalog"
            && r["keys"]
                .as_array()
                .unwrap()
                .iter()
                .any(|k| k == "record:delete")),
        "the static permission catalog is listed for building a grant UI"
    );
}

#[tokio::test]
async fn auth_source_is_gated_by_read_permission_for_a_remote_subject() {
    let e = engine().await;
    let bystander_role = auth::ensure_role(&e.store.pool, "bystander").await.unwrap();
    let bystander =
        auth::create_person_login(&e.store.pool, "B", "bystander", "hash", bystander_role)
            .await
            .unwrap();
    let reader_role = auth::ensure_role(&e.store.pool, "reader").await.unwrap();
    let perm_id = auth::ensure_permission(&e.store.pool, "role", "read")
        .await
        .unwrap();
    auth::grant(&e.store.pool, reader_role, perm_id)
        .await
        .unwrap();
    let reader = auth::create_person_login(&e.store.pool, "R", "reader", "hash", reader_role)
        .await
        .unwrap();

    let p = base(Source::Auth, vec![]);
    let hidden = execute_for(&e.store, &p, Some(&bystander)).await.unwrap();
    assert!(
        hidden.is_empty(),
        "no role/user/permission read grant -> nothing"
    );

    let visible = execute_for(&e.store, &p, Some(&reader)).await.unwrap();
    assert!(!visible.is_empty());
    assert!(visible.iter().all(|row| row["kind"] == "role"));
    assert!(visible.iter().all(|row| row["permissions"] == json!([])));

    auth::revoke(&e.store.pool, reader_role, perm_id)
        .await
        .unwrap();
    let permission = auth::ensure_permission(&e.store.pool, "permission", "read")
        .await
        .unwrap();
    auth::grant(&e.store.pool, reader_role, permission)
        .await
        .unwrap();
    let catalog = execute_for(&e.store, &p, Some(&reader)).await.unwrap();
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0]["kind"], "permission_catalog");

    let local = execute_for(&e.store, &p, None).await.unwrap();
    assert!(
        !local.is_empty(),
        "the local Cell (subject: None) always sees it"
    );
}
