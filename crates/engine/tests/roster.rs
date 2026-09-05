use engine::Engine;
use engine::roster::{CellEntry, RosterOutcome, SignedRoster};
use engine::trust::Signer;

async fn cell(base_url: &str) -> (Engine, String) {
    let e = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&e.store.pool, base_url)
        .await
        .expect("local organ")
        .uid;
    (e, organ)
}

fn entry(label: &str, node_id: &str, key: &str) -> CellEntry {
    CellEntry {
        cell_uid: format!("c-{label}"),
        node_id: node_id.into(),
        label: label.into(),
        operational_key: key.into(),
        sealing_key: None,
        front_door: false,
        capabilities: engine::roster::full_capabilities(),
    }
}

#[tokio::test]
async fn a_roster_of_one_is_published_signed_and_readable() {
    let (e, organ) = cell("http://a.test").await;
    let root = Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
    e.publish_root_key(&root).await.expect("publish root key");

    let signed = e
        .publish_roster(&root, vec![entry("laptop", "node-1", "opkey-1")])
        .await
        .expect("publish");

    assert_eq!(signed.roster.version, 1, "versions start at one");
    assert_eq!(signed.roster.cells.len(), 1, "a roster of one");
    assert_eq!(signed.roster.root_key, root.public_key_b64());

    let second = e
        .publish_roster(&root, vec![entry("laptop", "node-1", "opkey-1")])
        .await
        .expect("republish");
    assert_eq!(second.roster.version, 2);

    let held = e.roster_of(&organ).await.expect("read").expect("present");
    assert_eq!(held.roster.version, 2);
}

#[tokio::test]
async fn a_contact_accepts_a_roster_that_chains_from_the_key_it_paired_with() {
    let (them, their_organ) = cell("http://them.test").await;
    let (us, _) = cell("http://us.test").await;

    let their_root = Signer::generate(&their_organ, engine::roster::ROOT_KEY_ID);
    them.publish_root_key(&their_root).await.expect("publish");
    let signed = them
        .publish_roster(&their_root, vec![entry("vps", "node-vps", "opkey-vps")])
        .await
        .expect("publish roster");

    engine::trust::adopt_key(
        &us.store,
        &their_organ,
        engine::roster::ROOT_KEY_ID,
        &their_root.public_key_b64(),
    )
    .await
    .expect("adopt at pairing");

    assert_eq!(
        us.adopt_roster(&signed).await.expect("adopt"),
        RosterOutcome::Accepted
    );
    assert_eq!(
        us.roster_node_ids(&their_organ).await.expect("node ids"),
        vec!["node-vps".to_string()],
        "roster cells are additional dial candidates"
    );

    assert_eq!(
        us.adopt_roster(&signed).await.expect("replay"),
        RosterOutcome::NotNewer
    );
}

#[tokio::test]
async fn a_roster_that_does_not_chain_is_refused_and_changes_nothing() {
    let (them, their_organ) = cell("http://them.test").await;
    let (us, _) = cell("http://us.test").await;

    let their_root = Signer::generate(&their_organ, engine::roster::ROOT_KEY_ID);
    them.publish_root_key(&their_root).await.expect("publish");
    let genuine = them
        .publish_roster(
            &their_root,
            vec![entry("laptop", "node-real", "opkey-real")],
        )
        .await
        .expect("genuine roster");

    engine::trust::adopt_key(
        &us.store,
        &their_organ,
        engine::roster::ROOT_KEY_ID,
        &their_root.public_key_b64(),
    )
    .await
    .expect("pairing");
    assert_eq!(
        us.adopt_roster(&genuine).await.expect("adopt"),
        RosterOutcome::Accepted
    );

    let attacker = Signer::generate(&their_organ, engine::roster::ROOT_KEY_ID);
    let forged_roster = engine::roster::Roster {
        organ_uid: their_organ.clone(),
        root_key: attacker.public_key_b64(),
        version: 99,
        not_after: (chrono::Utc::now() + chrono::Duration::days(30)).to_rfc3339(),
        pickup: Vec::new(),
        cells: vec![entry("attacker", "node-evil", "opkey-evil")],
    };
    let payload = engine::roster::roster_signing_payload(&forged_roster).expect("payload");
    let forged = SignedRoster {
        signature: attacker.sign_bytes(&payload),
        roster: forged_roster,
    };

    assert_eq!(
        us.adopt_roster(&forged).await.expect("adopt forged"),
        RosterOutcome::Refused,
        "a roster must chain from a key we already hold"
    );

    let held = us
        .roster_of(&their_organ)
        .await
        .expect("read")
        .expect("still present");
    assert_eq!(held.roster.version, genuine.roster.version);
    assert_eq!(held.roster.root_key, their_root.public_key_b64());
    assert_eq!(
        us.roster_node_ids(&their_organ).await.expect("node ids"),
        vec!["node-real".to_string()],
        "the attacker's Cell must never become a dial candidate"
    );
}

