// non-lean-yet

//! Peer endpoint configuration, binding, and discovery controls.

use std::{
    collections::HashMap,
    net::SocketAddr,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use iroh::{Endpoint, EndpointId, SecretKey};

use super::{
    ALPN_AILUROS, ALPN_HELLO, ALPN_LIVE, ALPN_LOCATION, ALPN_MAILBOX, ALPN_SOCIAL, ALPN_SYNC,
    ALPN_THREAD, DIAL_TIMEOUT, MAX_CONNECTIONS_PER_PEER, Nearby, Wire, configured_relays,
    discovery, relays,
};
use crate::{Engine, error::EngineError};

pub const DEFAULT_PEER_PORT: u16 = 6175;

/// Reads the configured peer port. An absent setting uses [`DEFAULT_PEER_PORT`].
/// A configured zero asks the operating system to choose an available port.
///
/// # Errors
/// Returns an error for a non-object configuration or an invalid port.
pub fn configured_peer_port(fields: Option<&serde_json::Value>) -> Result<u16, EngineError> {
    let Some(fields) = fields else {
        return Ok(DEFAULT_PEER_PORT);
    };
    let fields = fields
        .as_object()
        .ok_or_else(|| EngineError::Consequence("Network settings must be an object".into()))?;
    let Some(port) = fields.get("peer_port") else {
        return Ok(DEFAULT_PEER_PORT);
    };
    port.as_u64()
        .and_then(|port| u16::try_from(port).ok())
        .ok_or_else(|| {
            EngineError::Consequence(
                "Peer port must be between 0 and 65535; 0 chooses an available port".into(),
            )
        })
}

/// Listen address, admission policy, and limits for a private endpoint.
#[derive(Debug, Clone, Copy)]
pub struct PrivateWireConfig {
    pub(super) full_protocols: bool,
    pub listen_addr: SocketAddr,
    pub accept_credentials: bool,
    pub max_connections: usize,
    pub max_connections_per_peer: usize,
    pub handshake_timeout: Duration,
    pub session_timeout: Duration,
}

impl PrivateWireConfig {
    pub fn new(listen_addr: SocketAddr, accept_credentials: bool) -> Self {
        Self {
            full_protocols: false,
            listen_addr,
            accept_credentials,
            max_connections: 64,
            max_connections_per_peer: MAX_CONNECTIONS_PER_PEER,
            handshake_timeout: DIAL_TIMEOUT,
            session_timeout: Duration::from_secs(3600),
        }
    }

    pub(super) fn validate(&self) -> Result<(), EngineError> {
        if self.listen_addr.ip().is_multicast()
            || matches!(self.listen_addr.ip(), std::net::IpAddr::V4(ip) if ip.is_broadcast())
        {
            return Err(EngineError::Consequence(format!(
                "private Wire requires a unicast or wildcard listen address: {}",
                self.listen_addr
            )));
        }
        if self.max_connections == 0
            || self.max_connections > tokio::sync::Semaphore::MAX_PERMITS
            || self.max_connections_per_peer == 0
            || self.max_connections_per_peer > self.max_connections
            || self.handshake_timeout.is_zero()
            || self.session_timeout.is_zero()
        {
            return Err(EngineError::Consequence(
                "invalid private Wire limits".into(),
            ));
        }
        Ok(())
    }
}

pub fn node_secret(path: &Path) -> Result<SecretKey, EngineError> {
    Ok(SecretKey::from_bytes(&crate::trust::load_or_create_secret(
        path,
    )?))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    Relay,
    Internet,
    Local,
}

impl Wire {
    pub async fn bind(
        engine: Arc<Engine>,
        secret: SecretKey,
        reach: Reach,
    ) -> Result<Wire, EngineError> {
        Wire::bind_as(engine, secret, reach, None).await
    }

    pub async fn bind_as(
        engine: Arc<Engine>,
        secret: SecretKey,
        reach: Reach,
        display_name: Option<&str>,
    ) -> Result<Wire, EngineError> {
        Self::bind_with_discovery(engine, secret, reach, display_name, true).await
    }

    pub async fn bind_with_discovery(
        engine: Arc<Engine>,
        secret: SecretKey,
        reach: Reach,
        display_name: Option<&str>,
        local_discovery: bool,
    ) -> Result<Wire, EngineError> {
        Self::bind_on_port(engine, secret, reach, display_name, local_discovery, 0).await
    }

    pub async fn bind_on_port(
        engine: Arc<Engine>,
        secret: SecretKey,
        reach: Reach,
        display_name: Option<&str>,
        local_discovery: bool,
        peer_port: u16,
    ) -> Result<Wire, EngineError> {
        Self::bind_inner(
            engine,
            secret,
            reach,
            display_name,
            local_discovery,
            peer_port,
            (None, None),
        )
        .await
    }

    pub async fn bind_with_endpoint_builder(
        engine: Arc<Engine>,
        secret: SecretKey,
        reach: Reach,
        builder: iroh::endpoint::Builder,
    ) -> Result<Wire, EngineError> {
        Self::bind_inner(engine, secret, reach, None, false, 0, (None, Some(builder))).await
    }

    pub async fn bind_private(
        engine: Arc<Engine>,
        secret: SecretKey,
        config: PrivateWireConfig,
    ) -> Result<Wire, EngineError> {
        config.validate()?;
        Self::bind_inner(
            engine,
            secret,
            Reach::Local,
            None,
            false,
            0,
            (Some(config), None),
        )
        .await
    }

    pub async fn bind_loopback(
        engine: Arc<Engine>,
        secret: SecretKey,
    ) -> Result<Wire, EngineError> {
        let mut config = PrivateWireConfig::new(SocketAddr::from(([127, 0, 0, 1], 0)), false);
        config.full_protocols = true;
        Self::bind_private(engine, secret, config).await
    }

    async fn bind_inner(
        engine: Arc<Engine>,
        secret: SecretKey,
        reach: Reach,
        display_name: Option<&str>,
        local_discovery: bool,
        peer_port: u16,
        binding: (Option<PrivateWireConfig>, Option<iroh::endpoint::Builder>),
    ) -> Result<Wire, EngineError> {
        let (private, builder) = binding;
        let alpns = if private.is_some_and(|config| !config.full_protocols) {
            vec![ALPN_LIVE.to_vec()]
        } else {
            vec![
                ALPN_SYNC.to_vec(),
                ALPN_THREAD.to_vec(),
                ALPN_LIVE.to_vec(),
                ALPN_LOCATION.to_vec(),
                ALPN_HELLO.to_vec(),
                ALPN_MAILBOX.to_vec(),
                ALPN_SOCIAL.to_vec(),
                ALPN_AILUROS.to_vec(),
                crate::blob_sync::ALPN.to_vec(),
                crate::blob_sync::DATA_ALPN.to_vec(),
            ]
        };
        let customized = builder.is_some();
        let relays = if private.is_none() && !customized {
            let fields = match store::cells::config(&engine.store.pool, "lince.discovery").await? {
                Some(fields) => Some(fields),
                None => match store::organs::local(&engine.store.pool).await? {
                    Some(organ) => {
                        store::records::get_extension(
                            &engine.store.pool,
                            &organ.uid,
                            "lince.discovery",
                        )
                        .await?
                    }
                    None => None,
                },
            };
            configured_relays(fields.as_ref())?
        } else {
            Vec::new()
        };
        let mut builder = builder.unwrap_or_else(|| relays::endpoint_builder(reach, &relays));
        if let Some(config) = private {
            builder = builder
                .portmapper_config(iroh::endpoint::PortmapperConfig::Disabled)
                .net_report_config(iroh::endpoint::NetReportConfig::minimal())
                .clear_ip_transports()
                .bind_addr(config.listen_addr)
                .map_err(|error| {
                    EngineError::Consequence(format!(
                        "private Wire listen address {}: {error}",
                        config.listen_addr
                    ))
                })?;
        } else if reach != Reach::Relay && !customized {
            builder = builder
                .bind_addr(SocketAddr::from(([0, 0, 0, 0], peer_port)))
                .map_err(|error| EngineError::Consequence(format!("Peer IPv4 port: {error}")))?
                .bind_addr_with_opts(
                    SocketAddr::from(([0u16; 8], peer_port)),
                    iroh::endpoint::BindOpts::default().set_is_required(false),
                )
                .map_err(|error| EngineError::Consequence(format!("Peer IPv6 port: {error}")))?;
        }
        let endpoint = builder
            .secret_key(secret)
            .alpns(alpns)
            .bind()
            .await
            .map_err(|error| {
                EngineError::Consequence(format!(
                    "iroh bind failed{}: {error}",
                    private
                        .map(|config| format!(" at {}", config.listen_addr))
                        .unwrap_or_default()
                ))
            })?;

        let known_addrs = iroh::address_lookup::MemoryLookup::new();
        let local_lookup = discovery::LocalDiscovery::default();
        if let Ok(services) = endpoint.address_lookup() {
            services.add(known_addrs.clone());
            services.add(local_lookup.clone());
        }
        let wire = Wire {
            endpoint,
            engine,
            reach,
            peer_port,
            relays,
            local_discovery: local_lookup,
            open_per_peer: Arc::new(Mutex::new(HashMap::new())),
            presence_connections: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            nearby: Nearby::default(),
            known_addrs,
            live: Arc::new(Mutex::new(None)),
            live_connections: Arc::new(Mutex::new(HashMap::new())),
            transfer: Arc::new(Mutex::new(None)),
            private,
            sync_lock: Arc::new(tokio::sync::Mutex::new(())),
            blob_slots: Arc::new(tokio::sync::Semaphore::new(8)),
            ailuros_auth_slots: Arc::new(tokio::sync::Semaphore::new(8)),
            connection_slots: Arc::new(tokio::sync::Semaphore::new(
                private.map_or(64, |config| config.max_connections),
            )),
        };
        if private.is_none() {
            wire.restore_sibling_routes().await?;
        }
        if let Some(name) = display_name {
            wire.set_display_name(name);
        }
        if let Err(error) = wire.set_local_discovery(local_discovery) {
            tracing::warn!(%error, "mDNS unavailable; nearby list will stay empty");
        }
        if private.is_none() {
            *wire.engine.nearby.lock().expect("nearby handle") = Some(wire.nearby.clone());
        }
        Ok(wire)
    }

    pub async fn shutdown(&self) {
        let _ = self.set_local_discovery(false);
        self.endpoint.close().await;
    }

    pub fn set_local_discovery(&self, enabled: bool) -> Result<(), EngineError> {
        if enabled && self.private.is_some() {
            return Err(EngineError::Consequence(
                "Private connections cannot advertise on the LAN".into(),
            ));
        }
        self.local_discovery
            .set_enabled(enabled, self.node_id(), self.nearby.clone())
    }

    pub fn set_display_name(&self, name: &str) {
        let clipped: String = name
            .chars()
            .take(iroh::address_lookup::UserData::MAX_LENGTH / 4)
            .collect();
        match iroh::address_lookup::UserData::try_from(clipped) {
            Ok(data) => self.endpoint.set_user_data_for_address_lookup(Some(data)),
            Err(error) => tracing::warn!(%error, "display name rejected for discovery"),
        }
    }

    pub fn nearby(&self) -> &Nearby {
        &self.nearby
    }

    pub async fn accept_unknown(&self) -> bool {
        if self.private.is_some_and(|config| !config.full_protocols) {
            return false;
        }
        let Ok(Some(organ)) = store::organs::local(&self.engine.store.pool).await else {
            return false;
        };
        match self.discovery_config(&organ.uid).await {
            Some(fields) => fields
                .get("accept_unknown")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            _ => false,
        }
    }

    pub async fn accept_logins(&self) -> bool {
        if let Some(config) = self.private.filter(|config| !config.full_protocols) {
            return config.accept_credentials;
        }
        let Ok(Some(organ)) = store::organs::local(&self.engine.store.pool).await else {
            return false;
        };
        match self.discovery_config(&organ.uid).await {
            Some(fields) => fields
                .get("accept_logins")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
            _ => true,
        }
    }

    pub fn node_id(&self) -> EndpointId {
        self.endpoint.id()
    }

    pub fn reach(&self) -> Reach {
        self.reach
    }

    pub fn local_discovery(&self) -> bool {
        self.local_discovery.enabled()
    }

    pub fn configured_port(&self) -> u16 {
        self.peer_port
    }

    pub fn configured_relays(&self) -> &[iroh::RelayUrl] {
        &self.relays
    }

    pub fn network_status(&self) -> serde_json::Value {
        serde_json::json!({
            "configured_port": self.peer_port,
            "relay_only": self.reach == Reach::Relay,
            "relay_selection": if self.reach == Reach::Local { "disabled" } else if self.relays.is_empty() { "preset" } else { "custom" },
            "selected_relays": self.relays.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "advertised_relays": self.endpoint.addr().relay_urls().map(ToString::to_string).collect::<Vec<_>>(),
            "listening": self.endpoint.bound_sockets().iter().map(ToString::to_string).collect::<Vec<_>>(),
            "addresses": self.endpoint.addr().ip_addrs().map(ToString::to_string).collect::<Vec<_>>(),
        })
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    async fn discovery_config(&self, organ_uid: &str) -> Option<serde_json::Value> {
        if let Ok(Some(fields)) =
            store::cells::config(&self.engine.store.pool, "lince.discovery").await
        {
            return Some(fields);
        }
        store::records::get_extension(&self.engine.store.pool, organ_uid, "lince.discovery")
            .await
            .ok()
            .flatten()
    }
}
