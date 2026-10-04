use engine::{Engine, actions::Action};
use serde_json::{Value, json};

async fn fixture() -> (Engine, String, String, i64, i64) {
    let engine = Engine::open_memory().await.unwrap();
    let record = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Before".into(),
                body: "Body".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let person = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Editor",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    let a = store::auth::ensure_role(&engine.store.pool, "Head editors")
        .await
        .unwrap();
    let b = store::auth::ensure_role(&engine.store.pool, "Body editors")
        .await
        .unwrap();
    for role in [a, b] {
        for action in ["read", "update"] {
            let permission = store::auth::ensure_permission(&engine.store.pool, "record", action)
                .await
                .unwrap();
            store::auth::grant(&engine.store.pool, role, permission)
                .await
                .unwrap();
        }
    }
    engine
        .act(
            Action::AssignRoles {
                person: person.clone(),
                roles: vec!["Head editors".into(), "Body editors".into()],
                expected_revision: 0,
            },
            None,
        )
        .await
        .unwrap();
    (engine, record, person, a, b)
}

async fn role_name(engine: &Engine, role: i64) -> String {
    store::sqlx::query_scalar("SELECT name FROM role WHERE id=?")
        .bind(role)
        .fetch_one(&engine.store.pool)
        .await
        .unwrap()
}

async fn policy(engine: &Engine, role: i64, read: Value, selector: Value, properties: Value) {
    engine.act(Action::SetRolePolicy { role: role_name(engine, role).await, expected_revision: 0, policy: json!({"read":read,"grants":[{"operation":"update","selector":selector,"properties":properties,"assertions_add":[],"assertions_remove":[]}]}) }, None).await.unwrap();
}

#[tokio::test]
async fn matching_role_grants_add_properties_without_cross_granting_selectors() {
    let (engine, record, person, a, b) = fixture().await;
    policy(
        &engine,
        a,
        json!({"all":[]}),
        json!({"uid_eq":record}),
        json!(["head"]),
    )
    .await;
    policy(
        &engine,
        b,
        json!({"all":[]}),
        json!({"uid_eq":person}),
        json!(["body"]),
    )
    .await;
    assert!(
        engine
            .act(
                Action::EditRecordText {
                    target: record.clone(),
                    head: Some("After".into()),
                    body: Some("Changed".into())
                },
                Some(person.clone())
            )
            .await
            .is_err()
    );
    assert_eq!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .head,
        "Before"
    );
    engine.act(Action::SetRolePolicy { role:role_name(&engine, b).await, expected_revision:1, policy:json!({"read":{"all":[]},"grants":[{"operation":"update","selector":{"uid_eq":record},"properties":["body"],"assertions_add":[],"assertions_remove":[]}]}) }, None).await.unwrap();
    engine
        .act(
            Action::EditRecordText {
                target: record.clone(),
                head: Some("After".into()),
                body: Some("Changed".into()),
            },
            Some(person),
        )
        .await
        .unwrap();
    let result = store::records::get(&engine.store.pool, &record)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.head, "After");
    assert_eq!(result.body, "Changed");
}

#[tokio::test]
async fn non_assertion_selector_checks_postimage_and_rolls_back() {
    let (engine, record, person, a, b) = fixture().await;
    policy(
        &engine,
        a,
        json!({"all":[]}),
        json!({"quantity_lte":"1"}),
        json!(["quantity"]),
    )
    .await;
    policy(&engine, b, json!({"all":[]}), json!({"any":[]}), json!([])).await;
    engine
        .act(
            Action::SetQuantityExact {
                target: record.clone(),
                amount: "1".into(),
            },
            Some(person.clone()),
        )
        .await
        .unwrap();
    assert!(
        engine
            .act(
                Action::SetQuantityExact {
                    target: record.clone(),
                    amount: "2".into()
                },
                Some(person)
            )
            .await
            .is_err()
    );
    assert_eq!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        store::exact::one()
    );
}

#[tokio::test]
async fn removing_the_assertion_that_supplies_read_access_is_atomic() {
    let (engine, record, person, a, b) = fixture().await;
    let concept = store::concepts::ensure(&engine.store.pool, "permitted")
        .await
        .unwrap();
    let assertion = engine
        .act(
            Action::AssertRecord {
                subject: record.clone(),
                predicate: concept.clone(),
                object: None,
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let permission =
        json!({"predicate_uid":concept,"target":"unary","role":"ordinary","properties":[]});
    engine.act(Action::SetRolePolicy { role:role_name(&engine, a).await, expected_revision:0, policy:json!({"read":{"concept_in":concept},"grants":[{"operation":"update","selector":{"all":[]},"properties":[],"assertions_add":[],"assertions_remove":[permission]}]}) }, None).await.unwrap();
    policy(&engine, b, json!({"any":[]}), json!({"any":[]}), json!([])).await;
    assert!(
        engine
            .act(
                Action::RetractAssertion {
                    assertion: assertion.clone()
                },
                Some(person)
            )
            .await
            .is_err()
    );
    assert!(
        store::assertions::get(&engine.store.pool, &assertion)
            .await
            .unwrap()
            .unwrap()
            .retracted_at
            .is_none()
    );
}

#[tokio::test]
async fn a_relationship_edit_cannot_change_authority_for_an_undeclared_record() {
    let (engine, record, person, a, b) = fixture().await;
    let other = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Other".into(),
                body: "".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let concept = store::concepts::ensure(&engine.store.pool, "linked")
        .await
        .unwrap();
    engine.act(Action::SetRolePolicy { role:role_name(&engine, a).await, expected_revision:0, policy:json!({"read":{"all":[]},"grants":[{"operation":"update","selector":{"uid_eq":record},"properties":[],"assertions_add":[{"predicate_uid":concept,"role":"ordinary","target":{"record":other},"properties":[]}],"assertions_remove":[]}]}) }, None).await.unwrap();
    policy(&engine, b, json!({"all":[]}), json!({"any":[]}), json!([])).await;
    store::auth::ensure_role(&engine.store.pool, "Readers of linked Records")
        .await
        .unwrap();
    engine.act(Action::SetRolePolicy { role:"Readers of linked Records".into(), expected_revision:0, policy:json!({"read":{"relation":{"kind":concept,"direction":"in","other":record}},"grants":[]}) }, None).await.unwrap();
    let denied = engine
        .act(
            Action::AssertRecord {
                subject: record.clone(),
                predicate: concept.clone(),
                object: Some(other),
                quantity: None,
                unit: None,
            },
            Some(person),
        )
        .await
        .unwrap_err();
    assert!(denied.to_string().contains("outside its declared scope"));
    let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record_assertion WHERE subject_uid=? AND predicate_uid=? AND retracted_at IS NULL").bind(record).bind(concept).fetch_one(&engine.store.pool).await.unwrap();
    assert_eq!(count, 0);
}
