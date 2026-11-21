use super::*;
use crate::{
    DecimalValue,
    karma::CanonicalHash,
    new_uid,
    transfer::{outcome::Role, receipt_fragment, receipt_history},
    trust_analysis::contacts::Audience,
    vocabulary::reference::MeaningReference,
};

fn hash(label: &str) -> CanonicalHash {
    digest("lince.transfer.receipt.test.v1", &label).unwrap()
}

fn measurement() -> Measurement {
    Measurement {
        meaning: MeaningReference {
            vocabulary_uid: new_uid("g"),
            concept_uid: new_uid("c"),
            revision: 1,
            definition_hash: "a".repeat(64),
        },
        unit: None,
        object_commitment: None,
    }
}

fn publication() -> Publication {
    let scope = Scope {
        audience: Audience::Public,
        upfront_terms: None,
        public_persons: Vec::new(),
        party_attribution: PartyAttribution::LaterOrganEndorsement,
        original_authorities: Vec::new(),
        source: Source {
            origin: new_uid("r"),
            occurrence: hash("occurrence"),
            activated_at: 900,
            agreement_revision: 1,
            agreement_commitment: hash("agreement"),
            activation_commitment: hash("activation"),
        },
        giver: new_uid("r"),
        receiver: new_uid("r"),
        measurements: vec![measurement()],
        stages: vec![
            Stage::Need,
            Stage::Giver,
            Stage::Receiver,
            Stage::Settled,
            Stage::Dispute,
        ],
        agreed_quantity: Some(RecordedFloat::new(10.0).unwrap()),
        canonical_unit: None,
        issued_at: 1000,
        expires_at: 2000,
    };
    Publication {
        person_approvals: Vec::new(),
        scope,
        approvals: [
            ApprovalRole::Origin,
            ApprovalRole::Giver,
            ApprovalRole::Receiver,
        ]
        .into_iter()
        .map(|role| Approval {
            role,
            issued_at: 1000,
            key_id: "test".into(),
            signature: "a".repeat(88),
        })
        .collect(),
        events: Vec::new(),
    }
}

fn statement(publication: &Publication, amount: &str, time: i64) -> SignedEvent {
    SignedEvent {
        event: Event {
            scope: publication.scope.id().unwrap(),
            source_commitment: hash(&format!("statement:{amount}:{time}")),
            recorded_at: 950,
            issued_at: time,
            supersedes: None,
            content: Content::Statement {
                role: Role::Receiver,
                measurement: publication.scope.measurements[0].clone(),
                quantity: DecimalValue::parse_inferred(amount).unwrap(),
                need: None,
                withdrawn: false,
            },
        },
        key_id: "test".into(),
        signature: "a".repeat(88),
    }
}

#[test]
fn lifecycle_requires_fresh_stages_and_keeps_compensated_slices_in_distinct_slots() {
    let mut publication = publication();
    let mut event = statement(&publication, "4", 1500);
    event.event.recorded_at = 1200;
    event.event.content = Content::SettlementStatus {
        quantity: RecordedFloat::new(4.0).unwrap(),
        remaining_after: RecordedFloat::new(6.0).unwrap(),
    };
    assert!(event.event.validate(&publication.scope, 1600).is_err());
    let earlier_scope = publication.scope.id().unwrap();
    publication.scope.stages.extend([
        Stage::SettlementStatus,
        Stage::Cancelled,
        Stage::Compensated,
    ]);
    let scope = &publication.scope;
    assert_ne!(earlier_scope, scope.id().unwrap());
    event.event.scope = scope.id().unwrap();
    event.event.validate(scope, 1600).unwrap();
    if let Content::SettlementStatus {
        remaining_after, ..
    } = &mut event.event.content
    {
        remaining_after.ieee754_binary64 = "7ff0000000000000".into();
    }
    assert!(event.event.validate(scope, 1600).is_err());
    event.event.content = Content::Cancelled {
        quantity: DecimalValue::parse_inferred("6").unwrap(),
        revision: 2,
    };
    event.event.validate(scope, 1600).unwrap();
    if let Content::Cancelled { revision, .. } = &mut event.event.content {
        *revision = 1;
    }
    assert!(event.event.validate(scope, 1600).is_err());
    event.event.content = Content::Compensated {
        role: Role::Giver,
        settlement: hash("first-slice"),
        application: hash("application"),
    };
    let first = event.event.slot(scope).unwrap();
    if let Content::Compensated { settlement, .. } = &mut event.event.content {
        *settlement = hash("second-slice");
    }
    assert_ne!(first, event.event.slot(scope).unwrap());
    event.event.recorded_at = 899;
    assert!(event.event.validate(scope, 1600).is_err());
}

