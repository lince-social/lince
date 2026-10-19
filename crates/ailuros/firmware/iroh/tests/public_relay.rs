use lince_ailuros_esp_iroh::endpoint_builder;
use std::time::Duration;

#[tokio::test]
#[ignore = "Requires outbound public iroh relay and HTTPS address-lookup access"]
async fn identity_lookup_and_verified_tls_deliver_data_without_direct_ip_transports() {
    let alpn = b"lince/ailuros-qualification/1";
    let receiver = endpoint_builder(iroh::SecretKey::generate())
        .clear_ip_transports()
        .alpns(vec![alpn.to_vec()])
        .bind()
        .await
        .unwrap();
    let sender = endpoint_builder(iroh::SecretKey::generate())
        .clear_ip_transports()
        .bind()
        .await
        .unwrap();
    assert!(receiver.bound_sockets().is_empty());
    assert!(sender.bound_sockets().is_empty());
    tokio::time::timeout(Duration::from_secs(45), async {
        tokio::join!(receiver.online(), sender.online());
        let target = receiver.clone();
        let accepted = tokio::spawn(async move { target.accept().await.unwrap().await.unwrap() });
        let connection = sender
            .connect(iroh::EndpointAddr::new(receiver.id()), alpn)
            .await
            .unwrap();
        let received = accepted.await.unwrap();
        assert_eq!(connection.remote_id(), receiver.id());
        let (mut send, _) = connection.open_bi().await.unwrap();
        send.write_all(b"identity lookup across verified public relays")
            .await
            .unwrap();
        send.finish().unwrap();
        let (_, mut receive) = received.accept_bi().await.unwrap();
        assert_eq!(
            receive.read_to_end(128).await.unwrap(),
            b"identity lookup across verified public relays"
        );
        connection.close(0u32.into(), b"qualified");
    })
    .await
    .expect("Public relay and identity lookup qualification exceeded its deadline");
    sender.close().await;
    receiver.close().await;
}
