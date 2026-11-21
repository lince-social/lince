use engine::Engine;
use nucleus::RecordKind;
use protein::{Include, Protein, Source};
use store::records::NewRecord;

pub(super) async fn engine() -> Engine {
    let engine = Engine::open_memory().await.expect("engine opens");
    store::organs::ensure_local(&engine.store.pool, "http://protein-sources.test")
        .await
        .expect("local Organ");
    engine
}

pub(super) async fn plain(e: &Engine, slug: &str, quantity: f64) -> String {
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

pub(super) fn protein(source: Source) -> Protein {
    Protein {
        source,
        filter: vec![],
        fields: None,
        include: Include::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    }
}
