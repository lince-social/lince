//! DST harness (blueprint Part 0): the organism runs on a virtual clock, in
//! memory, and the same script always produces the same Ledger; a recorded
//! fact log replays into a fresh Cell deterministically and idempotently.
//!
//! Everything here is clocked explicitly — no wall time reaches the engine.
//! (`Engine::act` stamps `Utc::now()` internally, so the script drives the
//! engine through `append`/`heartbeat`, the same entry points the daemon and
//! sync use.)

use chrono::{DateTime, Utc};
use engine::Engine;
use nucleus::{Cause, ConsequenceKind, NewFact, RecordKind};
use store::records::NewRecord;
use store::sqlx::Row;

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

async fn plain(e: &Engine, slug: &str, quantity: f64) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head: slug,
            body: "",
            quantity,
        },
    )
    .await
    .expect("record")
    .uid
}

/// The scripted week: a daily habit (Frequency + rule), a stock with a
/// threshold rule (cascade), and user edits — all on the virtual clock.
async fn run_script(e: &Engine) {
    plain(e, "apples.stock", 8.0).await;
    plain(e, "exercise", 0.0).await;
    plain(e, "alerts.low-apples", 0.0).await;

    store::freqs::create(
        &e.store.pool,
        store::freqs::NewFrequency {
            slug: "freq.daily-7am",
            head: "Daily 7am",
            seconds: 0,
            days: 1,
            months: 0,
            day_of_week: None,
            next_at: at("2026-07-05T07:00:00Z"),
            catch_up: false,
        },
    )
    .await
    .unwrap();
    store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.daily-exercise",
            head: "Daily exercise",
            condition: "-1 * freq(@freq.daily-7am)",
            gate: "!=0",
            carry: "value",
            consequences: vec![(ConsequenceKind::SetQuantity, Some("@exercise".into()), None)],
        },
    )
    .await
    .unwrap();
    store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.low-apples",
            head: "Low apples",
            condition: "@apples.stock",
            gate: "<3",
            carry: "one",
            consequences: vec![(
                ConsequenceKind::SetQuantity,
                Some("@alerts.low-apples".into()),
                None,
            )],
        },
    )
    .await
    .unwrap();
    e.reload_rules().await.unwrap();

    e.heartbeat(at("2026-07-05T10:00:00Z")).await.unwrap(); // daily fires
    user_delta(e, "apples.stock", -3.0, at("2026-07-05T12:00:00Z")).await; // 8 -> 5
    e.heartbeat(at("2026-07-06T08:00:00Z")).await.unwrap(); // daily fires again
    user_delta(e, "apples.stock", -3.0, at("2026-07-06T09:00:00Z")).await; // 5 -> 2, alert cascades
    e.heartbeat(at("2026-07-07T07:30:00Z")).await.unwrap(); // third day
    user_delta(e, "exercise", 1.0, at("2026-07-07T08:00:00Z")).await; // habit done: -1 -> 0
}

async fn user_delta(e: &Engine, slug: &str, delta: f64, t: DateTime<Utc>) {
    let uid = store::records::resolve(&e.store.pool, slug)
        .await
        .unwrap()
        .unwrap()
        .uid;
    e.append(
        NewFact {
            at: Some(t),
            ..NewFact::quantity(&uid, delta, Cause::user_edit())
        },
        t,
    )
    .await
    .unwrap();
}

/// One Ledger entry in run-comparable form: uids are minted per run (ULIDs
/// embed randomness) so records and causes are named by slug; `hash`/`uid` are
/// excluded because the hash covers the uid.
#[derive(Debug, PartialEq)]
struct LogEntry {
    record: String,
    delta: f64,
    at: String,
    cause_kind: String,
    cause: Option<String>,
    payload: Option<String>,
}

async fn slug_of(e: &Engine, uid: &str) -> String {
    store::records::get(&e.store.pool, uid)
        .await
        .unwrap()
        .and_then(|r| r.slug)
        .unwrap_or_else(|| uid.to_string())
}

/// The full Ledger in true (rowid) order.
async fn full_log(e: &Engine) -> Vec<nucleus::Fact> {
    let rows = store::sqlx::query("SELECT uid FROM fact ORDER BY rowid")
        .fetch_all(&e.store.pool)
        .await
        .unwrap();
    let mut facts = Vec::with_capacity(rows.len());
    for row in rows {
        let uid: String = row.get("uid");
        facts.push(store::facts::get(&e.store.pool, &uid).await.unwrap().unwrap());
    }
    facts
}

