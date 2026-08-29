//! The blind mailbox (Ontology C4): a carrier that holds sealed mail for
//! Organs it serves and can read none of it.
//!
//! Two Cells bind real endpoints and talk over `lince/mailbox/1`. What is
//! being tested is mostly REFUSAL — who may leave mail, who may collect it,
//! and what a carrier is structurally unable to do with what it holds.
//!
//! Everything runs on `Reach::Local`, so no relay or DNS is touched.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use engine::Engine;
use engine::roster::{CellEntry, SignedRoster};
use engine::sync::{OpBatch, WireOp};
use engine::trust::Signer;
use engine::wire::{ALPN_MAILBOX, ALPN_SYNC, Reach, Wire, WireRequest, WireResponse};
use iroh::{EndpointAddr, SecretKey};

async fn cell(base_url: &str) -> (Arc<Engine>, String) {
    let e = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&e.store.pool, base_url)
        .await
        .expect("local organ")
        .uid;
    e.set_signer(Signer::generate(&organ, "k1"))
        .await
        .expect("signer");
    (Arc::new(e), organ)
}

/// A private directory for one Cell's sealing keyring. Named per test so two
/// Cells in one process never share a keyring, which would make a bundle
/// openable by the wrong one and quietly pass a test that should fail.
fn scratch(who: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lince-pickup-{}-{}-{who}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn secret(seed: u8) -> SecretKey {
    SecretKey::from_bytes(&[seed; 32])
}

fn loopback(wire: &Wire) -> EndpointAddr {
    let port = wire
        .endpoint()
        .bound_sockets()
        .into_iter()
        .map(|addr| addr.port())
        .next()
        .expect("endpoint is bound");
    EndpointAddr::new(wire.node_id())
        .with_ip_addr(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
}

fn entry(cell_uid: &str, node_id: &str, sealing: Option<engine::seal::SealingKey>) -> CellEntry {
    CellEntry {
        cell_uid: cell_uid.into(),
        node_id: node_id.into(),
        label: cell_uid.into(),
        operational_key: format!("opkey-{cell_uid}"),
        sealing_key: sealing,
        front_door: false,
        capabilities: engine::roster::full_capabilities(),
    }
}

/// A recipient: its Organ, its root key, and a signed roster naming the Cells
/// that may collect for it.
async fn recipient(engine: &Engine, organ: &str, cells: Vec<CellEntry>) -> (Signer, SignedRoster) {
    let root = Signer::generate(organ, engine::roster::ROOT_KEY_ID);
    engine
        .publish_root_key(&root)
        .await
        .expect("publish root key");
    let signed = engine
        .publish_roster(&root, cells)
        .await
        .expect("publish roster");
    (root, signed)
}

fn batch(from_organ: &str, from_cell: &str) -> OpBatch {
    OpBatch {
        from_organ: from_organ.to_string(),
        ops: vec![WireOp {
            tbl: "record".into(),
            uid: "r-mailed".into(),
            field: "head".into(),
            kind: "set".into(),
            value: Some("\"left with a carrier\"".into()),
            hlc: nucleus::hlc::next(),
            actor_cell: from_cell.into(),
            organ_uid: from_organ.to_string(),
            fact: None,
        }],
    }
}

#[tokio::test]
async fn mail_left_with_a_carrier_reaches_the_recipient_and_the_carrier_reads_none_of_it() {
    let (carrier, _carrier_organ) = cell("http://carrier.test").await;
    let carrier_wire = Wire::bind(carrier.clone(), secret(41), Reach::Local)
        .await
        .expect("carrier binds");
    let carrier_addr = loopback(&carrier_wire);
    // The accept loop is explicit, as everywhere else in these tests: binding
    // an endpoint is not the same as answering on it.
    let _serving = tokio::spawn(async move { carrier_wire.serve().await });
    let (sender, sender_organ) = cell("http://sender.test").await;
    let sender_wire = Wire::bind(sender.clone(), secret(42), Reach::Local)
        .await
        .expect("sender binds");
    let (recipient_engine, recipient_organ) = cell("http://recipient.test").await;
    let recipient_wire = Wire::bind(recipient_engine.clone(), secret(43), Reach::Local)
        .await
        .expect("recipient binds");

    // The recipient publishes a sealing key and a roster naming the device
    // that will collect.
    let (secret_key, sealing) = engine::seal::generate("c-phone", 1, "2099-01-01T00:00:00Z");
    let (root, roster) = recipient(
        &recipient_engine,
        &recipient_organ,
        vec![entry(
            "c-phone",
            &recipient_wire.node_id().to_string(),
            Some(sealing.clone()),
        )],
    )
    .await;

    // The operator registers the recipient. Registration is what replaces
    // routing: a bundle goes to a box the RECIPIENT named, and that box knows
    // the name.
    store::mailbox::register(
        &carrier.store.pool,
        &recipient_organ,
        &root.public_key_b64(),
        "a friend of the institute",
        engine::mailbox::DEFAULT_QUOTA_BYTES,
    )
    .await
    .expect("register");

    // The sender seals and leaves it. It never dials the recipient at all —
    // that is the case a mailbox exists for.
    let signing = ed25519_dalek::SigningKey::from_bytes(&[11u8; 32]);
    let bundle = engine::seal::seal(
        &engine::seal::MailedBatch {
            root: None,
            batch: batch(&sender_organ, "c-sender"),
        },
        "c-sender",
        &recipient_organ,
        &[sealing],
        &signing,
    )
    .expect("seals");
    let body = serde_json::to_string(&bundle).expect("serializes");
    let accepted = sender_wire
        .deposit_bundle(carrier_addr.clone(), &body)
        .await
        .expect("deposit")
        .expect("accepted");
    assert!(!accepted.is_empty(), "the carrier hands back a handle");

    // THE CARRIER CANNOT READ IT. It holds the bytes and the routing metadata
    // and nothing else — which is the entire proposition, so it is asserted
    // rather than assumed.
    let held = store::mailbox::for_recipient(&carrier.store.pool, &recipient_organ, 10)
        .await
        .expect("held");
    assert_eq!(held.len(), 1);
    assert!(!held[0].body.contains("left with a carrier"));
    assert_eq!(held[0].from_organ, sender_organ);
    // And it converges nothing. Its own Cell has its own ops — it is a real
    // Organ, not a null one — so what must be absent is anything belonging to
    // the traffic it carried: no op attributed to the sender, and no trace of
    // the Record the sealed batch would have created had it been applied.
    let ops = store::sync_ops::all_by_hlc(&carrier.store.pool)
        .await
        .expect("ops");
    assert!(
        !ops.iter().any(|op| op.organ_uid == sender_organ),
        "a carrier that applied what it carried would not be a carrier"
    );
    assert!(
        store::records::get(&carrier.store.pool, "r-mailed")
            .await
            .expect("get")
            .is_none(),
        "the mailed Record must not exist on the carrier"
    );

    // The recipient comes online and collects.
    let waiting = recipient_wire
        .mail_waiting(carrier_addr.clone(), &recipient_organ, &roster)
        .await
        .expect("waiting");
    assert_eq!(waiting.bundles, 1);

    let collected = recipient_wire
        .collect_mail(carrier_addr.clone(), &recipient_organ, &roster, 10)
        .await
        .expect("collect");
    assert_eq!(collected.len(), 1);

    let opened = engine::seal::open(
        &serde_json::from_str(&collected[0].body).expect("a bundle"),
        &signing.verifying_key(),
        &[(sealing_key_id(&secret_key, "c-phone"), secret_key)],
    )
    .expect("opens with the recipient's key");
    assert_eq!(
        opened.batch.ops[0].value.as_deref(),
        Some("\"left with a carrier\"")
    );

    // Confirmed collection drops it, so a mailbox is not a growing archive.
    let left = store::mailbox::waiting(&carrier.store.pool, &recipient_organ)
        .await
        .expect("waiting");
    assert_eq!(left.bundles, 0, "confirmed mail is dropped");
}

/// The key id `seal::generate` produced, rebuilt for the test's convenience.
fn sealing_key_id(_secret: &[u8; 32], cell_uid: &str) -> String {
    format!("x25519:cell:{cell_uid}:1")
}

#[tokio::test]
async fn a_carrier_refuses_mail_for_an_organ_it_does_not_serve() {
    let (carrier, _) = cell("http://carrier.test").await;
    let carrier_wire = Wire::bind(carrier.clone(), secret(44), Reach::Local)
        .await
        .expect("binds");
    let carrier_addr = loopback(&carrier_wire);
    // The accept loop is explicit, as everywhere else in these tests: binding
    // an endpoint is not the same as answering on it.
    let _serving = tokio::spawn(async move { carrier_wire.serve().await });
    let (sender, sender_organ) = cell("http://sender.test").await;
    let sender_wire = Wire::bind(sender.clone(), secret(45), Reach::Local)
        .await
        .expect("binds");

    // Nobody is registered. Storage is bounded by people who actually asked —
    // otherwise an open door is an open disk.
    let (_, sealing) = engine::seal::generate("c-nobody", 1, "2099-01-01T00:00:00Z");
    let signing = ed25519_dalek::SigningKey::from_bytes(&[12u8; 32]);
    let bundle = engine::seal::seal(
        &engine::seal::MailedBatch {
            root: None,
            batch: batch(&sender_organ, "c-sender"),
        },
        "c-sender",
        "organ-a-stranger",
        &[sealing],
        &signing,
    )
    .expect("seals");
    let refused = sender_wire
        .deposit_bundle(
            carrier_addr.clone(),
            &serde_json::to_string(&bundle).expect("serializes"),
        )
        .await
        .expect("answered")
        .expect_err("refused");
    assert_eq!(refused, "mailbox_not_registered");
}

#[tokio::test]
async fn only_a_cell_the_recipients_roster_names_may_collect() {
    let (carrier, _) = cell("http://carrier.test").await;
    let carrier_wire = Wire::bind(carrier.clone(), secret(46), Reach::Local)
        .await
        .expect("binds");
    let carrier_addr = loopback(&carrier_wire);
    // The accept loop is explicit, as everywhere else in these tests: binding
    // an endpoint is not the same as answering on it.
    let _serving = tokio::spawn(async move { carrier_wire.serve().await });
    let (recipient_engine, recipient_organ) = cell("http://recipient.test").await;
    let recipient_wire = Wire::bind(recipient_engine.clone(), secret(47), Reach::Local)
        .await
        .expect("binds");
    let (thief, _thief_organ) = cell("http://thief.test").await;
    let thief_wire = Wire::bind(thief.clone(), secret(48), Reach::Local)
        .await
        .expect("binds");

    let (root, roster) = recipient(
        &recipient_engine,
        &recipient_organ,
        vec![entry(
            "c-phone",
            &recipient_wire.node_id().to_string(),
            None,
        )],
    )
    .await;
    store::mailbox::register(
        &carrier.store.pool,
        &recipient_organ,
        &root.public_key_b64(),
        "",
        engine::mailbox::DEFAULT_QUOTA_BYTES,
    )
    .await
    .expect("register");

    // The thief holds the recipient's PUBLIC roster — every contact does, and
    // it is published to be held. Presenting it must not be enough: the
    // connection has to be one of the Cells it names.
    let stolen = thief_wire
        .collect_mail(carrier_addr.clone(), &recipient_organ, &roster, 10)
        .await;
    assert!(
        stolen.is_err(),
        "holding a copy of someone's roster must not collect their mail"
    );

    // A roster the caller signed themselves is likewise no good: it must chain
    // from the root key the registration recorded.
    let forged_root = Signer::generate(&recipient_organ, engine::roster::ROOT_KEY_ID);
    let forged_roster = engine::roster::Roster {
        organ_uid: recipient_organ.clone(),
        root_key: forged_root.public_key_b64(),
        version: 99,
        not_after: (chrono::Utc::now() + chrono::Duration::days(30)).to_rfc3339(),
        pickup: Vec::new(),
        cells: vec![entry("c-thief", &thief_wire.node_id().to_string(), None)],
    };
    let payload = engine::roster::roster_signing_payload(&forged_roster).expect("payload");
    let forged = SignedRoster {
        signature: forged_root.sign_bytes(&payload),
        roster: forged_roster,
    };
    let self_signed = thief_wire
        .collect_mail(carrier_addr.clone(), &recipient_organ, &forged, 10)
        .await;
    assert!(
        self_signed.is_err(),
        "a roster that does not chain from the registered root key collects nothing"
    );

    // The genuine device collects, so the refusals above are not simply
    // everything failing.
    assert!(
        recipient_wire
            .collect_mail(carrier_addr.clone(), &recipient_organ, &roster, 10)
            .await
            .is_ok()
    );
}

/// The failure that only shows up in deployment: the recipient buys a phone.
///
/// A mailbox that snapshotted device addresses at registration would lock a
/// recipient out of their own mail the first time they enrolled anything. The
/// registration holds the ROOT key instead, and the collector presents a
/// current roster — so device changes need no re-registration and no polling.
#[tokio::test]
async fn a_device_enrolled_after_registration_can_still_collect() {
    let (carrier, _) = cell("http://carrier.test").await;
    let carrier_wire = Wire::bind(carrier.clone(), secret(49), Reach::Local)
        .await
        .expect("binds");
    let carrier_addr = loopback(&carrier_wire);
    // The accept loop is explicit, as everywhere else in these tests: binding
    // an endpoint is not the same as answering on it.
    let _serving = tokio::spawn(async move { carrier_wire.serve().await });
    let (recipient_engine, recipient_organ) = cell("http://recipient.test").await;
    let old_wire = Wire::bind(recipient_engine.clone(), secret(50), Reach::Local)
        .await
        .expect("binds");

    let (root, _first) = recipient(
        &recipient_engine,
        &recipient_organ,
        vec![entry("c-laptop", &old_wire.node_id().to_string(), None)],
    )
    .await;
    store::mailbox::register(
        &carrier.store.pool,
        &recipient_organ,
        &root.public_key_b64(),
        "",
        engine::mailbox::DEFAULT_QUOTA_BYTES,
    )
    .await
    .expect("register");

    // A new device joins the identity AFTER registration. The carrier is never
    // told, and must not need to be.
    let (new_engine, _) = cell("http://recipient-phone.test").await;
    let new_wire = Wire::bind(new_engine.clone(), secret(51), Reach::Local)
        .await
        .expect("binds");
    let second = recipient_engine
        .publish_roster(
            &root,
            vec![
                entry("c-laptop", &old_wire.node_id().to_string(), None),
                entry("c-phone", &new_wire.node_id().to_string(), None),
            ],
        )
        .await
        .expect("republish with the new device");

    assert!(
        new_wire
            .collect_mail(carrier_addr.clone(), &recipient_organ, &second, 10)
            .await
            .is_ok(),
        "a device enrolled after registration must be able to collect"
    );
}

#[tokio::test]
async fn the_mailbox_door_serves_mailbox_verbs_and_nothing_else() {
    let (carrier, _) = cell("http://carrier.test").await;
    let carrier_wire = Wire::bind(carrier.clone(), secret(52), Reach::Local)
        .await
        .expect("binds");
    let carrier_addr = loopback(&carrier_wire);
    // The accept loop is explicit, as everywhere else in these tests: binding
    // an endpoint is not the same as answering on it.
    let _serving = tokio::spawn(async move { carrier_wire.serve().await });
    let (sender, sender_organ) = cell("http://sender.test").await;
    let sender_wire = Wire::bind(sender.clone(), secret(53), Reach::Local)
        .await
        .expect("binds");

    // The mailbox door is open to ANYONE — that is the policy it exists to
    // express. So the thing keeping a carrier from converging is that the sync
    // verbs are not served here at all.
    let pushed = sender_wire
        .request(
            carrier_addr.clone(),
            ALPN_MAILBOX,
            &WireRequest::PushOps {
                batch: batch(&sender_organ, "c-sender"),
            },
        )
        .await
        .expect("answered");
    assert!(
        matches!(&pushed, WireResponse::Refused { code, .. } if code == "wrong_door"),
        "the sync verbs must not be reachable through the mailbox door: {pushed:?}"
    );

    // And the converse: a mailbox verb on the sync door is refused too, so the
    // admission rules cannot be chosen by picking an ALPN. An unknown Organ
    // cannot open the sync door at all, which is itself the refusal — assert
    // that nothing is served rather than which layer said no.
    let sneaked = sender_wire
        .request(
            carrier_addr.clone(),
            ALPN_SYNC,
            &WireRequest::MailboxDeposit { body: "{}".into() },
        )
        .await;
    assert!(
        sneaked.is_err() || matches!(sneaked, Ok(WireResponse::Refused { .. })),
        "a mailbox verb must not be served on the sync door"
    );
}

#[tokio::test]
async fn a_recipients_quota_bounds_what_a_stranger_can_leave() {
    let (carrier, _) = cell("http://carrier.test").await;
    let (recipient_engine, recipient_organ) = cell("http://recipient.test").await;
    let (root, _) = recipient(
        &recipient_engine,
        &recipient_organ,
        vec![entry("c-phone", "node-phone", None)],
    )
    .await;
    // A quota small enough that the second bundle cannot fit. The charge falls
    // on the RECIPIENT, which is the right incentive: abuse costs the person
    // who chose this carrier, not its operator.
    store::mailbox::register(
        &carrier.store.pool,
        &recipient_organ,
        &root.public_key_b64(),
        "",
        900,
    )
    .await
    .expect("register");

    let (_, sealing) = engine::seal::generate("c-phone", 1, "2099-01-01T00:00:00Z");
    let signing = ed25519_dalek::SigningKey::from_bytes(&[13u8; 32]);
    let body = serde_json::to_string(
        &engine::seal::seal(
            &engine::seal::MailedBatch {
                root: None,
                batch: batch("organ-sender", "c-sender"),
            },
            "c-sender",
            &recipient_organ,
            &[sealing],
            &signing,
        )
        .expect("seals"),
    )
    .expect("serializes");

    assert!(carrier.accept_bundle(&body, "node-sender").await.is_ok());
    assert_eq!(
        carrier
            .accept_bundle(&body, "node-sender")
            .await
            .unwrap_err(),
        engine::mailbox::Refusal::QuotaFull
    );

    // Oversized in one frame is refused on its own terms, so a quota cannot be
    // exhausted by a single caller in a single deposit.
    let huge = "x".repeat(engine::mailbox::MAX_BUNDLE_BYTES + 1);
    assert_eq!(
        carrier
            .accept_bundle(&huge, "node-sender")
            .await
            .unwrap_err(),
        engine::mailbox::Refusal::TooLarge
    );
}

#[tokio::test]
async fn expired_mail_is_deleted_but_the_sender_can_still_be_told() {
    let (carrier, _) = cell("http://carrier.test").await;
    let (recipient_engine, recipient_organ) = cell("http://recipient.test").await;
    let (root, _) = recipient(
        &recipient_engine,
        &recipient_organ,
        vec![entry("c-phone", "node-phone", None)],
    )
    .await;
    store::mailbox::register(
        &carrier.store.pool,
        &recipient_organ,
        &root.public_key_b64(),
        "",
        engine::mailbox::DEFAULT_QUOTA_BYTES,
    )
    .await
    .expect("register");

    // Deposited already past its date, which is what the sweep sees.
    store::mailbox::deposit(
        &carrier.store.pool,
        &store::mailbox::HeldBundle {
            uid: "mb-old".into(),
            to_organ: recipient_organ.clone(),
            from_organ: "organ-sender".into(),
            from_cell: "c-sender".into(),
            from_node: "node-sender".into(),
            body: "{\"sealed\":true}".into(),
            bytes: 15,
            received_at: "2000-01-01T00:00:00Z".into(),
            expires_at: "2000-02-01T00:00:00Z".into(),
        },
    )
    .await
    .expect("deposit");

    assert_eq!(carrier.sweep_mailbox().await.expect("sweep"), 1);
    let left = store::mailbox::waiting(&carrier.store.pool, &recipient_organ)
        .await
        .expect("waiting");
    assert_eq!(
        left.bundles, 0,
        "expired mail is deleted, not merely hidden"
    );

    // The notice survives, because the retention rule promises the SENDER is
    // told. A message that vanishes with nobody knowing is the one failure
    // people trust a messaging system not to have — and after a plain DELETE
    // there would be nothing left to say who to tell.
    let pending = store::mailbox::pending_notices(&carrier.store.pool)
        .await
        .expect("notices");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].from_organ, "organ-sender");
    // And the ciphertext is gone from it: the notice carries who and when,
    // never what.
    assert!(pending[0].body.is_empty());
}