#[test]
fn naming_a_person_requires_separate_exact_scope_consent_before_publication() {
    let mut publication = publication();
    let original = publication.scope.id().unwrap();
    let person = new_uid("r");
    publication.scope.public_persons.push(PublicPerson {
        role: ApprovalRole::Receiver,
        person: person.clone(),
    });
    assert_ne!(original, publication.scope.id().unwrap());
    let review = Review {
        scope: publication.scope.clone(),
        approvals: publication.approvals.clone(),
        person_approvals: Vec::new(),
        authorities: Vec::new(),
    };
    review.validate(1600).unwrap();
    assert!(!review.is_complete());
    assert!(publication.validate(1600).is_err());
    let approval = PersonApproval {
        scope: publication.scope.id().unwrap(),
        role: ApprovalRole::Receiver,
        person,
        issued_at: 1500,
        key_id: "person-key".into(),
        signature: "a".repeat(88),
    };
    publication.person_approvals.push(approval.clone());
    publication.validate(1600).unwrap();
    assert!(receipt_history::project(&publication).is_ok());
    assert!(receipt_fragment::references(&publication).is_ok());
    for change in 0..4 {
        let mut changed = approval.clone();
        match change {
            0 => changed.scope = original.clone(),
            1 => changed.role = ApprovalRole::Giver,
            2 => changed.person = new_uid("r"),
            _ => changed.issued_at = publication.scope.expires_at,
        }
        assert!(changed.validate(&publication.scope, 1600).is_err());
    }
    let mut early = statement(&publication, "12", 1010);
    publication.events.push(early.clone());
    assert!(publication.validate(1600).is_err());
    early.event.issued_at = 1510;
    publication.events = vec![early];
    publication.validate(1600).unwrap();
}

#[test]
fn personal_consents_cannot_repeat_roles_or_replace_an_existing_signature() {
    let mut scope = publication().scope;
    scope.public_persons = vec![PublicPerson {
        role: ApprovalRole::Receiver,
        person: new_uid("r"),
    }];
    let approval = PersonApproval {
        scope: scope.id().unwrap(),
        role: ApprovalRole::Receiver,
        person: scope.public_persons[0].person.clone(),
        issued_at: 1010,
        key_id: "personal".into(),
        signature: "a".repeat(88),
    };
    let review = Review {
        scope: scope.clone(),
        approvals: Vec::new(),
        person_approvals: vec![approval.clone()],
        authorities: Vec::new(),
    };
    let original = Document::Review(review.clone());
    let mut duplicate = review.clone();
    duplicate.person_approvals.push(approval.clone());
    assert!(duplicate.validate(1100).is_err());
    let mut changed = review;
    changed.person_approvals[0].issued_at += 1;
    assert!(original.merge(&Document::Review(changed), 1100).is_err());
    scope.public_persons.push(scope.public_persons[0].clone());
    assert!(scope.validate().is_err());
    scope.public_persons.truncate(1);
    scope.public_persons[0].role = ApprovalRole::Origin;
    assert!(scope.validate().is_err());
}

#[test]
fn reported_source_times_do_not_assume_the_origins_clock() {
    let mut publication = publication();
    let mut reported = statement(&publication, "12", 1010);
    reported.event.recorded_at = publication.scope.source.activated_at - 30;
    publication.events.push(reported);
    publication.validate(1100).unwrap();
    publication.events[0].event.recorded_at = 1200;
    publication.validate(1100).unwrap();
    publication.events[0].event.recorded_at = 0;
    assert!(publication.validate(1100).is_err());
}

#[test]
fn private_participation_requests_open_the_exact_source_and_party() {
    let source = new_uid("r");
    let transfer = new_uid("r");
    let person = new_uid("r");
    let terms = Participation {
        public_history: None,
        organ: source.clone(),
        origin: source.clone(),
        agreement_revision: 1,
        agreement_commitment: hash("agreement"),
        party_commitment: Some(
            digest(
                "lince.transfer.participation.party.v1",
                &(&source, &transfer, 1u64, &person),
            )
            .unwrap(),
        ),
    };
    let request = ParticipationRequest {
        public_history_terms: None,
        transfer,
        party: Some(person),
        terms,
    };
    request.validate().unwrap();
    let mut changed = request.clone();
    changed.party = Some(new_uid("r"));
    assert!(changed.validate().is_err());
    let mut changed = request.clone();
    changed.transfer = new_uid("r");
    assert!(changed.validate().is_err());
    let mut changed = request;
    changed.terms.agreement_revision += 1;
    assert!(changed.validate().is_err());
}

