use super::fixtures::{capture, concept, engine, find, query, record};
use engine::actions::Action;
use protein::{Predicate, Source, execute};

#[tokio::test]
async fn an_entry_carries_the_handle_a_correction_has_to_quote() {
    let e = engine().await;
    let food = concept(&e, "food").await;
    let checking = record(&e, "checking").await;
    let entry = capture(&e, &checking, "-10.50", Some(&food)).await;

    let rows = execute(&e.store, &query(Source::Entry, vec![]))
        .await
        .unwrap();
    let row = find(&rows, &entry);
    assert_eq!(row["revision"], 1);
    assert_eq!(row["amount"], "-10.50", "exact text, never a float");
    assert_eq!(row["concept"], food);
    assert_eq!(row["void"], false);
    assert!(row["fact"].is_string(), "re-tagging needs the Fact");
}

#[tokio::test]
async fn a_corrected_entry_reports_its_new_revision_and_keeps_its_category() {
    let e = engine().await;
    let food = concept(&e, "food").await;
    let checking = record(&e, "checking").await;
    let entry = capture(&e, &checking, "-10", Some(&food)).await;

    e.act(
        Action::ReviseEntry {
            entry: entry.clone(),
            expected_revision: 1,
            request_id: "fix-1".to_string(),
            amount: "-15".to_string(),
            note: Some("actually 15".to_string()),
            at: None,
        },
        None,
    )
    .await
    .unwrap();

    let rows = execute(&e.store, &query(Source::Entry, vec![]))
        .await
        .unwrap();
    let row = find(&rows, &entry);
    assert_eq!(row["revision"], 2);
    assert_eq!(row["amount"], "-15");
    assert_eq!(row["concept"], food, "a correction keeps its category");
}

#[tokio::test]
async fn a_voided_entry_is_still_listed() {
    let e = engine().await;
    let checking = record(&e, "checking").await;
    let entry = capture(&e, &checking, "-10", None).await;
    e.act(
        Action::VoidEntry {
            entry: entry.clone(),
            expected_revision: 1,
            request_id: "void-1".to_string(),
        },
        None,
    )
    .await
    .unwrap();

    let rows = execute(&e.store, &query(Source::Entry, vec![]))
        .await
        .unwrap();
    let row = find(&rows, &entry);
    assert_eq!(row["void"], true);
    assert_eq!(row["state"], "void");
}

#[tokio::test]
async fn filtering_by_category_searches_past_the_page_limit() {
    let e = engine().await;
    let rare = concept(&e, "rare").await;
    let common = concept(&e, "common").await;
    let checking = record(&e, "checking").await;

    let needle = capture(&e, &checking, "-1", Some(&rare)).await;
    for _ in 0..25 {
        capture(&e, &checking, "-2", Some(&common)).await;
    }

    let mut protein = query(
        Source::Entry,
        vec![Predicate::ClassifiedIn("rare".to_string())],
    );
    protein.limit = Some(5);
    let rows = execute(&e.store, &protein).await.unwrap();
    assert_eq!(rows.len(), 1, "the rare change must still be found");
    assert_eq!(rows[0]["uid"], needle);
}

#[tokio::test]
async fn an_unclassified_change_is_excluded_by_a_category_filter() {
    let e = engine().await;
    let food = concept(&e, "food").await;
    let checking = record(&e, "checking").await;
    capture(&e, &checking, "-10", None).await;
    let classified = capture(&e, &checking, "-20", Some(&food)).await;

    let rows = execute(
        &e.store,
        &query(
            Source::Entry,
            vec![Predicate::ClassifiedIn("food".to_string())],
        ),
    )
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["uid"], classified);
}
