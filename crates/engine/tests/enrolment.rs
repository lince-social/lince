//! Joining an existing Organ as a second Cell (Ontology §11, cluster C3).
//!
//! The server half — token, redemption, roster signing — has existed since
//! migration 0044. What is exercised here is the CLIENT: a fresh device that
//! stops being its own Organ and becomes a member of another, and every way
//! that must fail.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use engine::Engine;
use engine::pairing::EnrolmentInvite;
use engine::roster::{CellEntry, ROOT_KEY_ID, full_capabilities};
use engine::trust::Signer;
use engine::wire::{Reach, Wire};
use iroh::SecretKey;
use nucleus::RecordKind;
use store::records::NewRecord;

async fn cell(base_url: &str) -> (Arc<Engine>, String) {
    let engine = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&engine.store.pool, base_url)
        .await
        .expect("local organ")
        .uid;
    engine
        .set_signer(Signer::generate(&organ, "k1"))
        .await
        .expect("signer");
    (Arc::new(engine), organ)
}

fn secret(seed: u8) -> SecretKey {
    SecretKey::from_bytes(&[seed; 32])
}

fn addrs(wire: &Wire) -> Vec<String> {
    wire.endpoint()
        .bound_sockets()
        .into_iter()
        .map(|addr| SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port()).to_string())
        .collect()
}

/// An Organ with a root key, a roster of one, and an endpoint serving the
/// thread door — a Cell you already own, showing an Add-a-device code.
async fn enroller(seed: u8) -> (Arc<Engine>, String, Signer, Wire) {
    let (engine, organ) = cell("http://enroller.test").await;
    // The root has to be on DISK, because the serving side reaches it through
    // `root_signer()` — enrolling signs a new roster, and only the root can.
    // That is also the honest shape: an Organ whose root has been moved
    // offline correctly cannot enrol a device until it comes back.
    let key_path = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("enrolment-root-{seed}.key"));
    let _ = std::fs::remove_file(&key_path);
    let root = Signer::load_or_create(&key_path, &organ, ROOT_KEY_ID).expect("root key");
    engine.set_root_key_path(key_path);
    engine.publish_root_key(&root).await.expect("root key");
    let wire = Wire::bind(engine.clone(), secret(seed), Reach::Local)
        .await
        .expect("binds");
    let this_cell = store::cells::local(&engine.store.pool)
        .await
        .expect("cell")
        .expect("cell record");
    engine
        .publish_roster(
            &root,
            vec![CellEntry {
                cell_uid: this_cell.uid,
                node_id: wire.node_id().to_string(),
                label: "the first Cell".into(),
                operational_key: "k-first".into(),
                front_door: false,
                capabilities: full_capabilities(),
            }],
        )
        .await
        .expect("roster");
    (engine, organ, root, wire)
}

async fn invite_from(engine: &Engine, organ: &str, root: &Signer, wire: &Wire) -> EnrolmentInvite {
    EnrolmentInvite {
        node_id: wire.node_id().to_string(),
        organ_uid: organ.to_string(),
        root_key: root.public_key_b64(),
        token: engine.issue_enrolment_token().await.expect("token"),
        addrs: addrs(wire),
    }
}

