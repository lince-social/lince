use engine::actions::Action;
use engine::roster::{CellEntry, ROOT_KEY_ID, RosterOutcome, full_capabilities};
use engine::trust::{Signer, adopt_key, verify_fact};

#[tokio::test]
async fn a_trusted_roster_verifies_sibling_facts_without_a_separate_introduction() {
    let source = engine::Engine::open_memory().await.unwrap();
    let organ = store::organs::ensure_local(&source.store.pool, "http://source.test")
        .await
        .unwrap();
    let local = store::cells::local(&source.store.pool)
        .await
        .unwrap()
        .unwrap();
    let root = Signer::from_bytes(&organ.uid, ROOT_KEY_ID, [1; 32]);
    let operational = Signer::from_bytes(
        &organ.uid,
        &engine::roster::cell_key_id(&local.uid),
        [2; 32],
    );
    source.set_signer(operational.clone()).await.unwrap();
    source.publish_root_key(&root).await.unwrap();
    let roster = source
        .publish_roster(
            &root,
            vec![CellEntry {
                cell_uid: local.uid,
                node_id: "source-node".into(),
                label: "Source".into(),
                operational_key: operational.public_key_b64(),
                sealing_key: None,
                front_door: false,
                capabilities: full_capabilities(),
            }],
        )
        .await
        .unwrap();
    let outcome = source
        .act(
            Action::CreateRecord {
                slug: Some("stock".into()),
                kind: nucleus::RecordKind::Plain,
                head: "Stock".into(),
                body: String::new(),
                quantity: 10.0,
            },
            None,
        )
        .await
        .unwrap();
    let fact = store::facts::for_record(&source.store.pool, &outcome.created.unwrap(), 1)
        .await
        .unwrap()
        .pop()
        .unwrap();
    let destination = engine::Engine::open_memory().await.unwrap();
    assert!(!verify_fact(&destination.store, &fact).await.unwrap());
    assert!(matches!(
        destination.adopt_roster(&roster).await.unwrap(),
        RosterOutcome::Refused
    ));
    adopt_key(
        &destination.store,
        &organ.uid,
        ROOT_KEY_ID,
        &root.public_key_b64(),
    )
    .await
    .unwrap();
    assert!(matches!(
        destination.adopt_roster(&roster).await.unwrap(),
        RosterOutcome::Accepted
    ));
    assert!(verify_fact(&destination.store, &fact).await.unwrap());
    let mut forged = fact.clone();
    forged.signature = Some(root.sign_bytes(b"a different fact"));
    assert!(!verify_fact(&destination.store, &forged).await.unwrap());
    let mut other_actor = fact;
    other_actor.actor_uid = Some(nucleus::new_uid("r"));
    assert!(!verify_fact(&destination.store, &other_actor).await.unwrap());
}
