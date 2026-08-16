//! The public directory record (Ontology §11 "Identity, roster, and
//! publishing", cluster C3).
//!
//! Nothing here touches the network. The pkarr client is built on first use
//! and none of these tests reach a code path that builds one: encoding,
//! decoding, verification and storage are all offline, which is the whole
//! reason they are separable from broadcasting.

use engine::Engine;
use engine::directory::{self, MAX_PUBLIC_CELLS, PublicRecord};
use engine::roster::{CellEntry, Roster, SignedRoster, full_capabilities};
use engine::trust::Signer;

fn cell_entry(label: &str, front_door: bool) -> CellEntry {
    CellEntry {
        cell_uid: uuid::Uuid::new_v4().to_string(),
        // A NodeId is 32 bytes rendered as 64 hex characters. Real length
        // matters here: this file is partly a size measurement.
        node_id: (0..64)
            .map(|index| char::from(b'a' + ((index as u8) % 6)))
            .collect(),
        label: label.to_string(),
        operational_key: "A".repeat(44),
        sealing_key: None,
        front_door,
        capabilities: full_capabilities(),
    }
}

fn signed_roster(cells: Vec<CellEntry>) -> SignedRoster {
    SignedRoster {
        roster: Roster {
            organ_uid: "8f14e45f-ea5b-4b1e-9f0f-0e0d0c0b0a09".into(),
            root_key: "R".repeat(44),
            version: 7,
            not_after: "2026-09-10T12:00:00+00:00".into(),
            pickup: Vec::new(),
            cells,
        },
        signature: "S".repeat(88),
    }
}

fn secret() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = index as u8 + 1;
    }
    bytes
}

/// The privacy property, stated structurally: a personal Cell cannot reach the
/// public record by any path, because it does not survive `public_record`.
#[test]
fn only_front_doors_reach_the_public_tier() {
    let roster = signed_roster(vec![
        cell_entry("laptop", false),
        cell_entry("vps", true),
        cell_entry("phone", false),
    ]);
    let record = directory::public_record(&roster);

    assert_eq!(record.node_ids.len(), 1, "only the front door is published");
    assert_eq!(record.organ_uid, roster.roster.organ_uid);
    assert_eq!(
        record.roster_version, 7,
        "the version travels so a contact learns a newer roster exists"
    );
}

/// The measurement that decided this design, kept as a test so it cannot
/// quietly stop being true.
///
/// The Ontology estimated a five-Cell roster near 250 bytes, counting NodeIds
/// alone. With the uuid, label, operational key and capability list each entry
/// really carries, five Cells are past the 1000-byte DNS packet the DHT will
/// hold — so the full roster CANNOT be published and the public tier is not a
/// preference but the only thing that fits.
#[test]
fn the_full_roster_does_not_fit_and_the_public_tier_does() {
    let roster = signed_roster((0..5).map(|_| cell_entry("device", true)).collect());
    let as_json = serde_json::to_vec(&roster).expect("roster serializes");
    assert!(
        as_json.len() > 1000,
        "a five-Cell roster is {} bytes, which was supposed to be over the cap",
        as_json.len()
    );

    let record = PublicRecord {
        organ_uid: roster.roster.organ_uid.clone(),
        roster_version: roster.roster.version,
        node_ids: roster
            .roster
            .cells
            .iter()
            .take(MAX_PUBLIC_CELLS)
            .map(|cell| cell.node_id.clone())
            .collect(),
    };
    let packet = directory::encode(&record, &secret()).expect("the public tier encodes");
    assert!(
        packet.len() <= pkarr::SignedPacket::MAX_BYTES as usize + 8,
        "a full public tier is {} bytes, past what the DHT carries",
        packet.len()
    );
}

#[test]
fn a_signed_record_round_trips() {
    let record = PublicRecord {
        organ_uid: "8f14e45f-ea5b-4b1e-9f0f-0e0d0c0b0a09".into(),
        roster_version: 12,
        node_ids: vec!["a".repeat(64), "b".repeat(64), "c".repeat(64)],
    };
    let bytes = directory::encode(&record, &secret()).expect("encodes");
    let packet = directory::decode_stored(&bytes).expect("stored bytes verify");
    let decoded = directory::decode(&packet).expect("decodes");

    assert_eq!(decoded, record, "every field survives the DNS round trip");
    assert_eq!(
        decoded.node_ids, record.node_ids,
        "and in order — `attributes()` is a HashMap, so the index in the key \
         is what restores it"
    );
}

/// The packet is addressed BY the root key, which is what makes pkarr's own
/// signature check the root signature check. If this ever stopped holding, a
/// second signature layer would be needed and the module comment would be a
/// lie.
#[test]
fn the_packet_is_addressed_by_the_root_key() {
    let root = Signer::from_bytes("organ-1", "ed25519:root:v1", secret());
    let record = PublicRecord {
        organ_uid: "organ-1".into(),
        roster_version: 1,
        node_ids: vec!["d".repeat(64)],
    };
    let bytes = directory::encode(&record, &secret()).expect("encodes");
    let packet = directory::decode_stored(&bytes).expect("verifies");

    assert_eq!(
        directory::packet_root_key(&packet),
        root.public_key_b64(),
        "the lookup key IS the identity key a contact saved"
    );
    directory::lookup_key(&root.public_key_b64()).expect("that key resolves to a lookup key");
}