/// The whole point: a second device stops being its own Organ.
#[tokio::test]
async fn a_new_device_joins_an_existing_organ() {
    let (them, their_organ, root, their_wire) = enroller(51).await;
    let (us, our_first_organ) = cell("http://joiner.test").await;
    let our_wire = Wire::bind(us.clone(), secret(52), Reach::Local)
        .await
        .expect("binds");
    let invite = invite_from(&them, &their_organ, &root, &their_wire).await;
    let our_cell = store::cells::local(&us.store.pool)
        .await
        .expect("cell")
        .expect("cell record")
        .uid;

    let serving = tokio::spawn(async move { their_wire.serve().await });
    let roster = our_wire.enrol(&invite).await.expect("enrolment succeeds");

    assert_eq!(roster.roster.cells.len(), 2, "the roster now names both Cells");
    assert!(
        roster
            .roster
            .cells
            .iter()
            .any(|member| member.cell_uid == our_cell),
        "and one of them is us"
    );

    // The identity swap, which is what makes this a Cell rather than a contact.
    let now = store::organs::local(&us.store.pool)
        .await
        .expect("organ")
        .expect("local organ");
    assert_eq!(now.uid, their_organ, "our local Organ IS theirs now");
    assert_ne!(now.uid, our_first_organ, "the bootstrap Organ is gone");
    let cell_row = store::cells::local(&us.store.pool)
        .await
        .expect("cell")
        .expect("cell record");
    assert_eq!(cell_row.uid, our_cell, "the Cell Record keeps its identity");
    assert_eq!(
        cell_row.organ_uid, their_organ,
        "and is repointed at the joined Organ"
    );

    // The chain: we adopted their root, so the roster verifies against it.
    assert!(
        us.key_chains(&their_organ, &root.public_key_b64())
            .await
            .expect("chain"),
        "their root key now speaks for our Organ"
    );
    assert!(
        us.roster_of(&their_organ).await.expect("roster").is_some(),
        "and the roster is stored under the joined uid"
    );

    // The bootstrap ops are gone rather than orphaned: they were stamped with
    // an Organ that no longer exists, and `rebuild_read_model` replays the
    // whole log.
    let (ops, _) = us.ops_after(0, 500).await.expect("ops");
    assert!(
        ops.iter().all(|op| op.organ_uid == their_organ),
        "no op may still claim the discarded identity"
    );

    // C2b's machinery, pointed at the result. The log and the read model must
    // agree AFTER an identity swap — an orphaned op stamped with the discarded
    // Organ would replay here and either materialise under a Record that no
    // longer exists or abort on migration 0051's origin trigger. This is the
    // check that says the purge was complete.
    let audit = us.audit_read_model().await.expect("audit");
    assert!(
        audit.is_clean(),
        "the read model disagrees with the log after enrolment: {:?}",
        audit.diverged
    );
    us.rebuild_read_model()
        .await
        .expect("a freshly enrolled Cell must be able to replay its own log");

    serving.abort();
}

