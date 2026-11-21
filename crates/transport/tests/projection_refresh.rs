// non-lean-yet
// Scope: static QUERIES, fn snapshots, fn publish, fn forecast_rows.
// BeautifulRust (baseline lines):
// - [Clippy probe: clippy::clone_on_ref_ptr] Make shared ownership explicit at retained clones.
//   Sites 130.
// - [Clippy probe: clippy::panic] `panic` should not be present in production code. Sites 24.

#![cfg(test)]

use engine::{Engine, actions::Action};
use nucleus::projection::{Context, Incomplete, Window};
use protein::Protein;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use transport::{ClientMessage, LaneHub, ServerMessage, Session, SyncEvent, SyncEvents};

static QUERIES: AtomicU64 = AtomicU64::new(0);

fn snapshots(messages: Vec<ServerMessage>) -> BTreeMap<String, Vec<Value>> {
    messages
        .into_iter()
        .map(|message| match message {
            ServerMessage::Snapshot { id, rows } => (id, std::sync::Arc::unwrap_or_clone(rows)),
            message => panic!("Unexpected response: {message:?}"),
        })
        .collect()
}

async fn publish(engine: &Engine, context: &Context, incomplete: Option<&Incomplete>) {
    let source = store::projection::revision(&engine.store.pool)
        .await
        .unwrap();
    assert!(
        store::projection::publish_schedule(
            &engine.store,
            context,
            source,
            context.window.from_ms,
            context.window.until_ms,
            incomplete,
            &[],
            &[],
        )
        .await
        .unwrap()
    );
}

async fn forecast_rows(
    engine: &Engine,
    queries: &BTreeMap<String, Protein>,
) -> BTreeMap<String, Vec<Value>> {
    let mut expected = BTreeMap::new();
    for id in ["calendar", "schedule"] {
        expected.insert(
            id.into(),
            protein::execute(&engine.store, &queries[id]).await.unwrap(),
        );
    }
    expected
}

