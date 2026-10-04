use store::{
    Store,
    organ_access::{self, Status},
};

#[tokio::test]
async fn access_catalog_authenticates_hosts_and_retains_revocation_floor() {
    let store = Store::open_memory().await.unwrap();
    let host = nucleus::new_uid("r");
    store::organs::add_contact(&store.pool, &host, None, "Host", "", 1)
        .await
        .unwrap();
    store::organs::set_trust(&store.pool, &host, "known")
        .await
        .unwrap();
    let mut status = Status {
        organ_uid: host.clone(),
        generation: 1,
        granted: true,
    };
    assert!(
        organ_access::observe(&store.pool, "another-host", &status)
            .await
            .is_err()
    );
    organ_access::observe(&store.pool, &host, &status)
        .await
        .unwrap();
    assert!(
        organ_access::get(&store.pool, &host)
            .await
            .unwrap()
            .unwrap()
            .granted
    );
    status.generation = 2;
    status.granted = false;
    organ_access::observe(&store.pool, &host, &status)
        .await
        .unwrap();
    status.generation = 1;
    status.granted = true;
    organ_access::observe(&store.pool, &host, &status)
        .await
        .unwrap();
    assert!(
        !organ_access::get(&store.pool, &host)
            .await
            .unwrap()
            .unwrap()
            .granted
    );
    store::organs::set_trust(&store.pool, &host, "blocked")
        .await
        .unwrap();
    assert!(
        organ_access::get(&store.pool, &host)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn access_status_is_private_to_the_requesting_peer_and_tracks_revocation() {
    let store = Store::open_memory().await.unwrap();
    store::organs::ensure_local(&store.pool, "").await.unwrap();
    let peer = nucleus::new_uid("r");
    store::organs::add_contact(&store.pool, &peer, None, "Peer", "", 1)
        .await
        .unwrap();
    let person = store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Person",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    assert!(
        !organ_access::for_peer(&store.pool, &peer)
            .await
            .unwrap()
            .granted
    );
    store::logins::grant(&store.pool, &peer, &person)
        .await
        .unwrap();
    let granted = organ_access::for_peer(&store.pool, &peer).await.unwrap();
    assert!(granted.granted);
    assert!(
        !organ_access::for_peer(&store.pool, &nucleus::new_uid("r"))
            .await
            .unwrap()
            .granted
    );
    store::logins::revoke(&store.pool, &peer).await.unwrap();
    let revoked = organ_access::for_peer(&store.pool, &peer).await.unwrap();
    assert!(!revoked.granted);
    assert!(revoked.generation > granted.generation);
}
