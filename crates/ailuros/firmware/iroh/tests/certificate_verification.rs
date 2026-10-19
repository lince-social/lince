use lince_ailuros_esp_iroh::crypto_provider;
use rustls::pki_types::{PrivatePkcs8KeyDer, ServerName};
use std::{sync::Arc, time::Duration};
use tokio::{io::AsyncReadExt, io::AsyncWriteExt, net::TcpListener};
use tokio_rustls::{TlsAcceptor, TlsConnector};

#[test]
fn warm_signature_results_preserve_hostname_time_trust_and_tamper_rejection() {
    use rustls::{
        client::danger::ServerCertVerifier,
        pki_types::{CertificateDer, UnixTime},
    };
    let mut ca_parameters = rcgen::CertificateParams::new(Vec::new()).unwrap();
    ca_parameters.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    let ca_key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P384_SHA384).unwrap();
    let ca = ca_parameters.self_signed(&ca_key).unwrap();
    let issuer = rcgen::Issuer::from_params(&ca_parameters, &ca_key);
    let mut parameters = rcgen::CertificateParams::new(vec!["relay.lince.test".into()]).unwrap();
    parameters.not_before = rcgen::date_time_ymd(2020, 1, 1);
    parameters.not_after = rcgen::date_time_ymd(2030, 1, 1);
    let key = rcgen::KeyPair::generate().unwrap();
    let certificate = parameters.signed_by(&key, &issuer).unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(ca.der().clone()).unwrap();
    let verifier = rustls::client::WebPkiServerVerifier::builder_with_provider(
        Arc::new(roots),
        crypto_provider(),
    )
    .build()
    .unwrap();
    let name = ServerName::try_from("relay.lince.test").unwrap();
    let now = UnixTime::since_unix_epoch(Duration::from_secs(1_800_000_000));
    let before = lince_ailuros_esp_iroh::verification::cache_statistics()
        .unwrap()
        .1;
    for _ in 0..3 {
        verifier
            .verify_server_cert(certificate.der(), &[], &name, &[], now)
            .unwrap();
    }
    assert!(
        lince_ailuros_esp_iroh::verification::cache_statistics()
            .unwrap()
            .1
            > before
    );
    assert!(
        verifier
            .verify_server_cert(
                certificate.der(),
                &[],
                &ServerName::try_from("other.lince.test").unwrap(),
                &[],
                now
            )
            .is_err()
    );
    assert!(
        verifier
            .verify_server_cert(
                certificate.der(),
                &[],
                &name,
                &[],
                UnixTime::since_unix_epoch(Duration::from_secs(2_000_000_000))
            )
            .is_err()
    );
    let mut untrusted = rustls::RootCertStore::empty();
    let other_ca = ca_parameters
        .self_signed(&rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P384_SHA384).unwrap())
        .unwrap();
    untrusted.add(other_ca.der().clone()).unwrap();
    let untrusted = rustls::client::WebPkiServerVerifier::builder_with_provider(
        Arc::new(untrusted),
        crypto_provider(),
    )
    .build()
    .unwrap();
    assert!(
        untrusted
            .verify_server_cert(certificate.der(), &[], &name, &[], now)
            .is_err()
    );
    let mut corrupted = certificate.der().to_vec();
    *corrupted.last_mut().unwrap() ^= 1;
    assert!(
        verifier
            .verify_server_cert(&CertificateDer::from(corrupted), &[], &name, &[], now)
            .is_err()
    );
}

async fn handshake(
    algorithm: &'static rcgen::SignatureAlgorithm,
    trusted: bool,
    name: &str,
    expired: bool,
) -> bool {
    let mut parameters = rcgen::CertificateParams::new(vec!["relay.lince.test".into()]).unwrap();
    if expired {
        parameters.not_before = rcgen::date_time_ymd(2020, 1, 1);
        parameters.not_after = rcgen::date_time_ymd(2021, 1, 1);
    }
    let key = rcgen::KeyPair::generate_for(algorithm).unwrap();
    let certificate = parameters.self_signed(&key).unwrap();
    let server = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![certificate.der().clone()],
        PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        if let Ok(mut stream) = TlsAcceptor::from(Arc::new(server)).accept(socket).await {
            let _ = stream.write_all(b"verified").await;
        }
    });
    let mut roots = rustls::RootCertStore::empty();
    if trusted {
        roots.add(certificate.der().clone()).unwrap();
    }
    let client = rustls::ClientConfig::builder_with_provider(crypto_provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let name = ServerName::try_from(name.to_owned()).unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        let socket = tokio::net::TcpStream::connect(address).await.unwrap();
        let mut stream = TlsConnector::from(Arc::new(client))
            .connect(name, socket)
            .await?;
        let mut bytes = [0; 8];
        stream.read_exact(&mut bytes).await?;
        assert_eq!(&bytes, b"verified");
        Ok::<_, std::io::Error>(())
    })
    .await
    .expect("TLS verification must complete within the handshake deadline");
    task.await.unwrap();
    result.is_ok()
}

#[tokio::test]
async fn relay_tls_accepts_trusted_names_and_refuses_unknown_issuers_wrong_names_and_expiry() {
    for algorithm in [
        &rcgen::PKCS_ECDSA_P256_SHA256,
        &rcgen::PKCS_ECDSA_P384_SHA384,
    ] {
        assert!(handshake(algorithm, true, "relay.lince.test", false).await);
        assert!(!handshake(algorithm, false, "relay.lince.test", false).await);
        assert!(!handshake(algorithm, true, "substituted.lince.test", false).await);
        assert!(!handshake(algorithm, true, "relay.lince.test", true).await);
    }
}

#[tokio::test]
async fn endpoint_relay_builder_rejects_an_untrusted_certificate_with_the_shared_os_resolver() {
    let key = rcgen::KeyPair::generate().unwrap();
    let certificate = rcgen::CertificateParams::new(vec!["127.0.0.1".into()])
        .unwrap()
        .self_signed(&key)
        .unwrap();
    let server = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![certificate.der().clone()],
        PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay: iroh::RelayUrl = format!("https://{}/", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let endpoint = lince_ailuros_esp_iroh::endpoint_builder(iroh::SecretKey::generate())
        .clear_ip_transports()
        .clear_address_lookup()
        .relay_mode(iroh::RelayMode::Custom(
            iroh::RelayConfig::new(relay.clone(), None).into(),
        ))
        .bind()
        .await
        .unwrap();
    let target =
        iroh::EndpointAddr::new(iroh::SecretKey::generate().public()).with_relay_url(relay);
    let client = endpoint.clone();
    let connection =
        tokio::spawn(async move { client.connect(target, b"untrusted-relay-check").await });
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let (socket, _) = listener.accept().await.unwrap();
        TlsAcceptor::from(Arc::new(server)).accept(socket).await
    })
    .await;
    endpoint.close().await;
    connection.abort();
    assert!(
        result
            .expect("Endpoint did not attempt the configured local relay")
            .is_err(),
        "Endpoint accepted an untrusted relay certificate"
    );
}