#[tokio::test]
async fn forecast_refresh_preserves_live_status_errors_and_full_invalidation_without_unrelated_queries()
 {
    assert!(nucleus::performance::install(|reading| {
        if let nucleus::performance::Reading::Count("cell-queries", count) = reading {
            QUERIES.fetch_add(count, Ordering::Relaxed);
        }
    }));
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let config =
        engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host("forecast-refresh".into())
            .unwrap();
    store::projection::set_runtime(
        &engine.store.pool,
        config.projection_identity().unwrap().as_str(),
    )
    .await
    .unwrap();
    engine.install_karma_runtime_config(config).unwrap();
    let mut queries: BTreeMap<String, Protein> = BTreeMap::new();
    for index in 0..32 {
        let uid = engine
            .act(
                Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: format!("Field {index}"),
                    body: String::new(),
                    quantity: 1.0,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        queries.insert(
            format!("record-{index}"),
            serde_json::from_value(json!({
                "source":"record", "where":[{"uid_eq":uid}], "fields":["uid", "head", "quantity"]
            }))
            .unwrap(),
        );
    }
    queries.insert(
        "concept".into(),
        serde_json::from_value(json!({"source":"concept"})).unwrap(),
    );
    let now = nucleus::execution::now().timestamp_millis();
    let context = Context {
        actor: None,
        window: Window {
            from_ms: now,
            until_ms: now + 3_600_000,
            timezone: "UTC".into(),
        },
    };
    for source in ["calendar", "schedule"] {
        queries.insert(
            source.into(),
            serde_json::from_value(json!({
                "source": source, "where":[{"projection_window":context.window}]
            }))
            .unwrap(),
        );
    }
    publish(&engine, &context, Some(&Incomplete::Budget {})).await;
    let mut session = Session::local(engine.clone(), Arc::new(LaneHub::new()), "forecast-refresh");
    let mut initial = BTreeMap::new();
    for (id, query) in &queries {
        initial.extend(snapshots(
            session
                .handle(ClientMessage::Subscribe {
                    id: id.clone(),
                    protein: query.clone(),
                })
                .await,
        ));
    }
    assert_eq!(initial.len(), 35);
    assert_eq!(
        forecast_rows(&engine, &queries).await,
        initial
            .iter()
            .filter(|(id, _)| matches!(id.as_str(), "calendar" | "schedule"))
            .map(|(id, rows)| (id.clone(), rows.clone()))
            .collect()
    );

    let mut events = SyncEvents::new(&engine);
    let previous = *engine.watch_query_changes().borrow();
    store::sqlx::query("DELETE FROM projection_window")
        .execute(&engine.store.pool)
        .await
        .unwrap();
    engine.request_projection(context.clone()).await.unwrap();
    let event = tokio::time::timeout(Duration::from_secs(20), events.next(false))
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event, SyncEvent::Projection));
    let current = *engine.watch_query_changes().borrow();
    assert_eq!(current.general, previous.general);
    assert_eq!(current.revision(), previous.revision().wrapping_add(1));
    let before = QUERIES.load(Ordering::Relaxed);
    let ready = snapshots(session.on_sync_event(event).await);
    assert_eq!(QUERIES.load(Ordering::Relaxed) - before, 2);
    assert_eq!(ready, forecast_rows(&engine, &queries).await);
    assert_eq!(ready.len(), 2);
    for rows in ready.values() {
        let status = rows
            .iter()
            .find(|row| row["kind"] == "projection-status")
            .unwrap();
        assert_eq!(status["status"]["kind"], "ready");
    }

    publish(&engine, &context, Some(&Incomplete::UnavailableRuntime {})).await;
    let before = QUERIES.load(Ordering::Relaxed);
    let incomplete = snapshots(session.on_sync_event(SyncEvent::Projection).await);
    assert_eq!(QUERIES.load(Ordering::Relaxed) - before, 2);
    assert_eq!(incomplete, forecast_rows(&engine, &queries).await);
    assert_ne!(incomplete, ready);

    store::sqlx::query("UPDATE projection_window SET incomplete=?")
        .bind(r#"{"kind":"unknown-fixture-status"}"#)
        .execute(&engine.store.pool)
        .await
        .unwrap();
    let before = QUERIES.load(Ordering::Relaxed);
    let failed = session.on_sync_event(SyncEvent::Projection).await;
    assert_eq!(QUERIES.load(Ordering::Relaxed) - before, 2);
    assert_eq!(failed.len(), 2);
    assert!(failed.iter().all(|message| matches!(message,
        ServerMessage::Error { id, .. } if matches!(id.as_str(), "calendar" | "schedule"))));
    publish(&engine, &context, None).await;
    let before = QUERIES.load(Ordering::Relaxed);
    let recovered = snapshots(session.on_sync_event(SyncEvent::Projection).await);
    assert_eq!(QUERIES.load(Ordering::Relaxed) - before, 2);
    assert_eq!(recovered, forecast_rows(&engine, &queries).await);
    assert_eq!(recovered.len(), 2);

    let previous = *engine.watch_query_changes().borrow();
    engine.notify_query_changed();
    let current = *engine.watch_query_changes().borrow();
    assert_eq!(current.projection, previous.projection);
    assert_eq!(current.revision(), previous.revision().wrapping_add(1));
    let event = tokio::time::timeout(Duration::from_secs(1), events.next(false))
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event, SyncEvent::Refresh));
    let before = QUERIES.load(Ordering::Relaxed);
    assert!(session.on_sync_event(event).await.is_empty());
    assert_eq!(QUERIES.load(Ordering::Relaxed) - before, 35);
    let before = QUERIES.load(Ordering::Relaxed);
    let explicit = snapshots(session.refresh().await);
    assert_eq!(QUERIES.load(Ordering::Relaxed) - before, 35);
    assert_eq!(explicit.len(), 35);
    for (id, rows) in initial
        .iter()
        .filter(|(id, _)| !matches!(id.as_str(), "calendar" | "schedule"))
    {
        assert_eq!(&explicit[id], rows);
    }
    for id in ["calendar", "schedule"] {
        assert_eq!(explicit[id], recovered[id]);
    }
    drop(session);
    engine.store.pool.close().await;
}