/// A device that has been USED cannot join: merging two identities is a much
/// larger act than scanning a code, and doing it silently is the wrong default.
#[tokio::test]
async fn a_device_that_already_holds_records_is_refused() {
    let (them, their_organ, root, their_wire) = enroller(53).await;
    let (us, _) = cell("http://used.test").await;
    let our_wire = Wire::bind(us.clone(), secret(54), Reach::Local)
        .await
        .expect("binds");
    store::records::create(
        &us.store.pool,
        NewRecord {
            slug: Some("mine"),
            kind: RecordKind::Plain,
            head: "Something I wrote first",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record");
    let invite = invite_from(&them, &their_organ, &root, &their_wire).await;

    let serving = tokio::spawn(async move { their_wire.serve().await });
    let error = our_wire
        .enrol(&invite)
        .await
        .expect_err("a used device must be refused");

    assert!(
        error.to_string().contains("merge two identities"),
        "the refusal must say why, got: {error}"
    );
    assert!(
        store::records::resolve(&us.store.pool, "mine")
            .await
            .expect("resolve")
            .is_some(),
        "and must change nothing"
    );

    // THE POINT: the refusal happened before the dial, so the single-use token
    // was never spent. Checking only inside the swap would redeem it on the
    // other side first, leaving a spent code and a roster naming a device that
    // never joined. A clean Cell using the SAME invite is what proves it.
    let (clean, _) = cell("http://clean.test").await;
    let clean_wire = Wire::bind(clean.clone(), secret(58), Reach::Local)
        .await
        .expect("binds");
    clean_wire
        .enrol(&invite)
        .await
        .expect("the token must still be good");

    serving.abort();
}

/// Single-use, by the design that makes it safe to show on a screen.
#[tokio::test]
async fn a_token_works_exactly_once() {
    let (them, their_organ, root, their_wire) = enroller(55).await;
    let (first, _) = cell("http://first-joiner.test").await;
    let first_wire = Wire::bind(first.clone(), secret(56), Reach::Local)
        .await
        .expect("binds");
    let (second, second_organ) = cell("http://second-joiner.test").await;
    let second_wire = Wire::bind(second.clone(), secret(57), Reach::Local)
        .await
        .expect("binds");
    let invite = invite_from(&them, &their_organ, &root, &their_wire).await;

    let serving = tokio::spawn(async move { their_wire.serve().await });
    first_wire.enrol(&invite).await.expect("the first use works");
    let error = second_wire
        .enrol(&invite)
        .await
        .expect_err("the second use must fail");

    // Two shapes, both correct. If another enrolment is still open the server
    // reads the verb and refuses it by name; if this was the only one, the
    // window has closed and the door itself is shut — which is the stronger
    // outcome, and the client says so in those terms rather than "connection
    // lost".
    assert!(
        error.to_string().contains("already used")
            || error.to_string().contains("enrolment_denied")
            || error.to_string().contains("works once"),
        "got: {error}"
    );
    let unchanged = store::organs::local(&second.store.pool)
        .await
        .expect("organ")
        .expect("local organ");
    assert_eq!(
        unchanged.uid, second_organ,
        "a refused enrolment must leave the device exactly as it was"
    );

    serving.abort();
}

/// The joining Cell verifies the answer rather than trusting the connection:
/// a roster for a different Organ than the code offered is refused, and
/// nothing local changes.
#[tokio::test]
async fn a_roster_for_another_organ_is_refused() {
    let (us, our_organ) = cell("http://careful.test").await;
    let elsewhere = Signer::generate("organ-elsewhere", ROOT_KEY_ID);
    // Built by hand rather than published: this is what a hostile or confused
    // peer ANSWERS with, so it never passed through our own writer.
    let roster = engine::roster::Roster {
        organ_uid: "organ-elsewhere".into(),
        root_key: elsewhere.public_key_b64(),
        version: 1,
        not_after: (chrono::Utc::now() + chrono::Duration::days(30)).to_rfc3339(),
        cells: vec![CellEntry {
            cell_uid: store::cells::local(&us.store.pool)
                .await
                .expect("cell")
                .expect("cell record")
                .uid,
            node_id: "n-whatever".into(),
            label: "us, apparently".into(),
            operational_key: "k".into(),
            front_door: false,
            capabilities: full_capabilities(),
        }],
    };
    let signature = elsewhere.sign_bytes(
        &engine::roster::roster_signing_payload(&roster).expect("payload"),
    );
    let signed = engine::roster::SignedRoster { roster, signature };
    let invite = EnrolmentInvite {
        node_id: "n-enroller".into(),
        organ_uid: "organ-we-were-offered".into(),
        root_key: elsewhere.public_key_b64(),
        token: "t".into(),
        addrs: Vec::new(),
    };

    let operational = us.operational_key_for(&invite.organ_uid).await.expect("key");
    let error = us
        .join_organ(&invite, &signed, operational)
        .await
        .expect_err("must refuse");

    assert!(
        error.to_string().contains("different Organ"),
        "got: {error}"
    );
    assert_eq!(
        store::organs::local(&us.store.pool)
            .await
            .expect("organ")
            .expect("local organ")
            .uid,
        our_organ,
        "nothing local may change on a refusal"
    );
}

/// A Cell that has already published an identity of its own has contacts who
/// hold its key. Joining another Organ would strand every one of them.
#[tokio::test]
async fn a_cell_with_a_published_identity_will_not_join() {
    let (us, our_organ) = cell("http://established.test").await;
    let our_root = Signer::generate(&our_organ, ROOT_KEY_ID);
    us.publish_root_key(&our_root).await.expect("root key");
    // Naming ITSELF, which is what publishing your own roster means. A roster
    // that omits this Cell is a self-revocation, and the database now refuses
    // its writes accordingly — correct, but not what this test is about.
    us.publish_roster(
        &our_root,
        vec![CellEntry {
            cell_uid: store::cells::local(&us.store.pool)
                .await
                .expect("cell")
                .expect("cell record")
                .uid,
            node_id: "n-established".into(),
            label: "this one".into(),
            operational_key: "k".into(),
            front_door: false,
            capabilities: full_capabilities(),
        }],
    )
    .await
    .expect("our own roster");

    let error = us.may_enrol().await.expect_err("must refuse");
    assert!(
        error.to_string().contains("already has a published identity"),
        "got: {error}"
    );
}

/// The schema fact that made a second Cell impossible, stated as a test.
///
/// `identity_key` is keyed `(actor_uid, key_id)` and every Cell published its
/// transport key under `ed25519:organ:v1`, so two Cells of one Organ collided
/// on one row — and `require_published_key` refuses to overwrite a published
/// key, so the second could not bind its own at all. Per-Cell key ids are what
/// fix it.
#[tokio::test]
async fn two_cells_of_one_organ_each_hold_their_own_operational_key() {
    let (engine, organ) = cell("http://two-keys.test").await;
    let ours = store::cells::local(&engine.store.pool)
        .await
        .expect("cell")
        .expect("cell record")
        .uid;
    let sibling = "r-the-phone";

    let our_key = Signer::generate(&organ, &engine::roster::cell_key_id(&ours));
    engine
        .set_organ_signer(our_key.clone())
        .await
        .expect("this Cell binds its key");
    // The sibling's key, as it would arrive through an Introduction: same
    // Organ, different Cell, DIFFERENT public key.
    let their_key = Signer::generate(&organ, &engine::roster::cell_key_id(sibling));
    engine::trust::adopt_key(
        &engine.store,
        &organ,
        &engine::roster::cell_key_id(sibling),
        &their_key.public_key_b64(),
    )
    .await
    .expect("a sibling's key must be storable alongside ours");

    // Both resolve, independently, by the id the wire carries.
    assert_eq!(
        engine::trust::key_of(&engine.store, &organ, &engine::roster::cell_key_id(&ours))
            .await
            .expect("query"),
        Some(our_key.public_key_b64())
    );
    assert_eq!(
        engine::trust::key_of(&engine.store, &organ, &engine::roster::cell_key_id(sibling))
            .await
            .expect("query"),
        Some(their_key.public_key_b64()),
        "the sibling's key must not have been swallowed by ours"
    );

    // And rebinding this Cell's own key is still refused: immutability is now
    // per-Cell rather than per-Organ, which is the property that was wanted.
    let impostor = Signer::generate(&organ, &engine::roster::cell_key_id(&ours));
    assert!(
        engine.set_organ_signer(impostor).await.is_err(),
        "a published key id is still immutable"
    );
}

/// An operational key must NEVER validate a roster.
///
/// It signs traffic in the Organ's name; the root speaks for the identity. If
/// the two were one set, a stolen phone could sign itself a roster adding more
/// devices and the whole two-key split would be decoration. Before per-Cell
/// key ids there was exactly one operational key per Organ and it sat quietly
/// in the set `key_chains` walks.
#[tokio::test]
async fn a_cell_key_cannot_speak_for_the_identity() {
    let (engine, organ) = cell("http://not-a-root.test").await;
    let cell_uid = store::cells::local(&engine.store.pool)
        .await
        .expect("cell")
        .expect("cell record")
        .uid;
    let operational = Signer::generate(&organ, &engine::roster::cell_key_id(&cell_uid));
    engine
        .set_organ_signer(operational.clone())
        .await
        .expect("binds");

    assert!(
        !engine
            .key_chains(&organ, &operational.public_key_b64())
            .await
            .expect("chain"),
        "an operational key must not chain — it may sign traffic, never identity"
    );

    // A roster it signs is therefore refused outright.
    let roster = engine::roster::Roster {
        organ_uid: organ.clone(),
        root_key: operational.public_key_b64(),
        version: 99,
        not_after: (chrono::Utc::now() + chrono::Duration::days(30)).to_rfc3339(),
        cells: Vec::new(),
    };
    let signature =
        operational.sign_bytes(&engine::roster::roster_signing_payload(&roster).expect("payload"));
    assert_eq!(
        engine
            .adopt_roster(&engine::roster::SignedRoster { roster, signature })
            .await
            .expect("adopt"),
        engine::roster::RosterOutcome::Refused,
        "a device signing itself more devices is exactly what the split prevents"
    );
}

/// Two Cells of one Organ converge — the point of everything above.
///
/// The enrolled device is only useful once it actually syncs, and this is the
/// path no contact row can describe: a sibling shares our Organ uid, so
/// `organ_contact` cannot hold it. The signed roster is the sibling list, and
/// the accept gate recognises a Cell by it.
#[tokio::test]
async fn two_cells_of_one_organ_converge() {
    let (them, their_organ, root, their_wire) = enroller(61).await;
    let (us, _) = cell("http://sibling.test").await;
    let our_wire = Wire::bind(us.clone(), secret(62), Reach::Local)
        .await
        .expect("binds");
    let invite = invite_from(&them, &their_organ, &root, &their_wire).await;
    // Dialing by NodeId alone needs an address on a `Local` endpoint.
    our_wire.remember_addr(
        iroh::EndpointAddr::new(their_wire.node_id()).with_ip_addr(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            their_wire
                .endpoint()
                .bound_sockets()
                .first()
                .expect("bound")
                .port(),
        )),
    );

    let serving = tokio::spawn(async move { their_wire.serve().await });
    our_wire.enrol(&invite).await.expect("enrolment");

    // The first Cell writes something. Nothing is pushed: siblings are
    // pull-only, so this must arrive because the second Cell ASKED.
    store::records::create(
        &them.store.pool,
        NewRecord {
            slug: Some("written-on-the-first-device"),
            kind: RecordKind::Plain,
            head: "A note",
            body: "typed on the phone",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record");

    our_wire.sync_once().await.expect("a sync pass");

    let landed = store::records::resolve(&us.store.pool, "written-on-the-first-device")
        .await
        .expect("resolve")
        .expect("the sibling's write must arrive");
    assert_eq!(landed.head, "A note");
    assert_eq!(
        landed.organ_uid,
        Some(their_organ),
        "and it belongs to the shared Organ, not to either device"
    );

    serving.abort();
}

/// Membership is not enough: a Cell listed WITHOUT `CAP_WRITE` — a relay, a
/// front door — must not be able to push ops in the Organ's name. That is what
/// keeps "the front door holds no signing material" a structural fact rather
/// than a promise.
#[tokio::test]
async fn a_capability_less_sibling_is_not_a_writer() {
    let (them, their_organ, root, their_wire) = enroller(63).await;
    let relay_node = "kzcgcjhoxfvpplzvrdxi2ldw6h2qzr3rrl3vcgxzuwvnkfvfvzha";
    them.enrol_cell(
        &root,
        CellEntry {
            cell_uid: "c-relay".into(),
            node_id: relay_node.into(),
            label: "a carrier and nothing more".into(),
            operational_key: "k-relay".into(),
            front_door: true,
            capabilities: engine::roster::relay_capabilities(),
        },
    )
    .await
    .expect("the relay joins the roster");

    // A real writing sibling, so this test cannot pass by `sibling_organ`
    // returning `None` for everything.
    let phone_node = "kzcgcjhoxfvpplzvrdxi2ldw6h2qzr3rrl3vcgxzuwvnkfvfvzhb";
    them.enrol_cell(
        &root,
        CellEntry {
            cell_uid: "c-phone".into(),
            node_id: phone_node.into(),
            label: "the phone".into(),
            operational_key: "k-phone".into(),
            front_door: false,
            capabilities: full_capabilities(),
        },
    )
    .await
    .expect("the phone joins the roster");
    assert_eq!(
        their_wire.sibling_organ(phone_node).await,
        Some(their_organ),
        "an ordinary member IS recognised, and as this Organ"
    );

    assert!(
        their_wire.sibling_organ(relay_node).await.is_none(),
        "a Cell with no capabilities must not be recognised as a writer"
    );
    assert!(
        their_wire
            .sibling_organ(&their_wire.node_id().to_string())
            .await
            .is_none(),
        "and neither is this Cell itself — a sibling is another device"
    );
}

/// The front door (Ontology §11, "Front-door mechanics").
///
/// A stranger's "add me in Lince" reaches the always-on Cell, whose owner may
/// be on a phone that is offline and not in the public record. The door cannot
/// decide — it holds no `CAP_REPRESENT` — so it HOLDS the request until a Cell
/// that can decide comes and takes it.
#[tokio::test]
async fn a_front_door_holds_a_strangers_knock_for_the_owner() {
    // The door: a Cell of an Organ whose roster gives it nothing.
    let (door, door_organ) = cell("http://front-door.test").await;
    let door_wire = Wire::bind(door.clone(), secret(71), Reach::Local)
        .await
        .expect("binds");
    let door_cell = store::cells::local(&door.store.pool)
        .await
        .expect("cell")
        .expect("cell record")
        .uid;
    let root = Signer::generate(&door_organ, ROOT_KEY_ID);
    door.publish_root_key(&root).await.expect("root key");
    door.publish_roster(
        &root,
        vec![CellEntry {
            cell_uid: door_cell,
            node_id: door_wire.node_id().to_string(),
            label: "the always-on VPS".into(),
            operational_key: "k-door".into(),
            front_door: true,
            // A carrier and nothing more.
            capabilities: engine::roster::relay_capabilities(),
        }],
    )
    .await
    .expect("roster");

    // The stranger.
    let (them, their_organ) = cell("http://stranger.test").await;
    let their_wire = Wire::bind(them.clone(), secret(72), Reach::Local)
        .await
        .expect("binds");
    their_wire.remember_addr(
        iroh::EndpointAddr::new(door_wire.node_id()).with_ip_addr(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            door_wire
                .endpoint()
                .bound_sockets()
                .first()
                .expect("bound")
                .port(),
        )),
    );
    // The door must let a stranger knock at all.
    // Through the CELL path, logging no op — a relay Cell may not write, and
    // configuring itself is the one thing it must still be able to do.
    store::cells::set_config(
        &door.store.pool,
        "lince.discovery",
        // The WHOLE namespace, because the Discovery panel writes all three
        // keys together and replaces rather than merges.
        &serde_json::json!({ "local": false, "internet": false, "accept_unknown": true }),
    )
    .await
    .expect("open the invite door");

    let door_addr = iroh::EndpointAddr::new(door_wire.node_id());
    let serving = tokio::spawn(async move { door_wire.serve().await });
    let intro = them.introduction().await.expect("introduction");
    let response = their_wire
        .request(
            door_addr,
            engine::wire::ALPN_THREAD,
            &engine::wire::WireRequest::Introduce { intro },
        )
        .await
        .expect("the door answers");

    match response {
        engine::wire::WireResponse::Refused { code, .. } => assert_eq!(
            code, "held_for_owner",
            "the door must say it is holding, not pretend to decide"
        ),
        other => panic!("expected a held answer, got {other:?}"),
    }

    // Held, and NOT bound: a door that quietly created a contact row would be
    // deciding on the owner's behalf with no capability to do so.
    assert_eq!(
        store::door::count(&door.store.pool).await.expect("count"),
        1,
        "the knock must be waiting"
    );
    assert!(
        store::organs::contact(&door.store.pool, &their_organ)
            .await
            .expect("contact")
            .is_none(),
        "the front door must not bind a contact itself"
    );

    serving.abort();
}

/// The other half of the front door: a Cell that CAN decide comes and takes
/// what the door was holding.
#[tokio::test]
async fn the_owner_collects_what_the_front_door_held() {
    // One Organ, two Cells: the owner's laptop (full capabilities) and a
    // front door (a carrier and nothing more).
    let (owner, organ, root, owner_wire) = enroller(81).await;
    let (door, _) = cell("http://door-two.test").await;
    let door_wire = Wire::bind(door.clone(), secret(82), Reach::Local)
        .await
        .expect("binds");
    let owner_cell = store::cells::local(&owner.store.pool)
        .await
        .expect("cell")
        .expect("cell record")
        .uid;
    let door_cell = store::cells::local(&door.store.pool)
        .await
        .expect("cell")
        .expect("cell record")
        .uid;
    let roster = owner
        .publish_roster(
            &root,
            vec![
                CellEntry {
                    cell_uid: owner_cell,
                    node_id: owner_wire.node_id().to_string(),
                    label: "the laptop".into(),
                    operational_key: "k-laptop".into(),
                    front_door: false,
                    capabilities: full_capabilities(),
                },
                CellEntry {
                    cell_uid: door_cell,
                    node_id: door_wire.node_id().to_string(),
                    label: "the VPS".into(),
                    operational_key: "k-vps".into(),
                    front_door: true,
                    capabilities: engine::roster::relay_capabilities(),
                },
            ],
        )
        .await
        .expect("roster of two");

    // The door becomes a Cell of that Organ.
    store::organs::adopt_identity(&door.store.pool, &organ, "http://door-two.test")
        .await
        .expect("the door joins the Organ");
    engine::trust::adopt_key(&door.store, &organ, ROOT_KEY_ID, &root.public_key_b64())
        .await
        .expect("root key");
    door.adopt_roster(&roster).await.expect("roster");
    store::cells::set_config(
        &door.store.pool,
        "lince.discovery",
        &serde_json::json!({ "local": false, "internet": false, "accept_unknown": true }),
    )
    .await
    .expect("open the invite door");

    // A stranger knocks at the door.
    let (them, their_organ) = cell("http://stranger-two.test").await;
    let their_wire = Wire::bind(them.clone(), secret(83), Reach::Local)
        .await
        .expect("binds");
    let door_port = door_wire
        .endpoint()
        .bound_sockets()
        .first()
        .expect("bound")
        .port();
    let door_addr = iroh::EndpointAddr::new(door_wire.node_id())
        .with_ip_addr(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), door_port));
    their_wire.remember_addr(door_addr.clone());
    // The owner needs an address for the door too — it dials it by NodeId.
    owner_wire.remember_addr(door_addr.clone());

    let door_serving = tokio::spawn(async move { door_wire.serve().await });
    let intro = them.introduction().await.expect("introduction");
    their_wire
        .request(
            door_addr,
            engine::wire::ALPN_THREAD,
            &engine::wire::WireRequest::Introduce { intro },
        )
        .await
        .expect("the door answers");
    assert_eq!(
        store::door::count(&door.store.pool).await.expect("count"),
        1,
        "the door is holding it"
    );
    assert!(
        store::organs::contact(&owner.store.pool, &their_organ)
            .await
            .expect("contact")
            .is_none(),
        "and the owner has not seen it yet"
    );

    // The owner's device comes online and runs an ordinary sync pass.
    owner_wire.sync_once().await.expect("a sync pass");

    let arrived = store::organs::contact(&owner.store.pool, &their_organ)
        .await
        .expect("contact")
        .expect("the knock must reach the Cell that can decide about it");
    assert_eq!(
        arrived.trust, "unknown",
        "and it arrives PENDING a person — the door bought the knock not being \
         lost, never a decision"
    );
    assert_eq!(
        store::door::count(&door.store.pool).await.expect("count"),
        0,
        "the door releases what was taken"
    );

    door_serving.abort();
}