#[test]
fn original_authority_requires_complete_roles_and_cannot_authorize_publication_signatures() {
    let mut scope = publication().scope;
    scope.party_attribution = PartyAttribution::OriginalOrganAuthority;
    assert!(scope.validate().is_err());
    for role in [
        ApprovalRole::Origin,
        ApprovalRole::Giver,
        ApprovalRole::Receiver,
    ] {
        let organ = scope.organ(role).to_owned();
        let authority = Authority {
            organ: organ.clone(),
            roster_version: 1,
            root_key: "A".repeat(44),
            signing_key: "B".repeat(44),
            key_id: format!("ed25519:cell:{}:v1", new_uid("r")),
            issued_at: 800,
            expires_at: 1800,
            successions: Vec::new(),
            signature: "A".repeat(88),
            participation: Some(Participation {
                public_history: None,
                organ,
                origin: scope.source.origin.clone(),
                agreement_revision: scope.source.agreement_revision,
                agreement_commitment: scope.source.agreement_commitment.clone(),
                party_commitment: if role == ApprovalRole::Origin {
                    None
                } else {
                    Some(hash(&format!("{role:?}")))
                },
            }),
        };
        let mut generic = authority.clone();
        generic.participation = None;
        assert_ne!(
            authority.signing_bytes().unwrap(),
            generic.signing_bytes().unwrap()
        );
        scope
            .original_authorities
            .push(OriginalAuthority { role, authority });
    }
    scope.validate().unwrap();
    let mut late = scope.clone();
    late.original_authorities[0].authority.issued_at = scope.source.activated_at + 1;
    assert!(late.validate().is_err());
    let mut incomplete = scope.clone();
    incomplete.original_authorities.pop();
    assert!(incomplete.validate().is_err());
    let mut wrong = scope.clone();
    wrong.original_authorities[1]
        .authority
        .participation
        .as_mut()
        .unwrap()
        .agreement_revision += 1;
    assert!(wrong.validate().is_err());
    let mut later = scope.clone();
    later.party_attribution = PartyAttribution::LaterOrganEndorsement;
    assert!(later.validate().is_err());
    let approval = scope.original_authorities[0].authority.clone();
    let review = Review {
        person_approvals: Vec::new(),
        scope,
        approvals: vec![],
        authorities: vec![approval],
    };
    assert!(review.validate(1300).is_err());
}

#[test]
fn recorded_float_preserves_the_actual_bits_without_claiming_decimal_precision() {
    for value in [0.0, 0.1, 10.0, f64::MAX, f64::MIN_POSITIVE] {
        let quantity = RecordedFloat::new(value).unwrap();
        assert_eq!(quantity.value().unwrap().to_bits(), value.to_bits());
        assert!(
            !serde_json::to_string(&quantity)
                .unwrap()
                .contains("decimal")
        );
    }
    for value in [f64::NAN, f64::INFINITY, -0.0, -1.0] {
        assert!(RecordedFloat::new(value).is_err());
    }
    for bits in [
        "0",
        "3FB999999999999A",
        "7ff0000000000000",
        "8000000000000000",
    ] {
        assert!(
            RecordedFloat {
                ieee754_binary64: bits.into()
            }
            .value()
            .is_err()
        );
    }
}

#[test]
fn corrections_preserve_branches_but_reject_duplicate_roots_and_cross_role_rewrites() {
    let mut receipt = publication();
    let original = statement(&receipt, "12", 1100);
    let parent = original.event.id().unwrap();
    let mut correction = statement(&receipt, "11", 1200);
    correction.event.supersedes = Some(parent.clone());
    let mut competing = statement(&receipt, "13", 1200);
    competing.event.supersedes = Some(parent);
    receipt.events = vec![competing.clone(), original.clone(), correction];
    receipt.validate(1300).unwrap();
    receipt.events.push(statement(&receipt, "14", 1250));
    assert!(receipt.validate(1300).is_err());
    receipt.events.pop();
    if let Content::Statement { role, .. } = &mut receipt.events[0].event.content {
        *role = Role::Giver;
    }
    assert!(receipt.validate(1300).is_err());
    receipt.events = vec![competing];
    assert!(receipt.validate(1300).is_err());
}

#[test]
fn need_proof_is_exact_and_received_quantity_may_exceed_requested() {
    let mut receipt = publication();
    let mut need = statement(&receipt, "10", 1050);
    need.event.content = Content::Need {
        measurement: receipt.scope.measurements[0].clone(),
        requested: DecimalValue::parse_inferred("10").unwrap(),
        need_commitment: hash("private-need"),
    };
    let mut received = statement(&receipt, "12.000000000000000001", 1100);
    if let Content::Statement { need: binding, .. } = &mut received.event.content {
        *binding = Some(need.event.id().unwrap());
    }
    receipt.events = vec![received, need];
    receipt.validate(1300).unwrap();
    receipt.events.pop();
    assert!(receipt.validate(1300).is_err());
}