async fn comparable_log(e: &Engine) -> Vec<LogEntry> {
    let mut out = Vec::new();
    for f in full_log(e).await {
        let cause = match &f.cause.uid {
            Some(uid) => Some(slug_of(e, uid).await),
            None => None,
        };
        out.push(LogEntry {
            record: slug_of(e, &f.record_uid).await,
            delta: f.delta,
            at: f.at.to_rfc3339(),
            cause_kind: f.cause.kind.as_str().to_string(),
            cause,
            payload: f.payload.clone(),
        });
    }
    out
}

async fn quantities(e: &Engine) -> Vec<(String, f64)> {
    let mut all: Vec<(String, f64)> = store::records::list_all(&e.store.pool)
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.slug.unwrap_or(r.uid), r.quantity))
        .collect();
    all.sort_by(|x, y| x.0.cmp(&y.0));
    all
}

#[tokio::test]
async fn same_script_same_ledger() {
    let a = Engine::open_memory().await.unwrap();
    let b = Engine::open_memory().await.unwrap();
    run_script(&a).await;
    run_script(&b).await;

    let log_a = comparable_log(&a).await;
    let log_b = comparable_log(&b).await;
    assert!(!log_a.is_empty(), "the script produced a Ledger");
    assert_eq!(log_a, log_b, "two runs of one script: one Ledger");
    assert_eq!(quantities(&a).await, quantities(&b).await);

    // sanity: the week actually happened
    let q = quantities(&a).await;
    assert!(q.contains(&("apples.stock".into(), 2.0)));
    assert!(q.contains(&("alerts.low-apples".into(), 1.0)));
    assert!(
        q.contains(&("exercise".into(), 0.0)),
        "the rule re-arms the Need daily; day 3's +1 satisfies it: {q:?}"
    );
}

#[tokio::test]
async fn recorded_log_replays_deterministically_and_idempotently() {
    let a = Engine::open_memory().await.unwrap();
    run_script(&a).await;
    let recorded = full_log(&a).await;

    // A fresh Cell with the same record seeds (facts replay; rows travel by
    // uid in real sync — here the initial quantities are the seed state).
    let b = Engine::open_memory().await.unwrap();
    for r in store::records::list_all(&a.store.pool).await.unwrap() {
        store::sqlx::query(
            "INSERT INTO record (uid, slug, kind, head, body, quantity, created_at, updated_at)
             VALUES (?, ?, ?, ?, '', ?, ?, ?)",
        )
        .bind(&r.uid)
        .bind(&r.slug)
        .bind(&r.kind)
        .bind(&r.head)
        .bind(initial_quantity(&r, &recorded))
        .bind(at("2026-07-05T00:00:00Z").to_rfc3339())
        .bind(at("2026-07-05T00:00:00Z").to_rfc3339())
        .execute(&b.store.pool)
        .await
        .unwrap();
    }

    let replay: Vec<NewFact> = recorded
        .iter()
        .map(|f| NewFact {
            uid: Some(f.uid.clone()), // idempotency key
            record_uid: f.record_uid.clone(),
            delta: f.delta,
            at: Some(f.at),
            actor_uid: f.actor_uid.clone(),
            cause: f.cause.clone(),
            payload: f.payload.clone(),
        })
        .collect();

    let applied = engine::append::append_all(
        &b.store,
        replay.clone(),
        at("2026-07-08T00:00:00Z"),
        None,
    )
    .await
    .unwrap();
    assert_eq!(applied.len(), recorded.len(), "every fact re-applies once");
    assert_eq!(
        quantities(&a).await,
        quantities(&b).await,
        "replayed Ledger folds to the same state vector"
    );

    // replaying the same log again is a no-op (idempotent by fact uid)
    let again = engine::append::append_all(&b.store, replay, at("2026-07-09T00:00:00Z"), None)
        .await
        .unwrap();
    assert!(again.is_empty(), "second replay changes nothing");
    assert_eq!(quantities(&a).await, quantities(&b).await);
}

/// Rewind a record's final cached quantity to its pre-log seed value.
fn initial_quantity(r: &store::records::RecordRow, log: &[nucleus::Fact]) -> f64 {
    let played: f64 = log
        .iter()
        .filter(|f| f.record_uid == r.uid)
        .map(|f| f.delta)
        .sum();
    r.quantity - played
}
