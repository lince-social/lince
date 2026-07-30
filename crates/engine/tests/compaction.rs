//! Part II completion tests: sum variants (only-positive / only-negative /
//! end-lagged) and compaction (fold pre-checkpoint history into a cold archive
//! anchored from inside the Ledger).

use chrono::{DateTime, Utc};
use engine::Engine;
use nucleus::{Cause, CauseKind, Fact, NewFact, RecordKind};
use store::records::NewRecord;

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine opens")
}

async fn plain(e: &Engine, slug: &str) -> String {
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

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

async fn bump(e: &Engine, uid: &str, delta: f64, now: DateTime<Utc>) {
    e.append(
        NewFact::quantity_f64(uid.to_string(), delta, Cause::user_edit()),
        now,
    )
    .await
    .expect("append");
}

#[tokio::test]
async fn sum_variants_split_inflow_outflow_and_lag() {
    let e = engine().await;
    let apples = plain(&e, "apples.stock").await;

    bump(&e, &apples, 10.0, at("2026-07-01T08:00:00Z")).await; // old inflow
    bump(&e, &apples, -4.0, at("2026-07-05T08:00:00Z")).await; // recent outflow
    bump(&e, &apples, 2.0, at("2026-07-06T08:00:00Z")).await; // recent inflow

    let now = at("2026-07-07T08:00:00Z");
    let day = 86_400;
    let pool = &e.store.pool;

    // 3-day trailing window catches only the recent pair
    assert_eq!(
        store::facts::sum_window(pool, &apples, 3 * day, now)
            .await
            .unwrap().to_f64(),
        -2.0
    );
    assert_eq!(
        store::facts::sum_pos_window(pool, &apples, 3 * day, now)
            .await
            .unwrap().to_f64(),
        2.0
    );
    assert_eq!(
        store::facts::sum_neg_window(pool, &apples, 3 * day, now)
            .await
            .unwrap().to_f64(),
        -4.0
    );

    // end-lagged: the window [now-8d, now-3d) sees only the old +10
    assert_eq!(
        store::facts::sum_window_lagged(pool, &apples, 5 * day, 3 * day, now)
            .await
            .unwrap()
            .to_f64(),
        10.0
    );
}

#[tokio::test]
async fn compaction_folds_history_into_the_checkpoint_and_anchors_the_archive() {
    let e = engine().await;
    store::organs::ensure_local(&e.store.pool, "http://127.0.0.1:0")
        .await
        .expect("local organ");
    let apples = plain(&e, "apples.stock").await;
    let hammer = plain(&e, "hammer").await; // no retention policy: untouched

    // History: three old facts, then a checkpoint, then one fresh fact.
    bump(&e, &apples, 10.0, at("2026-01-01T08:00:00Z")).await;
    bump(&e, &apples, -3.0, at("2026-01-02T08:00:00Z")).await;
    bump(&e, &apples, 1.0, at("2026-01-03T08:00:00Z")).await;
    bump(&e, &hammer, 1.0, at("2026-01-03T09:00:00Z")).await;
    e.checkpoint_all(at("2026-02-01T00:00:00Z"))
        .await
        .expect("checkpoint");
    bump(&e, &apples, -2.0, at("2026-07-01T08:00:00Z")).await;

    // Policy: plain records keep 30 days.
    store::facts::set_retention(&e.store.pool, "plain", 30 * 86_400)
        .await
        .expect("policy");

    let dir = std::env::temp_dir().join(format!("lince-compact-{}", nucleus::new_uid("t")));
    let now = at("2026-07-10T00:00:00Z");
    let report = e.compact(now, &dir).await.expect("compact");

    // The three old apple facts and the old hammer fact are archived — the
    // hammer is `plain` too and had a checkpoint; both fold. Checkpoints and
    // the fresh fact stay hot.
    assert_eq!(report.archived, 4);
    let file = report.archive_file.clone().expect("archive file");
    let anchor = report.anchor.clone().expect("anchor fact");

    // Quantity cache untouched by compaction.
    assert_eq!(
        store::records::quantity(&e.store.pool, &apples)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(6.0)
    );

    // Hot table: apples keeps exactly its checkpoint + the fresh fact.
    let hot = store::facts::for_record(&e.store.pool, &apples, 100)
        .await
        .unwrap();
    assert_eq!(hot.len(), 2);
    assert!(hot.iter().any(|f| f.cause.kind == CauseKind::Checkpoint));
    assert!(hot.iter().any(|f| f.delta == store::exact::from_f64(-2.0)));

    // Fold invariant: checkpoint level + remaining deltas == cache.
    let checkpoint = hot
        .iter()
        .find(|f| f.cause.kind == CauseKind::Checkpoint)
        .unwrap();
    // The checkpoint level is canonical decimal TEXT, not a JSON float: after
    // compaction it IS the record's level, so it must not round.
    let payload =
        serde_json::from_str::<serde_json::Value>(checkpoint.payload.as_deref().unwrap()).unwrap();
    let level: f64 = payload["level"].as_str().unwrap().parse().unwrap();
    let remaining: f64 = hot
        .iter()
        .filter(|f| f.cause.kind != CauseKind::Checkpoint)
        .map(|f| f.delta.to_f64())
        .sum();
    assert_eq!(level + remaining, 6.0);

    // Cold file: every archived fact still parses and self-verifies.
    let bytes = std::fs::read(&file).expect("archive readable");
    let lines: Vec<Fact> = String::from_utf8(bytes.clone())
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).expect("archived fact parses"))
        .collect();
    assert_eq!(lines.len(), 4);
    assert!(lines.iter().all(nucleus::fact::verify_chain_step));

    // Anchor: on the local organ record, payload hash matches the file bytes.
    let organ = store::organs::local(&e.store.pool).await.unwrap().unwrap();
    assert_eq!(anchor.record_uid, organ.uid);
    let payload: serde_json::Value =
        serde_json::from_str(anchor.payload.as_deref().unwrap()).unwrap();
    assert_eq!(
        payload["archive_hash"].as_str().unwrap(),
        nucleus::fact::sha256_hex(&bytes)
    );
    assert_eq!(payload["archived"].as_u64().unwrap(), 4);

    // Idempotent: a second run archives nothing.
    let again = e.compact(now, &dir).await.expect("compact again");
    assert_eq!(again.archived, 0);
    assert!(again.archive_file.is_none());

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn compaction_skips_records_without_policy_or_checkpoint() {
    let e = engine().await;
    let apples = plain(&e, "apples.stock").await;
    bump(&e, &apples, 5.0, at("2026-01-01T08:00:00Z")).await;

    let dir = std::env::temp_dir().join(format!("lince-compact-{}", nucleus::new_uid("t")));
    let now = at("2026-07-10T00:00:00Z");

    // No policy at all: nothing happens.
    let report = e.compact(now, &dir).await.expect("compact");
    assert_eq!(report.archived, 0);

    // Policy but no checkpoint: still nothing (no level to fold into).
    store::facts::set_retention(&e.store.pool, "plain", 86_400)
        .await
        .unwrap();
    let report = e.compact(now, &dir).await.expect("compact");
    assert_eq!(report.archived, 0);
    assert_eq!(
        store::facts::for_record(&e.store.pool, &apples, 10)
            .await
            .unwrap()
            .len(),
        1
    );
    std::fs::remove_dir_all(&dir).ok();
}
