use super::*;
use engine::wire::{ALPN_SOCIAL, Reach, Wire};
use std::{sync::Arc, time::Duration};

const VERBS: &[&str] = &[
    "describe-service",
    "submit-report",
    "gossip-offer",
    "gossip-deliver",
    "ask-contacts",
    "update-reply-authority",
    "lookup-reply-routes",
    "inspect-private",
    "register-reply-route",
    "end-reply-post",
    "admit-private-sender",
    "deliver-private",
    "collect-private",
    "acknowledge-private",
    "discard-private",
    "publish-authority",
    "publish-posting-authority",
    "publish-profile-image",
    "fetch-profile-image",
    "publish-snippet",
    "publish-profile",
    "search",
    "fetch-profile",
];

async fn exchange(
    client: &iroh::Endpoint,
    address: iroh::EndpointAddr,
    bytes: &[u8],
) -> Option<Value> {
    tokio::time::timeout(Duration::from_secs(15), async {
        let connection = client.connect(address, ALPN_SOCIAL).await.ok()?;
        let (mut send, mut recv) = connection.open_bi().await.ok()?;
        send.write_all(bytes).await.ok()?;
        send.finish().ok()?;
        let body = recv
            .read_to_end(nucleus::social::MAX_PRIVATE_FRAME_BYTES)
            .await
            .ok()?;
        let response = serde_json::from_slice(&body).ok();
        connection.close(0u32.into(), b"protocol checked");
        response
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn every_social_verb_refuses_incompatible_alpn_and_malformed_version_without_import() {
    let host = Arc::new(host().await);
    let wire = Arc::new(
        Wire::bind_with_discovery(
            host.clone(),
            iroh::SecretKey::from_bytes(&[201; 32]),
            Reach::Local,
            None,
            false,
        )
        .await
        .unwrap(),
    );
    let serving = wire.clone();
    let task = tokio::spawn(async move { serving.serve().await });
    let port = wire
        .endpoint()
        .bound_sockets()
        .into_iter()
        .find(|addr| addr.is_ipv4())
        .unwrap()
        .port();
    let address =
        iroh::EndpointAddr::new(wire.node_id()).with_ip_addr(([127, 0, 0, 1], port).into());
    let client = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
        .clear_address_lookup()
        .bind()
        .await
        .unwrap();
    for verb in VERBS {
        let refused = tokio::time::timeout(
            Duration::from_secs(3),
            client.connect(address.clone(), b"lince/social/2"),
        )
        .await
        .unwrap();
        assert!(
            refused.is_err(),
            "Incompatible transport accepted for {verb}"
        );
        let bytes =
            serde_json::to_vec(&json!({"request":format!("{verb}-future-version")})).unwrap();
        let response = exchange(&client, address.clone(), &bytes).await.unwrap();
        assert!(response["error"].is_string(), "{verb}: {response}");
    }
    for table in [
        "social_service_envelope",
        "social_private_seen",
        "social_device_state",
        "social_document",
        "social_owner_control",
        "social_reply_route",
        "social_message_event",
        "replica_grant",
    ] {
        assert_eq!(
            store::sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&host.store.pool)
                .await
                .unwrap(),
            0,
            "Malformed requests changed {table}"
        );
    }
    task.abort();
    client.close().await;
    wire.endpoint().close().await;
}

#[tokio::test]
async fn actual_wire_accepts_exact_legal_frames_and_refuses_one_byte_over_each_limit() {
    let host = Arc::new(host().await);
    host.social_command(
        nucleus::social::Command::ConfigureServices {
            settings: ServiceSettings {
                mailbox: true,
                incoming_bytes_per_minute: 1024 * 1024 * 1024,
                outgoing_bytes_per_minute: 1024 * 1024 * 1024,
                ..Default::default()
            },
        },
        None,
        nucleus::execution::now(),
    )
    .await
    .unwrap();
    let wire = Arc::new(
        Wire::bind_with_discovery(
            host.clone(),
            iroh::SecretKey::from_bytes(&[202; 32]),
            Reach::Local,
            None,
            false,
        )
        .await
        .unwrap(),
    );
    let serving = wire.clone();
    let task = tokio::spawn(async move { serving.serve().await });
    let port = wire
        .endpoint()
        .bound_sockets()
        .into_iter()
        .find(|addr| addr.is_ipv4())
        .unwrap()
        .port();
    let address =
        iroh::EndpointAddr::new(wire.node_id()).with_ip_addr(([127, 0, 0, 1], port).into());
    let client = iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
        .clear_address_lookup()
        .bind()
        .await
        .unwrap();
    let mut public = serde_json::to_vec(&PublicRequest::DescribeService).unwrap();
    public.resize(nucleus::social::MAX_FRAME_BYTES, b' ');
    let response = exchange(&client, address.clone(), &public).await.unwrap();
    assert_eq!(
        response["data"]["descriptor"]["endpoint"],
        wire.node_id().to_string()
    );
    public.push(b' ');
    assert!(exchange(&client, address.clone(), &public).await.unwrap()["error"].is_string());
    let now = nucleus::execution::now().timestamp();
    let key = [203; 32];
    let account =
        session::new_account(&key, vec![wire.node_id().to_string()], now + 30 * 86400).unwrap();
    let owner = Signer::from_bytes("", "social", [204; 32]);
    let route = certified(&owner, &account, 1, now);
    let registration = serde_json::to_vec(&PublicRequest::RegisterReplyRoute {
        document: route.clone(),
        post: None,
    })
    .unwrap();
    let response = exchange(&client, address.clone(), &registration)
        .await
        .unwrap();
    assert!(response["error"].is_null(), "{response}");
    let mut private = serde_json::to_vec(&PublicRequest::CollectPrivate {
        access: access(&route, &account, vec![], now),
    })
    .unwrap();
    private.resize(nucleus::social::MAX_PRIVATE_FRAME_BYTES, b' ');
    let response = exchange(&client, address.clone(), &private).await.unwrap();
    assert_eq!(response["data"]["envelopes"], json!([]));
    private.push(b' ');
    assert!(exchange(&client, address, &private).await.is_none());
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_service_envelope")
            .fetch_one(&host.store.pool)
            .await
            .unwrap(),
        0
    );
    task.abort();
    client.close().await;
    wire.endpoint().close().await;
}
