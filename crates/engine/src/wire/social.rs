use super::*;
use nucleus::social::{MAX_PRIVATE_FRAME_BYTES, PublicRequest};
use serde_json::{Value, json};

#[async_trait::async_trait]
impl crate::social::Network for Wire {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        let id = destination
            .parse::<EndpointId>()
            .map_err(|_| EngineError::Consequence("Invalid selected service endpoint".into()))?;
        let body = serde_json::to_vec(&request)?;
        let frame_limit = request.frame_limit();
        if body.len() > frame_limit {
            return Err(EngineError::Consequence(
                "The social request is too large".into(),
            ));
        }
        let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let connection = self
                .endpoint
                .connect(EndpointAddr::new(id), ALPN_SOCIAL)
                .await
                .map_err(|_| {
                    EngineError::Consequence("The selected service is unavailable".into())
                })?;
            let result = async {
                let (mut send, mut recv) = connection
                    .open_bi()
                    .await
                    .map_err(|e| EngineError::Consequence(e.to_string()))?;
                send.write_all(&body)
                    .await
                    .map_err(|e| EngineError::Consequence(e.to_string()))?;
                send.finish()
                    .map_err(|e| EngineError::Consequence(e.to_string()))?;
                let bytes = recv.read_to_end(frame_limit).await.map_err(|_| {
                    EngineError::Consequence("The social reply is too large or incomplete".into())
                })?;
                let data: Value = serde_json::from_slice(&bytes)?;
                if let Some(error) = data["error"].as_str() {
                    return Err(EngineError::Consequence(error.into()));
                }
                Ok(data["data"].clone())
            }
            .await;
            connection.close(0u32.into(), b"social request complete");
            result
        })
        .await;
        result.map_err(|_| {
            EngineError::Consequence("The selected service did not reply in time".into())
        })?
    }
}

impl Wire {
    pub(super) async fn serve_social_connection(
        &self,
        connection: Connection,
    ) -> Result<(), EngineError> {
        let work = async {
            let (mut send, mut recv) = connection
                .accept_bi()
                .await
                .map_err(|e| EngineError::Consequence(e.to_string()))?;
            let bytes = recv
                .read_to_end(MAX_PRIVATE_FRAME_BYTES)
                .await
                .map_err(|_| EngineError::Consequence("The social request is too large".into()))?;
            let response = match self
                .engine
                .social_public_frame(
                    &connection.remote_id().to_string(),
                    &self.node_id().to_string(),
                    &bytes,
                    nucleus::execution::now().timestamp(),
                )
                .await
            {
                Ok(data) => json!({"data":data}),
                Err(_) => {
                    json!({"error":"This service refused the request. Check its roles, limits and the publication authority."})
                }
            };
            let body = serde_json::to_vec(&response)?;
            if body.len() > MAX_PRIVATE_FRAME_BYTES {
                return Err(EngineError::Consequence(
                    "The social reply is too large".into(),
                ));
            }
            send.write_all(&body)
                .await
                .map_err(|e| EngineError::Consequence(e.to_string()))?;
            send.finish()
                .map_err(|e| EngineError::Consequence(e.to_string()))?;
            send.stopped()
                .await
                .map_err(|e| EngineError::Consequence(e.to_string()))?;
            Ok(())
        };
        let result = tokio::time::timeout(std::time::Duration::from_secs(10), work).await;
        connection.close(0u32.into(), b"social request complete");
        result.map_err(|_| EngineError::Consequence("Social request timed out".into()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Arc, time::Duration};

    async fn slots(wire: &Wire, available: usize) {
        tokio::time::timeout(Duration::from_secs(3), async {
            while wire.connection_slots.available_permits() != available {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn public_social_connection_flood_enforces_peer_and_global_caps_then_recovers() {
        tokio::time::timeout(Duration::from_secs(30), async {
            let engine = Arc::new(crate::Engine::open_memory().await.unwrap());
            let server = Arc::new(
                Wire::bind_with_discovery(
                    engine,
                    SecretKey::from_bytes(&[200; 32]),
                    Reach::Local,
                    None,
                    false,
                )
                .await
                .unwrap(),
            );
            let serving = server.clone();
            let task = tokio::spawn(async move { serving.serve().await });
            let port = server
                .endpoint
                .bound_sockets()
                .into_iter()
                .find(|addr| addr.is_ipv4())
                .unwrap()
                .port();
            let address =
                EndpointAddr::new(server.node_id()).with_ip_addr(([127, 0, 0, 1], port).into());
            let mut clients = Vec::new();
            let mut connections = Vec::new();
            for peer in 0..8u8 {
                let client = Endpoint::builder(iroh::endpoint::presets::Minimal)
                    .clear_address_lookup()
                    .secret_key(SecretKey::from_bytes(&[201 + peer; 32]))
                    .bind()
                    .await
                    .unwrap();
                for _ in 0..8 {
                    connections.push(client.connect(address.clone(), ALPN_SOCIAL).await.unwrap());
                }
                slots(&server, 64 - (peer as usize + 1) * 8).await;
                if peer == 0 {
                    if let Ok(excess) = client.connect(address.clone(), ALPN_SOCIAL).await {
                        tokio::time::timeout(Duration::from_secs(3), excess.closed())
                            .await
                            .unwrap();
                    }
                    slots(&server, 56).await;
                    assert_eq!(
                        server
                            .open_per_peer
                            .lock()
                            .unwrap()
                            .values()
                            .copied()
                            .sum::<usize>(),
                        8
                    );
                }
                clients.push(client);
            }
            assert_eq!(
                server
                    .open_per_peer
                    .lock()
                    .unwrap()
                    .values()
                    .copied()
                    .sum::<usize>(),
                64
            );
            let stranger = Endpoint::builder(iroh::endpoint::presets::Minimal)
                .clear_address_lookup()
                .bind()
                .await
                .unwrap();
            assert!(
                stranger
                    .connect(address.clone(), ALPN_SOCIAL)
                    .await
                    .is_err()
            );
            connections
                .pop()
                .unwrap()
                .close(0u32.into(), b"release a slot");
            slots(&server, 1).await;
            let replacement = stranger.connect(address, ALPN_SOCIAL).await.unwrap();
            slots(&server, 0).await;
            replacement.close(0u32.into(), b"done");
            for connection in connections {
                connection.close(0u32.into(), b"done");
            }
            slots(&server, 64).await;
            assert!(server.open_per_peer.lock().unwrap().is_empty());
            task.abort();
            for client in clients {
                client.close().await;
            }
            stranger.close().await;
            server.endpoint.close().await;
        })
        .await
        .unwrap();
    }
}
