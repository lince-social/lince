mod generated;

use super::*;
use crate::{
    DecimalValue,
    karma::{CanonicalHash, canonical_hash},
    new_uid,
    transfer::{outcome::Role, receipt::Measurement},
    trust_analysis::{
        Condition, Identity, IdentityContext,
        contacts::Audience,
        paths,
        query::{Analysis, CounterpartyBasis, EvidenceLevel, EvidenceRow, Group, Query},
    },
    vocabulary::reference::MeaningReference,
};

fn amount(value: &str) -> DecimalValue {
    DecimalValue::parse_inferred(value).unwrap()
}
fn hash(value: &str) -> CanonicalHash {
    canonical_hash("lince.profile-claim.test.v1", &value).unwrap()
}

fn statement() -> Statement {
    let issuer = Identity::Person(new_uid("r"));
    Statement {
        category: Default::default(),
        title: "Food contribution history".into(),
        issuer: issuer.clone(),
        claim: new_uid("r"),
        revision: 1,
        descriptor: Descriptor {
            subject: issuer,
            role: Role::Giver,
            measurement: Measurement {
                meaning: MeaningReference {
                    vocabulary_uid: new_uid("g"),
                    concept_uid: new_uid("c"),
                    revision: 1,
                    definition_hash: "a".repeat(64),
                },
                unit: None,
                object_commitment: None,
            },
            level: EvidenceLevel::NeedLinkedReceiver,
            metric: Metric::Quantity,
            counterparty_basis: CounterpartyBasis::Organ,
            from: Some(900),
            until: None,
        },
        comparator: Comparator::AtLeast,
        amount: amount("100"),
        as_of: 1000,
        issued_at: 1001,
        expires_at: 2000,
        withdrawn: false,
        audience: Audience::default(),
        authorship: Authorship::Manual,
    }
}

fn analysis(statement: &Statement, quantity: &str) -> (Query, Analysis) {
    let mut policy = Query::for_subject(statement.descriptor.subject.clone());
    policy.rely_on_all_visible = true;
    let query = statement
        .descriptor
        .checking_query(&policy, statement.as_of)
        .unwrap();
    let group = Group {
        measurement: statement.descriptor.measurement.clone(),
        level: statement.descriptor.level,
        raw_quantity: Some(amount(quantity)),
        weighted_index: Some(amount(quantity)),
        occurrences: 1,
        issuers: Vec::new(),
        counterparties: Vec::new(),
        index_rounded: false,
        capped: false,
        arithmetic_error: false,
    };
    let result = Analysis {
        policy: query.id().unwrap(),
        groups: vec![group],
        rows: Vec::new(),
        protocol: Vec::new(),
        withheld: Vec::new(),
        partial: false,
        observed_only: true,
        input_events: 1,
    };
    (query, result)
}

#[test]
fn visible_ten_against_claimed_hundred_is_partial_support_not_disproof() {
    let mut statement = statement();
    let (query, mut analysis) = analysis(&statement, "10");
    let support = compare(&statement, &query, &analysis).unwrap();
    assert_eq!(support.state, SupportState::PartialSupport);
    assert!(support.observed_only);
    analysis.partial = true;
    assert!(compare(&statement, &query, &analysis).unwrap().partial);
    statement.amount = amount("10");
    assert_eq!(
        compare(&statement, &query, &analysis).unwrap().state,
        SupportState::AtLeastSupportedInVisibleScope
    );
    statement.comparator = Comparator::Exact;
    assert_eq!(
        compare(&statement, &query, &analysis).unwrap().state,
        SupportState::MatchesVisibleScope
    );
    statement.amount = amount("9");
    assert_eq!(
        compare(&statement, &query, &analysis).unwrap().state,
        SupportState::VisibleEvidenceExceedsClaim
    );
    analysis.groups.clear();
    statement.amount = amount("0");
    assert_eq!(
        compare(&statement, &query, &analysis).unwrap().state,
        SupportState::Unknown
    );
}

