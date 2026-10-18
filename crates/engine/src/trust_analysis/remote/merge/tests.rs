use super::*;
use nucleus::trust_analysis::contacts::{Audience, Document, Statement};

fn packet() -> Arc<Artifact> {
    let issuer = Identity::Person(nucleus::new_uid("r"));
    let signer = crate::trust::Signer::generate(issuer.uid(), "merged-budget");
    let mut document = Document {
        statement: Statement {
            issuer,
            contact: Identity::Organ(nucleus::new_uid("r")),
            revision: 1,
            issued_at: 1000,
            expires_at: 2000,
            withdrawn: false,
            audience: Audience::Public,
        },
        key_id: signer.key_id.clone(),
        public_key: signer.public_key_b64(),
        signature: String::new(),
    };
    crate::trust_analysis::sign_contact_document(&mut document, &signer).unwrap();
    Arc::new(Artifact::Contact(document))
}

#[test]
fn merged_originals_accept_exact_budgets_and_refuse_overflow_without_partial_insertion() {
    let artifact = packet();
    let size = artifact.bytes().unwrap().len();
    for event_bound in [false, true] {
        let mut originals = Originals {
            bytes: if event_bound { 0 } else { MAX_BYTES - size },
            events: if event_bound { MAX_DISCLOSURES - 1 } else { 0 },
            ..Default::default()
        };
        assert!(originals.push(artifact.clone(), None).unwrap());
        let before = (
            originals.bytes,
            originals.events,
            originals.identities.clone(),
        );
        assert!(!originals.push(artifact.clone(), None).unwrap());
        assert!(originals.partial);
        assert_eq!(originals.items.len(), 1);
        assert_eq!(
            (originals.bytes, originals.events, originals.identities),
            before
        );
    }
    let mut originals = Originals::default();
    originals
        .identities
        .extend((0..MAX_IDENTITIES - 2).map(|_| Identity::Person(nucleus::new_uid("r"))));
    assert!(originals.push(artifact, None).unwrap());
    assert_eq!(originals.identities.len(), MAX_IDENTITIES);
    let before = (
        originals.bytes,
        originals.events,
        originals.identities.clone(),
    );
    assert!(!originals.push(packet(), None).unwrap());
    assert!(originals.partial);
    assert_eq!(originals.items.len(), 1);
    assert_eq!(
        (originals.bytes, originals.events, originals.identities),
        before
    );
}
