//! Part IV completion: cycle warning on link creation for order-like kinds
//! (`@precedes`, `@before`, `@order`, or their descendants). The action still
//! succeeds — the loop is surfaced as a warning, not an error.

use engine::Engine;
use engine::actions::Action;
use nucleus::RecordKind;
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
            quantity: store::exact::from_f64(-1.0),
        },
    )
    .await
    .expect("record")
    .uid
}

fn add_link(from: &str, kind: &str, to: &str) -> Action {
    Action::AddLink {
        from: from.into(),
        kind: kind.into(),
        to: to.into(),
        quantity: None,
    }
}

#[tokio::test]
async fn closing_an_order_loop_warns_but_still_saves() {
    let e = engine().await;
    plain(&e, "a").await;
    plain(&e, "b").await;
    store::concepts::create(&e.store.pool, "before", &[])
        .await
        .unwrap();

    let out = e.act(add_link("a", "before", "b"), None).await.unwrap();
    assert!(out.warnings.is_empty(), "a chain is not a loop");

    let out = e.act(add_link("b", "before", "a"), None).await.unwrap();
    assert_eq!(out.warnings.len(), 1, "closing the loop warns");
    assert!(out.warnings[0].contains("loop"));
    assert!(out.created.is_some(), "warned, not rejected");
}

#[tokio::test]
async fn descendants_of_precedes_are_order_like_and_other_kinds_are_not() {
    let e = engine().await;
    plain(&e, "a").await;
    plain(&e, "b").await;
    let precedes = store::concepts::create(&e.store.pool, "precedes", &[])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "blocks-softly", &[&precedes])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "needs", &[])
        .await
        .unwrap();

    // needs-loops are legal structure (a recipe may be mutual): no warning.
    e.act(add_link("a", "needs", "b"), None).await.unwrap();
    let out = e.act(add_link("b", "needs", "a"), None).await.unwrap();
    assert!(out.warnings.is_empty(), "non-order kinds never warn");

    // a child of @precedes inherits order-likeness through the DAG.
    e.act(add_link("a", "blocks-softly", "b"), None)
        .await
        .unwrap();
    let out = e
        .act(add_link("b", "blocks-softly", "a"), None)
        .await
        .unwrap();
    assert_eq!(out.warnings.len(), 1);
}
