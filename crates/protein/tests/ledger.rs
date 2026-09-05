use engine::Engine;
use engine::actions::Action;
use nucleus::RecordKind;
use protein::{Aggregate, AggregateOp, GroupBy, Include, Predicate, Protein, Source};
use store::records::NewRecord;

async fn engine() -> Engine {
    let engine = Engine::open_memory().await.expect("engine opens");
    store::organs::ensure_local(&engine.store.pool, "http://protein-ledger.test")
        .await
        .expect("local Organ");
    engine
}

async fn record(e: &Engine, slug: &str, concept: &str, unit: Option<&str>) -> String {
    let uid = store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head: slug,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid;
    let concept_uid = store::concepts::resolve(&e.store.pool, concept)
        .await
        .unwrap()
        .expect("concept exists");
    store::assertions::set_identity(&e.store.pool, &uid, Some(&concept_uid), None)
        .await
        .unwrap();
    if let Some(unit) = unit {
        let unit_uid = store::concepts::resolve(&e.store.pool, unit)
            .await
            .unwrap()
            .expect("unit concept exists");
        store::records::set_unit(&e.store.pool, &uid, Some(&unit_uid))
            .await
            .unwrap();
    }
    uid
}

async fn capture(e: &Engine, target: &str, amount: &str, concept: &str, at: &str) {
    e.act(
        Action::CaptureEntry {
            target: target.to_string(),
            amount: amount.to_string(),
            concept: Some(concept.to_string()),
            note: None,
            at: Some(at.to_string()),
            request_id: None,
        },
        None,
    )
    .await
    .expect("capture");
}

fn march(by: GroupBy) -> Protein {
    Protein {
        source: Source::Fact,
        filter: vec![
            Predicate::AtSince("2026-03-01T00:00:00Z".into()),
            Predicate::AtBefore("2026-04-01T00:00:00Z".into()),
        ],
        fields: None,
        include: Include::default(),
        aggregate: Some(Aggregate {
            op: AggregateOp::Sum,
            by,
        }),
        order: vec![],
        limit: None,
    }
}

fn bucket<'a>(rows: &'a [serde_json::Value], group: &str) -> &'a serde_json::Value {
    rows.iter()
        .find(|r| r["group"] == group)
        .unwrap_or_else(|| panic!("no `{group}` bucket in {rows:#?}"))
}

async fn spending_month(e: &Engine) {
    let cost = store::concepts::create(&e.store.pool, "cost", &[])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "food", &[&cost])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "rent", &[&cost])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "balance", &[])
        .await
        .unwrap();
    record(e, "checking", "balance", None).await;

    capture(e, "checking", "-1000", "rent", "2026-03-01T09:00:00Z").await;
    capture(e, "checking", "-10.50", "food", "2026-03-05T12:00:00Z").await;
    capture(e, "checking", "-20.25", "food", "2026-03-06T12:00:00Z").await;
    capture(e, "checking", "10.50", "food", "2026-03-07T12:00:00Z").await;
    capture(e, "checking", "-999", "food", "2026-04-02T12:00:00Z").await;
}

#[tokio::test]
async fn totals_are_exact_and_come_back_as_text_not_floats() {
    let e = engine().await;
    spending_month(&e).await;

    let rows = protein::execute(&e.store, &march(GroupBy::Total))
        .await
        .unwrap();
    let total = bucket(&rows, "(total)");
    assert_eq!(total["net"], "-1020.25");
    assert!(total["net"].is_string());
    assert_eq!(total["gains"], "10.50");
    assert_eq!(total["losses"], "-1030.75");
    assert_eq!(total["count"], 4);
    assert!(total["unit_uid"].is_null());
}

#[tokio::test]
async fn grouping_by_classification_says_what_the_changes_were_for() {
    let e = engine().await;
    spending_month(&e).await;
    let food = store::concepts::resolve(&e.store.pool, "food")
        .await
        .unwrap()
        .unwrap();

    let rows = protein::execute(&e.store, &march(GroupBy::Classification))
        .await
        .unwrap();
    let food_row = bucket(&rows, &food);
    assert_eq!(food_row["net"], "-20.25");
    assert_eq!(food_row["gains"], "10.50");
    assert_eq!(food_row["count"], 3);
}

