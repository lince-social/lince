use super::*;
use crate::{
    trust_analysis::query::{EvidenceRow, Group},
    vocabulary::reference::MeaningReference,
};

fn amount(value: &str) -> DecimalValue {
    DecimalValue::parse_inferred(value).unwrap()
}
fn hash(value: &str) -> CanonicalHash {
    canonical_hash("lince.profile-claim.test.v1", &value).unwrap()
}

fn statement() -> Statement {
    let issuer = Identity::Person(crate::new_uid("r"));
    Statement {
        title: "Food contribution history".into(),
        issuer: issuer.clone(),
        claim: crate::new_uid("r"),
        revision: 1,
        descriptor: Descriptor {
            subject: issuer,
            role: Role::Giver,
            measurement: Measurement {
                meaning: MeaningReference {
                    vocabulary_uid: crate::new_uid("g"),
                    concept_uid: crate::new_uid("c"),
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
    let mut sources = Query::for_subject(Identity::Organ(crate::new_uid("r")));
    sources.issuers.exclude.push(super::super::Condition {
        identities: vec![Identity::Organ(crate::new_uid("r"))],
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
        counterparty: Identity::Organ(crate::new_uid("r")),
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

fn recipe() -> recipes::Recipe {
    let statement = statement();
    let mut inputs = Query::for_subject(statement.descriptor.subject.clone());
    inputs.measurements = vec![statement.descriptor.measurement.clone()];
    inputs.from = statement.descriptor.from;
    inputs.rely_on_all_visible = true;
    let definition = recipes::Definition {
        starting_claim_revision: 0,
        recipe: crate::new_uid("r"),
        revision: 1,
        controller: statement.issuer.uid().into(),
        terms: recipes::Terms {
            issuer: statement.issuer,
            claim: statement.claim,
            title: statement.title,
            descriptor: statement.descriptor,
            comparator: statement.comparator,
            audience: Audience::Public,
            issued_at: 1001,
            expires_at: 2000,
            lifetime_seconds: 600,
            delegate: recipes::Delegate {
                organ: crate::new_uid("r"),
                cell: crate::new_uid("r"),
                key_id: "publishing-cell".into(),
                public_key: format!("{}=", "A".repeat(43)),
            },
        },
        inputs,
        start_enabled: false,
        refresh_seconds: 60,
    };
    recipes::Recipe {
        permit: recipes::Permit {
            recipe: definition.id().unwrap(),
            revision: 1,
            terms: definition.terms.clone(),
            key_id: "original-person".into(),
            public_key: format!("{}=", "A".repeat(43)),
            signature: format!("{}==", "A".repeat(86)),
        },
        definition,
    }
}

#[test]
fn generated_permits_bind_scope_without_exporting_private_recipe_policy() {
    let recipe = recipe();
    recipe.validate().unwrap();
    let mut statement = statement();
    statement.title = recipe.definition.terms.title.clone();
    statement.issuer = recipe.definition.terms.issuer.clone();
    statement.claim = recipe.definition.terms.claim.clone();
    statement.descriptor = recipe.definition.terms.descriptor.clone();
    statement.audience = Audience::Public;
    statement.issued_at = 1100;
    statement.as_of = 1100;
    statement.expires_at = 1700;
    statement.authorship = Authorship::Generated {
        recipe: recipe.permit.recipe.clone(),
        revision: 1,
        partial: false,
        state: recipes::EvaluationState::Available,
        permit: Box::new(recipe.permit.clone()),
    };
    let document = Document {
        statement,
        key_id: recipe.permit.terms.delegate.key_id.clone(),
        public_key: recipe.permit.terms.delegate.public_key.clone(),
        signature: format!("{}==", "A".repeat(86)),
    };
    document.validate().unwrap();
    let encoded = serde_json::to_string(&document).unwrap();
    assert!(!encoded.contains("\"inputs\""));
    assert!(!encoded.contains("\"controller\""));
    assert!(!encoded.contains(&recipe.definition.recipe));
    let serving = crate::trust_analysis::serving::Review {
        person: recipe.definition.controller.clone(),
        previous: None,
        request: crate::trust_analysis::serving::Request {
            source: crate::trust_analysis::serving::Source {
                record: crate::new_uid("r"),
                kind: crate::trust_analysis::serving::SourceKind::Claim,
            },
            scope: crate::trust_analysis::serving::Scope::Recipe,
            as_organ: false,
            audience: Default::default(),
            lifetime_seconds: 1000,
        },
        artifact: crate::trust_analysis::serving::Artifact::Claim(document.clone()),
        source_version: 1,
        source_revision: 0,
        reviewed_at: 1100,
        expires_at: 2000,
    };
    serving.hash().unwrap();
    let mut newer = document.clone();
    newer.statement.amount = amount("125.000000000000000001");
    newer.statement.revision = 2;
    newer.statement.issued_at = 1200;
    newer.statement.as_of = 1200;
    newer.statement.expires_at = 1800;
    let artifact = crate::trust_analysis::serving::Artifact::Claim(newer.clone());
    assert!(serving.allows_current(&artifact, 1, 3));
    assert!(!serving.allows_current(&artifact, 2, 3));
    let mut fixed = serving.clone();
    fixed.request.scope = crate::trust_analysis::serving::Scope::Fixed;
    fixed.expires_at = 1700;
    fixed.hash().unwrap();
    assert!(!fixed.allows_current(&artifact, 1, 0));
    assert_ne!(fixed.hash().unwrap(), serving.hash().unwrap());
    let Authorship::Generated { permit, .. } = &mut newer.statement.authorship else {
        panic!("permit");
    };
    permit.terms.title = "Different reviewed title".into();
    newer.statement.title = permit.terms.title.clone();
    assert!(!serving.allows_current(
        &crate::trust_analysis::serving::Artifact::Claim(newer),
        1,
        0
    ));
    let mut contact = serving.request.clone();
    contact.source.kind = crate::trust_analysis::serving::SourceKind::Contact;
    assert!(contact.validate().is_err());
    let mut manual = serving.clone();
    let crate::trust_analysis::serving::Artifact::Claim(manual_document) = &mut manual.artifact
    else {
        panic!("claim");
    };
    manual_document.statement.authorship = Authorship::Manual;
    assert!(manual.hash().is_err());
    let mut changed = document.clone();
    changed.statement.audience = Audience::default();
    assert!(changed.validate().is_err());
    let mut changed = document.clone();
    changed.statement.descriptor.measurement.meaning.revision += 1;
    assert!(changed.validate().is_err());
    let mut changed = document.clone();
    changed.statement.expires_at = 1800;
    assert!(changed.validate().is_err());
    let mut changed = document.clone();
    changed.key_id = "unreviewed-key".into();
    assert!(changed.validate().is_err());
    let mut changed = recipe.clone();
    changed.definition.inputs.rely_on_all_visible = false;
    assert!(changed.validate().is_err());
}

#[test]
fn automatic_recipes_reject_private_graph_criteria_and_imported_instructions() {
    let recipe = recipe();
    let mut changed = recipe.definition.clone();
    changed.inputs.contacts = Some(super::super::paths::Policy::for_anchor(
        changed.terms.issuer.clone(),
    ));
    assert!(changed.validate().is_err());
    let mut changed = recipe.definition.clone();
    changed
        .inputs
        .issuers
        .include
        .push(super::super::Condition {
            known: Some(true),
            ..Default::default()
        });
    assert!(changed.validate().is_err());
    let mut changed = recipe.definition.clone();
    changed
        .inputs
        .counterparties
        .exclude
        .push(super::super::Condition {
            upper: Some(crate::visibility::Bound {
                value: 3,
                inclusive: true,
            }),
            ..Default::default()
        });
    assert!(changed.validate().is_err());
    let mut encoded = serde_json::to_value(&recipe).unwrap();
    encoded["execute"] = serde_json::json!({"kind":"set_quantity","quantity":1000});
    assert!(serde_json::from_value::<recipes::Recipe>(encoded).is_err());
}

#[test]
fn unavailable_generated_summaries_withdraw_instead_of_claiming_zero() {
    let recipe = recipe();
    let mut statement = statement();
    let terms = &recipe.definition.terms;
    statement.title = terms.title.clone();
    statement.issuer = terms.issuer.clone();
    statement.claim = terms.claim.clone();
    statement.descriptor = terms.descriptor.clone();
    statement.audience = terms.audience.clone();
    statement.issued_at = 1100;
    statement.as_of = 1100;
    statement.expires_at = 1700;
    statement.amount = amount("0");
    statement.authorship = Authorship::Generated {
        recipe: recipe.permit.recipe.clone(),
        revision: 1,
        partial: true,
        state: recipes::EvaluationState::Unknown,
        permit: Box::new(recipe.permit),
    };
    assert!(statement.validate().is_err());
    statement.withdrawn = true;
    statement.validate().unwrap();
    statement.amount = amount("100");
    assert!(statement.validate().is_err());
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
