use std::sync::Arc;
pub mod client;
#[cfg(target_os = "espidf")]
pub mod device;
pub mod network;
pub mod power;
pub mod readiness;
pub mod verification;

pub fn crypto_provider() -> Arc<rustls::crypto::CryptoProvider> {
    let mut provider = rusty_esp_iroh_host::crypto::provider();
    provider.signature_verification_algorithms =
        verification::algorithms(provider.signature_verification_algorithms);
    Arc::new(provider)
}

pub fn endpoint_builder(secret: iroh::SecretKey) -> iroh::endpoint::Builder {
    iroh::Endpoint::builder(iroh::endpoint::presets::Empty)
        .crypto_provider(crypto_provider())
        .secret_key(secret)
        .max_tls_tickets(0)
        .dns_resolver(iroh::dns::DnsResolver::custom(
            rusty_esp_iroh_host::relay::StdDnsResolver,
        ))
        .relay_mode(iroh::RelayMode::Default)
        .address_lookup(iroh::address_lookup::PkarrPublisher::n0_dns())
        .address_lookup(iroh::address_lookup::PkarrResolver::n0_dns())
}

pub async fn endpoint(secret: iroh::SecretKey) -> anyhow::Result<iroh::Endpoint> {
    Ok(endpoint_builder(secret).bind().await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::SocketAddr, time::Duration};

    async fn local_endpoint(
        secret: [u8; 32],
        provider: Arc<rustls::crypto::CryptoProvider>,
    ) -> iroh::Endpoint {
        iroh::Endpoint::builder(iroh::endpoint::presets::Empty)
            .crypto_provider(provider)
            .secret_key(iroh::SecretKey::from_bytes(&secret))
            .relay_mode(iroh::RelayMode::Disabled)
            .bind_addr("127.0.0.1:0".parse::<SocketAddr>().unwrap())
            .unwrap()
            .alpns(vec![b"lince/ailuros-qualification/1".to_vec()])
            .bind()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn quic_authenticates_the_pinned_peer_and_rejects_a_substituted_identity() {
        let receiver =
            local_endpoint([17; 32], Arc::new(rustls::crypto::ring::default_provider())).await;
        let sender = local_endpoint([19; 32], crypto_provider()).await;
        let target = receiver.clone();
        let accept = tokio::spawn(async move { target.accept().await.unwrap().await });
        let connection = tokio::time::timeout(
            Duration::from_secs(5),
            sender.connect(receiver.addr(), b"lince/ailuros-qualification/1"),
        )
        .await
        .unwrap()
        .unwrap();
        let received = accept.await.unwrap().unwrap();
        assert_eq!(connection.remote_id(), receiver.id());
        assert_eq!(received.remote_id(), sender.id());
        let (mut outgoing, _) = connection.open_bi().await.unwrap();
        outgoing
            .write_all(b"camera-transport-qualified")
            .await
            .unwrap();
        outgoing.finish().unwrap();
        let (_, mut incoming) = received.accept_bi().await.unwrap();
        assert_eq!(
            incoming.read_to_end(64).await.unwrap(),
            b"camera-transport-qualified"
        );
        connection.close(0u32.into(), b"qualified");
        let mut substituted = receiver.addr();
        substituted.id = iroh::SecretKey::from_bytes(&[23; 32]).public();
        let target = receiver.clone();
        let accept = tokio::spawn(async move { target.accept().await.unwrap().await });
        let refused = tokio::time::timeout(
            Duration::from_secs(5),
            sender.connect(substituted, b"lince/ailuros-qualification/1"),
        )
        .await
        .expect("A substituted peer identity must fail the handshake promptly");
        assert!(refused.is_err());
        receiver.close().await;
        let _ = accept.await;
        sender.close().await;
    }
}
