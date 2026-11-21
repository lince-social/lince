use super::fixtures::{capture, concept, context, engine, record, rows_of, timeline, window};
use chrono::{Duration, Months, Utc};
use engine::actions::Action;
use protein::{Include, Predicate, Protein, Source, execute};

#[tokio::test]
async fn a_timeline_needs_to_be_a_timeline_of_something() {
    let e = engine().await;
    let (since, before) = window();
    let refused = execute(
        &e.store,
        &Protein {
            source: Source::Timeline,
            filter: vec![Predicate::AtSince(since), Predicate::AtBefore(before)],
            fields: None,
            include: Include::default(),
            aggregate: None,
            order: vec![],
            limit: None,
        },
    )
    .await;
    assert!(refused.is_err(), "a timeline without a concept is refused");
}

#[tokio::test]
async fn the_past_is_bucketed_and_carries_a_running_position() {
    let e = engine().await;
    let rent = concept(&e, "rent", &[]).await;
    let checking = record(&e, "checking").await;
    let now = Utc::now();
    let (since, before) = window();

    for offset in [3_u32, 2, 1] {
        capture(
            &e,
            &checking,
            "-1200",
            &rent,
            &(now - Months::new(offset)).to_rfc3339(),
        )
        .await;
    }

    let rows = execute(&e.store, &timeline(&rent, since, before))
        .await
        .unwrap();
    let points = rows_of(&rows, "timeline_point");
    assert!(points.len() >= 3, "each month is its own bucket");

    let last = points.last().unwrap();
    assert_eq!(last["cumulative"], "-3600");
    assert_eq!(context(&rows)["current"], "-3600");
}

#[tokio::test]
async fn the_line_starts_where_the_concept_already_stood() {
    let e = engine().await;
    let rent = concept(&e, "rent", &[]).await;
    let checking = record(&e, "checking").await;
    let now = Utc::now();

    capture(
        &e,
        &checking,
        "-5000",
        &rent,
        &(now - Duration::days(400)).to_rfc3339(),
    )
    .await;
    capture(
        &e,
        &checking,
        "-1200",
        &rent,
        &(now - Duration::days(10)).to_rfc3339(),
    )
    .await;

    let (since, before) = window();
    let rows = execute(&e.store, &timeline(&rent, since, before))
        .await
        .unwrap();
    assert_eq!(
        context(&rows)["opening"],
        "-5000",
        "what happened before the window is the line's starting height"
    );
    let points = rows_of(&rows, "timeline_point");
    assert_eq!(points.last().unwrap()["cumulative"], "-6200");
    assert_eq!(context(&rows)["current"], "-6200");
}

#[tokio::test]
async fn an_unclassified_change_is_not_quietly_adopted() {
    let e = engine().await;
    let rent = concept(&e, "rent", &[]).await;
    let checking = record(&e, "checking").await;
    let now = Utc::now();

    e.act(
        Action::CaptureEntry {
            target: checking.clone(),
            amount: "-999".to_string(),
            concept: None,
            note: None,
            at: Some((now - Duration::days(3)).to_rfc3339()),
            request_id: None,
        },
        None,
    )
    .await
    .unwrap();

    let (since, before) = window();
    let rows = execute(&e.store, &timeline(&rent, since, before))
        .await
        .unwrap();
    assert_eq!(context(&rows)["current"], "0");
}

#[tokio::test]
async fn an_unknown_concept_returns_nothing_rather_than_everything() {
    let e = engine().await;
    let (since, before) = window();
    let rows = execute(&e.store, &timeline("no-such-concept", since, before))
        .await
        .unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn exactness_survives_the_whole_line() {
    let e = engine().await;
    let rent = concept(&e, "rent", &[]).await;
    let checking = record(&e, "checking").await;
    let now = Utc::now();

    for amount in ["-10.10", "-20.20", "-0.01"] {
        capture(
            &e,
            &checking,
            amount,
            &rent,
            &(now - Duration::days(4)).to_rfc3339(),
        )
        .await;
    }

    let (since, before) = window();
    let rows = execute(&e.store, &timeline(&rent, since, before))
        .await
        .unwrap();
    assert_eq!(context(&rows)["current"], "-30.31");
    let points = rows_of(&rows, "timeline_point");
    assert_eq!(points.last().unwrap()["cumulative"], "-30.31");
    assert!(points.iter().all(|p| p["cumulative"].is_string()));
}
