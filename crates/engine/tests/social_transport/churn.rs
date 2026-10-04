use super::*;
use engine::wire::ALPN_SOCIAL;
use nucleus::social::PublicRequest;
use std::time::Instant;

#[tokio::test]
async fn authenticated_public_connection_churn_remains_bounded_and_releases_slots() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let engine = Arc::new(Engine::open_memory().await.unwrap());
        let server = Arc::new(Wire::bind_with_discovery(engine, iroh::SecretKey::from_bytes(&[205;32]), Reach::Local, None, false).await.unwrap());
        let serving = server.clone();
        let task = tokio::spawn(async move { serving.serve().await });
        let port = server.endpoint().bound_sockets().into_iter().find(|addr| addr.is_ipv4()).unwrap().port();
        let address = iroh::EndpointAddr::new(server.node_id()).with_ip_addr(([127,0,0,1],port).into());
        let request = serde_json::to_vec(&PublicRequest::DescribeService).unwrap();
        let begin = Instant::now();
        let mut samples = Vec::new();
        let mut sent = 0u64;
        let mut received = 0u64;
        for wave in 0..8u32 {
            let mut workers = tokio::task::JoinSet::new();
            for offset in 0..16 {
                let address = address.clone();
                let request = request.clone();
                workers.spawn(async move {
                    let mut key = [206;32];
                    key[..4].copy_from_slice(&(wave*16+offset).to_le_bytes());
                    let client = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal).clear_address_lookup().secret_key(iroh::SecretKey::from_bytes(&key)).bind().await.unwrap();
                    let begin = Instant::now();
                    let connection = client.connect(address, ALPN_SOCIAL).await.unwrap();
                    let (mut send, mut recv) = connection.open_bi().await.unwrap();
                    send.write_all(&request).await.unwrap();
                    send.finish().unwrap();
                    let reply = recv.read_to_end(nucleus::social::MAX_FRAME_BYTES).await.unwrap();
                    let response: Value = serde_json::from_slice(&reply).unwrap();
                    assert_eq!(response["data"]["descriptor"]["protocol"], "lince.social-service.1");
                    let elapsed = begin.elapsed().as_secs_f64()*1000.0;
                    let stats = connection.stats();
                    connection.close(0u32.into(), b"churn complete");
                    client.close().await;
                    (elapsed, stats.udp_tx.bytes, stats.udp_rx.bytes)
                });
            }
            while let Some(result) = workers.join_next().await {
                let (elapsed, tx, rx) = result.unwrap();
                samples.push(elapsed); sent += tx; received += rx;
            }
        }
        samples.sort_by(f64::total_cmp);
        println!("LINCE_SOCIAL_CONNECTION_REPORT {}", json!({"authenticated_connections":samples.len(),"concurrent_connections":16,"elapsed_seconds":begin.elapsed().as_secs_f64(),"handshake_and_request_p95_ms":samples[121],"client_udp_sent_bytes":sent,"client_udp_received_bytes":received,"scope":"actual loopback authenticated QUIC, new endpoint identity per request, configured global 64/per-peer 8 limits"}));
        task.abort(); server.endpoint().close().await;
    }).await.unwrap();
}
