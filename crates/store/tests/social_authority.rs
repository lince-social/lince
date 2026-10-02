use nucleus::social::{Delegation, PostState, Profile, ProfileFields, RootSuccession};
use store::{Store, social};

fn authority(organ: &str, generation: i64, root: &str, editor: &str) -> Delegation {
    Delegation {
        organ: organ.into(),
        root_key: root.into(),
        editor_key: editor.into(),
        generation: generation.to_string(),
        issued_at: 1000,
        expires_at: 2000,
        successions: vec![],
        signature: "already-verified".into(),
    }
}

fn profile(authority: Delegation, revision: i64, parents: Vec<String>) -> Profile {
    Profile {
        protocol: "lince.profile.1".into(),
        authority,
        revision: revision.to_string(),
        parents,
        issued_at: 1000,
        expires_at: 2000,
        fields: ProfileFields {
            name: "Workshop".into(),
            ..Default::default()
        },
        state: PostState::Active,
        destinations: vec![],
        signature: "already-verified".into(),
    }
}

#[tokio::test]
async fn revoked_editor_and_root_pin_are_retained_after_cache_pruning() {
    let store = Store::open_memory().await.unwrap();
    let organ = nucleus::new_uid("r");
    let old = authority(&organ, 1, "root-one", "editor-one");
    let current = authority(&organ, 2, "root-one", "editor-two");
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    social::put_profile_on(
        &mut tx,
        &profile(old.clone(), 1, vec![]),
        &"a".repeat(64),
        "host",
    )
    .await
    .unwrap();
    social::anchor_profile_authority_on(&mut tx, &current, false)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    social::prune(&store.pool, 2601).await.unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        social::anchor_profile_authority_on(&mut tx, &old, false)
            .await
            .is_err()
    );
    social::anchor_profile_authority_on(&mut tx, &old, true)
        .await
        .unwrap();
    assert!(
        social::anchor_profile_authority_on(
            &mut tx,
            &authority(&organ, 3, "impostor", "editor-three"),
            false
        )
        .await
        .is_err()
    );
    let held: (String, String, i64) = store::sqlx::query_as(
        "SELECT root_key,editor_key,generation FROM social_profile_authority WHERE organ=?",
    )
    .bind(&organ)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(held, ("root-one".into(), "editor-two".into(), 2));
}

#[tokio::test]
async fn accepted_identity_succession_advances_the_pin_and_cannot_be_reversed() {
    let store = Store::open_memory().await.unwrap();
    let organ = nucleus::new_uid("r");
    let old = authority(&organ, 1, "root-one", "editor-one");
    let mut next = authority(&organ, 2, "root-two", "editor-two");
    next.successions.push(RootSuccession {
        old_key: "root-one".into(),
        new_key: "root-two".into(),
        created_at: "1000".into(),
        signature: "already-verified".into(),
    });
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    social::anchor_profile_authority_on(&mut tx, &old, false)
        .await
        .unwrap();
    social::anchor_profile_authority_on(&mut tx, &next, false)
        .await
        .unwrap();
    assert!(
        social::anchor_profile_authority_on(&mut tx, &old, false)
            .await
            .is_err()
    );
    let root: String =
        store::sqlx::query_scalar("SELECT root_key FROM social_profile_authority WHERE organ=?")
            .bind(&organ)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(root, "root-two");
}

#[tokio::test]
async fn revision_floor_and_visible_conflicts_survive_display_expiry() {
    let store = Store::open_memory().await.unwrap();
    let organ = nucleus::new_uid("r");
    let authority = authority(&organ, 1, "root-one", "editor-one");
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let first = "a".repeat(64);
    let second = "b".repeat(64);
    social::put_profile_on(
        &mut tx,
        &profile(authority.clone(), 1, vec![]),
        &first,
        "host",
    )
    .await
    .unwrap();
    social::put_profile_on(
        &mut tx,
        &profile(authority.clone(), 2, vec![first.clone()]),
        &second,
        "host",
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    social::prune(&store.pool, 2601).await.unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        !social::put_profile_on(
            &mut tx,
            &profile(authority.clone(), 1, vec![]),
            &first,
            "replay"
        )
        .await
        .unwrap()
    );
    assert!(
        !social::put_profile_on(
            &mut tx,
            &profile(authority.clone(), 3, vec![first]),
            &"c".repeat(64),
            "fork"
        )
        .await
        .unwrap()
    );
    let state: String = store::sqlx::query_scalar(
        "SELECT state FROM social_document WHERE kind='profile' AND id=?",
    )
    .bind(&organ)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(state, "conflict");
    assert!(
        social::put_profile_on(
            &mut tx,
            &profile(authority, 4, vec![second, "c".repeat(64)]),
            &"d".repeat(64),
            "resolved"
        )
        .await
        .unwrap()
    );
}

#[tokio::test]
async fn a_new_authority_generation_recovers_from_a_revoked_editors_maximum_revision() {
    let store = Store::open_memory().await.unwrap();
    let organ = nucleus::new_uid("r");
    let old = authority(&organ, 1, "root", "old-editor");
    let next = authority(&organ, 2, "root", "fresh-editor");
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    social::put_profile_on(
        &mut tx,
        &profile(old.clone(), i64::MAX, vec![]),
        &"a".repeat(64),
        "revoked editor",
    )
    .await
    .unwrap();
    social::anchor_profile_authority_on(&mut tx, &next, false)
        .await
        .unwrap();
    assert!(
        social::put_profile_on(
            &mut tx,
            &profile(next, 1, vec![]),
            &"b".repeat(64),
            "owner recovery",
        )
        .await
        .unwrap()
    );
    assert!(
        social::put_profile_on(
            &mut tx,
            &profile(old, i64::MAX, vec![]),
            &"a".repeat(64),
            "revoked replay",
        )
        .await
        .is_err()
    );
    let held: (i64, i64, String) = store::sqlx::query_as(
        "SELECT generation,profile_revision,profile_hash FROM social_profile_authority WHERE organ=?",
    )
    .bind(&organ)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(held, (2, 1, "b".repeat(64)));
    tx.commit().await.unwrap();
}
