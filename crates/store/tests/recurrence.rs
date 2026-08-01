//! A recurring declaration has to survive the things that make recurrence hard:
//! a retried write, a rule edited after some dates already ran, and the
//! difference between "declined" and "not looked at yet".

use chrono::{DateTime, Utc};
use nucleus::DecimalValue;
use nucleus::karma::{Cadence, Consequences};
use store::Store;
use store::recurrence::{
    NewRecurrence, Occurrence, ReviseRecurrence, create, occurrences,
    occurrence_request_id, revise, set_state, skip, unskip,
};

async fn store() -> Store {
    let path = std::env::temp_dir()
        .join(format!("lince-recurrence-{}.db", nucleus::new_uid("test")));
    Store::open(&format!("sqlite://{}", path.display()))
        .await
        .unwrap()
}

async fn record(store: &Store) -> String {
    store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::record::RecordKind::Plain,
            head: "Checking",
            body: "",
            quantity: amount("0"),
        },
    )
    .await
    .unwrap()
    .uid
}

fn amount(text: &str) -> DecimalValue {
    DecimalValue::parse_inferred(text).expect("test amount should parse")
}

fn at(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .expect("test timestamp should parse")
        .with_timezone(&Utc)
}

fn states(found: &[Occurrence]) -> Vec<(String, &'static str)> {
    found
        .iter()
        .map(|item| (item.due_at.to_rfc3339(), item.state.as_str()))
        .collect()
}

async fn monthly_rent(store: &Store, record_uid: &str, request: &str) -> String {
    create(
        &store.pool,
        NewRecurrence {
            record_uid,
            consequences: Consequences::capture(amount("-1200"), None),
            condition: None,
            note: Some("rent"),
            cadence: Cadence::every_months(1),
            anchor_at: at("2026-01-01T00:00:00Z"),
            request_id: request,
            actor_uid: None,
        },
        at("2026-01-01T00:00:00Z"),
    )
    .await
    .unwrap()
    .rule()
    .uid
    .clone()
}

#[tokio::test]
async fn a_retried_create_returns_the_first_rule_rather_than_a_second_one() {
    let store = store().await;
    let record_uid = record(&store).await;
    let first = monthly_rent(&store, &record_uid, "req-1").await;

    let again = create(
        &store.pool,
        NewRecurrence {
            record_uid: &record_uid,
            consequences: Consequences::capture(amount("-1200"), None),
            condition: None,
            note: Some("rent"),
            cadence: Cadence::every_months(1),
            anchor_at: at("2026-01-01T00:00:00Z"),
            request_id: "req-1",
            actor_uid: None,
        },
        at("2026-01-02T00:00:00Z"),
    )
    .await
    .unwrap();

    assert!(again.was_replayed(), "a retry must not create a second rule");
    assert_eq!(again.rule().uid, first);
    assert_eq!(store::recurrence::all(&store.pool).await.unwrap().len(), 1);
}

#[tokio::test]
async fn dates_are_derived_and_split_into_past_due_and_still_planned() {
    let store = store().await;
    let record_uid = record(&store).await;
    let uid = monthly_rent(&store, &record_uid, "req-1").await;
    let rule = store::recurrence::get(&store.pool, &uid).await.unwrap().unwrap();

    let found = occurrences(
        &store.pool,
        &rule,
        at("2026-01-01T00:00:00Z"),
        at("2026-06-01T00:00:00Z"),
        at("2026-03-15T00:00:00Z"),
    )
    .await
    .unwrap();

    // Everything on or before "now" that nobody answered is due; everything
    // after is merely planned. A screen that cannot tell these apart cannot
    // show a person what needs attention.
    assert_eq!(
        states(&found.dates)
            .iter()
            .map(|(_, state)| *state)
            .collect::<Vec<_>>(),
        vec!["due", "due", "due", "planned", "planned"]
    );
    assert_eq!(found.len(), 5);
    assert!(found.iter().all(|item| item.amount == Some(amount("-1200"))));
}

