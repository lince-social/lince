use anyhow::bail;
use std::{
    future::Future,
    net::{Ipv4Addr, SocketAddr},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub generation: u32,
    pub address: Ipv4Addr,
    pub gateway: Ipv4Addr,
    pub netmask: Ipv4Addr,
}

impl Snapshot {
    fn valid(&self) -> bool {
        !self.address.is_unspecified()
            && !self.address.is_loopback()
            && !self.address.is_multicast()
            && !self.address.is_broadcast()
    }
}

#[derive(Default)]
pub struct Network {
    current: Option<Snapshot>,
    endpoint: Option<iroh::Endpoint>,
}

impl Network {
    pub fn endpoint(&self) -> Option<&iroh::Endpoint> {
        self.endpoint.as_ref()
    }

    pub async fn update<F, Fut>(
        &mut self,
        current: Option<Snapshot>,
        secret: &iroh::SecretKey,
        open: F,
    ) -> anyhow::Result<bool>
    where
        F: FnOnce(iroh::SecretKey) -> Fut,
        Fut: Future<Output = anyhow::Result<iroh::Endpoint>>,
    {
        let current = current.filter(Snapshot::valid);
        if current == self.current
            && (current.is_none()
                || self
                    .endpoint
                    .as_ref()
                    .is_some_and(|endpoint| !endpoint.is_closed()))
        {
            return Ok(false);
        }
        if let Some(endpoint) = self.endpoint.take() {
            endpoint.close().await;
        }
        self.current = None;
        let Some(snapshot) = current else {
            return Ok(true);
        };
        let endpoint = open(secret.clone()).await?;
        if endpoint.id() != secret.public() {
            endpoint.close().await;
            bail!("Device endpoint identity changed during network reconnection");
        }
        if let Some(socket) = endpoint
            .bound_sockets()
            .into_iter()
            .find(|socket| socket.is_ipv4() && socket.port() != 0)
        {
            endpoint
                .add_external_addr(SocketAddr::from((snapshot.address, socket.port())))
                .await;
        }
        self.current = Some(snapshot);
        self.endpoint = Some(endpoint);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iroh::Watcher as _;
    use std::{sync::Arc, time::Duration};

    async fn advertised(endpoint: &iroh::Endpoint, ip: Ipv4Addr) {
        let mut changes = endpoint.watch_addr();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if changes.get().ip_addrs().any(|address| address.ip() == ip) {
                    return;
                }
                changes.updated().await.unwrap();
            }
        })
        .await
        .unwrap();
    }

    async fn open(secret: iroh::SecretKey) -> anyhow::Result<iroh::Endpoint> {
        Ok(iroh::Endpoint::builder(iroh::endpoint::presets::Empty)
            .crypto_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .secret_key(secret)
            .relay_mode(iroh::RelayMode::Disabled)
            .alpns(vec![b"lince/network-test/1".to_vec()])
            .bind_addr("127.0.0.1:0".parse::<SocketAddr>()?)?
            .bind()
            .await?)
    }

    fn snapshot() -> Snapshot {
        Snapshot {
            generation: 1,
            address: Ipv4Addr::new(10, 0, 0, 10),
            gateway: Ipv4Addr::new(10, 0, 0, 1),
            netmask: Ipv4Addr::new(255, 255, 255, 0),
        }
    }

    #[tokio::test]
    async fn address_changes_remove_old_hints_and_preserve_identity() {
        let secret = iroh::SecretKey::from_bytes(&[31; 32]);
        let mut network = Network::default();
        assert!(
            network
                .update(Some(snapshot()), &secret, open)
                .await
                .unwrap()
        );
        let old = network.endpoint().unwrap().clone();
        assert_eq!(old.id(), secret.public());
        advertised(&old, snapshot().address).await;
        let port = old
            .bound_sockets()
            .into_iter()
            .find(|socket| socket.is_ipv4())
            .unwrap()
            .port();
        let hint = SocketAddr::from((snapshot().address, port));
        assert!(old.addr().ip_addrs().any(|address| *address == hint));
        assert!(
            !network
                .update(Some(snapshot()), &secret, |_| async {
                    panic!("Unchanged network must retain its endpoint")
                })
                .await
                .unwrap()
        );
        let changed = Snapshot {
            address: Ipv4Addr::new(10, 0, 0, 11),
            ..snapshot()
        };
        assert!(network.update(Some(changed), &secret, open).await.unwrap());
        assert!(old.is_closed());
        let current = network.endpoint().unwrap();
        assert_eq!(current.id(), secret.public());
        advertised(current, changed.address).await;
        assert!(
            !current
                .addr()
                .ip_addrs()
                .any(|address| address.ip() == snapshot().address)
        );
        assert!(
            current
                .addr()
                .ip_addrs()
                .any(|address| address.ip() == changed.address)
        );
        network.update(None, &secret, open).await.unwrap();
    }

    #[tokio::test]
    async fn rapid_rejoins_and_route_changes_rebuild_even_with_the_same_address() {
        let secret = iroh::SecretKey::from_bytes(&[32; 32]);
        let mut network = Network::default();
        let initial = snapshot();
        network.update(Some(initial), &secret, open).await.unwrap();
        let rejoined = Snapshot {
            generation: 3,
            ..initial
        };
        let rerouted = Snapshot {
            gateway: Ipv4Addr::new(10, 0, 0, 2),
            ..rejoined
        };
        let remasked = Snapshot {
            netmask: Ipv4Addr::new(255, 255, 0, 0),
            ..rerouted
        };
        for changed in [rejoined, rerouted, remasked] {
            let old = network.endpoint().unwrap().clone();
            assert!(network.update(Some(changed), &secret, open).await.unwrap());
            assert!(old.is_closed());
            assert_eq!(network.endpoint().unwrap().id(), secret.public());
        }
        let old = network.endpoint().unwrap().clone();
        assert!(network.update(None, &secret, open).await.unwrap());
        assert!(old.is_closed());
        assert!(network.endpoint().is_none());
    }

    #[tokio::test]
    async fn invalid_addresses_never_open_a_network_endpoint() {
        let secret = iroh::SecretKey::from_bytes(&[33; 32]);
        let mut network = Network::default();
        for address in [
            Ipv4Addr::UNSPECIFIED,
            Ipv4Addr::LOCALHOST,
            Ipv4Addr::BROADCAST,
            Ipv4Addr::new(224, 0, 0, 1),
        ] {
            assert!(
                !network
                    .update(
                        Some(Snapshot {
                            address,
                            ..snapshot()
                        }),
                        &secret,
                        |_| async { panic!("Invalid station address") }
                    )
                    .await
                    .unwrap()
            );
        }
        assert!(network.endpoint().is_none());
    }

    #[tokio::test]
    async fn failed_open_and_wrong_identity_leave_reconnection_retryable() {
        let secret = iroh::SecretKey::from_bytes(&[34; 32]);
        let mut network = Network::default();
        assert!(
            network
                .update(Some(snapshot()), &secret, |_| async {
                    bail!("Unavailable")
                })
                .await
                .is_err()
        );
        assert!(network.endpoint().is_none());
        let wrong = open(iroh::SecretKey::from_bytes(&[35; 32])).await.unwrap();
        let observed = wrong.clone();
        assert!(
            network
                .update(Some(snapshot()), &secret, |_| async { Ok(wrong) })
                .await
                .is_err()
        );
        assert!(observed.is_closed());
        assert!(network.endpoint().is_none());
        network
            .update(Some(snapshot()), &secret, open)
            .await
            .unwrap();
        network.endpoint().unwrap().close().await;
        assert!(
            network
                .update(Some(snapshot()), &secret, open)
                .await
                .unwrap()
        );
        assert_eq!(network.endpoint().unwrap().id(), secret.public());
        network.update(None, &secret, open).await.unwrap();
    }

    async fn transfer(
        sender: &iroh::Endpoint,
        receiver: &iroh::Endpoint,
    ) -> iroh::endpoint::Connection {
        let socket = receiver
            .bound_sockets()
            .into_iter()
            .find(|address| address.is_ipv4())
            .unwrap();
        let address =
            iroh::EndpointAddr::from_parts(receiver.id(), [iroh::TransportAddr::Ip(socket)]);
        let (sent, received) =
            tokio::join!(sender.connect(address, b"lince/network-test/1"), async {
                receiver.accept().await.unwrap().await
            });
        let sent = sent.unwrap();
        let received = received.unwrap();
        assert_eq!(sent.remote_id(), receiver.id());
        let (mut stream, _) = sent.open_bi().await.unwrap();
        stream.write_all(b"network-check").await.unwrap();
        stream.finish().unwrap();
        let (_, mut stream) = received.accept_bi().await.unwrap();
        assert_eq!(stream.read_to_end(32).await.unwrap(), b"network-check");
        sent
    }

    #[tokio::test]
    async fn pinned_peers_transfer_again_after_network_recreation() {
        tokio::time::timeout(Duration::from_secs(10), async {
            let secret = iroh::SecretKey::from_bytes(&[36; 32]);
            let caller = open(iroh::SecretKey::from_bytes(&[37; 32])).await.unwrap();
            let mut network = Network::default();
            network
                .update(Some(snapshot()), &secret, open)
                .await
                .unwrap();
            let previous = transfer(&caller, network.endpoint().unwrap()).await;
            network
                .update(
                    Some(Snapshot {
                        generation: 3,
                        ..snapshot()
                    }),
                    &secret,
                    open,
                )
                .await
                .unwrap();
            let _ = previous.closed().await;
            let reconnected = transfer(&caller, network.endpoint().unwrap()).await;
            assert_eq!(reconnected.remote_id(), secret.public());
            network.update(None, &secret, open).await.unwrap();
            caller.close().await;
        })
        .await
        .unwrap();
    }
}