#[test]
fn projection_cannot_expand_scope_or_reinterpret_withdrawal() {
    let mut receipt = publication();
    receipt.scope.stages = vec![Stage::Giver];
    receipt.events = vec![statement(&receipt, "12", 1100)];
    assert!(receipt.validate(1300).is_err());
    receipt.scope.stages.push(Stage::Receiver);
    receipt.events[0].event.scope = receipt.scope.id().unwrap();
    receipt.validate(1300).unwrap();
    if let Content::Statement { withdrawn, .. } = &mut receipt.events[0].event.content {
        *withdrawn = true;
    }
    assert!(receipt.validate(1300).is_err());
    if let Content::Statement { quantity, .. } = &mut receipt.events[0].event.content {
        *quantity = DecimalValue::parse_inferred("0").unwrap();
    }
    receipt.validate(1300).unwrap();
}

#[test]
fn parsing_rejects_private_extra_fields_oversize_and_unapproved_time() {
    let mut receipt = publication();
    let mut value = serde_json::to_value(&receipt).unwrap();
    value["scope"]["private_need"] = serde_json::json!(new_uid("r"));
    assert!(Publication::parse(&serde_json::to_vec(&value).unwrap(), 1300).is_err());
    assert!(Publication::parse(&vec![b' '; MAX_BYTES + 1], 1300).is_err());
    receipt.approvals[2].issued_at = 1200;
    receipt.events = vec![statement(&receipt, "12", 1100)];
    assert!(receipt.validate(1300).is_err());
    receipt.events[0].event.issued_at = 1250;
    receipt.validate(1300).unwrap();
    receipt.events[0].event.issued_at = 2000;
    assert!(receipt.validate(2000).is_err());
}

#[test]
fn replica_and_scope_changes_keep_the_same_source_slot() {
    let receipt = publication();
    let original = statement(&receipt, "12", 1100);
    let mut changed = receipt.clone();
    changed.scope.expires_at += 1000;
    let changed_event = statement(&changed, "12", 1100);
    assert_eq!(
        original.event.slot(&receipt.scope).unwrap(),
        changed_event.event.slot(&changed.scope).unwrap()
    );
    assert_ne!(
        original.event.id().unwrap(),
        changed_event.event.id().unwrap()
    );
    let origin = new_uid("r");
    let occurrence = new_uid("toc");
    let source = Source::occurrence_id(&origin, &occurrence).unwrap();
    assert_eq!(source, Source::occurrence_id(&origin, &occurrence).unwrap());
    assert_ne!(
        source,
        Source::occurrence_id(&new_uid("r"), &occurrence).unwrap()
    );
}

#[test]
fn review_approvals_merge_without_authorizing_an_incomplete_publication() {
    let publication = publication();
    let review = Review {
        person_approvals: Vec::new(),
        scope: publication.scope.clone(),
        approvals: Vec::new(),
        authorities: Vec::new(),
    };
    review.validate(1300).unwrap();
    assert!(review.as_bundle().validate(1300).is_err());
    let first = Document::Review(review.clone());
    let mut approved = review;
    approved.approvals.push(publication.approvals[0].clone());
    let second = Document::Review(approved.clone());
    let merged = first.merge(&second, 1300).unwrap();
    assert_eq!(merged, second);
    assert_eq!(merged.identity().unwrap(), first.identity().unwrap());
    assert_ne!(merged.hash().unwrap(), first.hash().unwrap());
    assert!(approved.as_bundle().validate(1300).is_err());
    approved.approvals.push(approved.approvals[0].clone());
    assert!(approved.validate(1300).is_err());
    let mut changed = publication.scope;
    changed.expires_at += 1;
    assert!(
        first
            .merge(
                &Document::Review(Review {
                    person_approvals: Vec::new(),
                    scope: changed,
                    approvals: Vec::new(),
                    authorities: Vec::new()
                }),
                1300
            )
            .is_err()
    );
    let mut value = serde_json::to_value(&first).unwrap();
    value["value"]["private_address"] = serde_json::json!("secret");
    assert!(Document::parse(&serde_json::to_vec(&value).unwrap(), 1300).is_err());
    assert!(Document::parse(&vec![b' '; MAX_BUNDLE_BYTES + 65], 1300).is_err());
}

#[test]
fn reimporting_an_identical_document_keeps_its_original_approval_order_and_hash() {
    let mut publication = publication();
    publication.approvals.reverse();
    let review = Document::Review(Review {
        person_approvals: Vec::new(),
        scope: publication.scope.clone(),
        approvals: publication.approvals.clone(),
        authorities: Vec::new(),
    });
    let bundle = Document::Publication(Bundle {
        publication,
        authorities: Vec::new(),
    });
    for document in [review, bundle] {
        let original = serde_json::to_vec(&document).unwrap();
        let imported = Document::parse(&original, 1300).unwrap();
        let merged = imported.merge(&imported, 1300).unwrap();
        assert_eq!(merged.hash().unwrap(), imported.hash().unwrap());
        assert_eq!(serde_json::to_vec(&merged).unwrap(), original);
    }
}
