use store::{
    Store, StoreError,
    organs::{self, Delivery, presentation},
    sqlx::query,
};

const PEER: &str = "r_THEMTHEMTHEMTHEMTHEMTHEMTH";

async fn contact_fixture(store: &Store) -> Result<(), StoreError> {
    organs::add_contact(&store.pool, PEER, None, "Peer", " http://peer.test/// ", 1).await?;
    organs::set_node_id(&store.pool, PEER, Some("peer-node")).await
}

#[tokio::test]
async fn contact_views_preserve_scope_state_and_local_configuration() -> Result<(), StoreError> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    contact_fixture(&store).await?;
    let local = organs::ensure_local(pool, " https://local.test/// ").await?;
    assert_eq!(local.base_url, "https://local.test");
    assert_eq!(organs::ensure_local(pool, "  /// ").await?, local);
    assert_eq!(
        organs::local_head(pool).await?.as_deref(),
        Some(local.head.as_str())
    );

    organs::set_contact_scope(pool, PEER, Some(&[])).await?;
    organs::set_contact_accept_scope(pool, PEER, None).await?;
    organs::set_mode(pool, PEER, "mailbox").await?;
    organs::set_pending_introduction(pool, PEER, true).await?;
    query("UPDATE organ_contact SET accept_fields = 'unreadable' WHERE record_uid = ?")
        .bind(PEER)
        .execute(pool)
        .await?;

    let contact = organs::contact(pool, PEER).await?.expect("fixture contact");
    assert_eq!(contact.base_url, "http://peer.test");
    assert_eq!(contact.delivery(), Delivery::Mailbox);
    assert_eq!(contact.scope_fields, Some(Vec::new()));
    assert_eq!(contact.accept_fields, None);
    assert_eq!(contact.accept_unreadable.as_deref(), Some("unreadable"));
    assert_eq!(organs::pending_introductions(pool).await?.len(), 1);

    let mut details = presentation::for_records(pool, &[PEER, "missing", PEER]).await?;
    let view = details.remove(PEER).expect("fixture contact presentation");
    assert!(details.is_empty());
    assert_eq!(view.delivery, contact.delivery());
    assert_eq!(view.scope_fields, contact.scope_fields);
    assert_eq!(view.scope_version, contact.scope_version);
    assert_eq!(view.accept_unreadable, contact.accept_unreadable);
    assert_eq!(view.accept_version, contact.accept_version);

    let mut peers = presentation::peers_by_node(pool, &["peer-node"]).await?;
    assert_eq!(peers.remove("peer-node").expect("known peer").head, "Peer");
    Ok(())
}

#[tokio::test]
async fn malformed_proximity_is_rejected_by_contact_and_presentation_reads()
-> Result<(), StoreError> {
    let store = Store::open_memory().await?;
    let pool = &store.pool;
    contact_fixture(&store).await?;
    organs::set_proximity(pool, PEER, u32::MAX).await?;
    assert_eq!(
        organs::contact(pool, PEER)
            .await?
            .expect("fixture contact")
            .proximity,
        u32::MAX
    );

    for invalid in [-1, i64::from(u32::MAX) + 1] {
        query("UPDATE organ_contact SET proximity = ? WHERE record_uid = ?")
            .bind(invalid)
            .bind(PEER)
            .execute(pool)
            .await?;
        assert!(matches!(
            organs::contact_by_node_id(pool, "peer-node").await,
            Err(StoreError::Protocol(message)) if message.contains("proximity")
        ));
        assert!(matches!(
            presentation::for_records(pool, &[PEER]).await,
            Err(StoreError::Protocol(message)) if message.contains("proximity")
        ));
    }
    Ok(())
}
