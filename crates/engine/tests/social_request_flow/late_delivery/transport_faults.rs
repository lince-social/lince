use super::*;
use engine::wire::{ALPN_SOCIAL, Reach, Wire};
use serde_json::json;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tokio::sync::Notify;

struct LostResponse {
    wire: Arc<Wire>,
    envelope: String,
    entered: Notify,
    lost: AtomicBool,
}

#[async_trait::async_trait]
impl Network for LostResponse {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        let deposit = matches!(&request, PublicRequest::DeliverPrivate { document } if document.envelope.id == self.envelope);
        let result = Network::request(&*self.wire, destination, request).await?;
        if deposit && !self.lost.swap(true, Ordering::SeqCst) {
            self.entered.notify_one();
            std::future::pending::<()>().await;
        }
        Ok(result)
    }
}

async fn wire(engine: Arc<Engine>, seed: u8) -> Arc<Wire> {
    Arc::new(
        Wire::bind_with_discovery(
            engine,
            iroh::SecretKey::from_bytes(&[seed; 32]),
            Reach::Local,
            None,
            false,
        )
        .await
        .unwrap(),
    )
}

fn address(wire: &Wire) -> iroh::EndpointAddr {
    let port = wire
        .endpoint()
        .bound_sockets()
        .into_iter()
        .find(|addr| addr.is_ipv4())
        .unwrap()
        .port();
    iroh::EndpointAddr::new(wire.node_id()).with_ip_addr(([127, 0, 0, 1], port).into())
}

pub(super) async fn assert_once(receiver: &Engine, text: &str) {
    let rows: Vec<String> =
        store::sqlx::query_scalar("SELECT uid FROM record WHERE kind='message' AND body=?")
            .bind(text)
            .fetch_all(&receiver.store.pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        store::records::get(&receiver.store.pool, &rows[0])
            .await
            .unwrap()
            .unwrap()
            .quantity,
        store::exact::one()
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM social_message_event WHERE record_uid=?"
        )
        .bind(&rows[0])
        .fetch_one(&receiver.store.pool)
        .await
        .unwrap(),
        1
    );
}