#[tokio::test]
async fn the_record_axis_and_the_change_axis_are_different_questions() {
    let e = engine().await;
    spending_month(&e).await;
    let balance = store::concepts::resolve(&e.store.pool, "balance")
        .await
        .unwrap()
        .unwrap();

    let rows = protein::execute(&e.store, &march(GroupBy::Concept))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(bucket(&rows, &balance)["count"], 4);

    let rows = protein::execute(&e.store, &march(GroupBy::Classification))
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn classified_in_walks_down_the_lingua_dag() {
    let e = engine().await;
    spending_month(&e).await;

    let mut q = march(GroupBy::Total);
    q.filter.push(Predicate::ClassifiedIn("cost".into()));
    let rows = protein::execute(&e.store, &q).await.unwrap();
    assert_eq!(bucket(&rows, "(total)")["net"], "-1020.25");

    let mut q = march(GroupBy::Total);
    q.filter.push(Predicate::ClassifiedIn("food".into()));
    let rows = protein::execute(&e.store, &q).await.unwrap();
    assert_eq!(bucket(&rows, "(total)")["net"], "-20.25");
}

#[tokio::test]
async fn concept_in_reads_what_a_record_counts_as_not_only_what_it_is() {
    let e = engine().await;
    spending_month(&e).await;
    store::concepts::create(&e.store.pool, "savings-pot", &[])
        .await
        .unwrap();
    record(&e, "rainy-day", "savings-pot", None).await;
    e.act(
        Action::AssertRecord {
            subject: "rainy-day".into(),
            predicate: "balance".into(),
            object: None,
            quantity: None,
            unit: None,
        },
        None,
    )
    .await
    .unwrap();
    capture(&e, "rainy-day", "100", "rent", "2026-03-10T10:00:00Z").await;

    let mut q = march(GroupBy::Total);
    q.filter.push(Predicate::ConceptIn("balance".into()));
    let rows = protein::execute(&e.store, &q).await.unwrap();
    assert_eq!(bucket(&rows, "(total)")["count"], 5);
}

#[tokio::test]
async fn unclassified_changes_get_a_named_bucket_and_are_never_dropped() {
    let e = engine().await;
    store::concepts::create(&e.store.pool, "balance", &[])
        .await
        .unwrap();
    record(&e, "checking", "balance", None).await;
    e.act(
        Action::CaptureEntry {
            target: "checking".into(),
            amount: "-7.25".into(),
            concept: None,
            note: None,
            at: Some("2026-03-04T10:00:00Z".into()),
            request_id: None,
        },
        None,
    )
    .await
    .unwrap();

    let rows = protein::execute(&e.store, &march(GroupBy::Classification))
        .await
        .unwrap();
    let unclassified = bucket(&rows, "(unclassified)");
    assert_eq!(unclassified["net"], "-7.25");
    assert_eq!(unclassified["count"], 1);
}

#[tokio::test]
async fn the_same_query_answers_a_question_that_has_nothing_to_do_with_a_balance() {
    let e = engine().await;
    let reason = store::concepts::create(&e.store.pool, "consumption", &[])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "baking", &[&reason])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "spoilage", &[&reason])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "stock", &[])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "kg", &[])
        .await
        .unwrap();
    record(&e, "flour", "stock", Some("kg")).await;

    capture(&e, "flour", "-2.5", "baking", "2026-03-02T10:00:00Z").await;
    capture(&e, "flour", "-1.25", "baking", "2026-03-04T10:00:00Z").await;
    capture(&e, "flour", "-0.75", "spoilage", "2026-03-09T10:00:00Z").await;

    let mut q = march(GroupBy::Total);
    q.filter.push(Predicate::ClassifiedIn("consumption".into()));
    let rows = protein::execute(&e.store, &q).await.unwrap();
    assert_eq!(bucket(&rows, "(total)")["net"], "-4.50");

    let baking = store::concepts::resolve(&e.store.pool, "baking")
        .await
        .unwrap()
        .unwrap();
    let rows = protein::execute(&e.store, &march(GroupBy::Classification))
        .await
        .unwrap();
    assert_eq!(bucket(&rows, &baking)["net"], "-3.75");
}

