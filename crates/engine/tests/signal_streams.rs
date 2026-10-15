use engine::{
    Engine,
    signal_streams::{MAX_PAYLOAD, Metadata},
};
use std::sync::Arc;

async fn fixture(kind: &str) -> (Engine, String) {
    let engine = Engine::open_memory().await.unwrap();
    store::organs::ensure_local(&engine.store.pool, "http://stream.test")
        .await
        .unwrap();
    let record = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: Some("live-test".into()),
                kind: nucleus::RecordKind::Signal,
                head: "Live test".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine
        .configure_stream(&record, Some(kind), "", None, None)
        .await
        .unwrap();
    (engine, record)
}

fn metadata(epoch: &str, sequence: u64, kind: &str) -> Metadata {
    Metadata {
        epoch: epoch.into(),
        sequence,
        captured_at: nucleus::execution::now().timestamp_millis(),
        content_type: kind.into(),
    }
}

#[tokio::test]
async fn demand_shares_latest_value_and_final_drop_clears_payload_and_epoch() {
    let (engine, signal) = fixture("scalar").await;
    let channel = engine.stream_channel(&signal, None).await.unwrap();
    let producer = channel.publisher().unwrap();
    let idle = producer.snapshot().epoch;
    assert!(
        !producer
            .publish(metadata(&idle, 1, "text/decimal"), b"12".to_vec())
            .await
            .unwrap()
    );
    assert!(channel.snapshot().value.is_none());
    let first = channel.subscribe().unwrap();
    let second = channel.subscribe().unwrap();
    let epoch = producer.snapshot().epoch;
    assert_eq!(producer.snapshot().consumers, 2);
    for sequence in 1..=1000 {
        producer
            .publish(
                metadata(&epoch, sequence, "text/decimal"),
                sequence.to_string().into_bytes(),
            )
            .await
            .unwrap();
    }
    let a = first.updates.borrow().value.clone().unwrap();
    let b = second.updates.borrow().value.clone().unwrap();
    assert!(Arc::ptr_eq(&a, &b));
    assert_eq!(&*a.bytes, b"1000");
    assert_eq!(
        engine.signal_streams.scalar(&signal).unwrap().to_string(),
        "1000"
    );
    drop(first);
    assert_eq!(channel.snapshot().consumers, 1);
    drop(second);
    assert_eq!(channel.snapshot().consumers, 0);
    assert!(channel.snapshot().value.is_none());
    assert!(engine.signal_streams.scalar(&signal).is_err());
    assert_ne!(channel.snapshot().epoch, epoch);
    let _reopened = channel.subscribe().unwrap();
    assert!(
        producer
            .publish(metadata(&epoch, 1001, "text/decimal"), b"13".to_vec())
            .await
            .is_err()
    );
    assert!(channel.snapshot().value.is_none());
}

