//! Sealing a batch for a carrier that must not read it (Ontology C4).
//!
//! The round trip is the least interesting test here. What these mostly check
//! is what a MAILBOX makes possible that a direct connection did not: bytes
//! that sat on a third party's disk, arriving with no connection to say who
//! sent them. Every case below is something that would be caught by the QUIC
//! peer identity on the ordinary sync path and is caught by nothing at all on
//! this one unless the bundle itself carries the answer.

use ed25519_dalek::SigningKey;
use engine::seal::{self, SealError, SealedBundle};
use engine::sync::{OpBatch, WireOp};

fn batch(from_organ: &str, cell: &str) -> seal::MailedBatch {
    seal::MailedBatch {
        // The general feed; the grant channel is exercised in `mailbox.rs`,
        // where there is an accepted grant to import against.
        root: None,
        batch: OpBatch {
        from_organ: from_organ.to_string(),
        ops: vec![WireOp {
            tbl: "record".into(),
            uid: "record-1".into(),
            field: "title".into(),
            kind: "set".into(),
            value: Some("a thing said in confidence".into()),
            hlc: 1_000,
            actor_cell: cell.into(),
            organ_uid: from_organ.to_string(),
            fact: None,
        }],
        },
    }
}

fn sender() -> (SigningKey, ed25519_dalek::VerifyingKey) {
    let key = SigningKey::from_bytes(&[7u8; 32]);
    let public = key.verifying_key();
    (key, public)
}

/// A recipient Organ with two devices: both get a wrap, either can collect.
fn two_cells() -> (Vec<seal::SealingKey>, Vec<(String, [u8; 32])>) {
    let (phone_secret, phone) = seal::generate("cell-phone", 1, "2099-01-01T00:00:00Z");
    let (laptop_secret, laptop) = seal::generate("cell-laptop", 1, "2099-01-01T00:00:00Z");
    let held = vec![
        (phone.key_id.clone(), phone_secret),
        (laptop.key_id.clone(), laptop_secret),
    ];
    (vec![phone, laptop], held)
}

#[test]
fn a_sealed_batch_opens_on_every_cell_of_the_recipient() {
    let (signing, verifying) = sender();
    let (published, held) = two_cells();
    let bundle = seal::seal(
        &batch("organ-sender", "cell-sender"),
        "cell-sender",
        "organ-recipient",
        &published,
        &signing,
    )
    .expect("seals");

    // Mail must be collectable from whichever device comes online first, so
    // each Cell is asked with ONLY its own key.
    for (key_id, secret) in &held {
        let opened = seal::open(&bundle, &verifying, std::slice::from_ref(&(
            key_id.clone(),
            *secret,
        )))
        .unwrap_or_else(|why| panic!("{key_id} could not open its own mail: {why}"));
        assert_eq!(opened.from_organ, "organ-sender");
        assert_eq!(opened.from_cell, "cell-sender");
        assert_eq!(opened.batch.ops[0].value.as_deref(), Some("a thing said in confidence"));
    }
}

#[test]
fn the_carrier_learns_nothing_from_the_bytes_it_holds() {
    let (signing, _) = sender();
    let (published, _) = two_cells();
    let bundle = seal::seal(
        &batch("organ-sender", "cell-sender"),
        "cell-sender",
        "organ-recipient",
        &published,
        &signing,
    )
    .expect("seals");

    // Serialize the way a mailbox would store it and search the whole thing.
    // The plaintext must not survive anywhere in the stored form — not in the
    // ciphertext, and not leaked into a field somebody added for convenience.
    let stored = serde_json::to_string(&bundle).expect("serializes");
    assert!(!stored.contains("a thing said in confidence"));
    assert!(!stored.contains("record-1"));
    assert!(!stored.contains("title"));
    // What it DOES learn is stated in the design and must stay exactly this
    // much: who it is for, who left it, and which Cell signed it.
    assert!(stored.contains("organ-recipient"));
    assert!(stored.contains("organ-sender"));
}