/// The operator's surface, driven the way the panel drives it.
///
/// A feature is not done until a human can use it, and for a carrier that
/// means an operator can see who they carry for, what it is costing them, and
/// can stop. These are the three Actions behind that panel.
#[tokio::test]
async fn an_operator_can_see_and_change_what_this_cell_carries() {
    let (carrier, _) = cell("http://carrier.test").await;
    let (recipient_engine, recipient_organ) = cell("http://recipient.test").await;
    let (root, _) = recipient(
        &recipient_engine,
        &recipient_organ,
        vec![entry("c-phone", "node-phone", None)],
    )
    .await;

    // Offering to carry for an Organ we have never paired with must FAIL, not
    // half-succeed: without their root key there is nothing to check a
    // presented roster against, so the registration would be uncollectable
    // and the mail would rot until it expired.
    let unpaired = carrier
        .act(
            engine::actions::Action::MailboxCarryFor {
                organ_uid: recipient_organ.clone(),
                label: "the institute".into(),
                quota_bytes: 0,
            },
            None,
        )
        .await;
    assert!(unpaired.is_err(), "carrying needs their root key");

    // Pair, then offer.
    engine::trust::adopt_key(
        &carrier.store,
        &recipient_organ,
        engine::roster::ROOT_KEY_ID,
        &root.public_key_b64(),
    )
    .await
    .expect("pairing");
    carrier
        .act(
            engine::actions::Action::MailboxCarryFor {
                organ_uid: recipient_organ.clone(),
                label: "the institute".into(),
                quota_bytes: 0,
            },
            None,
        )
        .await
        .expect("carry");

    let status = carrier
        .act(engine::actions::Action::MailboxStatus, None)
        .await
        .expect("status")
        .data
        .expect("the panel needs data");
    assert_eq!(status["carrying_for"][0]["organ_uid"], recipient_organ);
    assert_eq!(status["carrying_for"][0]["held_bundles"], 0);
    // The panel states the terms it is operating under rather than hard-coding
    // them in the markup, so the two can never drift apart.
    assert_eq!(status["retention_days"], engine::seal::RETENTION_DAYS);

    carrier
        .act(
            engine::actions::Action::MailboxStopCarrying {
                organ_uid: recipient_organ.clone(),
            },
            None,
        )
        .await
        .expect("stop");
    let after = carrier
        .act(engine::actions::Action::MailboxStatus, None)
        .await
        .expect("status")
        .data
        .expect("data");
    assert_eq!(
        after["carrying_for"].as_array().map(|rows| rows.len()),
        Some(0)
    );
}

