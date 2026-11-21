#![cfg(test)]

use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering::Relaxed},
    },
    time::Duration,
};

use engine::{Engine, actions::Action, karma_runtime::KarmaDeadlineDirectorConfig};
use nucleus::{
    RecordKind,
    execution::now,
    performance::{Reading, install},
    projection::{Context, Incomplete, Window},
};
use protein::{Protein, execute};
use serde_json::{Value, from_value, json};
use store::{
    projection::{publish_schedule, revision, set_runtime},
    sqlx::query,
};
use tokio::time::timeout;
use transport::{ClientMessage, LaneHub, ServerMessage, Session, SyncEvent, SyncEvents};

static QUERIES: AtomicU64 = AtomicU64::new(0);
const FORECASTS: [&str; 2] = ["calendar", "schedule"];

type Snapshots = BTreeMap<String, Arc<Vec<Value>>>;

fn snapshots(messages: Vec<ServerMessage>) -> Snapshots {
    messages
        .into_iter()
        .map(|message| match message {
            ServerMessage::Snapshot { id, rows } => (id, rows),
            message => panic!("Unexpected response: {message:?}"),
        })
        .collect()
}

async fn publish(engine: &Engine, context: &Context, incomplete: Option<&Incomplete>) {
    let source = revision(&engine.store.pool)
        .await
        .expect("read the source revision");
    assert!(
        publish_schedule(
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
        .expect("publish the projection fixture")
    );
}

async fn assert_forecasts(
    engine: &Engine,
    queries: &BTreeMap<String, Protein>,
    actual: &Snapshots,
) {
    for id in FORECASTS {
        let protein = queries.get(id).expect("forecast query is registered");
        let expected = execute(&engine.store, protein)
            .await
            .expect("run the reference query");
        let rows = actual.get(id).expect("forecast snapshot is present");
        assert_eq!(rows.as_slice(), expected, "forecast {id}");
    }
}

async fn counted_event(session: &mut Session, event: SyncEvent, count: u64) -> Vec<ServerMessage> {
    let before = QUERIES.load(Relaxed);
    let messages = session.on_sync_event(event).await;
    assert_eq!(QUERIES.load(Relaxed) - before, count);
    messages
}

#[tokio::test]
async fn forecast_refresh_preserves_live_status_errors_and_full_invalidation_without_unrelated_queries()
 {
    assert!(install(|reading| {
        if let Reading::Count("cell-queries", count) = reading {
            QUERIES.fetch_add(count, Relaxed);
        }
    }));
    let engine = Arc::new(Engine::open_memory().await.expect("open the Cell"));
    let config = KarmaDeadlineDirectorConfig::for_host("forecast-refresh".into())
        .expect("build the projection runtime configuration");
    set_runtime(
        &engine.store.pool,
        config
            .projection_identity()
            .expect("projection runtime identity")
            .as_str(),
    )
    .await
    .expect("set the runtime identity");
    engine
        .install_karma_runtime_config(config)
        .expect("install the projection runtime");
    let mut queries: BTreeMap<String, Protein> = BTreeMap::new();
    for index in 0..32 {
        let uid = engine
            .act(
                Action::CreateRecord {
                    slug: None,
                    kind: RecordKind::Plain,
                    head: format!("Field {index}"),
                    body: String::new(),
                    quantity: 1.0,
                },
                None,
            )
            .await
            .expect("create a Record for the unrelated query")
            .created
            .expect("created Record uid");
        queries.insert(
            format!("record-{index}"),
            from_value(json!({
                "source":"record", "where":[{"uid_eq":uid}], "fields":["uid", "head", "quantity"]
            }))
            .expect("record query"),
        );
    }
    queries.insert(
        "concept".into(),
        from_value(json!({"source":"concept"})).expect("concept query"),
    );
    let now = now().timestamp_millis();
    let context = Context {
        actor: None,
        window: Window {
            from_ms: now,
            until_ms: now + 3_600_000,
            timezone: "UTC".into(),
        },
    };
    for source in FORECASTS {
        queries.insert(
            source.into(),
            from_value(json!({
                "source": source, "where":[{"projection_window":context.window}]
            }))
            .expect("forecast query"),
        );
    }
    publish(&engine, &context, Some(&Incomplete::Budget {})).await;
    let mut session = Session::local(
        Arc::clone(&engine),
        Arc::new(LaneHub::new()),
        "forecast-refresh",
    );
    let mut initial = Snapshots::new();
    for (id, protein) in &queries {
        initial.extend(snapshots(
            session
                .handle(ClientMessage::Subscribe {
                    id: id.clone(),
                    protein: protein.clone(),
                })
                .await,
        ));
    }
    assert_eq!(initial.len(), 35);
    assert_forecasts(&engine, &queries, &initial).await;

    let mut events = SyncEvents::new(&engine);
    let previous = *engine.watch_query_changes().borrow();
    query("DELETE FROM projection_window")
        .execute(&engine.store.pool)
        .await
        .expect("invalidate the stored forecast windows");
    engine
        .request_projection(context.clone())
        .await
        .expect("request fresh projections");
    let event = timeout(Duration::from_secs(20), events.next(false))
        .await
        .expect("projection completed before the deadline")
        .expect("projection watcher is active");
    assert!(matches!(event, SyncEvent::Projection));
    let current = *engine.watch_query_changes().borrow();
    assert_eq!(current.general, previous.general);
    assert_eq!(current.revision(), previous.revision().wrapping_add(1));
    let ready = snapshots(counted_event(&mut session, event, 2).await);
    assert_forecasts(&engine, &queries, &ready).await;
    assert_eq!(ready.len(), FORECASTS.len());
    for rows in ready.values() {
        let status = rows
            .iter()
            .find(|row| row.get("kind").and_then(Value::as_str) == Some("projection-status"))
            .expect("forecast contains a projection status");
        assert_eq!(
            status.pointer("/status/kind").and_then(Value::as_str),
            Some("ready")
        );
    }

    publish(&engine, &context, Some(&Incomplete::UnavailableRuntime {})).await;
    let incomplete = snapshots(counted_event(&mut session, SyncEvent::Projection, 2).await);
    assert_forecasts(&engine, &queries, &incomplete).await;
    assert_eq!(incomplete.len(), FORECASTS.len());
    assert_ne!(incomplete, ready);

    query("UPDATE projection_window SET incomplete=?")
        .bind(r#"{"kind":"unknown-fixture-status"}"#)
        .execute(&engine.store.pool)
        .await
        .expect("store the malformed forecast status");
    let failed = counted_event(&mut session, SyncEvent::Projection, 2).await;
    assert_eq!(failed.len(), FORECASTS.len());
    assert!(failed.iter().all(|message| matches!(message,
        ServerMessage::Error { id, .. } if FORECASTS.contains(&id.as_str()))));
    publish(&engine, &context, None).await;
    let recovered = snapshots(counted_event(&mut session, SyncEvent::Projection, 2).await);
    assert_forecasts(&engine, &queries, &recovered).await;
    assert_eq!(recovered.len(), FORECASTS.len());

    let previous = *engine.watch_query_changes().borrow();
    engine.notify_query_changed();
    let current = *engine.watch_query_changes().borrow();
    assert_eq!(current.projection, previous.projection);
    assert_eq!(current.revision(), previous.revision().wrapping_add(1));
    let event = timeout(Duration::from_secs(1), events.next(false))
        .await
        .expect("general refresh arrives before the deadline")
        .expect("query watcher is active");
    assert!(matches!(event, SyncEvent::Refresh));
    assert!(counted_event(&mut session, event, 35).await.is_empty());
    let before = QUERIES.load(Relaxed);
    let explicit = snapshots(session.refresh().await);
    assert_eq!(QUERIES.load(Relaxed) - before, 35);
    assert_eq!(explicit.len(), 35);
    for (id, rows) in initial
        .iter()
        .filter(|(id, _)| !FORECASTS.contains(&id.as_str()))
    {
        assert_eq!(explicit.get(id), Some(rows));
    }
    for id in FORECASTS {
        assert_eq!(explicit.get(id), recovered.get(id));
    }
    drop(session);
    engine.store.pool.close().await;
}