#[test]
fn checking_uses_the_readers_sources_and_pins_the_claims_exact_measurement_and_period() {
    let statement = statement();
    let mut sources = Query::for_subject(Identity::Organ(new_uid("r")));
    sources.issuers.exclude.push(Condition {
        identities: vec![Identity::Organ(new_uid("r"))],
        ..Default::default()
    });
    let checked = statement
        .descriptor
        .checking_query(&sources, statement.as_of)
        .unwrap();
    assert_eq!(checked.issuers, sources.issuers);
    assert_eq!(checked.subject, statement.descriptor.subject);
    assert_eq!(
        checked.measurements,
        vec![statement.descriptor.measurement.clone()]
    );
    assert_eq!(checked.until, Some(1001));
    let (mut query, analysis) = analysis(&statement, "10");
    query.measurements[0].meaning.revision += 1;
    assert!(compare(&statement, &query, &analysis).is_err());
}

#[test]
fn zero_reports_and_replicas_do_not_become_positive_contribution_occurrences() {
    let mut statement = statement();
    statement.descriptor.metric = Metric::ReportingOccurrences;
    let (query, mut analysis) = analysis(&statement, "0");
    let row = EvidenceRow {
        source: hash("source"),
        outcome: hash("outcome"),
        need: None,
        agreed_quantity: None,
        canonical_unit: None,
        issuer: statement.issuer.clone(),
        counterparty: Identity::Organ(new_uid("r")),
        measurement: statement.descriptor.measurement.clone(),
        level: statement.descriptor.level,
        recorded_at: 950,
        quantity: amount("0"),
        distance: None,
        path: None,
        records: Vec::new(),
        originals: Vec::new(),
        disputed: false,
        withdrawn: false,
        selected: true,
        source_qualified: true,
        weighted_index: Some(amount("0")),
        index_rounded: false,
        capped: false,
        arithmetic_error: false,
    };
    analysis.rows = vec![row.clone(), row];
    assert_eq!(
        compare(&statement, &query, &analysis).unwrap().observed,
        Some(amount("1"))
    );
    statement.descriptor.metric = Metric::PositiveOccurrences;
    assert_eq!(
        compare(&statement, &query, &analysis).unwrap().observed,
        Some(amount("0"))
    );
}

#[test]
fn signed_claim_format_binds_audience_versions_basis_and_purpose_and_rejects_instructions() {
    let mut document = Document {
        statement: statement(),
        key_id: "person-key".into(),
        public_key: format!("{}=", "A".repeat(43)),
        signature: format!("{}==", "A".repeat(86)),
    };
    let before = document.signing_bytes().unwrap();
    let series = document.statement.series().unwrap();
    document.statement.audience = Audience::Public;
    assert_ne!(before, document.signing_bytes().unwrap());
    assert_eq!(series, document.statement.series().unwrap());
    let encoded = serde_json::to_vec(&document).unwrap();
    assert_eq!(Document::parse(&encoded).unwrap(), document);
    let mut unknown = serde_json::to_value(&document).unwrap();
    unknown["execute"] = serde_json::json!({"kind":"delete_record"});
    assert!(Document::parse(&serde_json::to_vec(&unknown).unwrap()).is_err());
    document.statement.descriptor.metric = Metric::PositiveOccurrences;
    document.statement.amount = amount("0.5");
    assert!(document.validate().is_err());
    document.statement.amount = amount("1");
    document.statement.as_of = document.statement.issued_at + 1;
    assert!(document.validate().is_err());
}

