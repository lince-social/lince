use engine::{Engine, actions::Action};
use serde_json::json;

#[tokio::test]
async fn selected_record_properties_keep_exact_quantity_work_and_visibility() {
    let engine = Engine::open_memory().await.unwrap();
    let uid = engine
        .act(
            Action::CreateRecord {
                slug: Some("schema-task".into()),
                kind: nucleus::RecordKind::Plain,
                head: "Task".into(),
                body: "Description".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine
        .act(
            Action::SetQuantityExact {
                target: uid.clone(),
                amount: "9007199254740993.125".into(),
            },
            None,
        )
        .await
        .unwrap();
    engine.act(Action::SetExtension { target: uid.clone(), namespace: "work".into(), fds: json!({"start":"2026-09-01","due":"2026-09-30","estimate_min":90,"logs":[{"start":"2026-09-01T10:00:00Z","end":"2026-09-01T10:15:00Z"}]}) }, None).await.unwrap();
    let query: protein::Protein = serde_json::from_value(json!({"source":"record","where":[{"uid_eq":uid}],"fields":["head","body","slug","quantity","assertions","assignees","start_date","due_date","estimate_min","spent_seconds","running_since","work_logs"]})).unwrap();
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["quantity"], "9007199254740993.125");
    assert_eq!(rows[0]["spent_seconds"], 900);
    assert_eq!(rows[0]["estimate_min"], 90);
    assert_eq!(rows[0]["body"], "Description");
    assert_eq!(rows[0]["start_date"], "2026-09-01");
    assert_eq!(rows[0]["due_date"], "2026-09-30");
    assert!(rows[0].get("extension").is_none());
    assert_eq!(
        rows[0]
            .as_object()
            .unwrap()
            .keys()
            .filter(|key| key.starts_with("quantity"))
            .count(),
        1
    );
    assert_eq!(rows[0]["work_logs"].as_array().unwrap().len(), 1);
    let person = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Person,
                head: "Visitor".into(),
                body: String::new(),
                quantity: 1.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    assert!(
        protein::execute_for(&engine.store, &query, Some(&person))
            .await
            .unwrap()
            .is_empty()
    );
    let mut narrowed = query.clone();
    narrowed.fields = Some(vec!["head".into()]);
    let rows = protein::execute(&engine.store, &narrowed).await.unwrap();
    assert_eq!(rows[0].as_object().unwrap().len(), 3);
    assert!(
        protein::record_schema::fields()
            .iter()
            .any(|field| field.key == "work_logs")
    );
    assert_eq!(protein::record_schema::selected(&narrowed).len(), 2);
}

#[tokio::test]
async fn assignees_and_assertions_do_not_reveal_an_unreadable_person() {
    let engine = Engine::open_memory().await.unwrap();
    let mut ids = Vec::new();
    for (head, kind) in [
        ("Task", nucleus::RecordKind::Plain),
        ("Assignee", nucleus::RecordKind::Person),
        ("Viewer", nucleus::RecordKind::Person),
    ] {
        ids.push(
            engine
                .act(
                    Action::CreateRecord {
                        slug: None,
                        kind,
                        head: head.into(),
                        body: String::new(),
                        quantity: 1.0,
                    },
                    None,
                )
                .await
                .unwrap()
                .created
                .unwrap(),
        );
    }
    engine
        .act(
            Action::CreateConcept {
                lingua: "g_local".into(),
                name: "assigned-to".into(),
                parents: Vec::new(),
            },
            None,
        )
        .await
        .unwrap();
    engine
        .act(
            Action::AssertRecord {
                subject: ids[0].clone(),
                predicate: "assigned-to".into(),
                object: Some(ids[1].clone()),
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    let query = serde_json::from_value(
        json!({"source":"record","where":[{"uid_eq":ids[0]}],"fields":["assignees","assertions"]}),
    )
    .unwrap();
    let local = protein::execute(&engine.store, &query).await.unwrap();
    assert_eq!(local[0]["assignees"][0]["head"], "Assignee");
    assert_eq!(local[0]["assertions"][0]["predicate"], "assigned-to");
    let role = store::auth::ensure_role(&engine.store.pool, "schema-viewer")
        .await
        .unwrap();
    let read = store::auth::ensure_permission(&engine.store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, read)
        .await
        .unwrap();
    store::auth::set_user_role(&engine.store.pool, &ids[2], role)
        .await
        .unwrap();
    store::visibility::grant(&engine.store.pool, "actor", Some(&ids[2]), &ids[0])
        .await
        .unwrap();
    let remote = protein::execute_for(&engine.store, &query, Some(&ids[2]))
        .await
        .unwrap();
    assert_eq!(remote.len(), 1);
    assert_eq!(remote[0]["assignees"], json!([]));
    assert_eq!(remote[0]["assertions"], json!([]));
    store::visibility::grant(&engine.store.pool, "actor", Some(&ids[2]), &ids[1])
        .await
        .unwrap();
    let remote = protein::execute_for(&engine.store, &query, Some(&ids[2]))
        .await
        .unwrap();
    assert_eq!(remote[0]["assignees"][0]["head"], "Assignee");
}

#[tokio::test]
async fn quantity_is_the_only_quantity_property_and_orders_without_rounding() {
    let engine = Engine::open_memory().await.unwrap();
    for (head, quantity) in [
        ("First", "9007199254740993.25"),
        ("Second", "9007199254740993.125"),
    ] {
        let uid = engine
            .act(
                Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: head.into(),
                    body: String::new(),
                    quantity: 0.0,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        engine
            .act(
                Action::SetQuantityExact {
                    target: uid,
                    amount: quantity.into(),
                },
                None,
            )
            .await
            .unwrap();
    }
    let query = serde_json::from_value(json!({"source":"record", "where":[{"quantity_gt":"9007199254740993"}], "fields":["head","quantity"], "order":[{"asc":"quantity"}], "limit":null})).unwrap();
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| row["quantity"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["9007199254740993.125", "9007199254740993.25"]
    );
    assert_eq!(rows[0]["head"], "Second");
    let quantities: Vec<_> = protein::record_schema::fields()
        .into_iter()
        .filter(|field| field.title.contains("Quantity"))
        .collect();
    assert_eq!(quantities.len(), 1);
    assert_eq!(quantities[0].key, "quantity");
}