/// An epoch cut makes a stale Cell INVISIBLE rather than merely out of date —
/// the sync door does not open, and from the dialing side that is identical to
/// a device someone turned off. `ALPN_HELLO` is the one protocol that survives
/// a cut, so it is what tells the two apart (Ontology §11, decision 1).
#[tokio::test]
async fn a_sibling_answers_the_stable_hello_across_any_epoch() {
    let (owner, organ, root, owner_wire) = enroller(91).await;
    let (other, _) = cell("http://hello-peer.test").await;
    let other_wire = Wire::bind(other.clone(), secret(92), Reach::Local)
        .await
        .expect("binds");
    let owner_cell = store::cells::local(&owner.store.pool)
        .await
        .expect("cell")
        .expect("cell record")
        .uid;
    let other_cell = store::cells::local(&other.store.pool)
        .await
        .expect("cell")
        .expect("cell record")
        .uid;
    let roster = owner
        .publish_roster(
            &root,
            vec![
                CellEntry {
                    cell_uid: owner_cell,
                    node_id: owner_wire.node_id().to_string(),
                    label: "the laptop".into(),
                    operational_key: "k-a".into(),
                    front_door: false,
                    capabilities: full_capabilities(),
                },
                CellEntry {
                    cell_uid: other_cell,
                    node_id: other_wire.node_id().to_string(),
                    label: "the phone".into(),
                    operational_key: "k-b".into(),
                    front_door: false,
                    capabilities: full_capabilities(),
                },
            ],
        )
        .await
        .expect("roster of two");
    store::organs::adopt_identity(&other.store.pool, &organ, "http://hello-peer.test")
        .await
        .expect("join");
    engine::trust::adopt_key(&other.store, &organ, ROOT_KEY_ID, &root.public_key_b64())
        .await
        .expect("root key");
    other.adopt_roster(&roster).await.expect("roster");

    let port = other_wire
        .endpoint()
        .bound_sockets()
        .first()
        .expect("bound")
        .port();
    let addr = iroh::EndpointAddr::new(other_wire.node_id())
        .with_ip_addr(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port));
    let serving = tokio::spawn(async move { other_wire.serve().await });

    let epoch = owner_wire
        .hello(addr)
        .await
        .expect("a sibling must answer the stable hello");
    assert_eq!(
        epoch,
        engine::wire::WIRE_EPOCH,
        "and report the epoch it speaks"
    );

    // Same epoch, so nothing is reported as stale.
    owner_wire.sync_once().await.expect("a sync pass");
    assert!(
        owner_wire.stale_siblings().is_empty(),
        "a Cell on our own epoch is not a Cell that needs updating"
    );

    serving.abort();
}

