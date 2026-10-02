use super::*;

#[tokio::test]
async fn page_ranking_keeps_the_raw_id_cursor_and_does_not_skip_the_next_page() {
    let clock = nucleus::execution::Execution::new([245; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            let author = Engine::open_memory().await.unwrap();
            let client = Engine::open_memory().await.unwrap();
            let endpoint = iroh::SecretKey::from_bytes(&[246; 32]).public().to_string();
            let mut ids = Vec::new();
            for _ in 0..51 {
                clock
                    .set_time(clock.now().timestamp_millis() + 1000)
                    .unwrap();
                let (_, document, _) = publish(&author, vec![endpoint.clone()]).await;
                let hash = document_hash("snippet", &document).unwrap();
                let mut tx = store::write_tx(&client.store.pool).await.unwrap();
                store::social::put_snippet_on(
                    &mut tx,
                    &document,
                    &hash,
                    &endpoint,
                    clock.now().timestamp(),
                )
                .await
                .unwrap();
                tx.commit().await.unwrap();
                ids.push(document.id);
            }
            ids.sort();
            let first = search(&client, vec![]).await;
            assert_eq!(first["results"].as_array().unwrap().len(), 50);
            assert_eq!(first["next_after"], ids[49]);
            let rows = first["results"].as_array().unwrap();
            assert!(
                rows.windows(2)
                    .all(|pair| pair[0]["document"]["issued_at"].as_i64().unwrap()
                        >= pair[1]["document"]["issued_at"].as_i64().unwrap())
            );
            let mut returned: Vec<String> = rows
                .iter()
                .map(|row| row["document"]["id"].as_str().unwrap().to_owned())
                .collect();
            returned.sort();
            assert_eq!(returned, ids[..50]);
            let second = command(
                &client,
                Command::Search {
                    query: Search {
                        text: "bicycle".into(),
                        after: Some(ids[49].clone()),
                        ..Default::default()
                    },
                    services: vec![],
                },
            )
            .await;
            assert_eq!(second["results"].as_array().unwrap().len(), 1);
            assert_eq!(second["results"][0]["document"]["id"], ids[50]);
        }))
        .await;
}

#[tokio::test]
async fn exhausted_evidence_count_or_bytes_still_hides_a_proven_conflict() {
    let clock = nucleus::execution::Execution::new([239; 32], 1_790_899_200_000).unwrap();
    clock.scope(Box::pin(async {
        for full_bytes in [false, true] {
            let author = Engine::open_memory().await.unwrap();
            let client = Engine::open_memory().await.unwrap();
            let endpoint = iroh::SecretKey::from_bytes(&[240;32]).public().to_string();
            let (_, document, signer) = publish(&author, vec![endpoint.clone()]).await;
            let changed = variant(&document, &signer);
            let network = Arc::new(Sources {pages:Mutex::new(BTreeMap::from([(endpoint.clone(), page(&document))])),known:Mutex::new(Vec::new())});
            client.attach_social_network(network.clone());
            assert_eq!(search(&client, vec![endpoint.clone()]).await["results"].as_array().unwrap().len(),1);
            let mut tx = store::write_tx(&client.store.pool).await.unwrap();
            if full_bytes {
                store::sqlx::query("INSERT INTO social_discovery_conflict VALUES('byte-quota-fixture',1,1,'first',?,'second',?,1)")
                    .bind("a".repeat(4*1024*1024)).bind("b".repeat(4*1024*1024)).execute(&mut *tx).await.unwrap();
            } else {
                let ids:Vec<String> = (0..256).map(|value|format!("count-quota-fixture-{value}")).collect();
                store::sqlx::query("INSERT INTO social_discovery_conflict SELECT value,1,1,'first','{}','second','{}',1 FROM json_each(?)")
                    .bind(serde_json::to_string(&ids).unwrap()).execute(&mut *tx).await.unwrap();
            }
            tx.commit().await.unwrap();
            network.pages.lock().unwrap().insert(endpoint.clone(),page(&changed));
            let hidden = search(&client, vec![endpoint]).await;
            assert!(hidden["results"].as_array().unwrap().is_empty());
            assert_eq!(hidden["conflicts"][0]["post"],document.id);
            assert_eq!(hidden["conflicts"][0]["evidence_limited"],true);
            assert_eq!(store::sqlx::query_scalar::<_,String>("SELECT state FROM social_document WHERE kind='snippet' AND id=?").bind(&document.id).fetch_one(&client.store.pool).await.unwrap(),"conflict");
            assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM social_discovery_conflict WHERE post=?").bind(&document.id).fetch_one(&client.store.pool).await.unwrap(),0);
        }
    })).await;
}

