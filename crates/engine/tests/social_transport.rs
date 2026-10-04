use engine::{
    Engine,
    pairing::EnrolmentInvite,
    roster::{CellEntry, ROOT_KEY_ID, full_capabilities},
    trust::Signer,
    wire::{ALPN_SYNC, Reach, Wire, WireRequest},
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

#[path = "social_transport/sharing.rs"]
mod sharing;

#[path = "social_transport/churn.rs"]
mod churn;

async fn fetch(connection: &iroh::endpoint::Connection) -> Option<Value> {
    let request = WireRequest::FetchOpsSince {
        vector: Default::default(),
        limit: 2000,
    };
    let (mut send, mut recv) = connection.open_bi().await.ok()?;
    send.write_all(&serde_json::to_vec(&request).ok()?)
        .await
        .ok()?;
    send.finish().ok()?;
    let bytes = recv.read_to_end(engine::wire::MAX_FRAME_BYTES).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[tokio::test]
async fn private_own_history_is_refused_on_existing_and_new_connections_after_device_downgrade() {
    let owner = Arc::new(Engine::open_memory().await.unwrap());
    let second = Arc::new(Engine::open_memory().await.unwrap());
    let organ = store::organs::local(&owner.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let root = Signer::from_bytes(&organ, ROOT_KEY_ID, [211; 32]);
    let owner_node = iroh::SecretKey::from_bytes(&[212; 32]);
    let second_node = iroh::SecretKey::from_bytes(&[213; 32]);
    let mut members = Vec::new();
    let mut keys = Vec::new();
    for (device, node, label) in [
        (&*owner, &owner_node, "Owner"),
        (&*second, &second_node, "Second device"),
    ] {
        let cell = store::cells::local(&device.store.pool)
            .await
            .unwrap()
            .unwrap();
        let key = device.operational_key_for(&organ).await.unwrap();
        members.push(CellEntry {
            cell_uid: cell.uid,
            node_id: node.public().to_string(),
            label: label.into(),
            operational_key: key.public_key_b64(),
            sealing_key: None,
            front_door: false,
            capabilities: full_capabilities(),
        });
        keys.push(key);
    }
    owner.set_signer(keys[0].clone()).await.unwrap();
    owner.set_organ_signer(keys[0].clone()).await.unwrap();
    owner.publish_root_key(&root).await.unwrap();
    let roster = owner.publish_roster(&root, members.clone()).await.unwrap();
    second
        .join_organ(
            &EnrolmentInvite {
                node_id: members[0].node_id.clone(),
                organ_uid: organ.clone(),
                root_key: root.public_key_b64(),
                token: "private-history-transport".into(),
                addrs: vec![],
            },
            &roster,
            keys[1].clone(),
        )
        .await
        .unwrap();
    second.set_signer(keys[1].clone()).await.unwrap();
    let peer = nucleus::new_uid("r");
    store::organs::add_contact(&owner.store.pool, &peer, None, "Contact", "", 1)
        .await
        .unwrap();
    let (conversation, thread) = owner
        .start_conversation(&peer, "Private social history")
        .await
        .unwrap();
    owner
        .send_message(&thread, "Me", "PRIVATE_SOCIAL_HISTORY_SENTINEL")
        .await
        .unwrap();
    store::records::set_extension(
        &owner.store.pool,
        &conversation,
        nucleus::social::requests::PARTICIPANTS_NAMESPACE,
        &json!({"private":"PRIVATE_PARTICIPANT_SENTINEL"}),
    )
    .await
    .unwrap();
    let grants: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM replica_grant")
        .fetch_one(&owner.store.pool)
        .await
        .unwrap();
    let server = Arc::new(
        Wire::bind_with_discovery(owner.clone(), owner_node, Reach::Local, None, false)
            .await
            .unwrap(),
    );
    let client = Arc::new(
        Wire::bind_with_discovery(second.clone(), second_node, Reach::Local, None, false)
            .await
            .unwrap(),
    );
    let server_task = {
        let wire = server.clone();
        tokio::spawn(async move { wire.serve().await })
    };
    let port = server
        .endpoint()
        .bound_sockets()
        .into_iter()
        .next()
        .unwrap()
        .port();
    let addr =
        iroh::EndpointAddr::new(server.node_id()).with_ip_addr(([127, 0, 0, 1], port).into());
    let connection = client
        .endpoint()
        .connect(addr.clone(), ALPN_SYNC)
        .await
        .unwrap();
    let first = tokio::time::timeout(Duration::from_secs(3), fetch(&connection))
        .await
        .unwrap()
        .unwrap();
    let visible = serde_json::to_string(&first).unwrap();
    assert!(visible.contains("PRIVATE_SOCIAL_HISTORY_SENTINEL"));
    assert!(visible.contains("PRIVATE_PARTICIPANT_SENTINEL"));
    members[1]
        .capabilities
        .retain(|cap| cap != engine::roster::CAP_WRITE);
    owner.publish_roster(&root, members.clone()).await.unwrap();
    assert!(
        server
            .sibling_organ(&client.node_id().to_string())
            .await
            .is_none()
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(3), fetch(&connection))
            .await
            .unwrap()
            .is_none()
    );
    let refused = tokio::time::timeout(
        Duration::from_secs(3),
        client.request(
            addr.clone(),
            ALPN_SYNC,
            &WireRequest::FetchOpsSince {
                vector: Default::default(),
                limit: 2000,
            },
        ),
    )
    .await
    .unwrap();
    assert!(refused.is_err());
    owner
        .publish_roster(&root, vec![members.remove(0)])
        .await
        .unwrap();
    let refused = tokio::time::timeout(
        Duration::from_secs(3),
        client.request(
            addr,
            ALPN_SYNC,
            &WireRequest::FetchOpsSince {
                vector: Default::default(),
                limit: 2000,
            },
        ),
    )
    .await
    .unwrap();
    assert!(refused.is_err());
    assert_eq!(
        store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM replica_grant")
            .fetch_one(&owner.store.pool)
            .await
            .unwrap(),
        grants
    );
    server_task.abort();
    client.endpoint().close().await;
    server.endpoint().close().await;
}