#[test]
fn merged_claim_heads_withhold_hidden_newer_controls_and_preserve_forks_without_votes() {
    use crate::{
        transfer::receipt_history::Provenance,
        trust_analysis::evidence::{self, Claim, HeadState},
    };
    let first = Document {
        statement: statement(),
        key_id: "test-person".into(),
        public_key: format!("{}=", "A".repeat(43)),
        signature: format!("{}==", "A".repeat(86)),
    };
    let mut newer = first.clone();
    newer.statement.revision = 2;
    newer.statement.withdrawn = true;
    newer.statement.amount = amount("0");
    let original = Provenance::Artifact {
        hash: hash("original-artifact"),
    };
    let first_input = || Claim {
        document: &first,
        provenance: Some(original.clone()),
        authority_current: true,
    };
    let hidden = Claim {
        document: &newer,
        provenance: None,
        authority_current: true,
    };
    let heads = evidence::claim_heads(&[first_input(), hidden], 1200).unwrap();
    assert_eq!(heads[0].state, HeadState::Withheld);
    assert!(heads[0].revision.is_none());
    assert!(heads[0].originals.is_empty());
    let visible = || Claim {
        document: &newer,
        provenance: Some(Provenance::Artifact {
            hash: hash("newer-artifact"),
        }),
        authority_current: true,
    };
    let heads = evidence::claim_heads(&[first_input(), visible()], 1200).unwrap();
    assert_eq!(heads[0].state, HeadState::Withdrawn);
    assert_eq!(heads[0].revision, Some(2));
    let mut fork = newer.clone();
    fork.statement.withdrawn = false;
    let heads = evidence::claim_heads(
        &[
            first_input(),
            visible(),
            visible(),
            Claim {
                document: &fork,
                provenance: Some(Provenance::Artifact { hash: hash("fork") }),
                authority_current: true,
            },
        ],
        1200,
    )
    .unwrap();
    assert_eq!(heads[0].state, HeadState::Conflicted);
    let heads = evidence::claim_heads(&[first_input()], 2001).unwrap();
    assert_eq!(heads[0].state, HeadState::Expired);
    let heads = evidence::claim_heads(
        &[Claim {
            document: &first,
            provenance: Some(original),
            authority_current: false,
        }],
        1200,
    )
    .unwrap();
    assert_eq!(heads[0].state, HeadState::Withheld);
}

#[test]
fn recommendations_and_credentials_never_become_delivery_totals() {
    let mut statement = statement();
    let (query, evidence) = analysis(&statement, "100");
    for category in [Category::Credential, Category::Recommendation] {
        statement.category = category;
        if category == Category::Recommendation {
            assert!(statement.validate().is_err());
            statement.issuer = Identity::Person(new_uid("r"));
        }
        statement.validate().unwrap();
        assert!(compare(&statement, &query, &evidence).is_err());
        let before = statement.id().unwrap();
        statement.withdrawn = true;
        statement.amount = amount("0");
        statement.validate().unwrap();
        assert_ne!(before, statement.id().unwrap());
        assert!(compare(&statement, &query, &evidence).is_err());
        statement.withdrawn = false;
        statement.amount = amount("100");
    }
}

#[test]
fn typed_issuer_trust_is_activity_scoped_and_expiry_withdrawal_and_exclusions_win() {
    let mut statement = statement();
    statement.category = Category::Credential;
    let mut query = Query::for_subject(statement.descriptor.subject.clone());
    query.measurements = vec![statement.descriptor.measurement.clone()];
    query.issuers.include.push(Condition {
        identities: vec![statement.issuer.clone()],
        ..Default::default()
    });
    let context = IdentityContext {
        readable: true,
        ..Default::default()
    };
    assert!(trusted_issuer_for_activity(&statement, &query, &context, 1002).unwrap());
    assert!(!trusted_issuer_for_activity(&statement, &query, &context, 2000).unwrap());
    query.exclude_everywhere.push(statement.issuer.clone());
    assert!(!trusted_issuer_for_activity(&statement, &query, &context, 1002).unwrap());
    query.exclude_everywhere.clear();
    query.role = Role::Receiver;
    assert!(!trusted_issuer_for_activity(&statement, &query, &context, 1002).unwrap());
    query.role = Role::Giver;
    query.issuers.include.clear();
    assert!(!trusted_issuer_for_activity(&statement, &query, &context, 1002).unwrap());
    query.rely_on_all_visible = true;
    statement.withdrawn = true;
    statement.amount = amount("0");
    assert!(!trusted_issuer_for_activity(&statement, &query, &context, 1002).unwrap());
}