#[tokio::test]
async fn rotation_works_through_a_succession_chain() {
    let (them, their_organ) = cell("http://them.test").await;
    let (us, _) = cell("http://us.test").await;

    let old_root = Signer::generate(&their_organ, engine::roster::ROOT_KEY_ID);
    let new_root = Signer::generate(&their_organ, engine::roster::ROOT_KEY_ID);

    engine::trust::adopt_key(
        &us.store,
        &their_organ,
        engine::roster::ROOT_KEY_ID,
        &old_root.public_key_b64(),
    )
    .await
    .expect("pairing on the OLD key");

    assert!(
        !us.key_chains(&their_organ, &new_root.public_key_b64())
            .await
            .expect("chain"),
        "an unendorsed new key must not be trusted"
    );

    let created_at = chrono::Utc::now().to_rfc3339();
    let payload = engine::roster::succession_signing_payload(
        &their_organ,
        &old_root.public_key_b64(),
        &new_root.public_key_b64(),
        &created_at,
    );
    let signature = old_root.sign_bytes(&payload);
    assert!(
        us.adopt_succession(
            &their_organ,
            &old_root.public_key_b64(),
            &new_root.public_key_b64(),
            &created_at,
            &signature,
        )
        .await
        .expect("adopt succession"),
    );
    assert!(
        us.key_chains(&their_organ, &new_root.public_key_b64())
            .await
            .expect("chain"),
        "an endorsed key chains"
    );

    them.publish_root_key(&new_root).await.expect("publish");
    let signed = them
        .publish_roster(&new_root, vec![entry("phone", "node-phone", "opkey-phone")])
        .await
        .expect("roster");
    assert_eq!(
        us.adopt_roster(&signed).await.expect("adopt"),
        RosterOutcome::Accepted
    );
}

#[tokio::test]
async fn a_revoked_key_stops_chaining_even_though_it_still_verifies() {
    let (them, their_organ) = cell("http://them.test").await;
    let (us, _) = cell("http://us.test").await;

    let root = Signer::generate(&their_organ, engine::roster::ROOT_KEY_ID);
    engine::trust::adopt_key(
        &us.store,
        &their_organ,
        engine::roster::ROOT_KEY_ID,
        &root.public_key_b64(),
    )
    .await
    .expect("pairing");
    assert!(
        us.key_chains(&their_organ, &root.public_key_b64())
            .await
            .expect("chain")
    );

    let (key, signature) = them.revocation_certificate(&root);
    assert!(
        us.adopt_revocation(&their_organ, &key, &signature)
            .await
            .expect("adopt revocation"),
    );

    assert!(
        !us.key_chains(&their_organ, &root.public_key_b64())
            .await
            .expect("chain"),
        "a revoked key must stop speaking for the Organ"
    );

    them.publish_root_key(&root).await.expect("publish");
    let signed = them
        .publish_roster(&root, vec![entry("stolen", "node-stolen", "opkey")])
        .await
        .expect("roster");
    assert_eq!(
        us.adopt_roster(&signed).await.expect("adopt"),
        RosterOutcome::Refused
    );
}