#[tokio::test]
async fn real_wire_lost_response_host_restart_and_other_mailbox_recover_one_message() {
    let mut fixture = accepted_with_storage(true).await;
    tokio::time::timeout(Duration::from_secs(90), Box::pin(async {
        let sender = wire(fixture.sender.clone(), 238).await;
        let receiver = wire(fixture.receiver.clone(), 237).await;
        let mut servers = Vec::new();
        let mut tasks = Vec::new();
        for seed in [235, 236] {
            let id = iroh::SecretKey::from_bytes(&[seed; 32]).public().to_string();
            let server = wire(fixture.hosts.nodes[&id].clone(), seed).await;
            sender.remember_addr(address(&server));
            receiver.remember_addr(address(&server));
            let serving = server.clone();
            tasks.push(tokio::spawn(async move { serving.serve().await }));
            servers.push(server);
        }
        fixture.sender.attach_social_network(sender.clone());
        fixture.receiver.attach_social_network(receiver.clone());
        let text = "Real transport with durable host restart and independent mailbox recovery";
        let saved = command(&fixture.sender, Command::SendPrivate { conversation: fixture.conversation.clone(), text: text.into() }).await;
        let message = saved["message"].as_str().unwrap();
        due(&fixture.sender).await;
        fixture.sender.social_prepare_messages_once().await.unwrap();
        publication(&fixture.sender).await;
        let body: String = store::sqlx::query_scalar("SELECT body FROM social_private_outbox WHERE record_uid=? LIMIT 1").bind(message).fetch_one(&fixture.sender.store.pool).await.unwrap();
        let document: PrivateDelivery = serde_json::from_str(&body).unwrap();
        let loss = Arc::new(LostResponse { wire: sender.clone(), envelope: document.envelope.id.clone(), entered: Notify::new(), lost: AtomicBool::new(false) });
        fixture.sender.attach_social_network(loss.clone());
        let sending = fixture.sender.clone();
        let worker = tokio::spawn(async move { sending.social_send_private_once().await.unwrap() });
        tokio::time::timeout(Duration::from_secs(15), loss.entered.notified()).await.unwrap();
        worker.abort();
        assert!(worker.await.unwrap_err().is_cancelled());
        let first = fixture.services[0].clone();
        let index = servers.iter().position(|server| server.node_id().to_string() == first).unwrap();
        tasks[index].abort();
        servers[index].endpoint().close().await;
        let host = &fixture.hosts.nodes[&first];
        assert_eq!(store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_service_envelope WHERE id=?").bind(&document.envelope.id).fetch_one(&host.store.pool).await.unwrap(), 1);
        host.store.pool.close().await;
        let directory_index = if first == iroh::SecretKey::from_bytes(&[235; 32]).public().to_string() { 0 } else { 1 };
        let reopened = faults::reopen(fixture._host_directories[directory_index].path()).await;
        assert_eq!(store::sqlx::query_scalar::<_, String>("SELECT body FROM social_service_envelope WHERE id=?").bind(&document.envelope.id).fetch_one(&reopened.store.pool).await.unwrap(), serde_json::to_string(&document).unwrap());
        let seed = if directory_index == 0 { 235 } else { 236 };
        servers[index] = wire(reopened.clone(), seed).await;
        sender.remember_addr(address(&servers[index]));
        receiver.remember_addr(address(&servers[index]));
        let serving = servers[index].clone();
        tasks[index] = tokio::spawn(async move { serving.serve().await });
        let mut nodes = fixture.hosts.nodes.clone();
        nodes.insert(first.clone(), reopened);
        fixture.hosts = Arc::new(Hosts { nodes, offline: Mutex::new(BTreeSet::new()) });
        fixture.sender.attach_social_network(sender.clone());
        for _ in 0..2 { due(&fixture.sender).await; fixture.sender.social_send_private_once().await.unwrap(); }
        let delivery = store::records::get_extension(&fixture.sender.store.pool, message, nucleus::social::requests::DELIVERY_NAMESPACE).await.unwrap().unwrap();
        assert_eq!(delivery["stage"], "mailbox-stored");
        assert!(delivery["receipt"].is_null());
        assert_eq!(store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_private_destination WHERE envelope=? AND state='stored'").bind(&document.envelope.id).fetch_one(&fixture.sender.store.pool).await.unwrap(), 2);
        tasks[index].abort();
        servers[index].endpoint().close().await;
        due(&fixture.receiver).await;
        fixture.receiver.social_collect_private_once().await.unwrap();
        assert_once(&fixture.receiver, text).await;
        due(&fixture.sender).await;
        fixture.sender.social_send_private_once().await.unwrap();
        let delivery = store::records::get_extension(&fixture.sender.store.pool, message, nucleus::social::requests::DELIVERY_NAMESPACE).await.unwrap().unwrap();
        assert_eq!(delivery["stage"], "recipient-durable");
        assert_eq!(delivery["receipt"]["message"], document.envelope.message);
        assert_eq!(store::sqlx::query_scalar::<_, String>("SELECT body FROM social_private_outbox WHERE id=?").bind(&document.envelope.id).fetch_one(&fixture.sender.store.pool).await.unwrap(), body);
        for task in tasks { task.abort(); }
        for server in servers { server.endpoint().close().await; }
        sender.endpoint().close().await;
        receiver.endpoint().close().await;
        for engine in fixture.hosts.nodes.values() { engine.store.pool.close().await; }
        fixture.sender.store.pool.close().await;
        fixture.receiver.store.pool.close().await;
    })).await.unwrap();
}

struct RelayMailbox {
    client: iroh::Endpoint,
    routes: BTreeMap<String, iroh::EndpointAddr>,
}

#[async_trait::async_trait]
impl Network for RelayMailbox {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        let connection = tokio::time::timeout(
            Duration::from_secs(2),
            self.client
                .connect(self.routes[destination].clone(), ALPN_SOCIAL),
        )
        .await
        .map_err(|_| EngineError::Consequence("Relay unavailable".into()))?
        .map_err(|e| EngineError::Consequence(e.to_string()))?;
        let result = tokio::time::timeout(Duration::from_secs(2), async {
            let (mut send, mut recv) = connection
                .open_bi()
                .await
                .map_err(|e| EngineError::Consequence(e.to_string()))?;
            send.write_all(&serde_json::to_vec(&request)?)
                .await
                .map_err(|e| EngineError::Consequence(e.to_string()))?;
            send.finish()
                .map_err(|e| EngineError::Consequence(e.to_string()))?;
            let body = recv
                .read_to_end(request.frame_limit())
                .await
                .map_err(|e| EngineError::Consequence(e.to_string()))?;
            let response: Value = serde_json::from_slice(&body)?;
            if let Some(error) = response["error"].as_str() {
                return Err(EngineError::Consequence(error.into()));
            }
            Ok(response["data"].clone())
        })
        .await
        .map_err(|_| EngineError::Consequence("Relay response unavailable".into()))?;
        connection.close(0u32.into(), b"qualified");
        result
    }
}

#[tokio::test]
async fn real_relay_shutdown_preserves_queued_ciphertext_and_direct_mailbox_delivery() {
    let fixture = accepted_with_storage(true).await;
    tokio::time::timeout(Duration::from_secs(90), Box::pin(async {
        let mut config = iroh_relay::server::ServerConfig::default();
        config.relay = Some(iroh_relay::server::RelayConfig::new(([127, 0, 0, 1], 0)));
        let relay = iroh_relay::server::Server::spawn(config).await.unwrap();
        let url: iroh::RelayUrl = format!("http://{}", relay.http_addr().unwrap()).parse().unwrap();
        let endpoint = iroh::Endpoint::builder(iroh::endpoint::presets::N0).clear_address_lookup().clear_ip_transports().relay_mode(iroh::RelayMode::custom([url.clone()])).secret_key(iroh::SecretKey::from_bytes(&[235; 32])).alpns(vec![ALPN_SOCIAL.to_vec()]).bind().await.unwrap();
        endpoint.online().await;
        let direct_id = iroh::SecretKey::from_bytes(&[236; 32]).public().to_string();
        let direct = wire(fixture.hosts.nodes[&direct_id].clone(), 236).await;
        let serving = direct.clone();
        let direct_task = tokio::spawn(async move { serving.serve().await });
        let relay_host = fixture.hosts.nodes[&endpoint.id().to_string()].clone();
        let serving = endpoint.clone();
        let relay_task = tokio::spawn(async move {
            while let Some(incoming) = serving.accept().await {
                let connection = incoming.await.unwrap();
                let host = relay_host.clone();
                let node = serving.id().to_string();
                tokio::spawn(async move {
                    let (mut send, mut recv) = connection.accept_bi().await.unwrap();
                    let bytes = recv.read_to_end(nucleus::social::MAX_PRIVATE_FRAME_BYTES).await.unwrap();
                    let result = host.social_public_frame(&connection.remote_id().to_string(), &node, &bytes, nucleus::execution::now().timestamp()).await;
                    let response = match result { Ok(value) => json!({"data":value}), Err(error) => json!({"error":error.to_string()}) };
                    send.write_all(&serde_json::to_vec(&response).unwrap()).await.unwrap();
                    send.finish().unwrap();
                    let _ = send.stopped().await;
                    connection.close(0u32.into(), b"complete");
                });
            }
        });
        let mut routes = BTreeMap::new();
        routes.insert(endpoint.id().to_string(), endpoint.addr());
        routes.insert(direct_id, address(&direct));
        let client = iroh::Endpoint::builder(iroh::endpoint::presets::N0).clear_address_lookup().relay_mode(iroh::RelayMode::custom([url])).bind().await.unwrap();
        client.online().await;
        let network = Arc::new(RelayMailbox { client, routes });
        fixture.sender.attach_social_network(network.clone());
        fixture.receiver.attach_social_network(network.clone());
        let saved = command(&fixture.sender, Command::SendPrivate { conversation: fixture.conversation.clone(), text: "Offline delivery survives a real relay outage".into() }).await;
        let message = saved["message"].as_str().unwrap();
        due(&fixture.sender).await;
        fixture.sender.social_prepare_messages_once().await.unwrap();
        publication(&fixture.sender).await;
        due(&fixture.sender).await;
        fixture.sender.social_send_private_once().await.unwrap();
        assert_eq!(store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_private_destination d JOIN social_private_outbox o ON o.id=d.envelope WHERE o.record_uid=? AND d.state='stored'").bind(message).fetch_one(&fixture.sender.store.pool).await.unwrap(), 2);
        relay.shutdown().await.unwrap();
        due(&fixture.receiver).await;
        fixture.receiver.social_collect_private_once().await.unwrap();
        assert_once(&fixture.receiver, "Offline delivery survives a real relay outage").await;
        due(&fixture.sender).await;
        fixture.sender.social_send_private_once().await.unwrap();
        let status = store::records::get_extension(&fixture.sender.store.pool, message, nucleus::social::requests::DELIVERY_NAMESPACE).await.unwrap().unwrap();
        assert_eq!(status["stage"], "recipient-durable");
        relay_task.abort(); direct_task.abort();
        endpoint.close().await; direct.endpoint().close().await; network.client.close().await;
        fixture.sender.store.pool.close().await; fixture.receiver.store.pool.close().await;
        for engine in fixture.hosts.nodes.values() { engine.store.pool.close().await; }
    })).await.unwrap();
}