#[tokio::test]
async fn a_skip_is_remembered_and_can_be_taken_back() {
    let store = store().await;
    let record_uid = record(&store).await;
    let uid = monthly_rent(&store, &record_uid, "req-1").await;
    let rule = store::recurrence::get(&store.pool, &uid).await.unwrap().unwrap();

    skip(
        &store.pool,
        &uid,
        at("2026-02-01T00:00:00Z"),
        Some("landlord waived it"),
        None,
        at("2026-02-02T00:00:00Z"),
    )
    .await
    .unwrap();
    // Skipping twice is the same decision, not an error.
    skip(
        &store.pool,
        &uid,
        at("2026-02-01T00:00:00Z"),
        None,
        None,
        at("2026-02-03T00:00:00Z"),
    )
    .await
    .unwrap();

    let found = occurrences(
        &store.pool,
        &rule,
        at("2026-01-01T00:00:00Z"),
        at("2026-04-01T00:00:00Z"),
        at("2026-03-15T00:00:00Z"),
    )
    .await
    .unwrap();
    assert_eq!(
        found.iter().map(|i| i.state.as_str()).collect::<Vec<_>>(),
        vec!["due", "skipped", "due"]
    );

    unskip(&store.pool, &uid, at("2026-02-01T00:00:00Z")).await.unwrap();
    let after = occurrences(
        &store.pool,
        &rule,
        at("2026-01-01T00:00:00Z"),
        at("2026-04-01T00:00:00Z"),
        at("2026-03-15T00:00:00Z"),
    )
    .await
    .unwrap();
    assert_eq!(
        after.iter().map(|i| i.state.as_str()).collect::<Vec<_>>(),
        vec!["due", "due", "due"]
    );
}

#[tokio::test]
async fn the_occurrence_key_names_one_rule_and_one_date() {
    // This string is the occurrence's whole identity, and `entry_revision`'s
    // UNIQUE(request_id) is what it buys. Two rules on the same date, or one
    // rule on two dates, must never collide.
    let a = occurrence_request_id("rec_a", at("2026-02-01T00:00:00Z"));
    let b = occurrence_request_id("rec_b", at("2026-02-01T00:00:00Z"));
    let c = occurrence_request_id("rec_a", at("2026-03-01T00:00:00Z"));
    assert_ne!(a, b);
    assert_ne!(a, c);
    assert_eq!(a, occurrence_request_id("rec_a", at("2026-02-01T00:00:00Z")));
}

#[tokio::test]
async fn revising_a_rule_changes_what_is_expected_without_touching_what_ran() {
    let store = store().await;
    let record_uid = record(&store).await;
    let uid = monthly_rent(&store, &record_uid, "req-1").await;

    let revised = revise(
        &store.pool,
        ReviseRecurrence {
            recurrence_uid: &uid,
            expected_revision: 1,
            consequences: Consequences::capture(amount("-1300"), None),
            condition: None,
            note: Some("rent went up"),
            cadence: Cadence::every_months(1),
            anchor_at: at("2026-01-01T00:00:00Z"),
            request_id: "req-2",
            actor_uid: None,
        },
        at("2026-03-01T00:00:00Z"),
    )
    .await
    .unwrap();
    assert_eq!(revised.rule().revision, 2);
    assert_eq!(revised.rule().consequences.declared_delta().copied(), Some(amount("-1300")));

    // A stale edit is refused rather than silently overwriting a newer one.
    let stale = revise(
        &store.pool,
        ReviseRecurrence {
            recurrence_uid: &uid,
            expected_revision: 1,
            consequences: Consequences::capture(amount("-9999"), None),
            condition: None,
            note: None,
            cadence: Cadence::every_months(1),
            anchor_at: at("2026-01-01T00:00:00Z"),
            request_id: "req-3",
            actor_uid: None,
        },
        at("2026-03-02T00:00:00Z"),
    )
    .await;
    assert!(stale.is_err(), "a stale revision must not win");
}