// ---------------------------------------------------------------------------
// Pickup points: where the RECIPIENT says its mail may be left.
// ---------------------------------------------------------------------------

/// A full Cell that can seal, sign and be mailed: an Organ signer whose public
/// half is in its own roster, and a sealing keyring on disk.
async fn mailable(
    base_url: &str,
    keyring_dir: &std::path::Path,
    node_id: &str,
) -> (Arc<Engine>, String, Signer) {
    let (engine, organ) = cell(base_url).await;
    // The root on DISK, where `root_signer` looks for it: publishing a pickup
    // point re-signs the roster, so a Cell that cannot reach its root cannot
    // do it at all — and the panel says so rather than offering a button that
    // fails.
    engine.set_root_key_path(keyring_dir.join("root.key"));
    let organ_signer = Signer::generate(&organ, "organ");
    engine
        .set_organ_signer(organ_signer.clone())
        .await
        .expect("organ signer");
    engine.set_sealing_keyring_path(keyring_dir.join("keyring.json"));
    let sealing = engine
        .published_sealing_key()
        .await
        .expect("keyring")
        .expect("a fresh keyring generates one");
    let cell_uid = store::cells::local(&engine.store.pool)
        .await
        .expect("cell")
        .expect("a local Cell")
        .uid;
    let root = Signer::load_or_create(
        &keyring_dir.join("root.key"),
        &organ,
        engine::roster::ROOT_KEY_ID,
    )
    .expect("root key on disk");
    engine.publish_root_key(&root).await.expect("root key");
    engine
        .publish_roster(
            &root,
            vec![CellEntry {
                cell_uid: cell_uid.clone(),
                node_id: node_id.into(),
                label: "device".into(),
                operational_key: organ_signer.public_key_b64(),
                sealing_key: Some(sealing),
                front_door: false,
                capabilities: engine::roster::full_capabilities(),
            }],
        )
        .await
        .expect("roster");
    (engine, organ, root)
}

