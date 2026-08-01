//! One classified axis through time: what settled, where it stands, what is
//! declared ahead.
//!
//! The claim under test is that these are one query, not three panels. A client
//! that had to stitch them would have to add exact decimals in JavaScript, and
//! the running cumulative is precisely the number that must not be computed
//! there.
//!
//! Nothing here is domain-specific. The scenario reads as a running balance
//! because that is legible; the same query answers "how much flour is
//! committed" unchanged.

use chrono::{Duration, Months, Utc};
use engine::Engine;
use engine::actions::Action;
use nucleus::RecordKind;
use nucleus::karma::Cadence;
use protein::{Include, Predicate, Protein, Source};
use serde_json::Value;
use store::records::NewRecord;

async fn engine() -> Engine {
    let engine = Engine::open_memory().await.expect("engine opens");
    store::organs::ensure_local(&engine.store.pool, "http://protein-timeline.test")
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

async fn concept(e: &Engine, name: &str, parents: &[&str]) -> String {
    store::concepts::create(&e.store.pool, name, parents)
        .await
        .expect("concept")
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

fn timeline(concept: &str, since: String, before: String) -> Protein {
    Protein {
        source: Source::Timeline,
        filter: vec![
            Predicate::ClassifiedIn(concept.to_string()),
            Predicate::AtSince(since),
            Predicate::AtBefore(before),
        ],
        include: Include::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    }
}

fn rows_of<'a>(rows: &'a [Value], kind: &str) -> Vec<&'a Value> {
    rows.iter()
        .filter(|r| r["kind"].as_str() == Some(kind))
        .collect()
}

fn context(rows: &[Value]) -> &Value {
    rows.first().expect("a timeline always has a context row")
}

/// A window wide enough to hold the fixtures, expressed around now so the
/// "present" split is exercised rather than hard-coded to a date.
fn window() -> (String, String) {
    let now = Utc::now();
    (
        (now - Duration::days(120)).to_rfc3339(),
        (now + Duration::days(120)).to_rfc3339(),
    )
}

#[tokio::test]
async fn a_timeline_needs_to_be_a_timeline_of_something() {
    let e = engine().await;
    let (since, before) = window();
    let refused = protein::execute(
        &e.store,
        &Protein {
            source: Source::Timeline,
            filter: vec![Predicate::AtSince(since), Predicate::AtBefore(before)],
            include: Include::default(),
            aggregate: None,
            order: vec![],
            limit: None,
        },
    )
    .await;
    // A timeline of everything is just the Ledger, and answering that here
    // would quietly produce a line nobody asked for.
    assert!(refused.is_err(), "a timeline without a concept is refused");
}

#[tokio::test]
async fn the_past_is_bucketed_and_carries_a_running_position() {
    let e = engine().await;
    let rent = concept(&e, "rent", &[]).await;
    let checking = record(&e, "checking").await;
    let now = Utc::now();
    let (since, before) = window();

    // Whole calendar months back, not 90/60/30 days: day offsets collapse into
    // two buckets whenever the run date makes two of them land in one month,
    // which turns a real assertion into a calendar lottery.
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

    let rows = protein::execute(&e.store, &timeline(&rent, since, before))
        .await
        .unwrap();
    let points = rows_of(&rows, "timeline_point");
    assert!(points.len() >= 3, "each month is its own bucket");

    // The cumulative is the whole reason this is one source: it must arrive
    // computed, exact, and as text.
    let last = points.last().unwrap();
    assert_eq!(last["cumulative"], "-3600");
    assert_eq!(context(&rows)["current"], "-3600");
}

#[tokio::test]
async fn the_line_starts_where_the_concept_already_stood() {
    // A cumulative that restarted at zero on the window's edge would draw a
    // position the person has never actually been in.
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
    let rows = protein::execute(&e.store, &timeline(&rent, since, before))
        .await
        .unwrap();
    assert_eq!(
        context(&rows)["opening"], "-5000",
        "what happened before the window is the line's starting height"
    );
    let points = rows_of(&rows, "timeline_point");
    assert_eq!(points.last().unwrap()["cumulative"], "-6200");
    // "Current state" counts everything settled, inside the window or before it.
    assert_eq!(context(&rows)["current"], "-6200");
}

