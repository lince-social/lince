use super::*;

async fn publisher(owner: &Engine) -> Arc<LocalSocialHost> {
    let host = Arc::new(LocalSocialHost {
        engine: Arc::new(Engine::open_memory().await.unwrap()),
        node: iroh::SecretKey::from_bytes(&[206; 32]).public().to_string(),
    });
    owner.attach_social_network(host.clone());
    host
}

async fn base(owner: &Engine) -> Value {
    command(
        owner,
        Command::SaveProfile {
            fields: ProfileFields {
                name: "Base workshop".into(),
                ..Default::default()
            },
            parents: vec![],
            destinations: vec![],
        },
    )
    .await
}

#[tokio::test]
async fn removed_enrolled_device_draft_stays_visible_but_cannot_publish() {
    let clock = nucleus::execution::Execution::new([207; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            let directory = tempfile::tempdir().unwrap();
            let (owner, device, root, organ) = devices(directory.path()).await;
            let _network = publisher(&owner).await;
            let initial = base(&owner).await;
            copy(&owner, &device, &organ).await;
            clock
                .set_time(clock.now().timestamp_millis() + 8 * 86400 * 1000)
                .unwrap();
            command(
                &device,
                Command::SaveProfile {
                    fields: ProfileFields {
                        name: "Removed editor's queued change".into(),
                        ..Default::default()
                    },
                    parents: vec![initial["hash"].as_str().unwrap().into()],
                    destinations: vec![],
                },
            )
            .await;
            copy(&device, &owner, &organ).await;
            let local = store::cells::local(&owner.store.pool)
                .await
                .unwrap()
                .unwrap()
                .uid;
            let mut members = owner.roster_of(&organ).await.unwrap().unwrap().roster.cells;
            members.retain(|entry| entry.cell_uid == local);
            owner.publish_roster(&root, members).await.unwrap();
            owner.social_publish_once().await.unwrap();
            let state = store::records::get_extension(&owner.store.pool, &organ, PROFILE_NAMESPACE)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(state["published"]["fields"]["name"], "Base workshop");
            let pending = state
                .as_object()
                .unwrap()
                .iter()
                .find(|(key, _)| key.starts_with("pending_profile_"))
                .unwrap()
                .1;
            assert_eq!(pending["fields"]["name"], "Removed editor's queued change");
            assert!(
                pending["error"]
                    .as_str()
                    .unwrap()
                    .contains("no longer has valid write authority")
            );
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM social_publication_job WHERE body LIKE '%Removed editor%'"
                )
                .fetch_one(&owner.store.pool)
                .await
                .unwrap(),
                0
            );
        }))
        .await;
}

#[tokio::test]
async fn offline_device_draft_preserves_concurrent_owner_edit_until_explicit_resolution() {
    let clock = nucleus::execution::Execution::new([208; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            let directory = tempfile::tempdir().unwrap();
            let (owner, device, _, organ) = devices(directory.path()).await;
            let _network = publisher(&owner).await;
            let initial = base(&owner).await;
            copy(&owner, &device, &organ).await;
            clock
                .set_time(clock.now().timestamp_millis() + 8 * 86400 * 1000)
                .unwrap();
            command(
                &device,
                Command::SaveProfile {
                    fields: ProfileFields {
                        name: "Offline device name".into(),
                        ..Default::default()
                    },
                    parents: vec![initial["hash"].as_str().unwrap().into()],
                    destinations: vec![],
                },
            )
            .await;
            command(
                &owner,
                Command::SaveProfile {
                    fields: ProfileFields {
                        name: "Base workshop".into(),
                        description: "Owner's concurrent description".into(),
                        ..Default::default()
                    },
                    parents: vec![initial["hash"].as_str().unwrap().into()],
                    destinations: vec![],
                },
            )
            .await;
            copy(&device, &owner, &organ).await;
            owner.social_publish_once().await.unwrap();
            let overview = command(&owner, Command::Overview).await;
            let editor = &overview["profile"]["editor"];
            assert_eq!(editor["heads"].as_array().unwrap().len(), 2, "{editor}");
            assert_eq!(editor["fields"]["name"], "Offline device name");
            assert_eq!(
                editor["fields"]["description"],
                "Owner's concurrent description"
            );
            let selected: ProfileFields = serde_json::from_value(editor["fields"].clone()).unwrap();
            command(
                &owner,
                Command::SaveProfile {
                    fields: selected.clone(),
                    parents: serde_json::from_value(editor["heads"].clone()).unwrap(),
                    destinations: vec![],
                },
            )
            .await;
            copy(&owner, &device, &organ).await;
            for current in [&owner, &device] {
                let state = command(current, Command::Overview).await;
                assert_eq!(
                    state["profile"]["editor"]["heads"]
                        .as_array()
                        .unwrap()
                        .len(),
                    1
                );
                assert_eq!(state["profile"]["published"]["fields"], json!(selected));
            }
        }))
        .await;
}