#[test]
fn a_bundle_for_someone_else_does_not_open() {
    let (signing, verifying) = sender();
    let (published, _) = two_cells();
    let (nosy_secret, nosy) = seal::generate("cell-mailbox", 1, "2099-01-01T00:00:00Z");
    let eavesdropper = vec![(nosy.key_id, nosy_secret)];
    let bundle = seal::seal(
        &batch("organ-sender", "cell-sender"),
        "cell-sender",
        "organ-recipient",
        &published,
        &signing,
    )
    .expect("seals");

    // Holding the bundle and a perfectly good sealing key of your own is not
    // enough; this is the case the mailbox operator is in.
    assert_eq!(
        seal::open(&bundle, &verifying, &eavesdropper).unwrap_err(),
        SealError::NotForUs
    );

    // And a key that merely COLLIDES on identifier gets no further. Key ids
    // are derived from the Cell uid, so this cannot happen by accident; what
    // it checks is that matching is not what authorises the read — the wrap
    // is bound to the actual point, so a chosen key_id only changes which
    // error comes back.
    let (impostor_secret, _) = seal::generate("anything", 1, "2099-01-01T00:00:00Z");
    let collides = vec![(published[0].key_id.clone(), impostor_secret)];
    assert_eq!(
        seal::open(&bundle, &verifying, &collides).unwrap_err(),
        SealError::Undecipherable
    );
}

#[test]
fn an_unsigned_sender_is_refused_before_anything_is_decrypted() {
    let (signing, _) = sender();
    let (published, held) = two_cells();
    let bundle = seal::seal(
        &batch("organ-sender", "cell-sender"),
        "cell-sender",
        "organ-recipient",
        &published,
        &signing,
    )
    .expect("seals");

    // Out of a mailbox there is no connection to anchor `from_organ`, so the
    // signature is the ONLY thing standing between a stranger and an op
    // attributed to a contact. Verify against a different Cell's key.
    let impostor = SigningKey::from_bytes(&[9u8; 32]).verifying_key();
    assert_eq!(
        seal::open(&bundle, &impostor, &held).unwrap_err(),
        SealError::Unauthenticated
    );
}

#[test]
fn tampering_with_the_ciphertext_is_refused() {
    let (signing, verifying) = sender();
    let (published, held) = two_cells();
    let mut bundle = seal::seal(
        &batch("organ-sender", "cell-sender"),
        "cell-sender",
        "organ-recipient",
        &published,
        &signing,
    )
    .expect("seals");

    // A carrier holds these bytes at rest for weeks; flipping one is the
    // cheapest thing it can do.
    bundle.ciphertext = flip_last(&bundle.ciphertext);
    assert_eq!(
        seal::open(&bundle, &verifying, &held).unwrap_err(),
        SealError::Unauthenticated
    );
}

#[test]
fn a_bundle_readdressed_to_another_organ_does_not_open() {
    let (signing, verifying) = sender();
    let (published, held) = two_cells();
    let mut bundle = seal::seal(
        &batch("organ-sender", "cell-sender"),
        "cell-sender",
        "organ-recipient",
        &published,
        &signing,
    )
    .expect("seals");

    // The interesting attacker here is the SENDER, who can re-sign, so the
    // signature does not catch this one. Take the same sealed bytes and claim
    // they were addressed to somebody else — the content AAD is bound to the
    // pair, so it fails.
    bundle.to_organ = "organ-somebody-else".into();
    resign(&mut bundle, &signing);
    assert_eq!(
        seal::open(&bundle, &verifying, &held).unwrap_err(),
        SealError::Undecipherable
    );
}

#[test]
fn a_wrap_replayed_under_another_key_id_does_not_open() {
    let (signing, verifying) = sender();
    let (published, held) = two_cells();
    let mut bundle = seal::seal(
        &batch("organ-sender", "cell-sender"),
        "cell-sender",
        "organ-recipient",
        &published,
        &signing,
    )
    .expect("seals");

    // Move the phone's wrap onto the laptop's key_id and re-sign. What stops
    // this is the Diffie-Hellman itself: the laptop derives from its own
    // point, so it simply gets a different key. Verified by disabling the
    // key_id binding, which does NOT make this test fail — that binding is
    // domain separation and is proven by the test below instead.
    let phone = bundle.recipients[0].clone();
    let laptop_id = bundle.recipients[1].key_id.clone();
    bundle.recipients = vec![SealedToLike::rebuild(&phone, &laptop_id)];
    resign(&mut bundle, &signing);
    assert_eq!(
        seal::open(&bundle, &verifying, &held).unwrap_err(),
        SealError::Undecipherable
    );
}