#[tokio::test]
async fn the_future_is_declared_by_recurring_rules() {
    let e = engine().await;
    let rent = concept(&e, "rent", &[]).await;
    let checking = record(&e, "checking").await;
    let now = Utc::now();

    e.act(
        Action::CreateRecurrence {
            target: checking.clone(),
            consequences: vec![nucleus::karma::Consequence::CaptureEntry { amount: nucleus::DecimalValue::parse_inferred("-1200").unwrap(), concept: Some(rent.clone()) }],
condition: None,
gate: None,
carry: None,
            note: Some("rent".to_string()),
            cadence: Cadence::every_months(1),
            anchor_at: Some((now - Duration::days(2)).to_rfc3339()),
            request_id: Some("rule-1".to_string()),
        },
        None,
    )
    .await
    .unwrap();

    let (since, before) = window();
    let rows = protein::execute(&e.store, &timeline(&rent, since, before))
        .await
        .unwrap();

    let expected: Vec<&Value> = rows_of(&rows, "timeline_point")
        .into_iter()
        .filter(|p| !p["expected_net"].is_null())
        .collect();
    assert!(
        !expected.is_empty(),
        "an active rule puts declared amounts ahead of now"
    );

    // Every projected point names what produced it, so a number on a chart is
    // never one nobody can explain.
    let sources = rows_of(&rows, "timeline_source");
    assert!(!sources.is_empty());
    assert!(sources.iter().all(|s| s["origin"] == "recurrence"));
    assert!(sources.iter().all(|s| s["amount"] == "-1200"));
}

