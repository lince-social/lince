use super::fixtures::{concept, engine, query, record};
use super::recurrence_fixture::create_recurrence;
use chrono::{Duration, Utc};
use engine::actions::Action;
use nucleus::karma::Cadence;
use protein::{Source, execute};

#[tokio::test]
async fn a_rule_and_its_dates_arrive_as_one_query() {
    let e = engine().await;
    let rent = concept(&e, "rent").await;
    let checking = record(&e, "checking").await;
    let rule = create_recurrence(
        &e,
        checking,
        Some(&rent),
        "-1200",
        Cadence::every_days(7),
        (Utc::now() - Duration::days(1)).to_rfc3339(),
        Some("rent"),
    )
    .await;

    let rows = execute(&e.store, &query(Source::Recurrence, vec![]))
        .await
        .unwrap();
    let mut rules = rows.iter().filter(|row| row["kind"] == "recurrence");
    let listed = rules.next().expect("the created rule is listed");
    assert!(rules.next().is_none(), "only the created rule is listed");
    assert_eq!(listed["uid"], rule);
    assert_eq!(listed["revision"], 1, "a pause has to quote this");
    assert_eq!(listed["paused"], false);
    assert_eq!(listed["amount"], "-1200");
    assert_eq!(listed["cadence"]["every"]["days"], 7);
    assert_eq!(listed["cadence"]["every"]["months"], 0);
    assert_eq!(listed["cadence"]["invalid_day"], "clamp");
    assert!(
        listed["truncated"] == false,
        "a weekly rule fits in the window and must not claim otherwise"
    );
    assert!(
        rows.iter().any(|row| row["kind"] == "occurrence"),
        "an active rule produces dates"
    );
}

#[tokio::test]
async fn an_applied_date_leaves_the_pending_inbox() {
    let e = engine().await;
    let rent = concept(&e, "rent").await;
    let checking = record(&e, "checking").await;
    let due = (Utc::now() - Duration::days(1))
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

    let rows = execute(&e.store, &query(Source::Recurrence, vec![]))
        .await
        .unwrap();
    let due = due.to_rfc3339();
    let mut dates = rows
        .iter()
        .filter(|row| row["kind"] == "occurrence" && row["due_at"] == due);
    let applied = dates.next().expect("the applied date is listed");
    assert_eq!(applied["state"], "applied");
    assert!(
        applied["entry"].is_string(),
        "an applied date points at the change it made"
    );
    assert!(
        dates.next().is_none(),
        "the applied date leaves the pending inbox"
    );
}

#[tokio::test]
async fn a_paused_rule_is_listed_with_its_state_so_it_can_be_resumed() {
    let e = engine().await;
    let checking = record(&e, "checking").await;
    let rule = create_recurrence(
        &e,
        checking,
        None,
        "-9",
        Cadence::every_weeks(1),
        (Utc::now() - Duration::days(1)).to_rfc3339(),
        None,
    )
    .await;
    e.act(
        Action::SetRecurrencePaused {
            recurrence: rule.clone(),
            expected_revision: 1,
            request_id: "pause-1".to_string(),
            paused: true,
        },
        None,
    )
    .await
    .unwrap();

    let rows = execute(&e.store, &query(Source::Recurrence, vec![]))
        .await
        .unwrap();
    let listed = rows
        .iter()
        .find(|r| r["kind"] == "recurrence" && r["uid"] == rule)
        .expect("a paused rule is still listed");
    assert_eq!(listed["paused"], true);
    assert_eq!(listed["revision"], 2);
}
