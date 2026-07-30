//! The two read sources a surface needs to *control* changes rather than only
//! display them.
//!
//! `Source::Fact` answers what the Ledger holds. These answer what a person
//! typed and may still fix, and what a rule expects next — which needs `uid`
//! and `revision`, because a correction has to quote both and a stale one must
//! lose.

use chrono::{Duration, Utc};
use engine::Engine;
use engine::actions::Action;
use nucleus::RecordKind;
use nucleus::karma::Cadence;
use protein::{Include, Predicate, Protein, Source};
use serde_json::Value;
use store::records::NewRecord;

async fn engine() -> Engine {
    let engine = Engine::open_memory().await.expect("engine opens");
    store::organs::ensure_local(&engine.store.pool, "http://protein-entries.test")
        .await
        .expect("local Organ");
    engine
}

async fn record(e: &Engine, slug: &str) -> String {
    store::records::create(
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
    .uid
}

async fn concept(e: &Engine, name: &str) -> String {
    store::concepts::create(&e.store.pool, name, &[])
        .await
        .expect("concept")
}

fn query(source: Source, filter: Vec<Predicate>) -> Protein {
    Protein {
        source,
        filter,
        include: Include::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    }
}

async fn capture(e: &Engine, target: &str, amount: &str, concept: Option<&str>) -> String {
    e.act(
        Action::CaptureEntry {
            target: target.to_string(),
            amount: amount.to_string(),
            concept: concept.map(str::to_string),
            note: None,
            at: Some((Utc::now() - Duration::days(1)).to_rfc3339()),
            request_id: None,
        },
        None,
    )
    .await
    .expect("capture")
    .created
    .expect("an entry")
}

fn find<'a>(rows: &'a [Value], uid: &str) -> &'a Value {
    rows.iter()
        .find(|row| row["uid"] == uid)
        .expect("the entry should be listed")
}

#[tokio::test]
async fn an_entry_carries_the_handle_a_correction_has_to_quote() {
    let e = engine().await;
    let food = concept(&e, "food").await;
    let checking = record(&e, "checking").await;
    let entry = capture(&e, &checking, "-10.50", Some(&food)).await;

    let rows = protein::execute(&e.store, &query(Source::Entry, vec![]))
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
    // Edit-then-edit is ordinary. If the second edit read a stale revision the
    // surface would refuse a change the person is entitled to make; and if the
    // replacement Fact lost its classification, the category would quietly drop
    // the change.
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

    let rows = protein::execute(&e.store, &query(Source::Entry, vec![]))
        .await
        .unwrap();
    let row = find(&rows, &entry);
    assert_eq!(row["revision"], 2);
    assert_eq!(row["amount"], "-15");
    assert_eq!(row["concept"], food, "a correction keeps its category");
}

#[tokio::test]
async fn a_voided_entry_is_still_listed() {
    // An append-only Ledger has no delete. Hiding voided entries would make a
    // correction look like a disappearance.
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

    let rows = protein::execute(&e.store, &query(Source::Entry, vec![]))
        .await
        .unwrap();
    let row = find(&rows, &entry);
    assert_eq!(row["void"], true);
    assert_eq!(row["state"], "void");
}