#[tokio::test]
async fn an_applied_date_is_counted_once_as_history_not_twice() {
    // The double-count this prevents is the one that would make every
    // rule-driven month look twice as expensive as it was.
    let e = engine().await;
    let rent = concept(&e, "rent", &[]).await;
    let checking = record(&e, "checking").await;
    let now = Utc::now();
    let due = (now - Duration::days(1))
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();

    let rule = e
        .act(
            Action::CreateRecurrence {
                target: checking.clone(),
                consequences: vec![nucleus::karma::Consequence::CaptureEntry { amount: nucleus::DecimalValue::parse_inferred("-1200").unwrap(), concept: Some(rent.clone()) }],
condition: None,
gate: None,
carry: None,
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
    let rows = protein::execute(&e.store, &timeline(&rent, since, before))
        .await
        .unwrap();

    // It moved once, so the settled position reflects exactly one payment.
    assert_eq!(context(&rows)["current"], "-1200");
    let sources = rows_of(&rows, "timeline_source");
    assert!(
        sources
            .iter()
            .all(|s| s["at"].as_str().unwrap() != due.to_rfc3339()),
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

    let rule = e
        .act(
            Action::CreateRecurrence {
                target: checking.clone(),
                consequences: vec![nucleus::karma::Consequence::CaptureEntry { amount: nucleus::DecimalValue::parse_inferred("-40").unwrap(), concept: Some(rent.clone()) }],
condition: None,
gate: None,
carry: None,
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
    let rows = protein::execute(&e.store, &timeline(&rent, since, before))
        .await
        .unwrap();
    let sources = rows_of(&rows, "timeline_source");
    assert!(
        sources
            .iter()
            .all(|s| s["at"].as_str().unwrap() != due.to_rfc3339()),
        "a date that was declined is not something to expect"
    );
}

#[tokio::test]
async fn a_paused_rule_stops_declaring_a_future() {
    let e = engine().await;
    let rent = concept(&e, "rent", &[]).await;
    let checking = record(&e, "checking").await;
    let now = Utc::now();

    let rule = e
        .act(
            Action::CreateRecurrence {
                target: checking.clone(),
                consequences: vec![nucleus::karma::Consequence::CaptureEntry { amount: nucleus::DecimalValue::parse_inferred("-1200").unwrap(), concept: Some(rent.clone()) }],
condition: None,
gate: None,
carry: None,
                note: None,
                cadence: Cadence::every_days(7),
                anchor_at: Some((now - Duration::days(1)).to_rfc3339()),
                request_id: Some("rule-1".to_string()),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();

    let (since, before) = window();
    let live = protein::execute(&e.store, &timeline(&rent, since.clone(), before.clone()))
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

    let paused = protein::execute(&e.store, &timeline(&rent, since, before))
        .await
        .unwrap();
    assert!(
        rows_of(&paused, "timeline_source").is_empty(),
        "a paused rule declares nothing ahead"
    );
}

#[tokio::test]
async fn the_concept_dag_is_respected_on_both_halves_of_the_line() {
    // `@rent` sits under `@cost`, so a `@cost` timeline must contain the rent
    // that settled AND the rent that is coming.
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
    e.act(
        Action::CreateRecurrence {
            target: checking.clone(),
            consequences: vec![nucleus::karma::Consequence::CaptureEntry { amount: nucleus::DecimalValue::parse_inferred("-1200").unwrap(), concept: Some(rent) }],
condition: None,
gate: None,
carry: None,
            note: None,
            cadence: Cadence::every_days(7),
            anchor_at: Some((now - Duration::days(1)).to_rfc3339()),
            request_id: Some("rule-1".to_string()),
        },
        None,
    )
    .await
    .unwrap();

    let (since, before) = window();
    let rows = protein::execute(&e.store, &timeline(&cost, since, before))
        .await
        .unwrap();
    assert_eq!(context(&rows)["current"], "-1200");
    assert!(
        !rows_of(&rows, "timeline_source").is_empty(),
        "a parent concept inherits its children's declared future"
    );
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
    let rows = protein::execute(&e.store, &timeline(&rent, since, before))
        .await
        .unwrap();
    // "How is rent going" must not include a change nobody said was rent.
    assert_eq!(context(&rows)["current"], "0");
}

#[tokio::test]
async fn an_unknown_concept_returns_nothing_rather_than_everything() {
    let e = engine().await;
    let (since, before) = window();
    let rows = protein::execute(&e.store, &timeline("no-such-concept", since, before))
        .await
        .unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn two_units_under_one_concept_never_contaminate_each_other() {
    // Flour in kilograms and coins counted as coins can both be `@stock`. Adding
    // them produces a number that means nothing, and the running total is the
    // most prominent number on the screen — so it must refuse to be one scalar.
    let e = engine().await;
    let stock = concept(&e, "stock", &[]).await;
    let kg = concept(&e, "kg", &[]).await;
    let coin = concept(&e, "coin", &[]).await;

    let jar = record(&e, "jar").await;
    let till = record(&e, "till").await;
    store::records::set_unit(&e.store.pool, &jar, Some(&kg))
        .await
        .unwrap();
    store::records::set_unit(&e.store.pool, &till, Some(&coin))
        .await
        .unwrap();

    let now = Utc::now();
    capture(&e, &jar, "-3", &stock, &(now - Duration::days(5)).to_rfc3339()).await;
    capture(&e, &till, "-100", &stock, &(now - Duration::days(5)).to_rfc3339()).await;

    let (since, before) = window();
    let rows = protein::execute(&e.store, &timeline(&stock, since, before))
        .await
        .unwrap();
    let context = context(&rows);

    // No single "current" exists, and the source says so rather than inventing
    // one by adding -3 kg to -100 coins.
    assert!(
        context["current"].is_null(),
        "a concept spanning two units has no one running total"
    );
    let by_unit = &context["current_by_unit"];
    assert_eq!(by_unit[&kg], "-3");
    assert_eq!(by_unit[&coin], "-100");

    // Each line is seeded from its own unit's history, not the other's.
    let points = rows_of(&rows, "timeline_point");
    let kg_points: Vec<&Value> = points.iter().copied().filter(|p| p["unit"] == kg).collect();
    let coin_points: Vec<&Value> =
        points.iter().copied().filter(|p| p["unit"] == coin).collect();
    assert_eq!(kg_points.last().unwrap()["cumulative"], "-3");
    assert_eq!(coin_points.last().unwrap()["cumulative"], "-100");
}

#[tokio::test]
async fn one_unit_still_gets_a_plain_running_total() {
    let e = engine().await;
    let rent = concept(&e, "rent", &[]).await;
    let coin = concept(&e, "coin", &[]).await;
    let checking = record(&e, "checking").await;
    store::records::set_unit(&e.store.pool, &checking, Some(&coin))
        .await
        .unwrap();

    let now = Utc::now();
    capture(
        &e,
        &checking,
        "-1200",
        &rent,
        &(now - Duration::days(5)).to_rfc3339(),
    )
    .await;

    let (since, before) = window();
    let rows = protein::execute(&e.store, &timeline(&rent, since, before))
        .await
        .unwrap();
    assert_eq!(context(&rows)["current"], "-1200");
    assert_eq!(context(&rows)["current_by_unit"][&coin], "-1200");
}

#[tokio::test]
async fn exactness_survives_the_whole_line() {
    // The reason this source exists rather than a client-side stitch: cents
    // must still be cents after a cumulative fold.
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
    let rows = protein::execute(&e.store, &timeline(&rent, since, before))
        .await
        .unwrap();
    assert_eq!(context(&rows)["current"], "-30.31");
    let points = rows_of(&rows, "timeline_point");
    assert_eq!(points.last().unwrap()["cumulative"], "-30.31");
    // Never a float on the wire.
    assert!(points.iter().all(|p| p["cumulative"].is_string()));
}
