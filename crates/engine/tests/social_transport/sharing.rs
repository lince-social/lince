use engine::{
    Engine,
    sync::{Delivery, OpBatch},
    wire::{ALPN_SYNC, ALPN_THREAD, Reach, Wire, WireRequest, WireResponse},
};
use serde_json::json;
use std::sync::{Arc, Mutex};

async fn peer(seed: u8) -> (Arc<Engine>, String, Arc<Wire>) {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let signer = engine.operational_key_for(&organ).await.unwrap();
    engine.set_signer(signer.clone()).await.unwrap();
    engine.set_organ_signer(signer).await.unwrap();
    let wire = Arc::new(
        Wire::bind_with_discovery(
            engine.clone(),
            iroh::SecretKey::from_bytes(&[seed; 32]),
            Reach::Local,
            None,
            false,
        )
        .await
        .unwrap(),
    );
    (engine, organ, wire)
}

async fn known(engine: &Engine, other: &Engine, organ: &str, wire: &Wire) {
    engine
        .adopt_introduction(&other.introduction().await.unwrap(), 1)
        .await
        .unwrap();
    store::organs::set_node_id(&engine.store.pool, organ, Some(&wire.node_id().to_string()))
        .await
        .unwrap();
    store::organs::set_sync_policy(&engine.store.pool, organ, true, true)
        .await
        .unwrap();
    store::organs::set_trust(&engine.store.pool, organ, "known")
        .await
        .unwrap();
}

