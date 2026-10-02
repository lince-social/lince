use super::*;

async fn hosted(host: &Engine, node: &str, document: &Profile) -> Value {
    host.social_public_request(
        "profile-owner",
        node,
        PublicRequest::PublishProfile {
            document: document.clone(),
        },
        nucleus::execution::now().timestamp(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn owner_authority_advance_recovers_a_host_that_missed_more_than_64_profile_ancestors() {
    let clock = nucleus::execution::Execution::new([211; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            let directory = tempfile::tempdir().unwrap();
            let (owner, _, _, organ) = devices(directory.path()).await;
            let mut hosts = Vec::new();
            for secret in [212, 213] {
                let host = Engine::open_memory().await.unwrap();
                let node = iroh::SecretKey::from_bytes(&[secret; 32])
                    .public()
                    .to_string();
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
                hosts.push((host, node));
            }
            let destinations: Vec<String> = hosts.iter().map(|(_, node)| node.clone()).collect();
            let fields = ProfileFields {
                name: "One Organ on two hosts".into(),
                ..Default::default()
            };
            let mut saved = command(
                &owner,
                Command::SaveProfile {
                    fields: fields.clone(),
                    parents: vec![],
                    destinations: destinations.clone(),
                },
            )
            .await;
            let initial: Profile = serde_json::from_value(saved["profile"].clone()).unwrap();
            for (host, node) in &hosts {
                assert_eq!(hosted(host, node, &initial).await["accepted"], true);
            }
            for step in 0..70 {
                clock
                    .set_time(clock.now().timestamp_millis() + 1000)
                    .unwrap();
                saved = command(
                    &owner,
                    Command::SaveProfile {
                        fields: ProfileFields {
                            description: format!("Current workshop description {step}"),
                            ..fields.clone()
                        },
                        parents: vec![saved["hash"].as_str().unwrap().into()],
                        destinations: destinations.clone(),
                    },
                )
                .await;
                let current: Profile = serde_json::from_value(saved["profile"].clone()).unwrap();
                assert_eq!(
                    hosted(&hosts[0].0, &hosts[0].1, &current).await["accepted"],
                    true
                );
            }
            let current: Profile = serde_json::from_value(saved["profile"].clone()).unwrap();
            assert_eq!(current.parents.len(), 64);
            assert!(
                !current
                    .parents
                    .contains(&document_hash("profile", &initial).unwrap())
            );
            let missed = hosted(&hosts[1].0, &hosts[1].1, &current).await;
            assert_eq!(missed["accepted"], false);
            assert_eq!(missed["state"], "conflict");
            command(&owner, Command::RotateProfileAuthority).await;
            let resolved = command(
                &owner,
                Command::SaveProfile {
                    fields: current.fields.clone(),
                    parents: vec![saved["hash"].as_str().unwrap().into()],
                    destinations: destinations.clone(),
                },
            )
            .await;
            let resolved: Profile = serde_json::from_value(resolved["profile"].clone()).unwrap();
            assert_eq!(resolved.authority.organ, organ);
            assert_eq!(resolved.authority.root_key, initial.authority.root_key);
            assert_ne!(resolved.authority.editor_key, initial.authority.editor_key);
            assert_eq!(resolved.authority.generation, "2");
            for (host, node) in &hosts {
                assert_eq!(hosted(host, node, &resolved).await["accepted"], true);
                let view = host
                    .social_public_request(
                        "profile-reader",
                        node,
                        PublicRequest::FetchProfile {
                            organ: organ.clone(),
                        },
                        clock.now().timestamp(),
                    )
                    .await
                    .unwrap();
                assert_eq!(view["state"], "active");
                assert_eq!(view["profile"]["fields"], json!(current.fields));
                assert!(
                    host.social_public_request(
                        "old-profile-editor",
                        node,
                        PublicRequest::PublishProfile {
                            document: current.clone()
                        },
                        clock.now().timestamp()
                    )
                    .await
                    .is_err()
                );
            }
        }))
        .await;
}