/// This Cell's own uid — what its ops are authored by, and what its roster
/// entry names. A batch claiming any other Cell is inadmissible, which is the
/// gate working rather than a mailbox failing.
async fn local_cell(engine: &Engine) -> String {
    store::cells::local(&engine.store.pool)
        .await
        .expect("cell")
        .expect("a local Cell")
        .uid
}

/// A contact ROW, which is a different thing from holding their key: the
/// outbox queues per contact, so without one there is nothing to fail to
/// deliver and nothing to fall back with.
async fn know(engine: &Engine, organ_uid: &str) {
    store::organs::add_contact(
        &engine.store.pool,
        organ_uid,
        None,
        "them",
        "http://them.test",
        1,
    )
    .await
    .expect("contact");
    store::organs::set_trust(&engine.store.pool, organ_uid, "known")
        .await
        .expect("trust");
}

/// Both sides hold each other's root key and current roster — you can only be
/// mailed by someone you already know, so the test says so out loud.
async fn introduce(a: &Engine, a_organ: &str, a_root: &Signer, b: &Engine) {
    engine::trust::adopt_key(
        &b.store,
        a_organ,
        engine::roster::ROOT_KEY_ID,
        &a_root.public_key_b64(),
    )
    .await
    .expect("pairing");
    let roster = a.roster_of(a_organ).await.expect("roster").expect("signed");
    assert_eq!(
        b.adopt_roster(&roster).await.expect("adopt"),
        engine::roster::RosterOutcome::Accepted
    );
}

/// The whole path, end to end: a recipient publishes a box, a sender that
/// never dials the recipient leaves mail there, and the recipient collects it
/// on an ordinary pass and applies what was inside.
#[tokio::test]
async fn mail_reaches_a_recipient_through_the_pickup_point_it_published() {
    let carrier_dir = scratch("carrier");
    let sender_dir = scratch("sender");
    let recipient_dir = scratch("recipient");

    let (carrier, _carrier_organ) = cell("http://carrier.test").await;
    let carrier_wire = Wire::bind(carrier.clone(), secret(61), Reach::Local)
        .await
        .expect("carrier binds");
    let carrier_node = carrier_wire.node_id().to_string();
    let carrier_addr = loopback(&carrier_wire);
    let _serving = tokio::spawn(async move { carrier_wire.serve().await });
    let _ = carrier_dir;

    let (sender, sender_organ, sender_root) =
        mailable("http://sender.test", &sender_dir, "node-sender").await;
    let sender_cell = local_cell(&sender).await;
    let sender_wire = Wire::bind(sender.clone(), secret(62), Reach::Local)
        .await
        .expect("sender binds");

    let (recipient_engine, recipient_organ, recipient_root) =
        mailable("http://recipient.test", &recipient_dir, "node-recipient").await;
    let recipient_wire = Wire::bind(recipient_engine.clone(), secret(63), Reach::Local)
        .await
        .expect("recipient binds");
    // The roster has to name the node the recipient will actually collect
    // from, so re-publish now that the endpoint is bound.
    let mut cells = recipient_engine
        .roster_of(&recipient_organ)
        .await
        .expect("roster")
        .expect("signed")
        .roster
        .cells;
    cells[0].node_id = recipient_wire.node_id().to_string();
    recipient_engine
        .publish_roster(&recipient_root, cells)
        .await
        .expect("republish");

    introduce(
        &recipient_engine,
        &recipient_organ,
        &recipient_root,
        &sender,
    )
    .await;
    introduce(&sender, &sender_organ, &sender_root, &recipient_engine).await;

    // The carrier agrees to hold their mail.
    store::mailbox::register(
        &carrier.store.pool,
        &recipient_organ,
        &recipient_root.public_key_b64(),
        "the institute",
        engine::mailbox::DEFAULT_QUOTA_BYTES,
    )
    .await
    .expect("register");

    // The recipient PUBLISHES it. Until this line the box exists and no sender
    // has any way to find it — which is the property: the recipient chooses.
    let point = engine::roster::PickupPoint {
        organ_uid: "carrier-organ".into(),
        node_id: carrier_node,
        label: "the institute VPS".into(),
    };
    let published = recipient_engine
        .set_pickup_points(&recipient_root, vec![point])
        .await
        .expect("publish pickup");
    // The sender learns of it the ordinary way: their roster, refreshed.
    assert_eq!(
        sender.adopt_roster(&published).await.expect("adopt"),
        engine::roster::RosterOutcome::Accepted
    );

    let left = sender_wire
        .leave_mail(&recipient_organ, None, &batch(&sender_organ, &sender_cell))
        .await
        .expect("leave mail");
    assert!(
        matches!(left, engine::wire::MailLeft::Left { .. }),
        "the sender used the box the recipient named: {left:?}"
    );

    // The carrier still cannot read a word of it.
    let held = store::mailbox::for_recipient(&carrier.store.pool, &recipient_organ, 10)
        .await
        .expect("held");
    assert_eq!(held.len(), 1);
    assert!(!held[0].body.contains("left with a carrier"));

    // The recipient collects on an ordinary pass and APPLIES it.
    let imported = recipient_wire.collect_own_mail().await.expect("collect");
    assert!(imported > 0, "the mailed batch was imported");
    assert!(
        store::records::get(&recipient_engine.store.pool, "r-mailed")
            .await
            .expect("get")
            .is_some(),
        "what was mailed arrived as an ordinary change"
    );
    let _ = carrier_addr;
}

/// A pickup point is INSIDE the root signature. Without that, substituting one
/// costs an attacker nothing: the roster still verifies, still names the right
/// devices, and every contact who falls back leaves their mail in a box the
/// attacker runs.
///
/// The forgery changes ONLY the pickup entry — same version, same expiry, same
/// cells, same signature — so nothing but the signed payload can catch it.
#[tokio::test]
async fn a_pickup_point_the_root_did_not_sign_is_refused() {
    let dir = scratch("only");
    let (them, their_organ, their_root) = mailable("http://them.test", &dir, "node-them").await;
    let genuine = them
        .set_pickup_points(
            &their_root,
            vec![engine::roster::PickupPoint {
                organ_uid: "a-friend".into(),
                node_id: "node-the-friend".into(),
                label: "a friend".into(),
            }],
        )
        .await
        .expect("publish");
    assert!(engine::roster::roster_signature_is_valid(&genuine));

    let mut forged = genuine.clone();
    forged.roster.pickup[0].node_id = "node-the-attacker".into();
    assert!(
        !engine::roster::roster_signature_is_valid(&forged),
        "a redirected pickup point must not verify"
    );
    let _ = their_organ;
}

