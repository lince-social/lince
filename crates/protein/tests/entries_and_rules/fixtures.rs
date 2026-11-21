use chrono::{Duration, Utc};
use engine::{Engine, actions::Action};
use nucleus::RecordKind;
use protein::{Include, Predicate, Protein, Source};
use serde_json::Value;
use store::records::NewRecord;

pub(super) async fn engine() -> Engine {
    let engine = Engine::open_memory().await.expect("engine opens");
    store::organs::ensure_local(&engine.store.pool, "http://protein-entries.test")
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

pub(super) async fn concept(e: &Engine, name: &str) -> String {
    store::concepts::create(&e.store.pool, name, &[])
        .await
        .expect("concept")
}

pub(super) fn query(source: Source, filter: Vec<Predicate>) -> Protein {
    Protein {
        source,
        filter,
        fields: None,
        include: Include::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    }
}

pub(super) async fn capture(
    e: &Engine,
    target: &str,
    amount: &str,
    concept: Option<&str>,
) -> String {
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

pub(super) fn find<'a>(rows: &'a [Value], uid: &str) -> &'a Value {
    rows.iter()
        .find(|row| row["uid"] == uid)
        .expect("the entry should be listed")
}