#[tokio::test]
async fn filtering_by_category_searches_past_the_page_limit() {
    // The bug this pins: applying the caller's limit in SQL takes the newest N
    // rows and filters those, so a rare category reads as empty while plenty of
    // matching changes sit just past the cut.
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
    let rows = protein::execute(&e.store, &protein).await.unwrap();
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

    let rows = protein::execute(
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

#[tokio::test]
async fn a_rule_and_its_dates_arrive_as_one_query() {
    let e = engine().await;
    let rent = concept(&e, "rent").await;
    let checking = record(&e, "checking").await;
    let rule = e
        .act(
            Action::CreateRecurrence {
                target: checking.clone(),
                amount: "-1200".to_string(),
                concept: Some(rent.clone()),
                note: Some("rent".to_string()),
                cadence: Cadence::every_days(7),
                anchor_at: Some((Utc::now() - Duration::days(1)).to_rfc3339()),
                request_id: Some("rule-1".to_string()),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();

    let rows = protein::execute(&e.store, &query(Source::Recurrence, vec![]))
        .await
        .unwrap();
    let rules: Vec<&Value> = rows.iter().filter(|r| r["kind"] == "recurrence").collect();
    let dates: Vec<&Value> = rows.iter().filter(|r| r["kind"] == "occurrence").collect();

    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0]["uid"], rule);
    assert_eq!(rules[0]["revision"], 1, "a pause has to quote this");
    assert_eq!(rules[0]["paused"], false);
    assert_eq!(rules[0]["amount"], "-1200");
    // The cadence round-trips as the compound shape the sand sends back, with
    // every component addressable rather than one of a fixed set of presets.
    assert_eq!(rules[0]["cadence"]["every"]["days"], 7);
    assert_eq!(rules[0]["cadence"]["every"]["months"], 0);
    assert_eq!(rules[0]["cadence"]["invalid_day"], "clamp");
    assert!(
        rules[0]["truncated"] == false,
        "a weekly rule fits in the window and must not claim otherwise"
    );
    assert!(!dates.is_empty(), "an active rule produces dates");
}

#[tokio::test]
async fn an_applied_date_leaves_the_pending_inbox() {
    // The sand's "Expected next" list shows `due` and `planned` only. If an
    // applied date stayed pending, a person would be invited to pay it twice.
    let e = engine().await;
    let rent = concept(&e, "rent").await;
    let checking = record(&e, "checking").await;
    let due = (Utc::now() - Duration::days(1))
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    let rule = e
        .act(
            Action::CreateRecurrence {
                target: checking.clone(),
                amount: "-1200".to_string(),
                concept: Some(rent),
                note: None,
                cadence: Cadence::every_days(1),
                anchor_at: Some(due.to_rfc3339()),
                request_id: Some("rule-1".to_string()),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();

    e.act(
        Action::ApplyRecurrenceOccurrence {
            recurrence: rule.clone(),
            due_at: due.to_rfc3339(),
            amount: None,
            note: None,
        },
        None,
    )
    .await
    .unwrap();

    let rows = protein::execute(&e.store, &query(Source::Recurrence, vec![]))
        .await
        .unwrap();
    let applied: Vec<&Value> = rows
        .iter()
        .filter(|r| r["kind"] == "occurrence" && r["due_at"] == due.to_rfc3339())
        .collect();
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0]["state"], "applied");
    assert!(
        applied[0]["entry"].is_string(),
        "an applied date points at the change it made"
    );
    // And it is not offered again.
    let pending: Vec<&Value> = rows
        .iter()
        .filter(|r| {
            r["kind"] == "occurrence"
                && (r["state"] == "due" || r["state"] == "planned")
                && r["due_at"] == due.to_rfc3339()
        })
        .collect();
    assert!(pending.is_empty());
}

#[tokio::test]
async fn a_paused_rule_is_listed_with_its_state_so_it_can_be_resumed() {
    let e = engine().await;
    let checking = record(&e, "checking").await;
    let rule = e
        .act(
            Action::CreateRecurrence {
                target: checking.clone(),
                amount: "-9".to_string(),
                concept: None,
                note: None,
                cadence: Cadence::every_weeks(1),
                anchor_at: Some((Utc::now() - Duration::days(1)).to_rfc3339()),
                request_id: Some("rule-1".to_string()),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
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

    let rows = protein::execute(&e.store, &query(Source::Recurrence, vec![]))
        .await
        .unwrap();
    let listed = rows
        .iter()
        .find(|r| r["kind"] == "recurrence" && r["uid"] == rule)
        .expect("a paused rule is still listed");
    assert_eq!(listed["paused"], true);
    // The revision moved, and resuming must quote the new one.
    assert_eq!(listed["revision"], 2);
}
