use super::fixtures::{capture, concept, context, engine, record, rows_of, timeline, window};
use super::recurrence_fixture::create_recurrence;
use chrono::{Duration, Utc};
use engine::actions::Action;
use nucleus::karma::Cadence;
use protein::execute;

#[tokio::test]
async fn the_future_is_declared_by_recurring_rules() {
    let e = engine().await;
    let rent = concept(&e, "rent", &[]).await;
    let checking = record(&e, "checking").await;
    let now = Utc::now();

    create_recurrence(
        &e,
        checking,
        Some(&rent),
        "-1200",
        Cadence::every_months(1),
        (now - Duration::days(2)).to_rfc3339(),
        Some("rent"),
    )
    .await;

    let (since, before) = window();
    let rows = execute(&e.store, &timeline(&rent, since, before))
        .await
        .unwrap();

    assert!(
        rows.iter()
            .any(|row| row["kind"] == "timeline_point" && !row["expected_net"].is_null()),
        "an active rule puts declared amounts ahead of now"
    );

    let sources = rows_of(&rows, "timeline_source");
    assert!(!sources.is_empty());
    assert!(sources.iter().all(|s| s["origin"] == "recurrence"));
    assert!(sources.iter().all(|s| s["amount"] == "-1200"));
}

#[tokio::test]
async fn an_applied_date_is_counted_once_as_history_not_twice() {
    let e = engine().await;
    let rent = concept(&e, "rent", &[]).await;
    let checking = record(&e, "checking").await;
    let now = Utc::now();
    let due = (now - Duration::days(1))
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();

    let rule = create_recurrence(
        &e,
        checking,
        Some(&rent),
        "-1200",
        Cadence::every_days(1),
        due.to_rfc3339(),
        None,
    )
    .await;

    e.act(
        Action::ApplyRecurrenceOccurrence {
            recurrence: rule,
            due_at: due.to_rfc3339(),
            amount: None,
            note: None,
        },
        None,
    )
    .await
    .unwrap();

    let (since, before) = window();
    let rows = execute(&e.store, &timeline(&rent, since, before))
        .await
        .unwrap();

    assert_eq!(context(&rows)["current"], "-1200");
    let due = due.to_rfc3339();
    assert!(
        rows_of(&rows, "timeline_source")
            .iter()
            .all(|s| s["at"].as_str().expect("source timestamp") != due),
        "an applied date must not also be listed as still expected"
    );
}

#[tokio::test]
async fn a_skipped_date_is_not_expected() {
    let e = engine().await;
    let rent = concept(&e, "rent", &[]).await;
    let checking = record(&e, "checking").await;
    let now = Utc::now();
    let due = (now + Duration::days(3))
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();

    let rule = create_recurrence(
        &e,
        checking,
        Some(&rent),
        "-40",
        Cadence::every_days(1),
        due.to_rfc3339(),
        None,
    )
    .await;
    e.act(
        Action::SkipRecurrenceOccurrence {
            recurrence: rule,
            due_at: due.to_rfc3339(),
            note: Some("cancelled".to_string()),
        },
        None,
    )
    .await
    .unwrap();

    let (since, before) = window();
    let rows = execute(&e.store, &timeline(&rent, since, before))
        .await
        .unwrap();
    let due = due.to_rfc3339();
    assert!(
        rows_of(&rows, "timeline_source")
            .iter()
            .all(|s| s["at"].as_str().expect("source timestamp") != due),
        "a date that was declined is not something to expect"
    );
}

#[tokio::test]
async fn a_paused_rule_stops_declaring_a_future() {
    let e = engine().await;
    let rent = concept(&e, "rent", &[]).await;
    let checking = record(&e, "checking").await;
    let now = Utc::now();

    let rule = create_recurrence(
        &e,
        checking,
        Some(&rent),
        "-1200",
        Cadence::every_days(7),
        (now - Duration::days(1)).to_rfc3339(),
        None,
    )
    .await;

    let (since, before) = window();
    let live = execute(&e.store, &timeline(&rent, since.clone(), before.clone()))
        .await
        .unwrap();
    assert!(!rows_of(&live, "timeline_source").is_empty());

    e.act(
        Action::SetRecurrencePaused {
            recurrence: rule,
            expected_revision: 1,
            request_id: "pause-1".to_string(),
            paused: true,
        },
        None,
    )
    .await
    .unwrap();

    let paused = execute(&e.store, &timeline(&rent, since, before))
        .await
        .unwrap();
    assert!(
        rows_of(&paused, "timeline_source").is_empty(),
        "a paused rule declares nothing ahead"
    );
}

#[tokio::test]
async fn the_concept_dag_is_respected_on_both_halves_of_the_line() {
    let e = engine().await;
    let cost = concept(&e, "cost", &[]).await;
    let rent = concept(&e, "rent", &[cost.as_str()]).await;
    let checking = record(&e, "checking").await;
    let now = Utc::now();

    capture(
        &e,
        &checking,
        "-1200",
        &rent,
        &(now - Duration::days(5)).to_rfc3339(),
    )
    .await;
    create_recurrence(
        &e,
        checking,
        Some(&rent),
        "-1200",
        Cadence::every_days(7),
        (now - Duration::days(1)).to_rfc3339(),
        None,
    )
    .await;

    let (since, before) = window();
    let rows = execute(&e.store, &timeline(&cost, since, before))
        .await
        .unwrap();
    assert_eq!(context(&rows)["current"], "-1200");
    assert!(
        !rows_of(&rows, "timeline_source").is_empty(),
        "a parent concept inherits its children's declared future"
    );
}