/// Bytes off disk are parsed WITHOUT a signature check by pkarr itself, so
/// republishing them straight would let anything that could write the database
/// choose where contacts dial.
#[test]
fn tampered_stored_bytes_are_refused() {
    let record = PublicRecord {
        organ_uid: "organ-1".into(),
        roster_version: 1,
        node_ids: vec!["e".repeat(64)],
    };
    let mut bytes = directory::encode(&record, &secret()).expect("encodes");
    // Past the 8-byte last_seen, the 32-byte key, the 64-byte signature and
    // the 8-byte timestamp: the DNS packet itself, where an address lives.
    let target = bytes.len() - 5;
    bytes[target] ^= 0xff;

    assert!(
        directory::decode_stored(&bytes).is_err(),
        "an edited packet must not verify"
    );
}

/// Fail closed on a version this build does not know, the same rule the wire
/// follows: refuse rather than half-read fields that may no longer mean what
/// they used to.
#[test]
fn an_unknown_record_version_is_refused() {
    use pkarr::dns::{Name, rdata::TXT};

    let keypair = pkarr::Keypair::from_secret_key(&secret());
    let door = format!("c0={}", "f".repeat(64));
    let mut txt = TXT::new();
    txt.add_string("v=99").expect("v");
    txt.add_string("o=organ-1").expect("o");
    txt.add_string(&door).expect("c");
    let packet = pkarr::SignedPacket::builder()
        .txt(Name::new("_lince").expect("name"), txt, 1800)
        .sign(&keypair)
        .expect("signs");

    assert!(
        directory::decode(&packet).is_err(),
        "a newer record shape is refused, not partially believed"
    );
}

/// The signature proves the root key holder wrote it. It does NOT prove the
/// record is about the Organ we were looking for.
#[test]
fn a_record_naming_a_different_organ_is_refused() {
    let record = PublicRecord {
        organ_uid: "organ-elsewhere".into(),
        roster_version: 1,
        node_ids: vec!["a".repeat(64)],
    };
    assert!(directory::verify_for(&record, "organ-elsewhere").is_ok());
    assert!(
        directory::verify_for(&record, "organ-1").is_err(),
        "a key we saved under one uid answering with another is a substitution"
    );
}

/// Publishing an address-less record is worse than publishing none: it answers
/// "where is this Organ" with an authoritative "nowhere" until it expires.
#[test]
fn a_record_with_no_front_door_will_not_encode() {
    let record = PublicRecord {
        organ_uid: "organ-1".into(),
        roster_version: 1,
        node_ids: Vec::new(),
    };
    assert!(directory::encode(&record, &secret()).is_err());
}

async fn organ_with_root() -> (Engine, Signer, String) {
    let engine = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&engine.store.pool, "http://directory.test")
        .await
        .expect("local organ")
        .uid;
    let root = Signer::from_bytes(&organ, "ed25519:root:v1", secret());
    (engine, root, organ)
}

#[tokio::test]
async fn signing_stores_bytes_that_republish_without_the_key() {
    let (engine, root, organ) = organ_with_root().await;
    engine
        .publish_roster(&root, vec![cell_entry("vps", true)])
        .await
        .expect("roster");

    engine.sign_public_record(&root).await.expect("signs");
    let stored = store::roster::public_packet(&engine.store.pool, &organ)
        .await
        .expect("query")
        .expect("a packet was stored");

    // The whole point of storing it: this path holds no key at all.
    let packet = directory::decode_stored(&stored).expect("verifies");
    let decoded = directory::decode(&packet).expect("decodes");
    assert_eq!(decoded.organ_uid, organ);
    assert_eq!(decoded.node_ids.len(), 1);
    assert_eq!(decoded.roster_version, 1);
}

/// Signing again with an unchanged roster must produce the SAME BYTES.
///
/// pkarr orders packets by an embedded timestamp and refuses a publish older
/// than what a relay already holds, so a fresh signature on every boot would
/// put the Cell holding the root into a timestamp race with a keyless Cell
/// republishing the stored bytes — two packets for one key differing only in
/// when they were signed.
#[tokio::test]
async fn an_unchanged_roster_re_signs_nothing() {
    let (engine, root, organ) = organ_with_root().await;
    engine
        .publish_roster(&root, vec![cell_entry("vps", true)])
        .await
        .expect("roster");

    engine.sign_public_record(&root).await.expect("signs");
    let first = store::roster::public_packet(&engine.store.pool, &organ)
        .await
        .expect("query")
        .expect("stored");
    engine.sign_public_record(&root).await.expect("signs again");
    let second = store::roster::public_packet(&engine.store.pool, &organ)
        .await
        .expect("query")
        .expect("stored");

    assert_eq!(
        first, second,
        "the same content must sign to the same bytes"
    );
}

/// Turning the front door off must STOP the broadcast, not leave the republish
/// timer re-announcing an address that is no longer meant to be public.
#[tokio::test]
async fn dropping_the_last_front_door_clears_the_published_record() {
    let (engine, root, organ) = organ_with_root().await;
    engine
        .publish_roster(&root, vec![cell_entry("vps", true)])
        .await
        .expect("roster");
    engine.sign_public_record(&root).await.expect("signs");
    assert!(
        store::roster::public_packet(&engine.store.pool, &organ)
            .await
            .expect("query")
            .is_some()
    );

    engine
        .publish_roster(&root, vec![cell_entry("laptop", false)])
        .await
        .expect("roster without a front door");
    engine.sign_public_record(&root).await.expect("signs");

    assert!(
        store::roster::public_packet(&engine.store.pool, &organ)
            .await
            .expect("query")
            .is_none(),
        "an Organ with no front door publishes nothing"
    );
    assert!(
        !engine
            .republish_public_record(&organ)
            .await
            .expect("republish is not an error"),
        "and republishing finds nothing to broadcast — no network is touched"
    );
}
