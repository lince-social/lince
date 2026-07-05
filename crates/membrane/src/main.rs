//! Run the Cell surface. Opens (or creates) the Cell database, seeds a few
//! demo tasks the first time so the focus queue has something to show, and
//! serves the board on http://127.0.0.1:4600.

use std::sync::Arc;

use engine::actions::Action;
use engine::Engine;
use membrane::{default_db_url, router, Surface};
use nucleus::RecordKind;
use store::Store;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("LINCE_DB").unwrap_or_else(|_| default_db_url());
    let store = Store::open(&url).await?;
    let engine = Arc::new(Engine::new(store).await?);
    seed_if_empty(&engine).await?;

    let addr = std::env::var("LINCE_ADDR").unwrap_or_else(|_| "127.0.0.1:4600".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("Lince Cell surface on http://{addr}");
    axum::serve(listener, router(Surface::new(engine))).await?;
    Ok(())
}

/// First-run demo data: three ordered morning tasks, so the pilot shows a live
/// focus queue immediately. Ordering is links (`@before`), never timestamps.
async fn seed_if_empty(engine: &Engine) -> Result<(), Box<dyn std::error::Error>> {
    let existing = protein::execute(&engine.store, &protein::focus_queue("before")).await?;
    if !existing.is_empty() {
        return Ok(());
    }
    engine.act(Action::CreateConcept { name: "before".into(), parents: vec![] }, None).await?;
    for (slug, head) in [
        ("task.stretch", "Stretch"),
        ("task.coffee", "Make coffee"),
        ("task.review", "Review the day's plan"),
    ] {
        engine
            .act(
                Action::CreateRecord {
                    slug: Some(slug.into()),
                    kind: RecordKind::Plain,
                    head: head.into(),
                    body: String::new(),
                    quantity: -1.0,
                },
                None,
            )
            .await?;
    }
    engine
        .act(
            Action::AddLink {
                from: "task.stretch".into(),
                kind: "before".into(),
                to: "task.coffee".into(),
                quantity: None,
            },
            None,
        )
        .await?;
    engine
        .act(
            Action::AddLink {
                from: "task.coffee".into(),
                kind: "before".into(),
                to: "task.review".into(),
                quantity: None,
            },
            None,
        )
        .await?;
    Ok(())
}