/// Two boxes, and the first says no. The sender uses the other one rather than
/// giving up — which is the whole reason to publish two.
#[tokio::test]
async fn a_sender_falls_through_to_the_second_pickup_point() {
    let sender_dir = scratch("sender");
    let recipient_dir = scratch("recipient");

    // A carrier that serves nobody: it answers, and the answer is no.
    let (stranger, _) = cell("http://stranger.test").await;
    let stranger_wire = Wire::bind(stranger.clone(), secret(64), Reach::Local)
        .await
        .expect("binds");
    let stranger_node = stranger_wire.node_id().to_string();
    let _serving_stranger = tokio::spawn(async move { stranger_wire.serve().await });

    let (carrier, _) = cell("http://carrier.test").await;
    let carrier_wire = Wire::bind(carrier.clone(), secret(65), Reach::Local)
        .await
        .expect("binds");
    let carrier_node = carrier_wire.node_id().to_string();
    let _serving = tokio::spawn(async move { carrier_wire.serve().await });

    let (sender, sender_organ, _sender_root) =
        mailable("http://sender.test", &sender_dir, "node-sender").await;
    let sender_cell = local_cell(&sender).await;
    let sender_wire = Wire::bind(sender.clone(), secret(66), Reach::Local)
        .await
        .expect("binds");
    let (recipient_engine, recipient_organ, recipient_root) =
        mailable("http://recipient.test", &recipient_dir, "node-recipient").await;

    store::mailbox::register(
        &carrier.store.pool,
        &recipient_organ,
        &recipient_root.public_key_b64(),
        "the institute",
        engine::mailbox::DEFAULT_QUOTA_BYTES,
    )
    .await
    .expect("register");

    introduce(
        &recipient_engine,
        &recipient_organ,
        &recipient_root,
        &sender,
    )
    .await;
    let published = recipient_engine
        .set_pickup_points(
            &recipient_root,
            vec![
                engine::roster::PickupPoint {
                    organ_uid: "stranger".into(),
                    node_id: stranger_node,
                    label: "one that stopped".into(),
                },
                engine::roster::PickupPoint {
                    organ_uid: "carrier".into(),
                    node_id: carrier_node,
                    label: "the institute VPS".into(),
                },
            ],
        )
        .await
        .expect("publish");
    sender.adopt_roster(&published).await.expect("adopt");

    let left = sender_wire
        .leave_mail(&recipient_organ, None, &batch(&sender_organ, &sender_cell))
        .await
        .expect("leave mail");
    match left {
        engine::wire::MailLeft::Left { carrier, .. } => assert_eq!(carrier, "carrier"),
        other => panic!("the second box should have taken it: {other:?}"),
    }
    // And exactly once. A bundle left twice is a bundle charged twice to a
    // quota the recipient pays for.
    let waiting = store::mailbox::waiting(&carrier.store.pool, &recipient_organ)
        .await
        .expect("waiting");
    assert_eq!(waiting.bundles, 1);
}

/// An Organ that publishes no box cannot be mailed, and the sender is told
/// which nothing that is — "they have nowhere for mail to wait" is a different
/// answer from "we do not know them".
#[tokio::test]
async fn an_organ_with_no_published_box_cannot_be_mailed() {
    let sender_dir = scratch("sender");
    let recipient_dir = scratch("recipient");
    let (sender, sender_organ, _) =
        mailable("http://sender.test", &sender_dir, "node-sender").await;
    let sender_cell = local_cell(&sender).await;
    let sender_wire = Wire::bind(sender.clone(), secret(67), Reach::Local)
        .await
        .expect("binds");
    let (recipient_engine, recipient_organ, recipient_root) =
        mailable("http://recipient.test", &recipient_dir, "node-recipient").await;

    assert_eq!(
        sender_wire
            .leave_mail(
                "someone-we-never-met",
                None,
                &batch(&sender_organ, &sender_cell)
            )
            .await
            .expect("leave"),
        engine::wire::MailLeft::NoRoster
    );

    introduce(
        &recipient_engine,
        &recipient_organ,
        &recipient_root,
        &sender,
    )
    .await;
    assert_eq!(
        sender_wire
            .leave_mail(&recipient_organ, None, &batch(&sender_organ, &sender_cell))
            .await
            .expect("leave"),
        engine::wire::MailLeft::NoPickupPoints
    );
}

/// Rebooting must not unpublish where your mail goes. The boot path
/// republishes the roster whenever a device or a sealing key changed, and it
/// knows nothing about mailboxes — so the pickup points have to survive a
/// republish they were never passed to.
#[tokio::test]
async fn a_republished_roster_keeps_the_pickup_points() {
    let dir = scratch("only");
    let (them, their_organ, their_root) = mailable("http://them.test", &dir, "node-them").await;
    them.set_pickup_points(
        &their_root,
        vec![engine::roster::PickupPoint {
            organ_uid: "a-friend".into(),
            node_id: "node-the-friend".into(),
            label: "a friend".into(),
        }],
    )
    .await
    .expect("publish");

    // What a boot does: re-sign the member list, knowing nothing else.
    let cells = them
        .roster_of(&their_organ)
        .await
        .expect("roster")
        .expect("signed")
        .roster
        .cells;
    let after = them
        .publish_roster(&their_root, cells)
        .await
        .expect("republish");
    assert_eq!(after.roster.pickup.len(), 1, "a reboot must not unpublish");
    assert!(engine::roster::roster_signature_is_valid(&after));
}

/// The operator's half, through the Actions a person actually presses:
/// publishing is refused when the box says it is not carrying for us, and the
/// panel reports the live state rather than a stored one.
#[tokio::test]
async fn publishing_a_pickup_point_is_refused_by_a_box_that_does_not_carry_for_us() {
    let dir = scratch("only");
    let (stranger, _) = cell("http://stranger.test").await;
    let stranger_wire = Wire::bind(stranger.clone(), secret(68), Reach::Local)
        .await
        .expect("binds");
    let stranger_node = stranger_wire.node_id().to_string();
    let _serving = tokio::spawn(async move { stranger_wire.serve().await });

    let (us, our_organ, our_root) = mailable("http://us.test", &dir, "node-us").await;
    let our_wire = Arc::new(
        Wire::bind(us.clone(), secret(69), Reach::Local)
            .await
            .expect("binds"),
    );
    our_wire.serve_enrolment();
    let mut cells = us
        .roster_of(&our_organ)
        .await
        .expect("roster")
        .expect("signed")
        .roster
        .cells;
    cells[0].node_id = our_wire.node_id().to_string();
    us.publish_roster(&our_root, cells)
        .await
        .expect("republish");

    // The candidate list is honest about the one thing it cannot know: whether
    // a contact carries for us lives on THEIR disk, so the panel offers every
    // known contact and the probe is what answers.
    store::organs::add_contact(
        &us.store.pool,
        "stranger-organ",
        None,
        "a stranger",
        "http://stranger.test",
        1,
    )
    .await
    .expect("contact");
    store::organs::set_trust(&us.store.pool, "stranger-organ", "known")
        .await
        .expect("trust");
    store::organs::set_node_id(&us.store.pool, "stranger-organ", Some(&stranger_node))
        .await
        .expect("node id");

    let refused = us
        .act(
            engine::actions::Action::MailboxAddPickup {
                organ_uid: "stranger-organ".into(),
                node_id: String::new(),
                label: String::new(),
            },
            None,
        )
        .await;
    assert!(refused.is_err(), "a box that says no is not published");
    assert!(
        us.own_pickup_points().await.expect("points").is_empty(),
        "nothing was published"
    );

    let panel = us
        .act(engine::actions::Action::MailboxPickupPoints, None)
        .await
        .expect("panel")
        .data
        .expect("data");
    assert_eq!(panel["pickup"].as_array().map(|rows| rows.len()), Some(0));
    assert_eq!(
        panel["candidates"][0]["organ_uid"], "stranger-organ",
        "a contact we could ask is offered"
    );
    assert_eq!(panel["retention_days"], engine::seal::RETENTION_DAYS);
    assert_eq!(panel["may_change"], true);
}

