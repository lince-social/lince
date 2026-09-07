use store::Store;

async fn record(store: &Store, slug: &str, kind: &str) -> String {
    let organ = store::organs::ensure_local(&store.pool, "me")
        .await
        .unwrap();
    let uid = nucleus::new_uid("r");
    store::sqlx::query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                             organ_uid, created_at, updated_at)
         VALUES (?, ?, ?, ?, '', '0', 0, ?, '2026-08-15T00:00:00Z', '2026-08-15T00:00:00Z')",
    )
    .bind(&uid)
    .bind(slug)
    .bind(kind)
    .bind(slug)
    .bind(&organ.uid)
    .execute(&store.pool)
    .await
    .unwrap();
    uid
}

async fn person(store: &Store, slug: &str) -> String {
    record(store, slug, "person").await
}

#[tokio::test]
async fn a_person_nobody_has_touched_is_active() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "maria").await;

    assert!(store::people::is_active(&store.pool, &uid).await.unwrap());
    assert_eq!(
        store::people::standing(&store.pool, &uid).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn deactivating_stops_them_and_records_when() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "joao").await;

    store::people::deactivate(&store.pool, &uid, "2026-08-15T12:00:00Z", Some("moved out"))
        .await
        .unwrap();

    assert!(!store::people::is_active(&store.pool, &uid).await.unwrap());
    let standing = store::people::standing(&store.pool, &uid)
        .await
        .unwrap()
        .expect("standing was written");
    assert_eq!(standing.at.as_deref(), Some("2026-08-15T12:00:00Z"));
    assert_eq!(standing.note.as_deref(), Some("moved out"));
}

#[tokio::test]
async fn reactivating_returns_them_to_untouched() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "ana").await;

    store::people::deactivate(&store.pool, &uid, "2026-08-15T12:00:00Z", None)
        .await
        .unwrap();
    store::people::reactivate(&store.pool, &uid).await.unwrap();

    assert!(store::people::is_active(&store.pool, &uid).await.unwrap());
    assert_eq!(
        store::people::standing(&store.pool, &uid).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn a_deactivated_person_keeps_their_record_and_their_name() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "carla").await;

    store::people::deactivate(&store.pool, &uid, "2026-08-15T12:00:00Z", None)
        .await
        .unwrap();

    let record = store::records::get(&store.pool, &uid)
        .await
        .unwrap()
        .expect("the Person record is still there");
    assert_eq!(record.head, "carla");
    assert_eq!(record.kind, "person");
}

#[tokio::test]
async fn the_deactivated_list_holds_only_the_deactivated() {
    let store = Store::open_memory().await.unwrap();
    let gone = person(&store, "gone").await;
    let here = person(&store, "here").await;
    let back = person(&store, "back").await;

    store::people::deactivate(&store.pool, &gone, "2026-08-15T12:00:00Z", Some("left"))
        .await
        .unwrap();
    store::people::deactivate(&store.pool, &back, "2026-08-14T12:00:00Z", None)
        .await
        .unwrap();
    store::people::reactivate(&store.pool, &back).await.unwrap();

    let listed = store::people::deactivated(&store.pool).await.unwrap();
    assert_eq!(listed.len(), 1, "only `gone` is deactivated: {listed:?}");
    assert_eq!(listed[0].0, gone);
    assert!(store::people::is_active(&store.pool, &here).await.unwrap());
    assert!(store::people::is_active(&store.pool, &back).await.unwrap());
}

#[tokio::test]
async fn a_missing_record_is_not_an_active_person() {
    let store = Store::open_memory().await.unwrap();

    assert!(
        !store::people::is_active(&store.pool, &nucleus::new_uid("r"))
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn a_deleted_person_is_not_active() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "deleted").await;

    assert!(
        store::records::mark_deleted(&store.pool, &uid)
            .await
            .unwrap()
    );
    assert!(!store::people::is_active(&store.pool, &uid).await.unwrap());
}

#[tokio::test]
async fn a_non_person_record_is_not_an_active_person() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, "ordinary", "plain").await;

    assert!(!store::people::is_active(&store.pool, &uid).await.unwrap());
}

#[tokio::test]
async fn an_unreadable_standing_is_refused() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "corrupt").await;

    store::records::set_extension_raw(
        &store.pool,
        &uid,
        store::people::NAMESPACE,
        &serde_json::json!({ store::people::STANDING_KEY: "not an object" }),
    )
    .await
    .unwrap();

    assert!(store::people::standing(&store.pool, &uid).await.is_err());
    assert!(store::people::is_active(&store.pool, &uid).await.is_err());
}

#[tokio::test]
async fn non_object_person_extension_data_is_refused() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "non-object").await;

    store::records::set_extension_raw(
        &store.pool,
        &uid,
        store::people::NAMESPACE,
        &serde_json::json!(["standing"]),
    )
    .await
    .unwrap();

    assert!(store::people::is_active(&store.pool, &uid).await.is_err());
}

#[tokio::test]
async fn invalid_person_extension_json_is_refused() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "invalid-json").await;

    store::records::set_extension_raw(
        &store.pool,
        &uid,
        store::people::NAMESPACE,
        &serde_json::json!({}),
    )
    .await
    .unwrap();
    store::sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query(
        "UPDATE record_extension SET fds = 'not-json'
          WHERE record_uid = ? AND namespace = ?",
    )
    .bind(&uid)
    .bind(store::people::NAMESPACE)
    .execute(&store.pool)
    .await
    .unwrap();

    assert!(store::people::standing(&store.pool, &uid).await.is_err());
    assert!(store::people::is_active(&store.pool, &uid).await.is_err());
}

#[tokio::test]
async fn a_database_error_does_not_become_an_active_person() {
    let store = Store::open_memory().await.unwrap();
    let uid = person(&store, "closed").await;
    store.pool.close().await;

    assert!(store::people::is_active(&store.pool, &uid).await.is_err());
}

#[test]
fn the_standing_field_is_the_one_the_sync_filter_looks_for() {
    assert!(store::people::is_standing_field("lince.person.standing"));
    assert!(!store::people::is_standing_field("lince.person"));
    assert!(!store::people::is_standing_field(
        "lince.schedule.executor.cell"
    ));
}
