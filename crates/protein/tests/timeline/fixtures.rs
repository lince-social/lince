use chrono::{Duration, Utc};
use engine::{Engine, actions::Action};
use nucleus::RecordKind;
use protein::{Include, Predicate, Protein, Source};
use serde_json::Value;
use store::records::NewRecord;

pub(super) async fn engine() -> Engine {
    let engine = Engine::open_memory().await.expect("engine opens");
    store::organs::ensure_local(&engine.store.pool, "http://protein-timeline.test")
        .await
        .expect("local Organ");
    engine
}

pub(super) async fn record(e: &Engine, slug: &str) -> String {
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

pub(super) async fn concept(e: &Engine, name: &str, parents: &[&str]) -> String {
    store::concepts::create(&e.store.pool, name, parents)
        .await
        .expect("concept")
}

pub(super) async fn capture(e: &Engine, target: &str, amount: &str, concept: &str, at: &str) {
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

pub(super) fn timeline(concept: &str, since: String, before: String) -> Protein {
    Protein {
        source: Source::Timeline,
        filter: vec![
            Predicate::ClassifiedIn(concept.to_string()),
            Predicate::AtSince(since),
            Predicate::AtBefore(before),
        ],
        fields: None,
        include: Include::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    }
}

pub(super) fn rows_of<'a>(rows: &'a [Value], kind: &str) -> Vec<&'a Value> {
    rows.iter()
        .filter(|r| r["kind"].as_str() == Some(kind))
        .collect()
}

pub(super) fn context(rows: &[Value]) -> &Value {
    rows.first().expect("a timeline always has a context row")
}

pub(super) fn window() -> (String, String) {
    let now = Utc::now();
    (
        (now - Duration::days(120)).to_rfc3339(),
        (now + Duration::days(120)).to_rfc3339(),
    )
}
