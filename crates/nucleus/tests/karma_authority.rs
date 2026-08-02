use std::collections::BTreeSet;

use nucleus::karma::{
    CanonicalHash, Capability, CapabilitySet, DelegationGrantRevision, DelegationGrantSchema,
    DelegationGrantSpec, GrantAuthorityDenial, GrantAuthorityRequest, GrantBudget,
    GrantProgramRevisionScope, GrantRevisionChange, GrantTarget, GrantTargetScope,
    GrantTemplateScope, ReferenceKind, Slug, TimestampMs, TypedUid, canonical_hash,
};

const PERSON_UID: &str = "p_01ARZ3NDEKTSV4RRFFQ69G5FAV";
const OTHER_PERSON_UID: &str = "p_01ARZ3NDEKTSV4RRFFQ69G5FAW";
const PROGRAM_UID: &str = "r_01ARZ3NDEKTSV4RRFFQ69G5FAV";
const RECORD_UID: &str = "r_01ARZ3NDEKTSV4RRFFQ69G5FAW";

#[test]
fn exact_grant_authorizes_only_the_complete_typed_request() {
    let grant = grant();
    let request = request();
    assert!(grant.evaluate(&request).allowed);

    let mut wrong = request.clone();
    wrong.principal_person_uid = uid(ReferenceKind::Person, OTHER_PERSON_UID);
    wrong.capability = Capability::RecordSetQuantity;
    wrong.target = None;
    let decision = grant.evaluate(&wrong);
    assert!(!decision.allowed);
    assert_eq!(
        decision.denials,
        vec![
            GrantAuthorityDenial::PrincipalMismatch,
            GrantAuthorityDenial::CapabilityMissing,
        ]
    );

    let mut expired = request;
    expired.logical_at = at(2_000);
    assert_eq!(
        grant.evaluate(&expired).denials,
        vec![GrantAuthorityDenial::Expired]
    );
}

#[test]
fn replacement_comparison_rejects_every_widening_and_mixed_change() {
    let original = grant();
    let mut narrowed = original.clone();
    narrowed.spec.program_revision = GrantProgramRevisionScope::Exact {
        revision_hash: hash('a'),
    };
    narrowed.spec.capabilities = CapabilitySet::new([Capability::RecordAddQuantity]);
    narrowed.spec.candidate_templates = GrantTemplateScope::Only {
        templates: BTreeSet::from([slug("record.add-quantity")]),
    };
    narrowed.spec.targets = GrantTargetScope::Only {
        targets: BTreeSet::from([GrantTarget::Record(uid(ReferenceKind::Record, RECORD_UID))]),
    };
    narrowed.spec.valid_from = at(100);
    narrowed.spec.expires_at = at(1_500);
    assert_eq!(
        original.compare_replacement(&narrowed).unwrap(),
        GrantRevisionChange::Narrowing
    );
    assert_eq!(
        narrowed.compare_replacement(&original).unwrap(),
        GrantRevisionChange::Widening
    );

    let mut mixed = narrowed.clone();
    mixed.spec.expires_at = at(2_500);
    assert_eq!(
        original.compare_replacement(&mixed).unwrap(),
        GrantRevisionChange::Mixed
    );
}

#[test]
fn malformed_or_self_delegating_grants_fail_closed() {
    let mut value = grant();
    value.spec.capabilities = CapabilitySet::default();
    assert!(value.validate().is_err());

    value = grant();
    value.spec.capabilities = CapabilitySet::new([Capability::KarmaGrantWiden]);
    assert!(value.validate().is_err());

    value = grant();
    value.spec.targets = GrantTargetScope::Only {
        targets: BTreeSet::new(),
    };
    assert!(value.validate().is_err());
}

#[test]
fn authority_wire_has_a_golden_hash() {
    let fixture = (grant(), request());
    let digest = canonical_hash("karma.authority-golden.v1", &fixture).unwrap();
    assert_eq!(
        digest.as_str(),
        // Updated 2026-07-26: the fixture's template slug moved from the
        // domain-specific `economy.add` to the generic `record.add-quantity`.
        // The wire format is unchanged; only the test data is.
        "sha256:b48813e1435afc82fde9f6c59a967cfd7462721901ccf123989f0cc9b2930461",
        "an authority wire change requires a deliberate golden update"
    );
}

fn grant() -> DelegationGrantRevision {
    DelegationGrantRevision::new(
        uid(ReferenceKind::Person, PERSON_UID),
        DelegationGrantSpec {
            schema: DelegationGrantSchema::V1,
            purpose: "Bounded quantity update".to_string(),
            program_uid: uid(ReferenceKind::Program, PROGRAM_UID),
            program_revision: GrantProgramRevisionScope::AnyActive,
            candidate_templates: GrantTemplateScope::Any,
            capabilities: CapabilitySet::new([
                Capability::RecordAddQuantity,
                Capability::TaskCreate,
            ]),
            targets: GrantTargetScope::Any,
            budget: GrantBudget::default(),
            valid_from: at(0),
            expires_at: at(2_000),
        },
    )
    .unwrap()
}

fn request() -> GrantAuthorityRequest {
    GrantAuthorityRequest {
        principal_person_uid: uid(ReferenceKind::Person, PERSON_UID),
        program_uid: uid(ReferenceKind::Program, PROGRAM_UID),
        program_revision_hash: hash('a'),
        candidate_template: slug("record.add-quantity"),
        capability: Capability::RecordAddQuantity,
        target: Some(GrantTarget::Record(uid(ReferenceKind::Record, RECORD_UID))),
        logical_at: at(1_000),
    }
}

fn uid(kind: ReferenceKind, value: &str) -> TypedUid {
    TypedUid::new(kind, value).unwrap()
}

fn slug(value: &str) -> Slug {
    Slug::new(value).unwrap()
}

fn hash(value: char) -> CanonicalHash {
    CanonicalHash::parse(format!("sha256:{}", value.to_string().repeat(64))).unwrap()
}

fn at(milliseconds: i64) -> TimestampMs {
    TimestampMs::from_millis(milliseconds).unwrap()
}
