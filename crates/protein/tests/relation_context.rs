use nucleus::{
    DecimalValue,
    RecordKind::{self, Person, Plain},
};
use protein::{Protein, Source, execute, execute_for};
use serde_json::{Value, from_value, json};
use store::{
    Store,
    assertions::{
        AssertionRole::{Identity, Ordinary},
        NewAssertion, assert, incident_to, retract,
    },
    auth, concepts,
    exact::zero,
    records::{NewRecord, create},
    visibility::grant as allow,
};

async fn record(store: &Store, head: &str, kind: RecordKind) -> String {
    create(
        &store.pool,
        NewRecord {
            slug: None,
            kind,
            head,
            body: "",
            quantity: zero(),
        },
    )
    .await
    .expect("create relation fixture Record")
    .uid
}

fn query(uid: &str, families: &[&str]) -> Protein {
    from_value(json!({
        "source": "record", "where": [{"uid_eq": uid}],
        "fields": ["quantity", "unit", "relation_context"],
        "include": {"relation_context": {"families": families}},
    }))
    .expect("valid relation fixture query")
}

fn assertions(row: &Value) -> &[Value] {
    row["relation_context"]["assertions"]
        .as_array()
        .expect("projected Relation assertions")
}

#[tokio::test]
async fn incident_context_keeps_exact_values_families_units_and_outside_graph_records() {
    let store = Store::open_memory().await.unwrap();
    let pool = &store.pool;
    let displayed = record(&store, "Displayed", Plain).await;
    let outside = record(&store, "Outside graph", Plain).await;
    let parent = concepts::create(pool, "relation-parent", &[])
        .await
        .unwrap();
    let child = concepts::create(pool, "relation-child", &[&parent])
        .await
        .unwrap();
    let unit = concepts::create(pool, "relation-kg", &[]).await.unwrap();
    let exact = DecimalValue::parse_inferred("9007199254740993.125").unwrap();
    let outgoing = assert(
        pool,
        NewAssertion {
            subject_uid: &displayed,
            predicate_uid: &child,
            object_uid: Some(&outside),
            role: Ordinary,
            quantity: Some(exact),
            unit_uid: Some(&unit),
            asserted_by: None,
        },
    )
    .await
    .unwrap();
    let incoming = assert(
        pool,
        NewAssertion {
            subject_uid: &outside,
            predicate_uid: &child,
            object_uid: Some(&displayed),
            role: Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await
    .unwrap();
    let unary = assert(
        pool,
        NewAssertion {
            subject_uid: &displayed,
            predicate_uid: &child,
            object_uid: None,
            role: Identity,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await
    .unwrap();
    let rows = execute(&store, &query(&displayed, &[" relation-parent "]))
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
    let ids = [displayed.clone(), outside.clone()];
    assert_eq!(incident_to(pool, &ids).await.unwrap().len(), 3);
    let repeated = vec![displayed.clone(); 801];
    assert_eq!(incident_to(pool, &repeated).await.unwrap().len(), 3);
    let mut both = query(&displayed, &[]);
    both.filter.clear();
    let rows = execute(&store, &both).await.unwrap();
    let outside_row = rows.iter().find(|row| row["uid"] == outside).unwrap();
    assert_eq!(assertions(outside_row).len(), 2);
    retract(pool, &outgoing, None).await.unwrap();
    let rows = execute(&store, &query(&displayed, &[])).await.unwrap();
    assert_eq!(assertions(&rows[0]).len(), 2);
    assert!(
        !assertions(&rows[0])
            .iter()
            .any(|row| row["uid"] == outgoing)
    );
}

#[tokio::test]
async fn context_never_reveals_unreadable_incoming_or_outgoing_endpoints() {
    let store = Store::open_memory().await.unwrap();
    let pool = &store.pool;
    let displayed = record(&store, "Displayed", Plain).await;
    let hidden = record(&store, "Hidden", Plain).await;
    let viewer = record(&store, "Viewer", Person).await;
    let other_viewer = record(&store, "Other viewer", Person).await;
    allow(pool, "actor", Some(&other_viewer), &hidden)
        .await
        .unwrap();
    let predicate = concepts::create(pool, "relation-visible", &[])
        .await
        .unwrap();
    for (subject, object) in [
        (&displayed, Some(hidden.as_str())),
        (&hidden, Some(displayed.as_str())),
        (&displayed, None),
    ] {
        assert(
            pool,
            NewAssertion {
                subject_uid: subject,
                predicate_uid: &predicate,
                object_uid: object,
                role: Ordinary,
                quantity: None,
                unit_uid: None,
                asserted_by: None,
            },
        )
        .await
        .unwrap();
    }
    let role = auth::ensure_role(pool, "relation-viewer").await.unwrap();
    let read = auth::ensure_permission(pool, "record", "read")
        .await
        .unwrap();
    auth::grant(pool, role, read).await.unwrap();
    auth::set_user_role(pool, &viewer, role).await.unwrap();
    allow(pool, "actor", Some(&viewer), &displayed)
        .await
        .unwrap();
    let q = query(&displayed, &[]);
    assert!(
        execute_for(&store, &query(&hidden, &[]), Some(&viewer))
            .await
            .unwrap()
            .is_empty()
    );
    let rows = execute_for(&store, &q, Some(&viewer)).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(assertions(&rows[0]).len(), 1);
    assert!(assertions(&rows[0])[0]["to"].is_null());
    assert!(!rows[0].to_string().contains(&hidden));
    allow(pool, "actor", Some(&viewer), &hidden).await.unwrap();
    let rows = execute_for(&store, &q, Some(&viewer)).await.unwrap();
    assert_eq!(assertions(&rows[0]).len(), 3);
}

#[tokio::test]
async fn context_include_validates_bounds_source_and_projection() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, "Displayed", Plain).await;
    let mut q = query(&uid, &["'; DROP TABLE record_assertion; --"]);
    assert!(execute(&store, &q).await.is_ok());
    q.include.relation_context.as_mut().unwrap().families = vec!["x".repeat(129)];
    assert!(execute(&store, &q).await.is_err());
    q.include.relation_context.as_mut().unwrap().families = vec!["x".into(); 129];
    assert!(execute(&store, &q).await.is_err());
    q.include
        .relation_context
        .as_mut()
        .unwrap()
        .families
        .clear();
    q.source = Source::Concept;
    assert!(execute(&store, &q).await.is_err());
    q.source = Source::Record;
    q.fields = Some(vec!["head".into()]);
    let rows = execute(&store, &q).await.unwrap();
    assert!(rows[0].get("relation_context").is_none());
}
