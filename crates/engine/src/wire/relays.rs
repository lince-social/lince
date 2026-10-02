use crate::EngineError;
use iroh::RelayUrl;
use serde_json::Value;
use std::collections::HashSet;

pub(super) fn endpoint_builder(
    reach: super::Reach,
    relays: &[RelayUrl],
) -> iroh::endpoint::Builder {
    let builder = match reach {
        super::Reach::Internet => iroh::Endpoint::builder(iroh::endpoint::presets::N0),
        super::Reach::Relay => {
            iroh::Endpoint::builder(iroh::endpoint::presets::N0).clear_ip_transports()
        }
        super::Reach::Local => iroh::Endpoint::builder(iroh::endpoint::presets::Minimal),
    };
    if reach != super::Reach::Local && !relays.is_empty() {
        builder.relay_mode(iroh::RelayMode::custom(relays.iter().cloned()))
    } else {
        builder
    }
}

pub fn configured_relays(fields: Option<&Value>) -> Result<Vec<RelayUrl>, EngineError> {
    let invalid = || {
        EngineError::Consequence("Choose at most eight distinct root HTTPS relay URLs, without credentials, spaces, paths, query options or fragments".into())
    };
    let Some(fields) = fields else {
        return Ok(Vec::new());
    };
    if !fields.is_object() {
        return Err(invalid());
    }
    let Some(values) = fields.get("relays") else {
        return Ok(Vec::new());
    };
    let values = values.as_array().ok_or_else(invalid)?;
    if values.len() > 8 {
        return Err(invalid());
    }
    let mut seen = HashSet::new();
    let mut relays = Vec::with_capacity(values.len());
    for value in values {
        let text = value.as_str().ok_or_else(invalid)?;
        if text.len() > 1024 || text.chars().any(|ch| ch.is_whitespace() || ch.is_control() || ch == '\\') {
            return Err(invalid());
        }
        let (scheme, authority) = text.split_once("://").ok_or_else(invalid)?;
        if !scheme.eq_ignore_ascii_case("https") || authority.contains('@') || authority.split_once('/').is_some_and(|(_, path)| !path.is_empty()) {
            return Err(invalid());
        }
        let relay: RelayUrl = text.parse().map_err(|_| invalid())?;
        if relay.scheme() != "https"
            || relay.host_str().is_none()
            || !relay.username().is_empty()
            || relay.password().is_some()
            || relay.path() != "/"
            || relay.query().is_some()
            || relay.fragment().is_some()
            || relay.port() == Some(0)
        {
            return Err(invalid());
        }
        let identity = (
            relay.host_str().unwrap().trim_end_matches('.').to_owned(),
            relay.port_or_known_default(),
        );
        if !seen.insert(identity) {
            return Err(invalid());
        }
        relays.push(relay);
    }
    Ok(relays)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn relay_selection_rejects_ignored_options_and_canonical_duplicates() {
        assert!(configured_relays(None).unwrap().is_empty());
        assert!(configured_relays(Some(&json!({}))).unwrap().is_empty());
        let selected = configured_relays(Some(
            &json!({"relays":["https://RELAY.example:443", "https://second.example:9443/"]}),
        ))
        .unwrap();
        assert_eq!(selected[0].as_str(), "https://relay.example/");
        assert_eq!(selected[1].as_str(), "https://second.example:9443/");
        for fields in [
            json!([]),
            json!({"relays":null}),
            json!({"relays":"https://relay.example"}),
            json!({"relays":[{"url":"https://relay.example", "insecure":true}]}),
            json!({"relays":["http://relay.example"]}),
            json!({"relays":["https://user:secret@relay.example"]}),
            json!({"relays":["https://@relay.example"]}),
            json!({"relays":["https://relay.example/../"]}),
            json!({"relays":["\u{0001}https://relay.example"]}),
            json!({"relays":["https:\\relay.example"]}),
            json!({"relays":["https://relay.example/path"]}),
            json!({"relays":["https://relay.example?option=true"]}),
            json!({"relays":["https://relay.example#part"]}),
            json!({"relays":[" https://relay.example"]}),
            json!({"relays":["https://relay.example:0"]}),
            json!({"relays":["https://relay.example", "https://RELAY.example:443/"]}),
            json!({"relays":["https://relay.example", "https://relay.example./"]}),
            json!({"relays":(0..9).map(|n| format!("https://relay{n}.example")).collect::<Vec<_>>()}),
            json!({"relays":[format!("https://{}.example", "x".repeat(1024))]}),
        ] {
            assert!(configured_relays(Some(&fields)).is_err(), "{fields}");
        }
    }

    #[tokio::test]
    async fn saved_selection_rejects_changes_before_mutation_and_local_wire_never_uses_it() {
        let engine = std::sync::Arc::new(crate::Engine::open_memory().await.unwrap());
        let fields = json!({"internet":false,"relays":["https://RELAY.example:443"]});
        let command = |fds| crate::actions::Action::SetCellConfig {
            namespace: "lince.discovery".into(),
            fds,
        };
        engine.act(command(fields), None).await.unwrap();
        let saved = store::cells::config(&engine.store.pool, "lince.discovery")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved["relays"], json!(["https://relay.example/"]));
        assert!(
            engine
                .act(command(json!({"relays":["http://relay.example"]})), None)
                .await
                .is_err()
        );
        assert!(
            engine
                .act(
                    command(json!({"relays":[]})),
                    Some("unrecognized-person".into())
                )
                .await
                .is_err()
        );
        assert_eq!(
            store::cells::config(&engine.store.pool, "lince.discovery")
                .await
                .unwrap()
                .unwrap(),
            saved
        );
        let wire = super::super::Wire::bind_with_discovery(
            engine.clone(),
            iroh::SecretKey::from_bytes(&[201; 32]),
            super::super::Reach::Local,
            None,
            false,
        )
        .await
        .unwrap();
        assert_eq!(wire.network_status()["relay_selection"], "disabled");
        assert!(
            wire.network_status()["advertised_relays"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            wire.endpoint()
                .remove_relay(&wire.configured_relays()[0])
                .await
                .is_none()
        );
        wire.shutdown().await;
        store::cells::set_config(
            &engine.store.pool,
            "lince.discovery",
            &json!({"relays":[{"url":"https://relay.example","insecure":true}]}),
        )
        .await
        .unwrap();
        assert!(
            super::super::Wire::bind_with_discovery(
                engine,
                iroh::SecretKey::from_bytes(&[202; 32]),
                super::super::Reach::Local,
                None,
                false
            )
            .await
            .is_err()
        );
    }

    async fn endpoint(reach: super::super::Reach, relay: RelayUrl, seed: u8) -> iroh::Endpoint {
        endpoint_builder(reach, &[relay])
            .clear_address_lookup()
            .secret_key(iroh::SecretKey::from_bytes(&[seed; 32]))
            .alpns(vec![b"lince-relay-test".to_vec()])
            .bind()
            .await
            .unwrap()
    }

    async fn exchange(
        server: &iroh::Endpoint,
        client: &iroh::Endpoint,
        addr: iroh::EndpointAddr,
    ) -> (iroh::endpoint::Connection, iroh::endpoint::Connection) {
        let (connection, incoming) = tokio::join!(client.connect(addr, b"lince-relay-test"), async {
            server.accept().await.unwrap().await.unwrap()
        });
        let connection = connection.unwrap();
        let bytes = vec![91u8; 8192];
        let (mut send, mut receive) = connection.open_bi().await.unwrap();
        send.write_all(&bytes).await.unwrap();
        send.finish().unwrap();
        let (mut reply, mut request) = incoming.accept_bi().await.unwrap();
        assert_eq!(request.read_to_end(8192).await.unwrap(), bytes);
        reply.write_all(b"received").await.unwrap();
        reply.finish().unwrap();
        assert_eq!(receive.read_to_end(8).await.unwrap(), b"received");
        (connection, incoming)
    }

    #[tokio::test]
    async fn configured_builder_carries_real_relay_only_bytes_and_direct_paths_survive_relay_shutdown()
     {
        tokio::time::timeout(std::time::Duration::from_secs(40), async {
            let mut config = iroh_relay::server::ServerConfig::default();
            config.relay = Some(iroh_relay::server::RelayConfig::new(([127, 0, 0, 1], 0)));
            let relay = iroh_relay::server::Server::spawn(config).await.unwrap();
            let url: RelayUrl = format!("http://{}", relay.http_addr().unwrap())
                .parse()
                .unwrap();
            assert!(configured_relays(Some(&json!({"relays":[url.as_str()]}))).is_err());
            let server = endpoint(super::super::Reach::Relay, url.clone(), 203).await;
            let client = endpoint(super::super::Reach::Relay, url.clone(), 204).await;
            server.online().await;
            client.online().await;
            assert!(server.bound_sockets().is_empty());
            assert!(client.bound_sockets().is_empty());
            assert_eq!(
                server.addr().relay_urls().cloned().collect::<Vec<_>>(),
                [url.clone()]
            );
            let (connection, remote) = exchange(&server, &client, server.addr()).await;
            assert!(!connection.paths().is_empty());
            assert!(connection.paths().iter().all(|path| path.is_relay()));
            remote.close(0u32.into(), b"qualified");
            client.close().await;
            server.close().await;
            let server = endpoint(super::super::Reach::Internet, url.clone(), 207).await;
            let client = endpoint(super::super::Reach::Internet, url.clone(), 208).await;
            server.online().await;
            client.online().await;
            let addr = iroh::EndpointAddr::new(server.id()).with_relay_url(url.clone());
            let (connection, remote) = exchange(&server, &client, addr).await;
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                while !connection.paths().iter().any(|path| path.is_ip()) {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            })
            .await
            .unwrap();
            remote.close(0u32.into(), b"qualified");
            client.close().await;
            server.close().await;
            relay.shutdown().await.unwrap();
            let server = endpoint(super::super::Reach::Internet, url.clone(), 205).await;
            let client = endpoint(super::super::Reach::Internet, url, 206).await;
            let port = server
                .bound_sockets()
                .iter()
                .find(|addr| addr.is_ipv4())
                .unwrap()
                .port();
            let addr =
                iroh::EndpointAddr::new(server.id()).with_ip_addr(([127, 0, 0, 1], port).into());
            let (connection, remote) = exchange(&server, &client, addr).await;
            assert!(connection.paths().iter().any(|path| path.is_ip()));
            remote.close(0u32.into(), b"qualified");
            client.close().await;
            server.close().await;
        })
        .await
        .unwrap();
    }
}
