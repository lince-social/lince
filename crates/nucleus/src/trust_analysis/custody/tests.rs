use super::super::{Condition, Identity, IdentityContext};
use super::*;

#[test]
fn enabled_custody_requires_explicit_bounded_policies_and_exclusions_win() {
    let mut policy = Policy::default();
    assert!(!policy.enabled);
    policy.validate().unwrap();
    policy.enabled = true;
    assert!(policy.validate().is_err());
    let identity = Identity::Organ(crate::new_uid("r"));
    policy.senders.include.push(Condition {
        known: Some(true),
        ..Default::default()
    });
    policy.readers = policy.senders.clone();
    policy.validate().unwrap();
    let context = IdentityContext {
        known: true,
        readable: true,
        ..Default::default()
    };
    assert!(policy.senders.allows(&identity, &context));
    policy.senders.exclude.push(Condition {
        identities: vec![identity.clone()],
        ..Default::default()
    });
    assert!(!policy.senders.allows(&identity, &context));
    for (bytes, items, term) in [
        (0, 256, 86400),
        (MAX_STORAGE_BYTES + 1, 256, 86400),
        (1000, 0, 86400),
        (1000, MAX_ITEMS + 1, 86400),
        (1000, 1, 0),
        (1000, 1, 366 * 86400),
    ] {
        let mut invalid = policy.clone();
        invalid.storage_bytes = bytes;
        invalid.items = items;
        invalid.retention_seconds = term;
        assert!(invalid.validate().is_err());
    }
}

#[test]
fn custody_signatures_bind_target_exact_bytes_expiry_and_reader_conditions() {
    let mut offer = Offer {
        nonce: crate::new_uid("cust"),
        sender: crate::new_uid("r"),
        custodian: crate::new_uid("r"),
        artifact: canonical_hash("lince.test.custody.v1", &1).unwrap(),
        source_kind: SourceKind::Contact,
        bytes: 200,
        retained_until: 2000,
        deliver_by: 1100,
        readers: Filter {
            include: vec![Condition {
                known: Some(true),
                ..Default::default()
            }],
            exclude: vec![],
        },
        redistribute: true,
        key_id: "root".into(),
        public_key: "test".into(),
        signature: String::new(),
    };
    offer.validate(1000).unwrap();
    let bytes = offer.signing_bytes().unwrap();
    let hash = offer.hash().unwrap();
    offer.signature = "signed".into();
    assert_eq!(bytes, offer.signing_bytes().unwrap());
    assert_ne!(hash, offer.hash().unwrap());
    let ack = Acknowledgment {
        offer: offer.hash().unwrap(),
        artifact: offer.artifact.clone(),
        sender: offer.sender.clone(),
        custodian: offer.custodian.clone(),
        bytes: offer.bytes,
        retained_until: offer.retained_until,
        stored_at: 1000,
        key_id: "root".into(),
        public_key: "test".into(),
        signature: String::new(),
    };
    assert!(ack.matches(&offer));
    for variant in 0..5 {
        let mut altered = offer.clone();
        match variant {
            0 => altered.custodian = crate::new_uid("r"),
            1 => altered.bytes += 1,
            2 => altered.retained_until += 1,
            3 => altered.readers.exclude = altered.readers.include.clone(),
            _ => altered.redistribute = false,
        }
        assert!(!ack.matches(&altered));
        assert_ne!(bytes, altered.signing_bytes().unwrap());
    }
    offer.redistribute = false;
    assert!(offer.validate(1000).is_err());
}