/// The retry window: a contact that stopped answering ONE moment ago is not
/// mailed, and the same contact past the window is.
///
/// Both halves in one test on purpose. Either alone proves nothing — "no
/// bundle appeared" is also what a broken fallback looks like, and "a bundle
/// appeared" is also what falling back on the first failed dial looks like.
/// The only thing that differs between the two passes below is how long the
/// contact has been silent.
#[tokio::test]
async fn mail_is_left_only_after_the_retry_window_has_passed() {
    let carrier_dir = scratch("window-carrier");
    let sender_dir = scratch("window-sender");
    let recipient_dir = scratch("window-recipient");

    let (carrier, _carrier_organ) = cell("http://carrier.test").await;
    let carrier_wire = Wire::bind(carrier.clone(), secret(71), Reach::Local)
        .await
        .expect("carrier binds");
    let carrier_node = carrier_wire.node_id().to_string();
    let _serving = tokio::spawn(async move { carrier_wire.serve().await });
    let _ = carrier_dir;

    let (sender, sender_organ, sender_root) =
        mailable("http://sender.test", &sender_dir, "node-sender").await;
    let sender_wire = Wire::bind(sender.clone(), secret(72), Reach::Local)
        .await
        .expect("sender binds");

    // The recipient never binds an endpoint. Their roster names a node id
    // nothing answers at, which is exactly the production case this box is
    // about: a peer we hold a key for and cannot reach right now.
    let (recipient_engine, recipient_organ, recipient_root) =
        mailable("http://recipient.test", &recipient_dir, "node-recipient").await;
    introduce(
        &recipient_engine,
        &recipient_organ,
        &recipient_root,
        &sender,
    )
    .await;
    introduce(&sender, &sender_organ, &sender_root, &recipient_engine).await;

    store::mailbox::register(
        &carrier.store.pool,
        &recipient_organ,
        &recipient_root.public_key_b64(),
        "the institute",
        engine::mailbox::DEFAULT_QUOTA_BYTES,
    )
    .await
    .expect("register");
    let published = recipient_engine
        .set_pickup_points(
            &recipient_root,
            vec![engine::roster::PickupPoint {
                organ_uid: "carrier-organ".into(),
                node_id: carrier_node,
                label: "the institute VPS".into(),
            }],
        )
        .await
        .expect("publish pickup");
    assert_eq!(
        sender.adopt_roster(&published).await.expect("adopt"),
        engine::roster::RosterOutcome::Accepted
    );

    // Something to send them, queued the ordinary way.
    know(&sender, &recipient_organ).await;
    store::organs::set_sync_policy(&sender.store.pool, &recipient_organ, true, false)
        .await
        .expect("policy");
    store::records::create(
        &sender.store.pool,
        store::records::NewRecord {
            slug: Some("a.thing"),
            kind: nucleus::RecordKind::Plain,
            head: "a thing to say",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record");
    assert!(
        !store::sync_ops::outbox_due(&sender.store.pool)
            .await
            .expect("outbox")
            .is_empty(),
        "precondition: there is something queued for them"
    );

    // FIRST pass. They do not answer, and nothing is mailed: a failed dial at
    // this moment means "not right now", and mailing on it would route every
    // batch through somebody else's disk forever after.
    sender_wire.sync_once().await.expect("first pass");
    let held = store::mailbox::for_recipient(&carrier.store.pool, &recipient_organ, 10)
        .await
        .expect("held");
    assert!(
        held.is_empty(),
        "a contact that has been silent for seconds is not mailed: {held:?}"
    );
    assert!(
        !store::sync_ops::outbox_due(&sender.store.pool)
            .await
            .expect("outbox")
            .is_empty(),
        "and nothing was dropped from the queue while waiting"
    );

    // Now the same silence, older than the window. Nothing else changes.
    let long_ago =
        (chrono::Utc::now() - engine::wire::Wire::MAIL_AFTER - chrono::Duration::minutes(1))
            .to_rfc3339();
    store::organs::backdate_unreachable(&sender.store.pool, &recipient_organ, &long_ago)
        .await
        .expect("backdate");

    sender_wire.sync_once().await.expect("second pass");
    let held = store::mailbox::for_recipient(&carrier.store.pool, &recipient_organ, 10)
        .await
        .expect("held");
    assert_eq!(
        held.len(),
        1,
        "past the window the same batch is sealed and left with the box they published"
    );

    // And a third pass immediately after leaves NOTHING more. A peer that
    // stays down would otherwise be re-sealed every pass, filling their own
    // quota at their own carrier with copies of what is already there.
    sender_wire.sync_once().await.expect("third pass");
    let held = store::mailbox::for_recipient(&carrier.store.pool, &recipient_organ, 10)
        .await
        .expect("held");
    assert_eq!(held.len(), 1, "mailed once per window, not once per pass");
}

/// Mail is not an acknowledgement.
///
/// A deposited bundle drops the queue rows — re-sealing the same ops every
/// pass is what the cooldown exists to prevent — but it must NOT move the
/// retention floor. The floor means "this peer holds these ops"; a sealed
/// bundle on a stranger's disk means only that somebody agreed to hold it,
/// and if it expires uncollected the peer's own catch-up is what recovers.
/// Pruning on the strength of it would delete the last copy.
#[tokio::test]
async fn leaving_mail_does_not_move_the_retention_floor() {
    let carrier_dir = scratch("floor-carrier");
    let sender_dir = scratch("floor-sender");
    let recipient_dir = scratch("floor-recipient");

    let (carrier, _carrier_organ) = cell("http://carrier.test").await;
    let carrier_wire = Wire::bind(carrier.clone(), secret(73), Reach::Local)
        .await
        .expect("carrier binds");
    let carrier_node = carrier_wire.node_id().to_string();
    let _serving = tokio::spawn(async move { carrier_wire.serve().await });
    let _ = carrier_dir;

    let (sender, sender_organ, sender_root) =
        mailable("http://sender.test", &sender_dir, "node-sender").await;
    let sender_wire = Wire::bind(sender.clone(), secret(74), Reach::Local)
        .await
        .expect("sender binds");
    let (recipient_engine, recipient_organ, recipient_root) =
        mailable("http://recipient.test", &recipient_dir, "node-recipient").await;
    introduce(
        &recipient_engine,
        &recipient_organ,
        &recipient_root,
        &sender,
    )
    .await;
    introduce(&sender, &sender_organ, &sender_root, &recipient_engine).await;

    store::mailbox::register(
        &carrier.store.pool,
        &recipient_organ,
        &recipient_root.public_key_b64(),
        "the institute",
        engine::mailbox::DEFAULT_QUOTA_BYTES,
    )
    .await
    .expect("register");
    let published = recipient_engine
        .set_pickup_points(
            &recipient_root,
            vec![engine::roster::PickupPoint {
                organ_uid: "carrier-organ".into(),
                node_id: carrier_node,
                label: "the institute VPS".into(),
            }],
        )
        .await
        .expect("publish pickup");
    assert_eq!(
        sender.adopt_roster(&published).await.expect("adopt"),
        engine::roster::RosterOutcome::Accepted
    );
    know(&sender, &recipient_organ).await;
    store::organs::set_sync_policy(&sender.store.pool, &recipient_organ, true, false)
        .await
        .expect("policy");
    store::records::create(
        &sender.store.pool,
        store::records::NewRecord {
            slug: Some("a.thing"),
            kind: nucleus::RecordKind::Plain,
            head: "a thing to say",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record");

    let floor_before = store::organs::contact(&sender.store.pool, &recipient_organ)
        .await
        .expect("contact")
        .expect("row")
        .peer_acked_seq;

    let long_ago =
        (chrono::Utc::now() - engine::wire::Wire::MAIL_AFTER - chrono::Duration::minutes(1))
            .to_rfc3339();
    store::organs::mark_unreachable(&sender.store.pool, &recipient_organ)
        .await
        .expect("mark");
    store::organs::backdate_unreachable(&sender.store.pool, &recipient_organ, &long_ago)
        .await
        .expect("backdate");
    sender_wire.sync_once().await.expect("pass");

    assert_eq!(
        store::mailbox::for_recipient(&carrier.store.pool, &recipient_organ, 10)
            .await
            .expect("held")
            .len(),
        1,
        "precondition: it was mailed"
    );
    let after = store::organs::contact(&sender.store.pool, &recipient_organ)
        .await
        .expect("contact")
        .expect("row");
    assert_eq!(
        after.peer_acked_seq, floor_before,
        "nobody applied anything, so nothing may be pruned as delivered"
    );
    assert!(
        store::sync_ops::outbox_due(&sender.store.pool)
            .await
            .expect("outbox")
            .iter()
            .all(|row| row.contact_organ != recipient_organ),
        "but the queue rows are gone, or the next pass would seal them all over again"
    );
}

/// A conversation batch survives the mailbox.
///
/// Ops travel on two channels — the general Organ feed and one per
/// individually-replicated root — and the receiver refuses a conversation op
/// that arrives on the general feed, deliberately: that is how a contact with
/// ordinary sync would otherwise write into a conversation shared with
/// somebody else. Over a connection the channel is the request verb. Out of a
/// mailbox there is no verb, so the channel travels INSIDE the seal, where a
/// carrier cannot see which conversation anyone is writing to.
///
/// The second half is why believing it costs nothing: the root is authorized
/// on arrival against OUR OWN accepted grants, so naming one buys a sender
/// exactly what they were already granted and nothing more.
#[tokio::test]
async fn a_mailed_conversation_batch_lands_in_its_root_and_nowhere_else() {
    let sender_dir = scratch("root-sender");
    let recipient_dir = scratch("root-recipient");

    let (sender, sender_organ, sender_root) =
        mailable("http://sender.test", &sender_dir, "node-sender").await;
    let sender_cell = local_cell(&sender).await;
    let (recipient_engine, recipient_organ, recipient_root) =
        mailable("http://recipient.test", &recipient_dir, "node-recipient").await;
    introduce(
        &recipient_engine,
        &recipient_organ,
        &recipient_root,
        &sender,
    )
    .await;
    introduce(&sender, &sender_organ, &sender_root, &recipient_engine).await;

    // The recipient accepted a conversation with this sender.
    store::replica::offer(
        &recipient_engine.store.pool,
        "r-conversation",
        &sender_organ,
    )
    .await
    .expect("offer");
    store::replica::accept(
        &recipient_engine.store.pool,
        "r-conversation",
        &sender_organ,
    )
    .await
    .expect("accept");

    let sealed = sender
        .seal_batch_for(
            &recipient_organ,
            Some("r-conversation"),
            &batch(&sender_organ, &sender_cell),
        )
        .await
        .expect("seals");
    let opened = recipient_engine.open_mailed(&sealed).await.expect("opens");
    assert_eq!(
        opened.root.as_deref(),
        Some("r-conversation"),
        "the channel came through the seal"
    );
    assert!(
        recipient_engine
            .import_mailed_batch(&opened)
            .await
            .expect("imports")
            > 0
    );
    assert_eq!(
        store::replica::root_of(&recipient_engine.store.pool, "r-mailed")
            .await
            .expect("root"),
        Some("r-conversation".to_string()),
        "and the record belongs to the conversation, not to the general feed"
    );

    // A root nobody granted them. Same seal, same signature, same everything
    // else — the grant table is what refuses it.
    let forged = sender
        .seal_batch_for(
            &recipient_organ,
            Some("r-someone-elses-conversation"),
            &batch(&sender_organ, &sender_cell),
        )
        .await
        .expect("seals");
    let opened = recipient_engine.open_mailed(&forged).await.expect("opens");
    assert!(
        recipient_engine.import_mailed_batch(&opened).await.is_err(),
        "naming a root grants nothing: the grant is checked against our own table"
    );
}

/// Being ASKED, and inviting somebody who has never met you.
///
/// Two paths into one registration table, and the difference between them is
/// consent given when versus consent given in advance. The ask requires a
/// prior relationship — the request table is bounded by the contact list, so
/// a stranger cannot fill a disk with requests — and an invite is how somebody
/// with no relationship gets in anyway, because the operator handed them a
/// code over a channel they chose.
#[tokio::test]
async fn a_contact_can_ask_to_be_carried_and_a_stranger_can_spend_an_invite() {
    let carrier_dir = scratch("ask-carrier");
    let asker_dir = scratch("ask-asker");
    let stranger_dir = scratch("ask-stranger");

    let (carrier, carrier_organ, _carrier_root) =
        mailable("http://carrier.test", &carrier_dir, "node-carrier").await;
    let carrier_wire = Wire::bind(carrier.clone(), secret(81), Reach::Local)
        .await
        .expect("carrier binds");
    let carrier_addr = loopback(&carrier_wire);
    let carrier_node = carrier_wire.node_id().to_string();
    let _serving = tokio::spawn(async move { carrier_wire.serve().await });

    let (asker, asker_organ, asker_root) =
        mailable("http://asker.test", &asker_dir, "node-asker").await;
    let asker_wire = Wire::bind(asker.clone(), secret(82), Reach::Local)
        .await
        .expect("asker binds");

    // A STRANGER: the carrier holds no key for them and never will.
    let (stranger, stranger_organ, _stranger_root) =
        mailable("http://stranger.test", &stranger_dir, "node-stranger").await;
    let stranger_wire = Wire::bind(stranger.clone(), secret(83), Reach::Local)
        .await
        .expect("stranger binds");

    // The stranger's ask is refused, and nothing is written. This is the bound
    // on the table: not a rate limit, a relationship.
    let refused = stranger_wire.ask_to_be_carried(&carrier_node).await;
    assert!(
        refused.is_err(),
        "a carrier cannot be asked by someone it cannot identify"
    );
    assert!(
        store::mailbox::requests(&carrier.store.pool)
            .await
            .expect("requests")
            .is_empty(),
        "and a refused ask leaves no row"
    );

    // A contact asks, and it is NOTED — not accepted. Nothing on the wire may
    // commit the operator.
    introduce(&asker, &asker_organ, &asker_root, &carrier).await;
    asker_wire
        .ask_to_be_carried(&carrier_node)
        .await
        .expect("the ask is taken");
    let asks = store::mailbox::requests(&carrier.store.pool)
        .await
        .expect("requests");
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0].organ_uid, asker_organ);
    assert!(
        store::mailbox::registration(&carrier.store.pool, &asker_organ)
            .await
            .expect("registration")
            .is_none(),
        "asking is not being carried: a person decides that"
    );

    // Asking twice is one request, not two.
    asker_wire
        .ask_to_be_carried(&carrier_node)
        .await
        .expect("asks again");
    assert_eq!(
        store::mailbox::requests(&carrier.store.pool)
            .await
            .expect("requests")
            .len(),
        1
    );

    // The operator says yes.
    carrier
        .act(
            engine::actions::Action::MailboxAnswerRequest {
                organ_uid: asker_organ.clone(),
                accept: true,
                quota_bytes: 0,
            },
            None,
        )
        .await
        .expect("accept");
    assert!(
        store::mailbox::registration(&carrier.store.pool, &asker_organ)
            .await
            .expect("registration")
            .is_some(),
        "and now the box carries for them"
    );
    assert!(
        store::mailbox::requests(&carrier.store.pool)
            .await
            .expect("requests")
            .is_empty(),
        "the answered ask stops standing"
    );

    // The stranger gets in the other way. The code is the consent.
    let token = carrier
        .issue_mailbox_invite("a friend of a friend", 0)
        .await
        .expect("issue");
    stranger_wire
        .redeem_mailbox_invite(&carrier_node, &token)
        .await
        .expect("redeem");
    assert!(
        store::mailbox::registration(&carrier.store.pool, &stranger_organ)
            .await
            .expect("registration")
            .is_some(),
        "an invite registers somebody the carrier has no other reason to trust"
    );

    // Single use. The second spend is refused, and cannot be told apart from
    // an unknown or expired code.
    let again = stranger_wire
        .redeem_mailbox_invite(&carrier_node, &token)
        .await;
    assert!(again.is_err(), "an invite works exactly once");

    let _ = (carrier_addr, carrier_organ);
}

/// The sender is told when their mail was never picked up — and is told about
/// their own mail and nobody else's.
///
/// Two senders leave mail at one box for the same recipient, who never comes.
/// The retention window passes, the carrier deletes both and keeps two
/// notices. What each sender may then learn is scoped by the node id the
/// transport proved, which is the only sender fact a carrier holds that was
/// not simply written into a bundle by whoever wrote it.
#[tokio::test]
async fn an_expiry_notice_reaches_the_sender_that_left_it_and_nobody_else() {
    let carrier_dir = scratch("expiry-carrier");
    let one_dir = scratch("expiry-one");
    let two_dir = scratch("expiry-two");
    let recipient_dir = scratch("expiry-recipient");

    let (carrier, _carrier_organ) = cell("http://carrier.test").await;
    let carrier_wire = Wire::bind(carrier.clone(), secret(91), Reach::Local)
        .await
        .expect("carrier binds");
    let carrier_node = carrier_wire.node_id().to_string();
    let carrier_addr = loopback(&carrier_wire);
    let _serving = tokio::spawn(async move { carrier_wire.serve().await });
    let _ = carrier_dir;

    let (recipient_engine, recipient_organ, recipient_root) =
        mailable("http://recipient.test", &recipient_dir, "node-recipient").await;
    store::mailbox::register(
        &carrier.store.pool,
        &recipient_organ,
        &recipient_root.public_key_b64(),
        "the institute",
        engine::mailbox::DEFAULT_QUOTA_BYTES,
    )
    .await
    .expect("register");
    let published = recipient_engine
        .set_pickup_points(
            &recipient_root,
            vec![engine::roster::PickupPoint {
                organ_uid: "carrier-organ".into(),
                node_id: carrier_node.clone(),
                label: "the institute VPS".into(),
            }],
        )
        .await
        .expect("publish pickup");

    let (one, one_organ, _one_root) = mailable("http://one.test", &one_dir, "node-one").await;
    let one_wire = Wire::bind(one.clone(), secret(92), Reach::Local)
        .await
        .expect("one binds");
    let (two, two_organ, _two_root) = mailable("http://two.test", &two_dir, "node-two").await;
    let two_wire = Wire::bind(two.clone(), secret(93), Reach::Local)
        .await
        .expect("two binds");
    // Pairing carries the roster the recipient has just published, pickup
    // points and all — which is the whole reason a sender knows where to
    // leave anything.
    for sender in [&one, &two] {
        introduce(&recipient_engine, &recipient_organ, &recipient_root, sender).await;
        assert!(
            sender
                .roster_of(&recipient_organ)
                .await
                .expect("roster")
                .is_some_and(|held| !held.roster.pickup.is_empty())
        );
    }
    let _ = published;

    // Both leave mail with the same box, for the same person.
    let left_by_one = one_wire
        .leave_mail(&recipient_organ, None, &batch(&one_organ, "c-one"))
        .await
        .expect("one leaves mail");
    let left_by_two = two_wire
        .leave_mail(&recipient_organ, None, &batch(&two_organ, "c-two"))
        .await
        .expect("two leaves mail");
    let (our_uid, their_uid) = match (&left_by_one, &left_by_two) {
        (
            engine::wire::MailLeft::Left { uid: ours, .. },
            engine::wire::MailLeft::Left { uid: theirs, .. },
        ) => (ours.clone(), theirs.clone()),
        other => panic!("both deposits should have been accepted: {other:?}"),
    };

    // Nobody ever collects, and the window passes. Backdated rather than
    // waited out: what is under test is the sweep and the report, not the
    // clock.
    for uid in [&our_uid, &their_uid] {
        store::mailbox::backdate_expiry(&carrier.store.pool, uid, "2000-01-01T00:00:00Z")
            .await
            .expect("age the mail");
    }
    assert_eq!(carrier.sweep_mailbox().await.expect("sweep"), 2);

    // What the wire will say to sender one. Asked raw, because the scoping is
    // the property: an answer is built from the connection's node id and there
    // is no field in the request that could name anybody else.
    let answered = one_wire
        .request(
            carrier_addr.clone(),
            ALPN_MAILBOX,
            &WireRequest::MailboxExpiries,
        )
        .await
        .expect("the carrier answers");
    match answered {
        WireResponse::MailboxExpired { expired } => {
            assert_eq!(expired.len(), 1, "one sender, one notice — never both");
            assert_eq!(expired[0].uid, our_uid);
            assert_eq!(expired[0].to_organ, recipient_organ);
        }
        other => panic!("unexpected answer: {other:?}"),
    }

    // And the ordinary path, the one the sync pass runs: ask, believe what
    // matches our own record, acknowledge, and have something to show a
    // person.
    assert_eq!(
        one_wire
            .hear_about_expired_mail()
            .await
            .expect("hears about it"),
        1
    );
    let told = store::mail_left::expired(&one.store.pool, 10)
        .await
        .expect("expired");
    assert_eq!(told.len(), 1);
    assert_eq!(
        told[0].to_organ, recipient_organ,
        "and it names WHO it was for"
    );
    assert!(told[0].expired_at.is_some());

    // Acknowledged means forgotten: the carrier's ledger of who wrote to whom
    // is the most sensitive thing it holds, so a delivered notice does not
    // become a permanent row.
    let left_at_carrier =
        store::mailbox::expiries_for_node(&carrier.store.pool, &one_wire.node_id().to_string())
            .await
            .expect("notices");
    assert!(
        left_at_carrier.is_empty(),
        "an acknowledged notice is dropped"
    );

    // Sender two has heard nothing and lost nothing. Their notice is still
    // owed, which is the other half of the scoping.
    let owed = store::mailbox::pending_notices(&carrier.store.pool)
        .await
        .expect("pending");
    assert_eq!(owed.len(), 1);
    assert_eq!(owed[0].from_organ, two_organ);
    assert!(
        store::mail_left::expired(&two.store.pool, 10)
            .await
            .expect("expired")
            .is_empty(),
        "and sender two was told nothing by somebody else's question"
    );

    // A report about mail we never left is dropped in silence. A carrier is
    // not trusted to name a failure; it is trusted only to report one we
    // already recorded making.
    assert!(
        one.note_expired_mail(
            &carrier_node,
            &[("mb-never-happened".into(), "2000-01-01T00:00:00Z".into())]
        )
        .await
        .expect("reports")
        .is_empty(),
        "an unrecognised uid alarms nobody"
    );
}
