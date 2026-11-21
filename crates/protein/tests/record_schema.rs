use engine::{Engine, actions::Action};
use nucleus::RecordKind::{self, Person, Plain};
use protein::{
    execute, execute_for,
    record_schema::{fields, selected},
};
use serde_json::{Value, from_value, json};
use store::{auth, visibility::grant as allow, vocabulary::feature_binding};

async fn record(
    engine: &Engine,
    kind: RecordKind,
    slug: Option<&str>,
    head: &str,
    body: &str,
    quantity: f64,
) -> String {
    engine
        .act(
            Action::CreateRecord {
                slug: slug.map(str::to_owned),
                kind,
                head: head.into(),
                body: body.into(),
                quantity,
            },
            None,
        )
        .await
        .expect("create fixture Record")
        .created
        .expect("Record UID")
}

async fn set_quantity(engine: &Engine, target: String, amount: &str) {
    engine
        .act(
            Action::SetQuantityExact {
                target,
                amount: amount.into(),
            },
            None,
        )
        .await
        .expect("set exact fixture quantity");
}

async fn set_viewer_policy(engine: &Engine, expected_revision: i64, read: Value) {
    engine
        .act(
            Action::SetRolePolicy {
                role: "schema-viewer".into(),
                expected_revision,
                policy: json!({ "read": read, "grants": [] }),
            },
            None,
        )
        .await
        .expect("set fixture Role policy");
}

#[tokio::test]
async fn selected_record_properties_keep_exact_quantity_work_and_visibility() {
    let engine = Engine::open_memory().await.unwrap();
    let store = &engine.store;
    let uid = record(
        &engine,
        Plain,
        Some("schema-task"),
        "Task",
        "Description",
        0.0,
    )
    .await;
    let exact = "9007199254740993.125";
    set_quantity(&engine, uid.clone(), exact).await;
    engine
        .act(
            Action::SetExtension {
                target: uid.clone(),
                namespace: "work".into(),
                fds: json!({
                    "start": "2026-09-01", "due": "2026-09-30", "estimate_min": 90,
                    "logs": [{"start": "2026-09-01T10:00:00Z", "end": "2026-09-01T10:15:00Z"}],
                }),
            },
            None,
        )
        .await
        .unwrap();
    let mut query = from_value(json!({
        "source": "record", "where": [{"uid_eq": uid}],
        "fields": ["head", "body", "slug", "quantity", "assertions", "assignees",
                   "start_date", "due_date", "estimate_min", "spent_seconds", "running_since", "work_logs"],
    })).unwrap();
    let rows = execute(store, &query).await.unwrap();
    let [row] = rows.as_slice() else {
        panic!("expected one selected Record")
    };
    assert_eq!(row["quantity"], exact);
    assert_eq!(row["spent_seconds"], 900);
    assert_eq!(row["estimate_min"], 90);
    assert_eq!(row["body"], "Description");
    assert_eq!(row["start_date"], "2026-09-01");
    assert_eq!(row["due_date"], "2026-09-30");
    assert!(row.get("extension").is_none());
    assert_eq!(
        row.as_object()
            .unwrap()
            .keys()
            .filter(|key| key.starts_with("quantity"))
            .count(),
        1
    );
    assert_eq!(row["work_logs"].as_array().unwrap().len(), 1);

    let person = record(&engine, Person, None, "Visitor", "", 1.0).await;
    assert!(
        execute_for(store, &query, Some(&person))
            .await
            .unwrap()
            .is_empty()
    );
    query.fields = Some(vec!["head".into()]);
    let rows = execute(store, &query).await.unwrap();
    assert_eq!(rows[0].as_object().unwrap().len(), 3);
    assert!(fields().iter().any(|field| field.key == "work_logs"));
    assert_eq!(selected(&query).len(), 2);
}

#[tokio::test]
async fn assignees_and_assertions_do_not_reveal_an_unreadable_person() {
    let engine = Engine::open_memory().await.unwrap();
    let store = &engine.store;
    let pool = &store.pool;
    let task = record(&engine, Plain, None, "Task", "", 1.0).await;
    let assignee = record(&engine, Person, None, "Assignee", "", 1.0).await;
    let viewer = record(&engine, Person, None, "Viewer", "", 1.0).await;
    let predicate = feature_binding(pool, "assignment")
        .await
        .unwrap()
        .concept_uids
        .into_iter()
        .next()
        .expect("assignment Concept");
    engine
        .act(
            Action::AssertRecord {
                subject: task.clone(),
                predicate,
                object: Some(assignee.clone()),
                quantity: None,
                unit: None,
            },
            None,
        )
        .await
        .unwrap();
    let query = from_value(
        json!({"source": "record", "where": [{"uid_eq": task}], "fields": ["assignees", "assertions"]}),
    ).unwrap();
    let local = execute(store, &query).await.unwrap();
    assert_eq!(local[0]["assignees"][0]["head"], "Assignee");
    assert_eq!(local[0]["assertions"][0]["predicate"], "assigned-to");

    let role = auth::ensure_role(pool, "schema-viewer").await.unwrap();
    let read = auth::ensure_permission(pool, "record", "read")
        .await
        .unwrap();
    auth::grant(pool, role, read).await.unwrap();
    auth::set_user_role(pool, &viewer, role).await.unwrap();
    set_viewer_policy(&engine, 0, json!({"uid_eq": task})).await;
    allow(pool, "actor", Some(&viewer), &task).await.unwrap();
    let scoped = || execute_for(store, &query, Some(&viewer));
    let remote = scoped().await.unwrap();
    let [row] = remote.as_slice() else {
        panic!("expected the visible task")
    };
    assert_eq!(row["assignees"], json!([]));
    assert_eq!(row["assertions"], json!([]));

    allow(pool, "actor", Some(&viewer), &assignee)
        .await
        .unwrap();
    set_viewer_policy(
        &engine,
        1,
        json!({"any": [{"uid_eq": task}, {"uid_eq": assignee}]}),
    )
    .await;
    let remote = scoped().await.unwrap();
    assert_eq!(remote[0]["assignees"][0]["head"], "Assignee");
}

#[tokio::test]
async fn quantity_is_the_only_quantity_property_and_orders_without_rounding() {
    let engine = Engine::open_memory().await.unwrap();
    for (head, quantity) in [
        ("First", "9007199254740993.25"),
        ("Second", "9007199254740993.125"),
    ] {
        let uid = record(&engine, Plain, None, head, "", 0.0).await;
        set_quantity(&engine, uid, quantity).await;
    }
    let query = from_value(json!({
        "source": "record", "where": [{"quantity_gt": "9007199254740993"}],
        "fields": ["head", "quantity"], "order": [{"asc": "quantity"}], "limit": null,
    }))
    .unwrap();
    let rows = execute(&engine.store, &query).await.unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| row["quantity"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["9007199254740993.125", "9007199254740993.25"]
    );
    assert_eq!(rows[0]["head"], "Second");
    let quantities: Vec<_> = fields()
        .into_iter()
        .filter(|field| field.title.contains("Quantity"))
        .collect();
    let [quantity] = quantities.as_slice() else {
        panic!("expected one quantity property")
    };
    assert_eq!(quantity.key, "quantity");
}
