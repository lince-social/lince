use super::*;

#[tokio::test]
async fn pending_profile_images_respect_existing_sync_budget_without_partial_draft_writes() {
    let clock = nucleus::execution::Execution::new([211; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            let directory = tempfile::tempdir().unwrap();
            let (owner, device, _, organ) = devices(directory.path()).await;
            let initial = command(
                &owner,
                Command::SaveProfile {
                    fields: ProfileFields {
                        name: "Base workshop".into(),
                        ..Default::default()
                    },
                    parents: vec![],
                    destinations: vec![],
                },
            )
            .await;
            copy(&owner, &device, &organ).await;
            clock
                .set_time(clock.now().timestamp_millis() + 8 * 86400 * 1000)
                .unwrap();
            let mut image = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                16,
                16,
                image::Rgb([20, 50, 90]),
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
            let mut state =
                store::records::get_extension(&device.store.pool, &organ, PROFILE_NAMESPACE)
                    .await
                    .unwrap()
                    .unwrap();
            state["budget_fixture"] = json!("");
            let retained_bytes = serde_json::to_vec(&state).unwrap().len();
            state["budget_fixture"] =
                json!("x".repeat(store::records::MAX_EXTENSION_BYTES - retained_bytes - 200));
            store::records::set_extension(&device.store.pool, &organ, PROFILE_NAMESPACE, &state)
                .await
                .unwrap();
            let error = device
                .social_command(
                    Command::SaveProfile {
                        fields: ProfileFields {
                            name: "A pending image edit beyond the shared budget".into(),
                            avatar: Some(prepared["asset_hash"].as_str().unwrap().into()),
                            ..Default::default()
                        },
                        parents: vec![initial["hash"].as_str().unwrap().into()],
                        destinations: vec![],
                    },
                    None,
                    clock.now(),
                )
                .await
                .unwrap_err();
            assert!(
                error.to_string().contains("Resolve saved profile branches"),
                "{error}"
            );
            assert_eq!(
                store::records::get_extension(&device.store.pool, &organ, PROFILE_NAMESPACE)
                    .await
                    .unwrap()
                    .unwrap(),
                state
            );
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM social_publication_job")
                    .fetch_one(&device.store.pool)
                    .await
                    .unwrap(),
                0
            );
        }))
        .await;
}

#[tokio::test]
async fn signed_pending_images_reject_wrong_hash_unselected_corrupt_and_oversized_data() {
    let clock = nucleus::execution::Execution::new([209; 32], 1_790_899_200_000).unwrap();
    clock.scope(Box::pin(async {
        let directory = tempfile::tempdir().unwrap();
        let (owner,device,_,organ) = devices(directory.path()).await;
        let network = Arc::new(LocalSocialHost {engine:Arc::new(Engine::open_memory().await.unwrap()),node:iroh::SecretKey::from_bytes(&[210;32]).public().to_string()});
        owner.attach_social_network(network.clone());
        let initial = command(&owner,Command::SaveProfile {fields:ProfileFields {name:"Base workshop".into(),..Default::default()},parents:vec![],destinations:vec![]}).await;
        copy(&owner,&device,&organ).await;
        clock.set_time(clock.now().timestamp_millis()+8*86400*1000).unwrap();
        let mut image = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(16,16,image::Rgb([80,40,10]))).write_to(&mut image,image::ImageFormat::Png).unwrap();
        let prepared = command(&device,Command::ImportProfileImageData {encoded:B64.encode(image.into_inner())}).await;
        let hash = prepared["asset_hash"].as_str().unwrap();
        command(&device,Command::SaveProfile {fields:ProfileFields {name:"Rejected image change".into(),avatar:Some(hash.into()),..Default::default()},parents:vec![initial["hash"].as_str().unwrap().into()],destinations:vec![]}).await;
        copy(&device,&owner,&organ).await;
        let source = store::records::get_extension(&owner.store.pool,&organ,PROFILE_NAMESPACE).await.unwrap().unwrap();
        let (key,saved) = source.as_object().unwrap().iter().find(|(key,_)|key.starts_with("pending_profile_")).unwrap();
        assert_eq!(saved["images"].as_array().unwrap().len(),1);
        let signing_key = device.operational_key_for(&organ).await.unwrap();
        for kind in ["wrong-hash","unselected","corrupt","oversized","unsigned-tamper"] {
            let mut draft = saved.clone();
            match kind {
                "wrong-hash" => draft["images"][0]["encoded"] = json!(B64.encode(b"a different content hash")),
                "unselected" => draft["images"][0]["hash"] = json!("a".repeat(64)),
                "corrupt" => {
                    let bytes = b"this cannot be decoded as an image";
                    let corrupt = nucleus::fact::sha256_hex(bytes);
                    draft["fields"]["avatar"] = json!(corrupt);
                    draft["images"][0] = json!({"hash":corrupt,"encoded":B64.encode(bytes)});
                }
                "oversized" => draft["images"][0]["encoded"] = json!(B64.encode(vec![7;128*1024+1])),
                "unsigned-tamper" => draft["fields"]["name"] = json!("Unapproved altered draft"),
                _ => unreachable!(),
            }
            draft["retry_at"] = json!(0);
            draft["error"] = Value::Null;
            if kind != "unsigned-tamper" {
                let payload = json!({"organ":draft["organ"],"cell":draft["cell"],"device_key":draft["device_key"],"fields":draft["fields"],"images":draft["images"],"parents":draft["parents"],"destinations":draft["destinations"],"state":draft["state"],"saved_at":draft["saved_at"]});
                draft["signature"] = json!(signing_key.sign_bytes(&engine::social::signing_bytes("private-profile-draft",&payload).unwrap()));
            }
            let mut current = store::records::get_extension(&owner.store.pool,&organ,PROFILE_NAMESPACE).await.unwrap().unwrap();
            current[key] = draft;
            store::records::set_extension(&owner.store.pool,&organ,PROFILE_NAMESPACE,&current).await.unwrap();
            owner.social_publish_once().await.unwrap();
            let current = store::records::get_extension(&owner.store.pool,&organ,PROFILE_NAMESPACE).await.unwrap().unwrap();
            assert_eq!(current["published"]["fields"]["name"],"Base workshop","{kind}");
            assert!(current[key]["error"].as_str().is_some(),"{kind}");
            assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM social_public_asset").fetch_one(&owner.store.pool).await.unwrap(),0,"{kind}");
            assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM social_publication_job WHERE body LIKE '%Rejected image%' OR body LIKE '%Unapproved altered%'").fetch_one(&owner.store.pool).await.unwrap(),0,"{kind}");
        }
    })).await;
}