#[tokio::test]
async fn pausing_hides_the_future_but_keeps_the_past() {
    let store = store().await;
    let record_uid = record(&store).await;
    let uid = monthly_rent(&store, &record_uid, "req-1").await;

    set_state(&store.pool, &uid, 1, true, "req-pause", None, at("2026-03-15T00:00:00Z"))
        .await
        .unwrap();
    let rule = store::recurrence::get(&store.pool, &uid).await.unwrap().unwrap();
    assert!(rule.is_paused());

    let found = occurrences(
        &store.pool,
        &rule,
        at("2026-01-01T00:00:00Z"),
        at("2026-06-01T00:00:00Z"),
        at("2026-03-15T00:00:00Z"),
    )
    .await
    .unwrap();
    // Jan, Feb and Mar already happened and are still explained by this rule;
    // April and May are no longer offered.
    assert_eq!(found.len(), 3);
    assert!(found.iter().all(|item| item.due_at <= at("2026-03-15T00:00:00Z")));

    set_state(&store.pool, &uid, 2, false, "req-resume", None, at("2026-03-16T00:00:00Z"))
        .await
        .unwrap();
    let resumed = store::recurrence::get(&store.pool, &uid).await.unwrap().unwrap();
    assert!(!resumed.is_paused());
    let after = occurrences(
        &store.pool,
        &resumed,
        at("2026-01-01T00:00:00Z"),
        at("2026-06-01T00:00:00Z"),
        at("2026-03-15T00:00:00Z"),
    )
    .await
    .unwrap();
    assert_eq!(after.len(), 5);
}

#[tokio::test]
async fn a_rule_stops_producing_dates_at_its_own_end() {
    let store = store().await;
    let record_uid = record(&store).await;
    let uid = create(
        &store.pool,
        NewRecurrence {
            record_uid: &record_uid,
            consequences: Consequences::capture(amount("-50"), None),
            condition: None,
            note: Some("gym"),
            // The close is part of the rule now, not a column beside it.
            cadence: Cadence::every_months(1)
                .until(nucleus::karma::CivilDateTime::parse_canonical("2026-04-01T00:00:00.000").unwrap()),
            anchor_at: at("2026-01-01T00:00:00Z"),
            request_id: "req-1",
            actor_uid: None,
        },
        at("2026-01-01T00:00:00Z"),
    )
    .await
    .unwrap()
    .rule()
    .uid
    .clone();
    let rule = store::recurrence::get(&store.pool, &uid).await.unwrap().unwrap();

    let found = occurrences(
        &store.pool,
        &rule,
        at("2026-01-01T00:00:00Z"),
        at("2026-12-01T00:00:00Z"),
        at("2026-12-01T00:00:00Z"),
    )
    .await
    .unwrap();
    // The end is exclusive like every other window here, so April's own 1st is
    // not produced.
    assert_eq!(found.len(), 3);
    assert!(found.iter().all(|item| item.due_at < at("2026-04-01T00:00:00Z")));
}

#[tokio::test]
async fn a_cadence_that_never_advances_is_refused_at_the_boundary() {
    let store = store().await;
    let record_uid = record(&store).await;
    let refused = create(
        &store.pool,
        NewRecurrence {
            record_uid: &record_uid,
            consequences: Consequences::capture(amount("-10"), None),
            condition: None,
            note: None,
            cadence: Cadence::every_days(0),
            anchor_at: at("2026-01-01T00:00:00Z"),
            request_id: "req-bad",
            actor_uid: None,
        },
        at("2026-01-01T00:00:00Z"),
    )
    .await;
    assert!(refused.is_err(), "a rule that cannot advance must not be stored");
}

#[tokio::test]
async fn a_rule_survives_a_reopen_with_its_cadence_intact() {
    let path = std::env::temp_dir()
        .join(format!("lince-recurrence-{}.db", nucleus::new_uid("test")));
    let url = format!("sqlite://{}", path.display());
    let store = Store::open(&url).await.unwrap();
    let record_uid = record(&store).await;
    let uid = create(
        &store.pool,
        NewRecurrence {
            record_uid: &record_uid,
            consequences: Consequences::capture(amount("-12.50"), None),
            condition: None,
            note: Some("streaming"),
            cadence: Cadence::every_weeks(2),
            anchor_at: at("2026-01-05T09:30:00Z"),
            request_id: "req-1",
            actor_uid: None,
        },
        at("2026-01-05T09:30:00Z"),
    )
    .await
    .unwrap()
    .rule()
    .uid
    .clone();
    store.pool.close().await;

    let reopened = Store::open(&url).await.unwrap();
    let rule = store::recurrence::get(&reopened.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rule.cadence, Cadence::every_weeks(2));
    // Exactness survives the round trip: a subscription is 12.50, not 12.5.
    assert_eq!(rule.consequences.declared_delta().copied(), Some(amount("-12.50")));
    assert_eq!(rule.note.as_deref(), Some("streaming"));
}