#[test]
fn two_key_ids_sharing_one_point_do_not_share_a_wrap() {
    let (signing, verifying) = sender();
    // A Cell that rotates carelessly — or is made to — can end up publishing
    // the SAME X25519 point under two generations. The Diffie-Hellman cannot
    // tell those apart, so it is the only case where naming the key_id in the
    // HKDF `info` and the wrap AAD is what does the work. Without that
    // binding a wrap made for generation 1 would open as generation 2.
    let (secret, gen1) = seal::generate("cell-phone", 1, "2099-01-01T00:00:00Z");
    let gen2 = seal::SealingKey {
        key_id: "x25519:cell:cell-phone:2".into(),
        public: gen1.public.clone(),
        not_after: gen1.not_after.clone(),
    };
    let mut bundle = seal::seal(
        &batch("organ-sender", "cell-sender"),
        "cell-sender",
        "organ-recipient",
        &[gen1],
        &signing,
    )
    .expect("seals");

    bundle.recipients[0].key_id = gen2.key_id.clone();
    resign(&mut bundle, &signing);
    assert_eq!(
        seal::open(&bundle, &verifying, &[(gen2.key_id, secret)]).unwrap_err(),
        SealError::Undecipherable
    );
}

#[test]
fn a_batch_that_disagrees_with_the_label_the_carrier_routed_on_is_refused() {
    let (signing, verifying) = sender();
    let (published, held) = two_cells();
    // Seal a batch whose inner `from_organ` is NOT the bundle's. The carrier
    // routes and quotas on the outer label; the import path believes the
    // inner one. If those may differ, a sender bills one Organ's quota while
    // writing ops attributed to another.
    let mut inner = batch("organ-sender", "cell-sender");
    inner.batch.from_organ = "organ-victim".into();
    let mut bundle = seal::seal(
        &inner,
        "cell-sender",
        "organ-recipient",
        &published,
        &signing,
    )
    .expect("seals");
    bundle.from_organ = "organ-sender".into();
    resign(&mut bundle, &signing);

    // The AAD is built from the bundle label on both sides, so a mismatch
    // shows up as undecipherable rather than reaching the equality check —
    // either way it never becomes an op.
    assert!(matches!(
        seal::open(&bundle, &verifying, &held).unwrap_err(),
        SealError::Undecipherable | SealError::Malformed(_)
    ));
}

#[test]
fn an_unknown_construction_version_refuses_rather_than_guessing() {
    let (signing, verifying) = sender();
    let (published, held) = two_cells();
    let mut bundle = seal::seal(
        &batch("organ-sender", "cell-sender"),
        "cell-sender",
        "organ-recipient",
        &published,
        &signing,
    )
    .expect("seals");

    // `AGENTS.md`: no negotiating down, no fallback branch — fail closed.
    bundle.v = 99;
    resign(&mut bundle, &signing);
    assert_eq!(
        seal::open(&bundle, &verifying, &held).unwrap_err(),
        SealError::UnknownVersion(99)
    );
}

#[test]
fn a_retained_old_key_still_opens_mail_sealed_before_the_rotation() {
    let (signing, verifying) = sender();
    // Sealed against generation 1, which is what the sender's cached roster
    // said at the time.
    let (old_secret, old_published) = seal::generate("cell-phone", 1, "2099-01-01T00:00:00Z");
    let bundle = seal::seal(
        &batch("organ-sender", "cell-sender"),
        "cell-sender",
        "organ-recipient",
        &[old_published.clone()],
        &signing,
    )
    .expect("seals");

    // The recipient has since rotated to generation 2 but retains the old
    // private key for the window plus grace. This is the whole reason old
    // keys are retained: a rotation must never strand mail already sealed
    // and not yet collected.
    let (new_secret, new_published) = seal::generate("cell-phone", 2, "2099-01-01T00:00:00Z");
    let held = vec![
        (new_published.key_id.clone(), new_secret),
        (old_published.key_id.clone(), old_secret),
    ];
    assert!(seal::open(&bundle, &verifying, &held).is_ok());

    // And once the window passes and the old private key is dropped, the
    // bundle is unopenable by anyone, including us. That is forward secrecy,
    // and it is the point of a separate rotated key.
    let only_new = vec![(new_published.key_id, new_secret)];
    assert_eq!(
        seal::open(&bundle, &verifying, &only_new).unwrap_err(),
        SealError::NotForUs
    );
}

#[test]
fn an_organ_with_no_published_sealing_key_cannot_be_mailed() {
    let (signing, _) = sender();
    // Better to refuse than to send something readable: the failure a mailbox
    // must never have is quietly falling back to plaintext when the seal is
    // unavailable.
    assert!(matches!(
        seal::seal(
            &batch("organ-sender", "cell-sender"),
            "cell-sender",
            "organ-recipient",
            &[],
            &signing,
        ),
        Err(SealError::BadRecipients(_))
    ));
}

// --- helpers -------------------------------------------------------------

