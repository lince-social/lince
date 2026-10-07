use engine::{Engine, actions::Action};
use serde_json::{Value, json};

async fn record(engine: &Engine, head: &str, kind: nucleus::RecordKind) -> String {
    engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind,
                head: head.into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

fn query(uid: &str, families: Vec<&str>) -> protein::Protein {
    serde_json::from_value(json!({"source":"record", "where":[{"uid_eq":uid}], "fields":["quantity","unit","relation_context"], "include":{"relation_context":{"families":families}}})).unwrap()
}

fn assertions(row: &Value) -> &[Value] {
    row["relation_context"]["assertions"].as_array().unwrap()
}

#[tokio::test]
async fn incident_context_keeps_exact_values_families_units_and_outside_graph_records() {
    let engine = Engine::open_memory().await.unwrap();
    let displayed = record(&engine, "Displayed", nucleus::RecordKind::Plain).await;
    let outside = record(&engine, "Outside graph", nucleus::RecordKind::Plain).await;
    let parent = store::concepts::create(&engine.store.pool, "relation-parent", &[])
        .await
        .unwrap();
    let child = store::concepts::create(&engine.store.pool, "relation-child", &[&parent])
        .await
        .unwrap();
    let unit = store::concepts::create(&engine.store.pool, "relation-kg", &[])
        .await
        .unwrap();
    let exact = nucleus::DecimalValue::parse_inferred("9007199254740993.125").unwrap();
    let outgoing = store::assertions::assert(
        &engine.store.pool,
        store::assertions::NewAssertion {
            subject_uid: &displayed,
            predicate_uid: &child,
            object_uid: Some(&outside),
            role: store::assertions::AssertionRole::Ordinary,
            quantity: Some(exact),
            unit_uid: Some(&unit),
            asserted_by: None,
        },
    )
    .await
    .unwrap();
    let incoming = store::assertions::assert(
        &engine.store.pool,
        store::assertions::NewAssertion {
            subject_uid: &outside,
            predicate_uid: &child,
            object_uid: Some(&displayed),
            role: store::assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await
    .unwrap();
    let unary = store::assertions::assert(
        &engine.store.pool,
        store::assertions::NewAssertion {
            subject_uid: &displayed,
            predicate_uid: &child,
            object_uid: None,
            role: store::assertions::AssertionRole::Identity,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await
    .unwrap();
    let rows = protein::execute(&engine.store, &query(&displayed, vec![" relation-parent "]))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let values = assertions(&rows[0]);
    assert_eq!(values.len(), 3);
    let value = values.iter().find(|row| row["uid"] == outgoing).unwrap();
    assert_eq!(value["quantity"], "9007199254740993.125");
    assert_eq!(value["unit"], unit);
    assert_eq!(value["unit_name"], "relation-kg");
    assert_eq!(value["families"], json!(["relation-parent"]));
    assert_eq!(value["to"], outside);
    assert!(
        values
            .iter()
            .any(|row| row["uid"] == incoming && row["from"] == outside)
    );
    assert!(
        values
            .iter()
            .any(|row| row["uid"] == unary && row["to"].is_null() && row["quantity"].is_null())
    );
    let ids = vec![displayed.clone(), outside.clone()];
    assert_eq!(
        store::assertions::incident_to(&engine.store.pool, &ids)
            .await
            .unwrap()
            .len(),
        3
    );
    let repeated = vec![displayed.clone(); 801];
    assert_eq!(
        store::assertions::incident_to(&engine.store.pool, &repeated)
            .await
            .unwrap()
            .len(),
        3
    );
    let mut both = query(&displayed, vec![]);
    both.filter.clear();
    let rows = protein::execute(&engine.store, &both).await.unwrap();
    let outside_row = rows.iter().find(|row| row["uid"] == outside).unwrap();
    assert_eq!(assertions(outside_row).len(), 2);
    store::assertions::retract(&engine.store.pool, &outgoing, None)
        .await
        .unwrap();
    let rows = protein::execute(&engine.store, &query(&displayed, vec![]))
        .await
        .unwrap();
    assert_eq!(assertions(&rows[0]).len(), 2);
    assert!(
        !assertions(&rows[0])
            .iter()
            .any(|row| row["uid"] == outgoing)
    );
}

#[tokio::test]
async fn context_never_reveals_unreadable_incoming_or_outgoing_endpoints() {
    let engine = Engine::open_memory().await.unwrap();
    let displayed = record(&engine, "Displayed", nucleus::RecordKind::Plain).await;
    let hidden = record(&engine, "Hidden", nucleus::RecordKind::Plain).await;
    let viewer = record(&engine, "Viewer", nucleus::RecordKind::Person).await;
    let other_viewer = record(&engine, "Other viewer", nucleus::RecordKind::Person).await;
    store::visibility::grant(&engine.store.pool, "actor", Some(&other_viewer), &hidden)
        .await
        .unwrap();
    let predicate = store::concepts::create(&engine.store.pool, "relation-visible", &[])
        .await
        .unwrap();
    for (subject, object) in [
        (&displayed, Some(hidden.as_str())),
        (&hidden, Some(displayed.as_str())),
        (&displayed, None),
    ] {
        store::assertions::assert(
            &engine.store.pool,
            store::assertions::NewAssertion {
                subject_uid: subject,
                predicate_uid: &predicate,
                object_uid: object,
                role: store::assertions::AssertionRole::Ordinary,
                quantity: None,
                unit_uid: None,
                asserted_by: None,
            },
        )
        .await
        .unwrap();
    }
    let role = store::auth::ensure_role(&engine.store.pool, "relation-viewer")
        .await
        .unwrap();
    let read = store::auth::ensure_permission(&engine.store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, read)
        .await
        .unwrap();
    store::auth::set_user_role(&engine.store.pool, &viewer, role)
        .await
        .unwrap();
    store::visibility::grant(&engine.store.pool, "actor", Some(&viewer), &displayed)
        .await
        .unwrap();
    let q = query(&displayed, vec![]);
    assert!(
        protein::execute_for(&engine.store, &query(&hidden, vec![]), Some(&viewer))
            .await
            .unwrap()
            .is_empty()
    );
    let rows = protein::execute_for(&engine.store, &q, Some(&viewer))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(assertions(&rows[0]).len(), 1);
    assert!(assertions(&rows[0])[0]["to"].is_null());
    assert!(!rows[0].to_string().contains(&hidden));
    store::visibility::grant(&engine.store.pool, "actor", Some(&viewer), &hidden)
        .await
        .unwrap();
    let rows = protein::execute_for(&engine.store, &q, Some(&viewer))
        .await
        .unwrap();
    assert_eq!(assertions(&rows[0]).len(), 3);
}

#[tokio::test]
async fn context_include_validates_bounds_source_and_projection() {
    let engine = Engine::open_memory().await.unwrap();
    let uid = record(&engine, "Displayed", nucleus::RecordKind::Plain).await;
    let mut q = query(&uid, vec!["'; DROP TABLE record_assertion; --"]);
    assert!(protein::execute(&engine.store, &q).await.is_ok());
    q.include.relation_context.as_mut().unwrap().families = vec!["x".repeat(129)];
    assert!(protein::execute(&engine.store, &q).await.is_err());
    q.include.relation_context.as_mut().unwrap().families = vec!["x".into(); 129];
    assert!(protein::execute(&engine.store, &q).await.is_err());
    q.include
        .relation_context
        .as_mut()
        .unwrap()
        .families
        .clear();
    q.source = protein::Source::Concept;
    assert!(protein::execute(&engine.store, &q).await.is_err());
    q.source = protein::Source::Record;
    q.fields = Some(vec!["head".into()]);
    let rows = protein::execute(&engine.store, &q).await.unwrap();
    assert!(rows[0].get("relation_context").is_none());
}
