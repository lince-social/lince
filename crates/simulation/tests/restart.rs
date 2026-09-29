use std::sync::atomic::Ordering;

use engine::actions::Action;
use nucleus::projection::{Context, Status, Window};

#[test]
fn live_cell_restart_reuses_a_projection_after_pairing_address_refresh() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_stack_size(16 * 1024 * 1024)
                .enable_all()
                .build()
                .unwrap()
                .block_on(check_restart())
        })
        .unwrap()
        .join()
        .unwrap();
}

async fn check_restart() {
    let directory = tempfile::tempdir().unwrap();
    let store = store::Store::open(&format!(
        "sqlite://{}",
        directory.path().join("lince.db").display()
    ))
    .await
    .unwrap();
    let organ = store::organs::ensure_local(&store.pool, "http://127.0.0.1:6174")
        .await
        .unwrap();
    store::records::set_extension_raw(
        &store.pool,
        &organ.uid,
        "lince.discovery",
        &serde_json::json!({"internet": false, "local": false, "direct": false}),
    )
    .await
    .unwrap();
    store.pool.close().await;
    let options = cell::CellOptions {
        data_dir: Some(directory.path().into()),
        peer_port: Some(0),
        ..Default::default()
    };
    let cell = cell::Cell::open(options.clone()).await.unwrap();
    let organ = store::organs::local(&cell.engine().store.pool)
        .await
        .unwrap()
        .unwrap();
    let signer = cell.engine().operational_key_for(&organ.uid).await.unwrap();
    let root = engine::trust::Signer::load_or_create(
        &directory.path().join("keys/root-ed25519-v1.key"),
        &organ.uid,
        engine::roster::ROOT_KEY_ID,
    )
    .unwrap();
    let device = store::cells::local(&cell.engine().store.pool)
        .await
        .unwrap()
        .unwrap();
    let node_id = engine::wire::node_secret(&directory.path().join("keys/node-ed25519-v1.key"))
        .unwrap()
        .public()
        .to_string();
    cell.engine().publish_root_key(&root).await.unwrap();
    cell.engine()
        .publish_roster(
            &root,
            vec![engine::roster::CellEntry {
                cell_uid: device.uid,
                node_id: node_id.clone(),
                label: device.label,
                operational_key: signer.public_key_b64(),
                sealing_key: cell.engine().published_sealing_key().await.unwrap(),
                front_door: false,
                capabilities: engine::roster::full_capabilities(),
            }],
        )
        .await
        .unwrap();
    cell.engine().set_signer(signer).await.unwrap();
    let now = chrono::Utc::now();
    for mut invocation in simulation::fixtures::daily().cells.remove(0).seed {
        match &mut invocation.action {
            Action::CreateFrequency { anchor_at, .. } => {
                *anchor_at = Some((now + chrono::TimeDelta::days(1)).to_rfc3339());
            }
            Action::CreateRecurrence { anchor_at, .. } => {
                *anchor_at = Some(now.to_rfc3339());
            }
            _ => {}
        }
        cell.engine().act(invocation.action, None).await.unwrap();
    }
    let context = Context {
        actor: None,
        window: Window {
            from_ms: now.timestamp_millis(),
            until_ms: now.timestamp_millis() + 5 * 86_400_000,
            timezone: "UTC".into(),
        },
    };
    cell.engine()
        .request_projection(context.clone())
        .await
        .unwrap();
    let spans = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            cell.engine()
                .request_projection(context.clone())
                .await
                .unwrap();
            if let Some(cached) = store::projection::read(
                &cell.engine().store.pool,
                &context,
                chrono::Utc::now().timestamp_millis(),
            )
            .await
            .unwrap()
            {
                assert!(
                    matches!(cached.status, Status::Ready { .. }),
                    "{:?}",
                    cached.status
                );
                assert!(
                    cached
                        .spans
                        .iter()
                        .any(|span| span.quantity.value.to_string() == "-2")
                );
                break cached.spans;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let revision = store::projection::revision(&cell.engine().store.pool)
        .await
        .unwrap();
    cell.shutdown().await;
    let cell = cell::Cell::open(options).await.unwrap();
    let invite = engine::pairing::PairingInvite {
        node_id,
        root_key: Some(root.public_key_b64()),
        label: Some(organ.head),
        addrs: vec!["127.0.0.1:49152".into()],
    };
    store::records::set_extension_raw(
        &cell.engine().store.pool,
        &organ.uid,
        "lince.pairing",
        &serde_json::json!({"invite":invite.encode(),"qr_svg":invite.qr_svg().unwrap()}),
    )
    .await
    .unwrap();
    assert_eq!(
        store::projection::revision(&cell.engine().store.pool)
            .await
            .unwrap(),
        revision
    );
    cell.engine()
        .request_projection(context.clone())
        .await
        .unwrap();
    assert_eq!(
        cell.engine()
            .projection
            .metrics
            .snapshots
            .load(Ordering::Relaxed),
        0
    );
    assert_eq!(
        cell.engine()
            .projection
            .metrics
            .steps
            .load(Ordering::Relaxed),
        0
    );
    assert_eq!(
        cell.engine()
            .projection
            .metrics
            .hits
            .load(Ordering::Relaxed),
        1
    );
    let cached = store::projection::read(
        &cell.engine().store.pool,
        &context,
        chrono::Utc::now().timestamp_millis(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(cached.spans, spans);
    cell.shutdown().await;
}