/// Re-sign after tampering, for the cases where the attacker IS the sender and
/// a signature check would otherwise mask what is being tested.
fn resign(bundle: &mut SealedBundle, signing: &SigningKey) {
    use base64::Engine as _;
    use ed25519_dalek::Signer as _;
    let signature = signing.sign(&seal::transcript(bundle));
    bundle.signature = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());
}

fn flip_last(value: &str) -> String {
    use base64::Engine as _;
    let mut raw = base64::engine::general_purpose::STANDARD
        .decode(value)
        .expect("was base64");
    let last = raw.len() - 1;
    raw[last] ^= 0x01;
    base64::engine::general_purpose::STANDARD.encode(raw)
}

struct SealedToLike;

impl SealedToLike {
    fn rebuild(from: &engine::seal::SealedTo, key_id: &str) -> engine::seal::SealedTo {
        engine::seal::SealedTo {
            key_id: key_id.to_string(),
            nonce: from.nonce.clone(),
            wrapped: from.wrapped.clone(),
        }
    }
}

// --- the keyring: rotation, retention, and what the roster carries ---------

#[test]
fn a_fresh_cell_generates_one_key_and_stops_generating() {
    let dir = std::env::temp_dir().join(format!("lince-seal-{}", std::process::id()));
    let path = dir.join("keyring.json");
    let _ = std::fs::remove_file(&path);

    let first = seal::load_keyring(&path, "cell-phone").expect("creates");
    assert_eq!(first.entries.len(), 1);
    let published = first.current().expect("publishes one");

    // Every boot loads this; a boot must not mint a key. Rotation that runs
    // whenever the process starts would burn generations and re-sign the
    // roster each time, training contacts to accept a stream of rosters they
    // have no reason to inspect.
    let second = seal::load_keyring(&path, "cell-phone").expect("loads");
    assert_eq!(second.entries.len(), 1);
    assert_eq!(second.current().expect("same key"), published);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn rotation_keeps_the_old_private_key_and_stops_publishing_it() {
    let mut keyring = seal::Keyring::default();
    keyring.ensure_current("cell-phone");
    let first = keyring.current().expect("has one");

    // Age the current key past its rotation point, the way real time would.
    keyring.entries[0].not_after = "2000-01-01T00:00:00Z".into();
    assert!(keyring.ensure_current("cell-phone"));

    // Published: the new one only. Retained: both — that is the whole point,
    // and the reason the mailbox TTL and the retention window are one number.
    let second = keyring.current().expect("rotated");
    assert_ne!(second.key_id, first.key_id);
    assert_eq!(keyring.entries.len(), 2);
    let held = keyring.open_keys();
    assert!(held.iter().any(|(key_id, _)| *key_id == first.key_id));
    assert!(held.iter().any(|(key_id, _)| *key_id == second.key_id));

    // Generations go forward, so a rotation can never reuse the identifier of
    // a key some sender is still holding in a cached roster.
    assert!(second.key_id.ends_with(":2"));
}

#[test]
fn a_key_past_its_retention_window_is_deleted_outright() {
    let mut keyring = seal::Keyring::default();
    keyring.ensure_current("cell-phone");
    // Past the point where nothing uncollected can still need it. Deleting is
    // the intended behaviour, not a leak of storage being tidied: while this
    // key exists, a bundle captured a year ago can still be opened.
    keyring.entries[0].delete_after = "2000-01-01T00:00:00Z".into();
    keyring.ensure_current("cell-phone");
    assert_eq!(keyring.entries.len(), 1);
    assert!(keyring.entries[0].key_id.ends_with(":2"));
}

#[test]
fn the_published_key_always_has_most_of_its_life_left() {
    let mut keyring = seal::Keyring::default();
    keyring.ensure_current("cell-phone");
    let published = keyring.current().expect("has one");

    // A sender seals against whatever its cached roster says. If a key were
    // published right up to its expiry, an ordinary stale roster would produce
    // refusals as a matter of course and the refusal would stop meaning
    // anything. Rotation at half-life is what keeps it exceptional.
    let floor = (chrono::Utc::now() + chrono::Duration::days(seal::ROTATE_AFTER_DAYS))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    assert!(published.not_after > floor);

    // The assertion above passes by construction — the lifetime is set to
    // twice the threshold in the same function — so on its own it would keep
    // passing with the rotation threshold wrong in either direction. This is
    // the property that actually matters: having just rotated, the keyring
    // must not immediately want to rotate again. Get that wrong and every
    // read mints a key and re-signs the roster.
    assert!(
        !keyring.ensure_current("cell-phone"),
        "a fresh key must not be due for replacement the moment it is made"
    );
    assert_eq!(keyring.entries.len(), 1);
}