#[tokio::test]
async fn conflict_paging_advances_past_muted_or_unmatched_candidates() {
    let clock = nucleus::execution::Execution::new([241; 32], 1_790_899_200_000).unwrap();
    clock
        .scope(Box::pin(async {
            let author = Engine::open_memory().await.unwrap();
            let client = Engine::open_memory().await.unwrap();
            let endpoint = iroh::SecretKey::from_bytes(&[242; 32]).public().to_string();
            let network = Arc::new(Sources {
                pages: Mutex::new(BTreeMap::new()),
                known: Mutex::new(Vec::new()),
            });
            client.attach_social_network(network.clone());
            let mut ids = Vec::new();
            let mut documents = Vec::new();
            for _ in 0..13 {
                let (_, document, signer) = publish(&author, vec![endpoint.clone()]).await;
                network
                    .pages
                    .lock()
                    .unwrap()
                    .insert(endpoint.clone(), page(&document));
                search(&client, vec![endpoint.clone()]).await;
                ids.push(document.id.clone());
                documents.push((document, signer));
            }
            ids.sort();
            for post in &ids[..12] {
                command(
                    &client,
                    Command::MutePost {
                        post: post.clone(),
                        whole_author: false,
                    },
                )
                .await;
            }
            for (document, signer) in documents {
                network
                    .pages
                    .lock()
                    .unwrap()
                    .insert(endpoint.clone(), page(&variant(&document, &signer)));
                search(&client, vec![endpoint.clone()]).await;
            }
            let first = search(&client, vec![]).await;
            assert!(first["conflicts"].as_array().unwrap().is_empty());
            assert_eq!(first["next_conflict_after"], ids[11]);
            let second = command(
                &client,
                Command::Search {
                    query: Search {
                        text: "bicycle".into(),
                        after: Some(ids[11].clone()),
                        ..Default::default()
                    },
                    services: vec![],
                },
            )
            .await;
            assert_eq!(second["conflicts"].as_array().unwrap().len(), 1);
            assert_eq!(second["conflicts"][0]["post"], ids[12]);
            assert_eq!(second["next_conflict_after"], Value::Null);
            let unmatched = command(
                &client,
                Command::Search {
                    query: Search {
                        text: "unmatched".into(),
                        ..Default::default()
                    },
                    services: vec![],
                },
            )
            .await;
            assert!(unmatched["conflicts"].as_array().unwrap().is_empty());
            assert_eq!(unmatched["next_conflict_after"], ids[11]);
        }))
        .await;
}

#[tokio::test]
async fn a_signed_contradiction_cannot_replace_an_existing_withdrawal() {
    let clock = nucleus::execution::Execution::new([243; 32], 1_790_899_200_000).unwrap();
    clock.scope(Box::pin(async {
        let author = Engine::open_memory().await.unwrap();
        let client = Engine::open_memory().await.unwrap();
        let endpoint = iroh::SecretKey::from_bytes(&[244;32]).public().to_string();
        let (record,_,signer) = publish(&author,vec![endpoint.clone()]).await;
        let withdrawn = transition(&author,&record,PostState::Withdrawn).await;
        let network = Arc::new(Sources {pages:Mutex::new(BTreeMap::from([(endpoint.clone(),json!({"results":[],"updates":[{"document":withdrawn,"hash":document_hash("snippet",&withdrawn).unwrap()}]}))])),known:Mutex::new(Vec::new())});
        client.attach_social_network(network.clone());
        search(&client,vec![endpoint.clone()]).await;
        let mut conflicting = withdrawn.clone();
        conflicting.state = PostState::Active;
        conflicting.signature = signer.sign_bytes(&signing_bytes("snippet",&conflicting).unwrap());
        network.pages.lock().unwrap().insert(endpoint.clone(),page(&conflicting));
        let result = search(&client,vec![endpoint.clone()]).await;
        assert!(result["results"].as_array().unwrap().is_empty());
        assert_eq!(result["conflicts"][0]["withdrawn"],true);
        assert_eq!(store::sqlx::query_scalar::<_,String>("SELECT state FROM social_document WHERE kind='snippet' AND id=?").bind(&withdrawn.id).fetch_one(&client.store.pool).await.unwrap(),"withdrawn");
        assert_eq!(store::sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM social_ended_post WHERE id=?").bind(&withdrawn.id).fetch_one(&client.store.pool).await.unwrap(),1);
        command(&client,Command::ConfigureServices {settings:ServiceSettings {directory:true,..Default::default()}}).await;
        let response = client.social_public_request("reader",&endpoint,PublicRequest::Search {query:Search {text:"bicycle".into(),..Default::default()},known:vec![format!("{}:{}",withdrawn.id,document_hash("snippet",&conflicting).unwrap())]},clock.now().timestamp()).await.unwrap();
        assert!(response["results"].as_array().unwrap().is_empty());
        assert_eq!(response["updates"][0]["document"]["state"],"withdrawn");
    })).await;
}
