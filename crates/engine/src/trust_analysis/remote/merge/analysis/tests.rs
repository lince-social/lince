use super::*;
use nucleus::trust_analysis::{claims::*, contacts::Audience};

fn fixture() -> (Review, claims::Document, crate::trust::Signer) {
    let person = nucleus::new_uid("r");
    let signer = crate::trust::Signer::generate(&person, "combined-claim-test");
    let identity = Identity::Person(person.clone());
    let mut document = claims::Document {
        statement: Statement {
            title: "Contribution history".into(),
            issuer: identity.clone(),
            claim: nucleus::new_uid("r"),
            revision: 1,
            descriptor: Descriptor {
                subject: identity.clone(),
                role: nucleus::transfer::outcome::Role::Giver,
                measurement: nucleus::transfer::receipt::Measurement {
                    meaning: nucleus::vocabulary::reference::MeaningReference {
                        vocabulary_uid: nucleus::new_uid("g"),
                        concept_uid: nucleus::new_uid("c"),
                        revision: 1,
                        definition_hash: "a".repeat(64),
                    },
                    unit: None,
                    object_commitment: None,
                },
                level: query::EvidenceLevel::NeedLinkedReceiver,
                metric: Metric::Quantity,
                counterparty_basis: query::CounterpartyBasis::Organ,
                from: None,
                until: None,
            },
            comparator: Comparator::AtLeast,
            amount: nucleus::DecimalValue::parse_inferred("100").unwrap(),
            as_of: 1000,
            issued_at: 1000,
            expires_at: 2000,
            withdrawn: false,
            audience: Audience::Public,
            authorship: Authorship::Manual,
        },
        key_id: signer.key_id.clone(),
        public_key: signer.public_key_b64(),
        signature: String::new(),
    };
    crate::trust_analysis::sign_claim_document(&mut document, &signer).unwrap();
    let query = document.statement.descriptor.checking_query(&query::Query::for_subject(identity.clone()), 1000).unwrap();
    let draft = model::Draft {
        query,
        as_organ: false,
        linked_claims: vec![model::Series { issuer: identity, claim: document.statement.claim.clone(), snapshot: Some(document.clone()) }],
        include_contacts: false,
        receipt_sources: Vec::new(),
        contact_subjects: Vec::new(),
    };
    let review = Review { nonce: nucleus::new_uid("ask"), person, questions: draft.questions().unwrap(), draft, endpoints: Vec::new(), profiles: Vec::new(), reviewed_at: 1000 };
    (review, document, signer)
}

fn evaluate(review: &Review, document: claims::Document, signature: Verification, visible: bool) -> Value {
    let artifact = Arc::new(Artifact::Claim(document));
    let provenance = visible.then(|| Provenance::Artifact { hash: artifact.hash().unwrap() });
    let originals = Originals {
        items: vec![Original { artifact, provenance, signature }],
        local_complete: true,
        ..Default::default()
    };
    let inputs = originals.items.iter().filter_map(|original| {
        if !matches!(original.signature, Verification::Accepted | Verification::WithheldAuthority) { return None; }
        let Artifact::Claim(document) = original.artifact.as_ref() else { return None };
        Some(evidence::Claim { document, provenance: original.provenance.clone(), authority_current: original.signature == Verification::Accepted })
    }).collect::<Vec<_>>();
    let heads = evidence::claim_heads(&inputs, 1100).unwrap();
    let paths = paths::ResultSet { paths: Vec::new(), observed_only: true, identity_limit_reached: false, depth_limit_reached: false };
    claim_checks(review, &originals, &[], &BTreeMap::new(), &paths, &heads, false, 1100).unwrap().remove(0)
}

#[test]
fn combined_claim_checks_require_review_for_changed_meaning_period_and_audience() {
    let (review, document, signer) = fixture();
    assert_eq!(evaluate(&review, document.clone(), Verification::Accepted, true)["state"], "checked");
    let mut same_scope = document.clone();
    same_scope.statement.revision = 2;
    same_scope.statement.amount = nucleus::DecimalValue::parse_inferred("200").unwrap();
    same_scope.signature.clear();
    crate::trust_analysis::sign_claim_document(&mut same_scope, &signer).unwrap();
    let current = evaluate(&review, same_scope, Verification::Accepted, true);
    assert_eq!(current["state"], "checked");
    assert_eq!(current["document"]["statement"]["revision"], 2);
    assert_eq!(review.draft.linked_claims[0].snapshot.as_ref(), Some(&document));
    for variant in 0..4 {
        let mut revised = document.clone();
        revised.statement.revision = 2;
        match variant {
            0 => revised.statement.descriptor.measurement.meaning.revision += 1,
            1 => revised.statement.descriptor.measurement.unit = Some(revised.statement.descriptor.measurement.meaning.clone()),
            2 => revised.statement.descriptor.from = Some(900),
            _ => revised.statement.audience = Audience::Selected { viewers: vec![Identity::Person(review.person.clone())] },
        }
        revised.signature.clear();
        crate::trust_analysis::sign_claim_document(&mut revised, &signer).unwrap();
        let result = evaluate(&review, revised.clone(), Verification::Accepted, true);
        assert_eq!(result["state"], "review_required");
        assert!(result.get("support").is_none());
        assert_eq!(review.draft.linked_claims[0].snapshot.as_ref(), Some(&document));
        let mut approved = review.clone();
        approved.draft.linked_claims[0].snapshot = Some(revised.clone());
        assert_eq!(evaluate(&approved, revised, Verification::Accepted, true)["state"], "checked");
    }
}

#[test]
fn combined_claim_checks_never_compare_withdrawn_invalid_or_hidden_current_heads() {
    let (review, mut document, signer) = fixture();
    for signature in [Verification::Invalid, Verification::UnknownIdentity, Verification::WithheldAuthority] {
        let result = evaluate(&review, document.clone(), signature, true);
        assert_ne!(result["state"], "checked");
        assert!(result.get("support").is_none());
    }
    let hidden = evaluate(&review, document.clone(), Verification::Accepted, false);
    assert_eq!(hidden["state"], "unknown");
    assert!(hidden.get("head").is_none());
    assert!(hidden.get("document").is_none());
    document.statement.withdrawn = true;
    document.statement.amount = nucleus::DecimalValue::parse_inferred("0").unwrap();
    document.signature.clear();
    crate::trust_analysis::sign_claim_document(&mut document, &signer).unwrap();
    let withdrawn = evaluate(&review, document, Verification::Accepted, true);
    assert_eq!(withdrawn["state"], "withdrawn");
    assert!(withdrawn.get("support").is_none());
}
