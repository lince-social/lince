use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use engine::{
    pairing::EnrolmentInvite,
    roster::{CellEntry, ROOT_KEY_ID, full_capabilities},
    trust::Signer,
};

#[path = "profile_devices/authority.rs"]
mod authority;

#[path = "profile_devices/cleanup.rs"]
mod cleanup;

#[path = "profile_devices/media_denials.rs"]
mod media_denials;

#[path = "profile_devices/missed_history.rs"]
mod missed_history;

#[path = "profile_devices/retirement.rs"]
mod retirement;

#[path = "profile_devices/root_succession.rs"]
mod root_succession;

async fn devices(directory: &std::path::Path) -> (Engine, Engine, Signer, String) {
    let owner = Engine::open_memory().await.unwrap();
    let device = Engine::open_memory().await.unwrap();
    let root_path = directory.join("root.key");
    std::fs::write(&root_path, [201; 32]).unwrap();
    owner.set_root_key_path(root_path);
    let organ = store::organs::local(&owner.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let root = Signer::from_bytes(&organ, ROOT_KEY_ID, [201; 32]);
    let mut members = Vec::new();
    let mut keys = Vec::new();
    for (engine, label, node_secret) in [(&owner, "Owner", 202), (&device, "Second device", 203)] {
        let cell = store::cells::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let key = engine.operational_key_for(&organ).await.unwrap();
        members.push(CellEntry {
            cell_uid: cell,
            node_id: iroh::SecretKey::from_bytes(&[node_secret; 32])
                .public()
                .to_string(),
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
    let roster = owner.publish_roster(&root, members.clone()).await.unwrap();
    device
        .join_organ(
            &EnrolmentInvite {
                node_id: members[0].node_id.clone(),
                organ_uid: organ.clone(),
                root_key: root.public_key_b64(),
                token: "profile-device".into(),
                addrs: Vec::new(),
            },
            &roster,
            keys[1].clone(),
        )
        .await
        .unwrap();
    device.set_signer(keys[1].clone()).await.unwrap();
    (owner, device, root, organ)
}

async fn copy(source: &Engine, target: &Engine, organ: &str) {
    for _ in 0..6 {
        let vector = store::sync_ops::version_vector_for_organ(&target.store.pool, organ)
            .await
            .unwrap();
        let page = source.export_sync_page(organ, &vector, 2000).await.unwrap();
        if page.batch.ops.is_empty() {
            break;
        }
        target.receive_sync_batch(organ, &page.batch).await.unwrap();
    }
}

#[tokio::test]
async fn enrolled_device_offline_avatar_draft_reaches_owner_and_public_host_through_own_sync() {
    let clock = nucleus::execution::Execution::new([204; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            let directory = tempfile::tempdir().unwrap();
            let (owner, device, _, organ) = devices(directory.path()).await;
            let host = Arc::new(Engine::open_memory().await.unwrap());
            let node = iroh::SecretKey::from_bytes(&[205; 32]).public().to_string();
            command(
                &host,
                Command::ConfigureServices {
                    settings: ServiceSettings {
                        directory: true,
                        ..Default::default()
                    },
                },
            )
            .await;
            let network = Arc::new(LocalSocialHost {
                engine: host.clone(),
                node: node.clone(),
            });
            owner.attach_social_network(network.clone());
            let initial = command(
                &owner,
                Command::SaveProfile {
                    fields: ProfileFields {
                        name: "Base workshop".into(),
                        ..Default::default()
                    },
                    parents: vec![],
                    destinations: vec![node.clone()],
                },
            )
            .await;
            owner.social_publish_once().await.unwrap();
            copy(&owner, &device, &organ).await;
            clock
                .set_time(clock.now().timestamp_millis() + 8 * 86400 * 1000)
                .unwrap();
            let mut image = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                32,
                32,
                image::Rgb([10, 80, 160]),
            ))
            .write_to(&mut image, image::ImageFormat::Png)
            .unwrap();
            let prepared = command(
                &device,
                Command::ImportProfileImageData {
                    encoded: B64.encode(image.into_inner()),
                },
            )
            .await;
            let hash = prepared["asset_hash"].as_str().unwrap();
            let fields = ProfileFields {
                name: "Avatar saved on an offline device".into(),
                avatar: Some(hash.into()),
                ..Default::default()
            };
            let pending = command(
                &device,
                Command::SaveProfile {
                    fields: fields.clone(),
                    parents: vec![initial["hash"].as_str().unwrap().into()],
                    destinations: vec![node.clone()],
                },
            )
            .await;
            assert_eq!(pending["profile_draft"]["fields"], json!(fields));
            assert_eq!(pending["profile_draft"]["images"][0]["hash"], hash);
            assert!(pending["profile_draft"]["images"][0]["encoded"].is_null());
            let view = command(&device, Command::Overview).await;
            assert!(
                view["profile"]["editor"]["pending_drafts"][0]["images"][0]["encoded"].is_null()
            );
            let private =
                store::records::get_extension(&device.store.pool, &organ, PROFILE_NAMESPACE)
                    .await
                    .unwrap()
                    .unwrap();
            let retained = private
                .as_object()
                .unwrap()
                .iter()
                .find(|(key, _)| key.starts_with("pending_profile_"))
                .unwrap()
                .1;
            assert!(retained["images"][0]["encoded"].as_str().is_some());
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM social_public_asset WHERE hash=?"
                )
                .bind(hash)
                .fetch_one(&owner.store.pool)
                .await
                .unwrap(),
                0
            );
            copy(&device, &owner, &organ).await;
            for _ in 0..6 {
                owner.social_publish_once().await.unwrap();
            }
            let retained =
                store::records::get_extension(&owner.store.pool, &organ, PROFILE_NAMESPACE)
                    .await
                    .unwrap()
                    .unwrap();
            assert_eq!(retained["published"]["fields"], json!(fields), "{retained}");
            let hosted = host
                .social_public_request(
                    "profile-device",
                    &node,
                    PublicRequest::FetchProfile {
                        organ: organ.clone(),
                    },
                    clock.now().timestamp(),
                )
                .await
                .unwrap();
            assert_eq!(hosted["profile"]["fields"], json!(fields));
            let asset = host
                .social_public_request(
                    "profile-device",
                    &node,
                    PublicRequest::FetchProfileImage {
                        organ: organ.clone(),
                        hash: hash.into(),
                    },
                    clock.now().timestamp(),
                )
                .await
                .unwrap();
            assert_eq!(asset["hash"], hash);
            assert_eq!(
                nucleus::fact::sha256_hex(&B64.decode(asset["encoded"].as_str().unwrap()).unwrap()),
                hash
            );
            copy(&owner, &device, &organ).await;
            assert_eq!(
                store::records::get_extension(&device.store.pool, &organ, PROFILE_NAMESPACE)
                    .await
                    .unwrap()
                    .unwrap()["published"]["fields"],
                json!(fields)
            );
        }))
        .await;
}
