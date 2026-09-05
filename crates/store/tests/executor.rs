use store::Store;

async fn record(store: &Store, slug: &str) -> String {
    let organ = store::organs::ensure_local(&store.pool, "me")
        .await
        .unwrap();
    let uid = nucleus::new_uid("r");
    store::sqlx::query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                             organ_uid, created_at, updated_at)
         VALUES (?, ?, 'thing', ?, '', '1', 0, ?, '2026-08-15T00:00:00Z', '2026-08-15T00:00:00Z')",
    )
    .bind(&uid)
    .bind(slug)
    .bind(slug)
    .bind(&organ.uid)
    .execute(&store.pool)
    .await
    .unwrap();
    uid
}

#[tokio::test]
async fn an_undesignated_record_runs_here() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, "undesignated").await;
    assert!(store::executor::runs_here(&store.pool, &uid).await.unwrap());
}

#[tokio::test]
async fn a_record_designated_to_this_cell_runs_here() {
    let store = Store::open_memory().await.unwrap();
    let organ = store::organs::ensure_local(&store.pool, "me")
        .await
        .unwrap();
    let cell = store::cells::ensure_local(&store.pool, &organ.uid, "laptop")
        .await
        .unwrap();
    let uid = record(&store, "mine").await;

    store::executor::designate(&store.pool, &uid, Some(&cell.uid))
        .await
        .unwrap();
    assert!(store::executor::runs_here(&store.pool, &uid).await.unwrap());
}

#[tokio::test]
async fn a_record_designated_to_another_cell_does_not_run_here() {
    let store = Store::open_memory().await.unwrap();
    let organ = store::organs::ensure_local(&store.pool, "me")
        .await
        .unwrap();
    store::cells::ensure_local(&store.pool, &organ.uid, "laptop")
        .await
        .unwrap();
    let uid = record(&store, "theirs").await;

    store::executor::designate(&store.pool, &uid, Some("r_01ARZ3NDEKTSV4RRFFQ69G5FAX"))
        .await
        .unwrap();
    assert!(!store::executor::runs_here(&store.pool, &uid).await.unwrap());
}

#[tokio::test]
async fn clearing_a_designation_returns_the_work_to_every_cell() {
    let store = Store::open_memory().await.unwrap();
    let organ = store::organs::ensure_local(&store.pool, "me")
        .await
        .unwrap();
    store::cells::ensure_local(&store.pool, &organ.uid, "laptop")
        .await
        .unwrap();
    let uid = record(&store, "handed-back").await;

    store::executor::designate(&store.pool, &uid, Some("r_01ARZ3NDEKTSV4RRFFQ69G5FAX"))
        .await
        .unwrap();
    store::executor::designate(&store.pool, &uid, None)
        .await
        .unwrap();

    assert_eq!(
        store::executor::designated(&store.pool, &uid)
            .await
            .unwrap(),
        None
    );
    assert!(store::executor::runs_here(&store.pool, &uid).await.unwrap());
}

#[tokio::test]
async fn a_cell_that_cannot_identify_itself_is_not_the_designated_one() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, "unidentified").await;

    store::executor::designate(&store.pool, &uid, Some("r_01ARZ3NDEKTSV4RRFFQ69G5FAX"))
        .await
        .unwrap();
    assert!(!store::executor::runs_here(&store.pool, &uid).await.unwrap());
}