#[tokio::test]
async fn permissive_general_and_explicit_root_sharing_never_export_or_import_private_social_fields()
{
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        let (owner, organ, server) = peer(214).await;
        let (foreign, contact, client) = peer(215).await;
        known(&owner, &foreign, &contact, &client).await;
        known(&foreign, &owner, &organ, &server).await;
        let public = store::records::create(
            &owner.store.pool,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Permitted note",
                body: "PERMITTED_GENERAL_TEXT",
                quantity: store::exact::zero(),
            },
        )
        .await
        .unwrap();
        store::records::set_extension(
            &owner.store.pool,
            &public.uid,
            "lince.social.future-private-state",
            &json!({"secret":"PRIVATE_GENERAL_SENTINEL"}),
        )
        .await
        .unwrap();
        let (root, thread) = owner
            .start_conversation(&contact, "Explicitly shared history")
            .await
            .unwrap();
        let message = owner
            .send_message(&thread, "Owner", "PERMITTED_GRANTED_TEXT")
            .await
            .unwrap();
        store::records::set_extension(
            &owner.store.pool,
            &root,
            nucleus::social::requests::PARTICIPANTS_NAMESPACE,
            &json!({"mapping":"PRIVATE_PARTICIPANT_SENTINEL"}),
        )
        .await
        .unwrap();
        store::records::set_extension(
            &owner.store.pool,
            &root,
            "lince.social.future-private-state",
            &json!({"secret":"PRIVATE_ROOT_SENTINEL"}),
        )
        .await
        .unwrap();
        store::records::set_extension(
            &owner.store.pool,
            &root,
            "ordinary.metadata",
            &json!({"label":"PERMITTED_ROOT_METADATA"}),
        )
        .await
        .unwrap();
        owner.accept_conversation(&root, &contact).await.unwrap();
        store::replica::offer(&foreign.store.pool, &root, &organ)
            .await
            .unwrap();
        foreign.accept_conversation(&root, &organ).await.unwrap();
        let own = owner.export_sync_page(&organ, &[], 2000).await.unwrap();
        let own_bytes = serde_json::to_string(&own.batch).unwrap();
        for marker in [
            "PRIVATE_GENERAL_SENTINEL",
            "PRIVATE_PARTICIPANT_SENTINEL",
            "PRIVATE_ROOT_SENTINEL",
        ] {
            assert!(own_bytes.contains(marker));
        }
        let port = server
            .endpoint()
            .bound_sockets()
            .into_iter()
            .find(|addr| addr.is_ipv4())
            .unwrap()
            .port();
        let addr =
            iroh::EndpointAddr::new(server.node_id()).with_ip_addr(([127, 0, 0, 1], port).into());
        let serving = {
            let wire = server.clone();
            tokio::spawn(async move { wire.serve().await })
        };
        let general = client
            .request(
                addr.clone(),
                ALPN_SYNC,
                &WireRequest::FetchOpsSince {
                    vector: vec![],
                    limit: 2000,
                },
            )
            .await
            .unwrap();
        let bytes = serde_json::to_string(&general).unwrap();
        assert!(bytes.contains("PERMITTED_GENERAL_TEXT"));
        assert!(!bytes.contains("PRIVATE_GENERAL_SENTINEL"));
        assert!(!bytes.contains("PERMITTED_GRANTED_TEXT"));
        let granted = client
            .request(
                addr,
                ALPN_THREAD,
                &WireRequest::FetchGrantOpsSince {
                    root: root.clone(),
                    vector: vec![],
                    limit: 2000,
                },
            )
            .await
            .unwrap();
        let bytes = serde_json::to_string(&granted).unwrap();
        assert!(bytes.contains("PERMITTED_GRANTED_TEXT"));
        assert!(bytes.contains("PERMITTED_ROOT_METADATA"));
        assert!(!bytes.contains("PRIVATE_PARTICIPANT_SENTINEL"));
        assert!(!bytes.contains("PRIVATE_ROOT_SENTINEL"));
        let WireResponse::Ops { ops, .. } = granted else {
            panic!("Expected granted history");
        };
        foreign
            .import_grant_batch(
                &root,
                &OpBatch {
                    from_organ: organ.clone(),
                    ops,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            store::records::get(&foreign.store.pool, &message)
                .await
                .unwrap()
                .unwrap()
                .body,
            "PERMITTED_GRANTED_TEXT"
        );
        let raw = store::sync_ops::after_in_root(&owner.store.pool, &root, 0, 2000)
            .await
            .unwrap();
        let raw = owner.hydrate_ops(raw).await.unwrap();
        let private: Vec<_> = raw
            .into_iter()
            .filter(|op| nucleus::social::private_sync_field(&op.tbl, &op.field))
            .collect();
        assert!(!private.is_empty());
        foreign
            .import_grant_batch(
                &root,
                &OpBatch {
                    from_organ: organ,
                    ops: private,
                },
            )
            .await
            .unwrap();
        assert!(
            store::records::get_extension(
                &foreign.store.pool,
                &root,
                nucleus::social::requests::PARTICIPANTS_NAMESPACE
            )
            .await
            .unwrap()
            .is_none()
        );
        store::records::set_extension(
            &owner.store.pool,
            &root,
            "ordinary.metadata",
            &json!({"label":"PERMITTED_PUSH_METADATA"}),
        )
        .await
        .unwrap();
        store::records::set_extension(
            &owner.store.pool,
            &root,
            "lince.social.future-private-state",
            &json!({"secret":"PRIVATE_PUSH_SENTINEL"}),
        )
        .await
        .unwrap();
        let pushed = Arc::new(Mutex::new(Vec::new()));
        let captured = pushed.clone();
        owner
            .drain_outbox(move |_, _, batch| {
                captured
                    .lock()
                    .unwrap()
                    .push(serde_json::to_string(&batch).unwrap());
                async { Delivery::Sent }
            })
            .await
            .unwrap();
        let bytes = pushed.lock().unwrap().join("\n");
        assert!(bytes.contains("PERMITTED_PUSH_METADATA"));
        for marker in [
            "PRIVATE_GENERAL_SENTINEL",
            "PRIVATE_PARTICIPANT_SENTINEL",
            "PRIVATE_ROOT_SENTINEL",
            "PRIVATE_PUSH_SENTINEL",
        ] {
            assert!(!bytes.contains(marker));
        }
        serving.abort();
        client.shutdown().await;
        server.shutdown().await;
    })
    .await
    .unwrap();
}