#[tokio::test]
async fn records_of_different_units_are_separated_rather_than_added() {
    let e = engine().await;
    store::concepts::create(&e.store.pool, "stock", &[])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "used", &[])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "kg", &[])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "litre", &[])
        .await
        .unwrap();
    record(&e, "flour", "stock", Some("kg")).await;
    record(&e, "milk", "stock", Some("litre")).await;

    capture(&e, "flour", "-2.5", "used", "2026-03-02T10:00:00Z").await;
    capture(&e, "milk", "-1.5", "used", "2026-03-03T10:00:00Z").await;

    let rows = protein::execute(&e.store, &march(GroupBy::Total))
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r["group"] == "(total)"));
    assert!(rows.iter().any(|r| r["net"] == "-2.5"));
    assert!(rows.iter().any(|r| r["net"] == "-1.5"));
    assert!(rows.iter().all(|r| !r["unit_uid"].is_null()));
}

#[tokio::test]
async fn adjacent_windows_tile_without_double_counting() {
    let e = engine().await;
    spending_month(&e).await;

    let count_between = |from: &'static str, to: &'static str| {
        let store = &e.store;
        async move {
            let mut q = march(GroupBy::Total);
            q.filter = vec![
                Predicate::AtSince(from.into()),
                Predicate::AtBefore(to.into()),
            ];
            protein::execute(store, &q)
                .await
                .unwrap()
                .iter()
                .map(|r| r["count"].as_i64().unwrap())
                .sum::<i64>()
        }
    };

    let whole = count_between("2026-03-01T00:00:00Z", "2026-04-01T00:00:00Z").await;
    let first = count_between("2026-03-01T00:00:00Z", "2026-03-06T00:00:00Z").await;
    let second = count_between("2026-03-06T00:00:00Z", "2026-04-01T00:00:00Z").await;
    assert_eq!(whole, 4);
    assert_eq!(first + second, whole);
}

#[tokio::test]
async fn a_count_aggregate_does_not_invent_a_net() {
    let e = engine().await;
    spending_month(&e).await;

    let mut q = march(GroupBy::Classification);
    q.aggregate = Some(Aggregate {
        op: AggregateOp::Count,
        by: GroupBy::Classification,
    });
    let rows = protein::execute(&e.store, &q).await.unwrap();
    for row in &rows {
        assert!(row["count"].is_i64());
        assert!(row["net"].is_null());
        assert!(row["gains"].is_null());
    }
}

#[tokio::test]
async fn re_tagging_a_change_moves_the_total_without_touching_the_fact() {
    let e = engine().await;
    spending_month(&e).await;
    let food = store::concepts::resolve(&e.store.pool, "food")
        .await
        .unwrap()
        .unwrap();
    let rent = store::concepts::resolve(&e.store.pool, "rent")
        .await
        .unwrap()
        .unwrap();

    let checking = store::records::resolve(&e.store.pool, "checking")
        .await
        .unwrap()
        .unwrap()
        .uid;
    let facts = store::facts::for_record(&e.store.pool, &checking, 100)
        .await
        .unwrap();
    let mistagged = facts
        .iter()
        .find(|f| f.delta.to_string() == "-1000")
        .expect("the rent change");
    let before = mistagged.hash.clone();

    e.act(
        Action::ClassifyFact {
            fact: mistagged.uid.clone(),
            concept: Some("food".into()),
            note: Some("was not rent".into()),
        },
        None,
    )
    .await
    .unwrap();

    let after = store::facts::get(&e.store.pool, &mistagged.uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.hash, before);
    assert_eq!(after.delta.to_string(), "-1000");

    let rows = protein::execute(&e.store, &march(GroupBy::Classification))
        .await
        .unwrap();
    assert_eq!(bucket(&rows, &food)["net"], "-1020.25");
    assert!(rows.iter().all(|r| r["group"] != rent.as_str()));

    let history = store::ledger::classification_history(&e.store.pool, &mistagged.uid)
        .await
        .unwrap();
    assert_eq!(history.len(), 2);
}

#[tokio::test]
async fn totals_never_leave_the_cell_for_a_remote_subject() {
    let e = engine().await;
    spending_month(&e).await;

    let rows = protein::execute_for(&e.store, &march(GroupBy::Total), Some("org_somebody"))
        .await
        .unwrap();
    assert!(rows.is_empty());
}