#[tokio::test]
async fn an_expired_roster_is_refused_but_leaves_the_old_one_dialable() {
    let (them, their_organ) = cell("http://them.test").await;
    let (us, _) = cell("http://us.test").await;

    let root = Signer::generate(&their_organ, engine::roster::ROOT_KEY_ID);
    them.publish_root_key(&root).await.expect("publish");
    engine::trust::adopt_key(
        &us.store,
        &their_organ,
        engine::roster::ROOT_KEY_ID,
        &root.public_key_b64(),
    )
    .await
    .expect("pairing");

    let good = them
        .publish_roster(&root, vec![entry("laptop", "node-good", "opkey")])
        .await
        .expect("roster");
    assert_eq!(
        us.adopt_roster(&good).await.expect("adopt"),
        RosterOutcome::Accepted
    );

    let stale_roster = engine::roster::Roster {
        organ_uid: their_organ.clone(),
        root_key: root.public_key_b64(),
        version: 50,
        not_after: (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339(),
        pickup: Vec::new(),
        cells: vec![entry("new", "node-new", "opkey-new")],
    };
    let payload = engine::roster::roster_signing_payload(&stale_roster).expect("payload");
    let stale = SignedRoster {
        signature: root.sign_bytes(&payload),
        roster: stale_roster,
    };

    assert_eq!(
        us.adopt_roster(&stale).await.expect("adopt"),
        RosterOutcome::Expired,
        "an expired roster must not introduce new Cells"
    );
    assert_eq!(
        us.roster_node_ids(&their_organ).await.expect("node ids"),
        vec!["node-good".to_string()],
        "the last-known-good roster stays dialable rather than dropping the contact"
    );
}

#[test]
fn a_reserved_separator_in_a_label_is_rejected_not_escaped() {
    let roster = engine::roster::Roster {
        organ_uid: "o-1".into(),
        root_key: "k".into(),
        version: 1,
        not_after: "2030-01-01T00:00:00Z".into(),
        pickup: Vec::new(),
        cells: vec![entry("lap\u{1f}top", "node", "key")],
    };
    assert!(engine::roster::roster_signing_payload(&roster).is_err());
}

#[tokio::test]
async fn an_enrolment_token_works_once_and_grows_the_roster() {
    let (e, organ) = cell("http://a.test").await;
    let root = Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
    e.publish_root_key(&root).await.expect("publish");
    e.publish_roster(&root, vec![entry("laptop", "node-1", "op-1")])
        .await
        .expect("roster");

    let token = e.issue_enrolment_token().await.expect("token");

    let roster = e
        .redeem_enrolment(&root, &token, entry("phone", "node-2", "op-2"))
        .await
        .expect("enrol");
    assert_eq!(roster.roster.cells.len(), 2, "the roster must grow");
    assert_eq!(roster.roster.version, 2, "and be republished, newer");

    assert!(
        e.redeem_enrolment(&root, &token, entry("attacker", "node-3", "op-3"))
            .await
            .is_err(),
        "an enrolment token must work exactly once"
    );
    let held = e.roster_of(&organ).await.expect("read").expect("present");
    assert_eq!(
        held.roster.cells.len(),
        2,
        "the replay must not have landed"
    );

    assert!(
        e.redeem_enrolment(&root, "not-a-real-token", entry("x", "node-4", "op-4"))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn revoking_a_cell_republishes_a_newer_roster_without_it() {
    let (e, organ) = cell("http://a.test").await;
    let root = Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
    e.publish_root_key(&root).await.expect("publish");
    e.publish_roster(
        &root,
        vec![
            entry("laptop", "node-1", "op-1"),
            entry("stolen", "node-2", "op-2"),
        ],
    )
    .await
    .expect("roster");

    let after = e.revoke_cell(&root, "c-stolen").await.expect("revoke");
    assert_eq!(after.roster.cells.len(), 1);
    assert_eq!(
        after.roster.version, 2,
        "newer, so the old cannot be replayed"
    );
    assert!(
        !after.roster.cells.iter().any(|c| c.label == "stolen"),
        "the revoked Cell must be gone"
    );
    let _ = organ;
}

#[test]
fn root_key_detach_refuses_without_a_verified_copy() {
    let dir = std::env::temp_dir().join(format!("lince-root-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("dir");
    let local = dir.join("root.key");
    std::fs::write(&local, [7u8; 32]).expect("write");

    let missing = dir.join("nope.key");
    assert!(engine::roster::detach_root_key(&local, &missing).is_err());
    assert!(local.exists(), "a failed detach must not delete the key");

    let wrong = dir.join("wrong.key");
    std::fs::write(&wrong, [9u8; 32]).expect("write");
    assert!(engine::roster::detach_root_key(&local, &wrong).is_err());
    assert!(local.exists());

    let good = dir.join("usb.key");
    engine::roster::export_root_key(&local, &good).expect("export");
    assert!(
        engine::roster::export_root_key(&local, &good).is_err(),
        "export must never overwrite a file that might be another identity"
    );
    assert!(engine::roster::root_key_present(&local));
    engine::roster::detach_root_key(&local, &good).expect("detach");
    assert!(
        !engine::roster::root_key_present(&local),
        "after detach the root is gone from this Cell"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_signed_succession_travels_and_lets_a_rotated_roster_land() {
    let (them, their_organ) = cell("http://them.test").await;
    let (us, _) = cell("http://us.test").await;

    let old_root = Signer::generate(&their_organ, engine::roster::ROOT_KEY_ID);
    let new_root = Signer::generate(&their_organ, engine::roster::ROOT_KEY_ID);
    engine::trust::adopt_key(
        &us.store,
        &their_organ,
        engine::roster::ROOT_KEY_ID,
        &old_root.public_key_b64(),
    )
    .await
    .expect("pairing on the OLD key");

    assert!(
        them.published_successions(&their_organ)
            .await
            .expect("successions")
            .is_empty()
    );

    them.sign_succession(&old_root, &new_root.public_key_b64())
        .await
        .expect("sign the succession");

    let published = them
        .published_successions(&their_organ)
        .await
        .expect("successions");
    assert_eq!(published.len(), 1, "the endorsement is there to be served");
    let cert = &published[0];
    assert_eq!(cert.old_key, old_root.public_key_b64());
    assert_eq!(cert.new_key, new_root.public_key_b64());

    assert!(
        us.adopt_succession(
            &their_organ,
            &cert.old_key,
            &cert.new_key,
            &cert.created_at,
            &cert.signature,
        )
        .await
        .expect("adopt"),
        "a succession signed by the held key is accepted"
    );

    them.publish_root_key(&new_root).await.expect("publish");
    let signed = them
        .publish_roster(&new_root, vec![entry("phone", "node-phone", "opkey-phone")])
        .await
        .expect("roster");
    assert_eq!(
        us.adopt_roster(&signed).await.expect("adopt"),
        RosterOutcome::Accepted,
        "and the rotated roster lands with nobody re-pairing"
    );
}

#[tokio::test]
async fn a_succession_from_an_unheld_key_is_refused() {
    let (them, their_organ) = cell("http://them.test").await;
    let (us, _) = cell("http://us.test").await;

    let real_root = Signer::generate(&their_organ, engine::roster::ROOT_KEY_ID);
    let attacker = Signer::generate(&their_organ, engine::roster::ROOT_KEY_ID);
    let theirs_next = Signer::generate(&their_organ, engine::roster::ROOT_KEY_ID);
    engine::trust::adopt_key(
        &us.store,
        &their_organ,
        engine::roster::ROOT_KEY_ID,
        &real_root.public_key_b64(),
    )
    .await
    .expect("pairing");

    them.sign_succession(&attacker, &theirs_next.public_key_b64())
        .await
        .expect("sign");
    let cert = &them
        .published_successions(&their_organ)
        .await
        .expect("successions")[0];
    assert!(
        !us.adopt_succession(
            &their_organ,
            &cert.old_key,
            &cert.new_key,
            &cert.created_at,
            &cert.signature,
        )
        .await
        .expect("adopt"),
        "an endorsement that chains from nothing we hold is refused"
    );
    assert!(
        !us.key_chains(&their_organ, &theirs_next.public_key_b64())
            .await
            .expect("chain"),
        "and the key it tried to install still speaks for nobody"
    );
}

#[test]
fn revoking_a_different_cell_still_needs_publishing() {
    use engine::roster::{Roster, needs_publishing};

    let held = SignedRoster {
        roster: Roster {
            organ_uid: "organ-1".into(),
            root_key: "root".into(),
            version: 3,
            not_after: "2026-09-01T00:00:00+00:00".into(),
            pickup: Vec::new(),
            cells: vec![
                entry("laptop", "n-laptop", "k-laptop"),
                entry("phone", "n-phone", "k-phone"),
            ],
        },
        signature: "sig".into(),
    };

    assert!(
        !needs_publishing(
            Some(&held),
            "root",
            &[
                entry("laptop", "n-laptop", "k-laptop"),
                entry("phone", "n-phone", "k-phone"),
            ]
        ),
        "an unchanged roster must not burn a version on every boot"
    );
    assert!(
        !needs_publishing(
            Some(&held),
            "root",
            &[
                entry("phone", "n-phone", "k-phone"),
                entry("laptop", "n-laptop", "k-laptop"),
            ]
        ),
        "reordering is not a change"
    );
    assert!(
        needs_publishing(
            Some(&held),
            "root",
            &[entry("laptop", "n-laptop", "k-laptop")]
        ),
        "THE BUG: the phone was revoked and this Cell is still present, which \
         used to read as unchanged"
    );
    assert!(
        needs_publishing(
            Some(&held),
            "root",
            &[
                entry("laptop", "n-laptop", "k-laptop"),
                entry("phone", "n-phone", "k-phone"),
                entry("vps", "n-vps", "k-vps"),
            ]
        ),
        "an enrolment is a change too"
    );
    assert!(
        needs_publishing(
            Some(&held),
            "rotated-root",
            &[
                entry("laptop", "n-laptop", "k-laptop"),
                entry("phone", "n-phone", "k-phone"),
            ]
        ),
        "a rotated root must be published under the new key"
    );
    assert!(
        needs_publishing(None, "root", &[entry("laptop", "n-laptop", "k-laptop")]),
        "with nothing held there is nothing to compare against"
    );
}

#[test]
fn a_capability_less_member_forces_a_republish() {
    use engine::roster::{Roster, needs_publishing};

    let mut stale = entry("laptop", "n-laptop", "k-laptop");
    stale.capabilities.clear();
    let held = SignedRoster {
        roster: Roster {
            organ_uid: "organ-1".into(),
            root_key: "root".into(),
            version: 1,
            not_after: "2026-09-01T00:00:00+00:00".into(),
            pickup: Vec::new(),
            cells: vec![stale.clone()],
        },
        signature: "sig".into(),
    };

    assert!(needs_publishing(Some(&held), "root", &[stale]));
}

#[tokio::test]
async fn a_contact_offline_across_two_rotations_still_chains() {
    let (them, their_organ) = cell("http://rotating.test").await;
    let (us, _) = cell("http://returning.test").await;

    let first = Signer::generate(&their_organ, engine::roster::ROOT_KEY_ID);
    let second = Signer::generate(&their_organ, engine::roster::ROOT_KEY_ID);
    let third = Signer::generate(&their_organ, engine::roster::ROOT_KEY_ID);

    engine::trust::adopt_key(
        &us.store,
        &their_organ,
        engine::roster::ROOT_KEY_ID,
        &first.public_key_b64(),
    )
    .await
    .expect("pairing on the first key");

    them.sign_succession(&first, &second.public_key_b64())
        .await
        .expect("first rotation");
    them.sign_succession(&second, &third.public_key_b64())
        .await
        .expect("second rotation");

    for cert in them
        .published_successions(&their_organ)
        .await
        .expect("successions")
    {
        us.adopt_succession(
            &their_organ,
            &cert.old_key,
            &cert.new_key,
            &cert.created_at,
            &cert.signature,
        )
        .await
        .expect("adopt");
    }

    assert!(
        us.key_chains(&their_organ, &third.public_key_b64())
            .await
            .expect("chain"),
        "a key two rotations away must chain from the one we last knew"
    );
    them.publish_root_key(&third).await.expect("publish");
    let signed = them
        .publish_roster(&third, vec![entry("laptop", "node-laptop", "opkey-laptop")])
        .await
        .expect("roster");
    assert_eq!(
        us.adopt_roster(&signed).await.expect("adopt"),
        RosterOutcome::Accepted,
        "and a roster signed by it is accepted without re-pairing"
    );
}

#[tokio::test]
async fn a_revoked_key_cannot_endorse_a_successor() {
    let (them, their_organ) = cell("http://stolen.test").await;
    let (us, _) = cell("http://careful-chain.test").await;

    let root = Signer::generate(&their_organ, engine::roster::ROOT_KEY_ID);
    let stolen = Signer::generate(&their_organ, engine::roster::ROOT_KEY_ID);
    let thiefs_choice = Signer::generate(&their_organ, engine::roster::ROOT_KEY_ID);

    engine::trust::adopt_key(
        &us.store,
        &their_organ,
        engine::roster::ROOT_KEY_ID,
        &root.public_key_b64(),
    )
    .await
    .expect("pairing");
    them.sign_succession(&root, &stolen.public_key_b64())
        .await
        .expect("rotation");
    for cert in them
        .published_successions(&their_organ)
        .await
        .expect("successions")
    {
        us.adopt_succession(
            &their_organ,
            &cert.old_key,
            &cert.new_key,
            &cert.created_at,
            &cert.signature,
        )
        .await
        .expect("adopt");
    }
    us.adopt_revocation(
        &their_organ,
        &stolen.public_key_b64(),
        &them.revocation_certificate(&stolen).1,
    )
    .await
    .expect("revocation");
    assert!(
        !us.key_chains(&their_organ, &stolen.public_key_b64())
            .await
            .expect("chain"),
        "the revoked key itself must not chain"
    );

    let created_at = chrono::Utc::now().to_rfc3339();
    let payload = engine::roster::succession_signing_payload(
        &their_organ,
        &stolen.public_key_b64(),
        &thiefs_choice.public_key_b64(),
        &created_at,
    );
    let signature = stolen.sign_bytes(&payload);
    let _ = us
        .adopt_succession(
            &their_organ,
            &stolen.public_key_b64(),
            &thiefs_choice.public_key_b64(),
            &created_at,
            &signature,
        )
        .await;

    assert!(
        !us.key_chains(&their_organ, &thiefs_choice.public_key_b64())
            .await
            .expect("chain"),
        "a revoked key must not be able to install a successor"
    );
}

#[tokio::test]
async fn a_swapped_sealing_key_breaks_the_root_signature() {
    let (them, their_organ) = cell("http://them.test").await;
    let (us, _) = cell("http://us.test").await;
    let root = Signer::generate(&their_organ, engine::roster::ROOT_KEY_ID);
    them.publish_root_key(&root)
        .await
        .expect("publish root key");

    engine::trust::adopt_key(
        &us.store,
        &their_organ,
        engine::roster::ROOT_KEY_ID,
        &root.public_key_b64(),
    )
    .await
    .expect("adopt at pairing");

    let (_, published) = engine::seal::generate("c-laptop", 1, "2099-01-01T00:00:00Z");
    let mut listed = entry("laptop", "node-1", "opkey-1");
    listed.sealing_key = Some(published);
    let signed = them
        .publish_roster(&root, vec![listed])
        .await
        .expect("publish");

    assert_eq!(
        us.adopt_roster(&signed).await.expect("adopt"),
        RosterOutcome::Accepted
    );
    let held = us
        .roster_of(&their_organ)
        .await
        .expect("read")
        .expect("held");
    assert!(held.roster.cells[0].sealing_key.is_some());

    let (_, rotated) = engine::seal::generate("c-laptop", 2, "2099-06-01T00:00:00Z");
    let mut rotating = entry("laptop", "node-1", "opkey-1");
    rotating.sealing_key = Some(rotated);
    let later = them
        .publish_roster(&root, vec![rotating])
        .await
        .expect("publish a rotation");
    assert!(later.roster.version > signed.roster.version);

    let (_, theirs) = engine::seal::generate("c-laptop", 2, "2099-06-01T00:00:00Z");
    let mut forged: SignedRoster = later.clone();
    forged.roster.cells[0].sealing_key = Some(theirs.clone());
    assert_eq!(
        us.adopt_roster(&forged).await.expect("refuses"),
        RosterOutcome::Refused
    );

    let after = us
        .roster_of(&their_organ)
        .await
        .expect("read")
        .expect("held");
    assert_ne!(
        after.roster.cells[0]
            .sealing_key
            .as_ref()
            .map(|k| &k.public),
        Some(&theirs.public)
    );
}

#[tokio::test]
async fn the_mirrored_roster_carries_the_sealing_key_for_the_device_list() {
    let (e, organ) = cell("http://mirror.test").await;
    let root = Signer::generate(&organ, engine::roster::ROOT_KEY_ID);
    e.publish_root_key(&root).await.expect("publish root key");

    let (_, published) = engine::seal::generate("c-laptop", 1, "2099-01-01T00:00:00Z");
    let mut listed = entry("laptop", "node-1", "opkey-1");
    listed.sealing_key = Some(published.clone());
    e.publish_roster(&root, vec![listed])
        .await
        .expect("publish");

    let mirrored = store::records::get_extension(&e.store.pool, &organ, "lince.roster")
        .await
        .expect("read the projection")
        .expect("the projection exists");
    let shown = mirrored["cells"][0]["sealing_key"]["key_id"]
        .as_str()
        .expect("the device list can see the mail key");
    assert_eq!(shown, published.key_id);
}