/// The cross-Organ audit (Ontology §11, C2b): ask a contact what they hold of
/// OUR ops and say whether the two logs agree, without moving any ops.
///
/// `audit_read_model` compares this Cell against its own log and catches
/// nothing about a peer. Catch-up only ever asks what IT is missing, so a
/// contact silently behind is invisible — which is the gap this closes.
#[tokio::test]
async fn an_audit_sees_what_a_contact_is_missing() {
    let (us, our_organ) = cell("http://auditor.test").await;
    let (them, their_organ) = cell("http://audited.test").await;
    let our_wire = Wire::bind(us.clone(), secret(101), Reach::Local)
        .await
        .expect("binds");
    let their_wire = Wire::bind(them.clone(), secret(102), Reach::Local)
        .await
        .expect("binds");
    // Each side knows the other, and can reach it.
    for (engine, organ, node) in [
        (&us, &their_organ, their_wire.node_id().to_string()),
        (&them, &our_organ, our_wire.node_id().to_string()),
    ] {
        store::organs::add_contact(&engine.store.pool, organ, None, "peer", "", 0)
            .await
            .expect("contact");
        store::organs::set_node_id(&engine.store.pool, organ, Some(&node))
            .await
            .expect("node id");
        store::organs::set_trust(&engine.store.pool, organ, "known")
            .await
            .expect("trust");
    }
    let port = their_wire
        .endpoint()
        .bound_sockets()
        .first()
        .expect("bound")
        .port();
    our_wire.remember_addr(
        iroh::EndpointAddr::new(their_wire.node_id())
            .with_ip_addr(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)),
    );

    // We write something they have never seen.
    store::records::create(
        &us.store.pool,
        NewRecord {
            slug: Some("only-on-our-side"),
            kind: RecordKind::Plain,
            head: "Not sent yet",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record");

    let serving = tokio::spawn(async move { their_wire.serve().await });
    let report = our_wire
        .audit_against(&their_organ)
        .await
        .expect("the audit runs")
        .expect("and reaches them");

    assert!(
        report.they_lack > 0,
        "the audit must see that they hold none of our ops"
    );
    assert!(
        report.unknown_cells > 0,
        "and that they have never seen an op from this Cell at all"
    );

    serving.abort();
}

/// "The front door holds no signing material" as an ENFORCED property
/// (Ontology §11, C4), not a promise kept by everything politely not asking.
///
/// The refusal lives in the DATABASE, below every client, because a property
/// you can defeat by pointing a second client at the same store is not a
/// security property.
#[tokio::test]
async fn a_relay_cell_cannot_author_anything() {
    let (relay, organ) = cell("http://relay-cell.test").await;
    let root = Signer::generate(&organ, ROOT_KEY_ID);
    relay.publish_root_key(&root).await.expect("root key");
    let relay_cell = store::cells::local(&relay.store.pool)
        .await
        .expect("cell")
        .expect("cell record")
        .uid;

    // Before any roster exists this Cell IS the whole Organ, and writing works.
    store::records::create(
        &relay.store.pool,
        NewRecord {
            slug: Some("before-the-roster"),
            kind: RecordKind::Plain,
            head: "Allowed",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("a Cell with no roster is the whole Organ");

    // The root then says this Cell is a carrier and nothing more.
    relay
        .publish_roster(
            &root,
            vec![CellEntry {
                cell_uid: relay_cell,
                node_id: "n-relay".into(),
                label: "the VPS".into(),
                operational_key: "k-relay".into(),
                front_door: true,
                capabilities: engine::roster::relay_capabilities(),
            }],
        )
        .await
        .expect("roster");

    let refused = store::records::create(
        &relay.store.pool,
        NewRecord {
            slug: Some("after-the-roster"),
            kind: RecordKind::Plain,
            head: "Not allowed",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await;
    assert!(
        refused.is_err(),
        "a Cell the root gave no write capability must not be able to author"
    );
    assert!(
        format!("{refused:?}").contains("no write capability"),
        "and the refusal must say why: {refused:?}"
    );

    // Configuring ITSELF still works, and must: a relay that cannot be
    // configured cannot be operated. Cell config logs no op.
    store::cells::set_config(
        &relay.store.pool,
        "lince.discovery",
        &serde_json::json!({ "local": false, "internet": true, "accept_unknown": true }),
    )
    .await
    .expect("a relay must still be able to configure itself");

    // And the root can give it back, which is what makes this a capability
    // rather than a one-way door.
    relay
        .publish_roster(
            &root,
            vec![CellEntry {
                cell_uid: store::cells::local(&relay.store.pool)
                    .await
                    .expect("cell")
                    .expect("cell record")
                    .uid,
                node_id: "n-relay".into(),
                label: "the VPS".into(),
                operational_key: "k-relay".into(),
                front_door: true,
                capabilities: full_capabilities(),
            }],
        )
        .await
        .expect("roster");
    store::records::create(
        &relay.store.pool,
        NewRecord {
            slug: Some("after-the-grant"),
            kind: RecordKind::Plain,
            head: "Allowed again",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("the root can restore the capability");
}