#[tokio::test]
async fn values_never_become_facts_or_configuration_and_old_handles_are_closed() {
    let (engine, signal) = fixture("scalar").await;
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    let channel = engine.stream_channel(&signal, None).await.unwrap();
    let _consumer = channel.subscribe().unwrap();
    let producer = channel.publisher().unwrap();
    producer
        .publish(
            metadata(&producer.snapshot().epoch, 1, "text/decimal"),
            b"937".to_vec(),
        )
        .await
        .unwrap();
    let after: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fact")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        store::records::get(&engine.store.pool, &signal)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .to_string(),
        "0"
    );
    let mut connection = engine.store.pool.acquire().await.unwrap();
    let config = store::signal_streams::get_on(&mut connection, &signal)
        .await
        .unwrap()
        .unwrap();
    assert!(!serde_json::to_string(&config).unwrap().contains("937"));
    drop(connection);
    let restarted = Engine::new(engine.store.clone()).await.unwrap();
    assert!(
        restarted
            .stream_channel(&signal, None)
            .await
            .unwrap()
            .snapshot()
            .value
            .is_none()
    );
    assert!(
        engine
            .configure_stream(&signal, Some("jpeg"), "", Some(999), None)
            .await
            .is_err()
    );
    engine
        .configure_stream(&signal, Some("jpeg"), "", Some(1), None)
        .await
        .unwrap();
    assert!(channel.is_closed());
    assert!(channel.snapshot().value.is_none());
    assert!(channel.subscribe().is_err());
    assert!(
        producer
            .publish(
                metadata(&channel.snapshot().epoch, 2, "text/decimal"),
                b"5".to_vec()
            )
            .await
            .is_err()
    );
    assert!(
        engine
            .stream_channel(&signal, Some("unauthorized"))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn malformed_frames_replays_types_and_competing_producers_are_rejected() {
    let (engine, signal) = fixture("jpeg").await;
    let channel = engine.stream_channel(&signal, None).await.unwrap();
    let _consumer = channel.subscribe().unwrap();
    let producer = channel.publisher().unwrap();
    assert!(channel.publisher().is_err());
    let epoch = channel.snapshot().epoch;
    assert!(
        producer
            .publish(metadata(&epoch, 1, "image/jpeg"), vec![1; MAX_PAYLOAD + 1])
            .await
            .is_err()
    );
    assert!(
        producer
            .publish(metadata(&epoch, 1, "image/jpeg"), b"not JPEG".to_vec())
            .await
            .is_err()
    );
    assert!(
        producer
            .publish(metadata(&epoch, 1, "text/decimal"), b"3".to_vec())
            .await
            .is_err()
    );
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
        .encode(&[255, 0, 0], 1, 1, image::ExtendedColorType::Rgb8)
        .unwrap();
    producer
        .publish(metadata(&epoch, 1, "image/jpeg"), jpeg.clone())
        .await
        .unwrap();
    assert!(
        producer
            .publish(metadata(&epoch, 1, "image/jpeg"), jpeg)
            .await
            .is_err()
    );
    let value = channel.snapshot().value.unwrap();
    let (width, height, pixels) = engine::signal_streams::decode_jpeg(&value.bytes).unwrap();
    assert_eq!((width, height, pixels.len()), (1, 1, 4));
    drop(producer);
    assert!(channel.snapshot().value.is_none());
    assert!(channel.publisher().is_ok());
}

#[tokio::test]
async fn consumer_and_signal_capacity_are_bounded() {
    let (engine, signal) = fixture("scalar").await;
    let channel = engine.stream_channel(&signal, None).await.unwrap();
    let consumers: Vec<_> = (0..engine::signal_streams::MAX_CONSUMERS)
        .map(|_| channel.subscribe().unwrap())
        .collect();
    assert!(channel.subscribe().is_err());
    drop(consumers);
    assert_eq!(channel.snapshot().consumers, 0);
}

#[tokio::test]
async fn upstream_connection_opens_on_demand_closes_when_idle_and_reopens_fresh() {
    use futures::{SinkExt, StreamExt};
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };
    let (engine, signal) = fixture("scalar").await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let active = Arc::new(AtomicUsize::new(0));
    let count = accepted.clone();
    let running = active.clone();
    let server = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let count = count.clone();
            let running = running.clone();
            tokio::spawn(async move {
                let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
                count.fetch_add(1, Ordering::SeqCst);
                running.fetch_add(1, Ordering::SeqCst);
                let mut interval = tokio::time::interval(Duration::from_millis(20));
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            if socket.send(tokio_tungstenite::tungstenite::Message::Text("42".into())).await.is_err() { break; }
                        }
                        incoming = socket.next() => {
                            if !matches!(incoming, Some(Ok(tokio_tungstenite::tungstenite::Message::Ping(_) | tokio_tungstenite::tungstenite::Message::Pong(_)))) { break; }
                        }
                    }
                }
                running.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    engine
        .configure_stream(
            &signal,
            Some("scalar"),
            &format!("ws://{address}"),
            Some(1),
            None,
        )
        .await
        .unwrap();
    let channel = engine.stream_channel(&signal, None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(accepted.load(Ordering::SeqCst), 0);
    let first = channel.subscribe().unwrap();
    let second = channel.subscribe().unwrap();
    wait_until(|| channel.snapshot().value.is_some()).await;
    assert_eq!(accepted.load(Ordering::SeqCst), 1);
    let epoch = channel.snapshot().epoch;
    drop(first);
    assert_eq!(channel.snapshot().consumers, 1);
    assert_eq!(active.load(Ordering::SeqCst), 1);
    drop(second);
    wait_until(|| active.load(Ordering::SeqCst) == 0).await;
    assert!(channel.snapshot().value.is_none());
    let reopened = channel.subscribe().unwrap();
    wait_until(|| channel.snapshot().value.is_some()).await;
    assert_eq!(accepted.load(Ordering::SeqCst), 2);
    assert_ne!(channel.snapshot().epoch, epoch);
    assert_eq!(
        engine.signal_streams.scalar(&signal).unwrap().to_string(),
        "42"
    );
    drop(reopened);
    wait_until(|| active.load(Ordering::SeqCst) == 0).await;
    engine
        .configure_stream(&signal, None, "", Some(2), None)
        .await
        .unwrap();
    server.abort();
}

async fn wait_until(done: impl Fn() -> bool) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !done() {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
