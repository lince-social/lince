use nucleus::social::{AuthorMode, Direction, PostState, Search, Snippet};
use serde_json::Value;
use store::{Store, social};

fn document(index: usize) -> Snippet {
    Snippet {
        protocol: "lince.snippet.1".into(),
        id: format!("post_{index:026}"),
        nonce: format!("fixture-{index}"),
        revision: "1".into(),
        parent: None,
        resolves: vec![],
        created_at: 1000,
        issued_at: 1000,
        expires_at: 2000,
        mode: AuthorMode::Anonymous,
        signing_key: "store-fixture-authority".into(),
        profile: None,
        anonymous: None,
        alias: String::new(),
        title: "Bicycle repair".into(),
        text: "Public tools".into(),
        direction: Direction::Need,
        quantity: None,
        unit: Some("hours".into()),
        concept: Some("repairs".into()),
        language: "pt".into(),
        area: "workshop".into(),
        availability: String::new(),
        state: PostState::Active,
        redistribute: false,
        destinations: vec!["host-a".into()],
        reply: None,
        signature: "store-fixture-prevalidated".into(),
    }
}

async fn put(store: &Store, document: &Snippet) {
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    social::put_snippet_on(
        &mut tx,
        document,
        &format!("{:064x}", document.revision.parse::<u64>().unwrap()),
        "fixture",
        1000,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

fn ids(rows: &[Value]) -> Vec<String> {
    rows.iter()
        .map(|row| row["document"]["id"].as_str().unwrap().to_owned())
        .collect()
}

async fn unrelated_entries(store: &Store) {
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    for index in 700..10001 {
        let mut doc = document(index);
        doc.title = "Other topic".into();
        doc.text = "Different query terms".into();
        store::sqlx::query("INSERT INTO social_document(kind,id,authority,revision,hash,body,expires_at,state,title,text,direction,language,area,concept,unit,source) VALUES('snippet',?,?,1,?,?,?,'active',?,?,'need','pt','workshop','repairs','hours','fixture')")
            .bind(&doc.id).bind(&doc.signing_key).bind(format!("{index:064x}"))
            .bind(serde_json::to_string(&doc).unwrap()).bind(doc.expires_at)
            .bind(&doc.title).bind(&doc.text).execute(&mut *tx).await.unwrap();
        store::sqlx::query("INSERT INTO social_search(id,title,text) VALUES(?,?,?)")
            .bind(&doc.id)
            .bind(&doc.title)
            .bind(&doc.text)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn dense_and_sparse_pages_never_skip_the_unprobed_tail() {
    let store = Store::open_memory().await.unwrap();
    for index in 0..700 {
        let mut doc = document(index);
        if index % 10 == 0 {
            doc.text = "Public tools with rare orchids".into();
        }
        put(&store, &doc).await;
    }
    unrelated_entries(&store).await;
    for (text, step, total) in [("bicycle repair", 1, 700), ("rare orchids", 10, 70)] {
        let mut query = Search {
            text: text.into(),
            ..Default::default()
        };
        let mut collected = Vec::new();
        loop {
            let page = social::search(&store.pool, &query, 1000).await.unwrap();
            assert!(page.len() <= 50);
            let page_ids = ids(&page);
            if page_ids.is_empty() {
                break;
            }
            assert!(page_ids.windows(2).all(|pair| pair[0] < pair[1]));
            assert!(
                query
                    .after
                    .as_ref()
                    .is_none_or(|after| page_ids[0] > *after)
            );
            query.after = page_ids.last().cloned();
            collected.extend(page_ids);
        }
        let expected: Vec<_> = (0..700).step_by(step).map(|i| document(i).id).collect();
        assert_eq!(collected.len(), total);
        assert_eq!(collected, expected);
    }
    for text in ["post", "bicycle\" OR \"repair"] {
        assert!(
            social::search(
                &store.pool,
                &Search {
                    text: text.into(),
                    ..Default::default()
                },
                1000,
            )
            .await
            .unwrap()
            .is_empty()
        );
    }
}

#[tokio::test]
async fn sparse_filters_and_host_visibility_remain_complete_after_the_probe() {
    let store = Store::open_memory().await.unwrap();
    for index in 0..700 {
        let mut doc = document(index);
        if index >= 600 {
            doc.direction = Direction::Contribution;
            doc.language = "en".into();
            doc.area = "garden".into();
            doc.concept = Some("planting".into());
            doc.unit = Some("days".into());
            doc.destinations = vec!["host-b".into()];
        }
        put(&store, &doc).await;
    }
    unrelated_entries(&store).await;
    let query = Search {
        text: "bicycle".into(),
        direction: Some(Direction::Contribution),
        language: "en".into(),
        area: "garden".into(),
        concept: "planting".into(),
        unit: "days".into(),
        ..Default::default()
    };
    let page = social::search_public(&store.pool, &query, 1000, "host-b")
        .await
        .unwrap();
    assert_eq!(
        ids(&page),
        (600..650).map(|i| document(i).id).collect::<Vec<_>>()
    );
    assert!(page.iter().all(|row| row["source"] == "host-b"));
    assert!(
        social::search_public(&store.pool, &query, 1000, "host-a")
            .await
            .unwrap()
            .is_empty()
    );
    let all = Search {
        text: "bicycle".into(),
        ..Default::default()
    };
    assert_eq!(
        ids(&social::search_public(&store.pool, &all, 1000, "host-b")
            .await
            .unwrap()),
        ids(&page)
    );
    let mut shared = document(10);
    shared.revision = "2".into();
    shared.redistribute = true;
    put(&store, &shared).await;
    let page = social::search_public(&store.pool, &all, 1000, "host-c")
        .await
        .unwrap();
    assert_eq!(ids(&page), vec![shared.id]);
    let mut private = document(11);
    private.revision = "2".into();
    private.redistribute = true;
    private.destinations.clear();
    put(&store, &private).await;
    assert_eq!(
        social::search_public(&store.pool, &all, 1000, "host-c")
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn stale_index_entries_do_not_override_controls_and_edits_survive_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("search.sqlite");
    let url = format!("sqlite://{}", path.display());
    let store = Store::open(&url).await.unwrap();
    for index in 0..80 {
        put(&store, &document(index)).await;
    }
    for (index, state) in [
        (0, "revoked"),
        (1, "conflict"),
        (2, "withdrawn"),
        (3, "paused"),
    ] {
        store::sqlx::query("UPDATE social_document SET state=? WHERE id=?")
            .bind(state)
            .bind(document(index).id)
            .execute(&store.pool)
            .await
            .unwrap();
    }
    store::sqlx::query("UPDATE social_document SET expires_at=1000 WHERE id=?")
        .bind(document(4).id)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("INSERT INTO social_listing_removal(post,reason,removed_at) VALUES(?,'reviewed removal',1000)")
        .bind(document(5).id).execute(&store.pool).await.unwrap();
    for text in ["", "bicycle repair"] {
        let page = social::search_public(
            &store.pool,
            &Search {
                text: text.into(),
                ..Default::default()
            },
            1000,
            "host-a",
        )
        .await
        .unwrap();
        assert_eq!(
            ids(&page),
            (6..56).map(|i| document(i).id).collect::<Vec<_>>()
        );
    }
    let mut edited = document(6);
    edited.title = "Unique pottery".into();
    edited.revision = "2".into();
    put(&store, &edited).await;
    store.pool.close().await;
    let store = Store::open(&url).await.unwrap();
    let query = Search {
        text: "unique pottery".into(),
        ..Default::default()
    };
    assert_eq!(
        ids(&social::search(&store.pool, &query, 1000).await.unwrap()),
        vec![edited.id.clone()]
    );
    let page = social::search(
        &store.pool,
        &Search {
            text: "bicycle".into(),
            ..Default::default()
        },
        1000,
    )
    .await
    .unwrap();
    assert!(!ids(&page).contains(&edited.id));
    store.pool.close().await;
}

#[tokio::test]
async fn indexed_expiry_cleanup_removes_only_expired_announcement_terms() {
    let store = Store::open_memory().await.unwrap();
    for index in 0..80 {
        let mut doc = document(index);
        if index < 2 {
            doc.created_at = 0;
            doc.issued_at = 0;
            doc.expires_at = if index == 0 { 1000 } else { 300 };
        }
        put(&store, &doc).await;
    }
    for _ in 0..2 {
        social::prune(&store.pool, 1000).await.unwrap();
        let indexed: Vec<String> =
            store::sqlx::query_scalar("SELECT id FROM social_search ORDER BY id")
                .fetch_all(&store.pool)
                .await
                .unwrap();
        assert_eq!(
            indexed,
            (2..80).map(|index| document(index).id).collect::<Vec<_>>()
        );
        let held: Vec<String> =
            store::sqlx::query_scalar("SELECT id FROM social_document ORDER BY id")
                .fetch_all(&store.pool)
                .await
                .unwrap();
        assert_eq!(
            held,
            (0..80)
                .filter(|index| *index != 1)
                .map(|index| document(index).id)
                .collect::<Vec<_>>()
        );
        let rows = social::search(
            &store.pool,
            &Search {
                text: "bicycle repair".into(),
                ..Default::default()
            },
            1000,
        )
        .await
        .unwrap();
        assert_eq!(
            ids(&rows),
            (2..52).map(|index| document(index).id).collect::<Vec<_>>()
        );
    }
}
