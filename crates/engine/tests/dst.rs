use chrono::{DateTime, Utc};
use engine::Engine;
use nucleus::karma::{Cadence, Consequence};
use nucleus::{Cause, NewFact, RecordKind};

mod support;
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
            quantity: store::exact::from_f64(quantity),
        },
    )
    .await
    .expect("record")
    .uid
}

async fn run_script(e: &Engine) {
    plain(e, "apples.stock", 8.0).await;
    plain(e, "exercise", 0.0).await;
    plain(e, "alerts.low-apples", 0.0).await;

    support::declare_rule(
        e,
        "@exercise",
        Cadence::every_days(1),
        "2026-07-05T07:00:00Z",
        None,
        None,
        None,
        vec![Consequence::SetQuantity {
            value: Some(nucleus::DecimalValue::parse_inferred("-1").unwrap()),
        }],
    )
    .await;
    support::declare_rule(
        e,
        "@alerts.low-apples",
        Cadence::every_days(1),
        "2026-07-05T07:00:00Z",
        Some("@apples.stock"),
        Some("<3"),
        Some("one"),
        vec![Consequence::SetQuantity {
            value: Some(nucleus::DecimalValue::parse_inferred("1").unwrap()),
        }],
    )
    .await;

    e.heartbeat(at("2026-07-05T10:00:00Z")).await.unwrap();
    user_delta(e, "apples.stock", -3.0, at("2026-07-05T12:00:00Z")).await;
    e.heartbeat(at("2026-07-06T08:00:00Z")).await.unwrap();
    user_delta(e, "apples.stock", -3.0, at("2026-07-06T09:00:00Z")).await;
    e.heartbeat(at("2026-07-07T07:30:00Z")).await.unwrap();
    user_delta(e, "exercise", 1.0, at("2026-07-07T08:00:00Z")).await;
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
            ..NewFact::quantity_f64(&uid, delta, Cause::user_edit())
        },
        t,
    )
    .await
    .unwrap();
}

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

async fn full_log(e: &Engine) -> Vec<nucleus::Fact> {
    let rows = store::sqlx::query("SELECT uid FROM fact ORDER BY rowid")
        .fetch_all(&e.store.pool)
        .await
        .unwrap();
    let mut facts = Vec::with_capacity(rows.len());
    for row in rows {
        let uid: String = row.get("uid");
        facts.push(
            store::facts::get(&e.store.pool, &uid)
                .await
                .unwrap()
                .unwrap(),
        );
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
            delta: f.delta.to_f64(),
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
        .map(|r| {
            (
                r.slug.clone().unwrap_or_else(|| r.uid.clone()),
                r.quantity_f64(),
            )
        })
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

    let b = Engine::open_memory().await.unwrap();
    for r in store::records::list_all(&a.store.pool).await.unwrap() {
        if matches!(
            r.slug.as_deref(),
            Some(store::organs::LOCAL_ORGAN_SLUG) | Some(store::cells::LOCAL_CELL_SLUG)
        ) {
            continue;
        }
        store::sqlx::query(
            "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                                 organ_uid, created_at, updated_at)
             VALUES (?, ?, ?, ?, '', ?, ?, ?, ?, ?)",
        )
        .bind(&r.uid)
        .bind(&r.slug)
        .bind(&r.kind)
        .bind(&r.head)
        .bind(initial_quantity(&r, &recorded).mantissa().to_string())
        .bind(i64::from(initial_quantity(&r, &recorded).scale()))
        .bind(&r.organ_uid)
        .bind(at("2026-07-05T00:00:00Z").to_rfc3339())
        .bind(at("2026-07-05T00:00:00Z").to_rfc3339())
        .execute(&b.store.pool)
        .await
        .unwrap();
    }

    let replay: Vec<NewFact> = recorded
        .iter()
        .map(|f| NewFact {
            uid: Some(f.uid.clone()),
            record_uid: f.record_uid.clone(),
            delta: f.delta,
            at: Some(f.at),
            actor_uid: f.actor_uid.clone(),
            cause: f.cause.clone(),
            payload: f.payload.clone(),
        })
        .collect();

    let applied =
        engine::append::append_all(&b.store, replay.clone(), at("2026-07-08T00:00:00Z"), None)
            .await
            .unwrap();
    assert_eq!(applied.len(), recorded.len(), "every fact re-applies once");
    assert_eq!(
        quantities(&a).await,
        quantities(&b).await,
        "replayed Ledger folds to the same state vector"
    );

    let again = engine::append::append_all(&b.store, replay, at("2026-07-09T00:00:00Z"), None)
        .await
        .unwrap();
    assert!(again.is_empty(), "second replay changes nothing");
    assert_eq!(quantities(&a).await, quantities(&b).await);
}

fn initial_quantity(r: &store::records::RecordRow, log: &[nucleus::Fact]) -> nucleus::DecimalValue {
    let played = store::exact::sum_exact(
        log.iter()
            .filter(|f| f.record_uid == r.uid)
            .map(|f| f.delta),
    )
    .expect("exact fold");
    store::exact::difference(r.quantity, played).expect("exact difference")
}
