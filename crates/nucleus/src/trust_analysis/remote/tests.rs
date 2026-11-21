use super::*;
use crate::{
    DecimalValue,
    karma::canonical_hash,
    new_uid,
    trust_analysis::{Identity, query::Query},
};

fn review() -> Review {
    let draft = Draft {
        query: Query::for_subject(Identity::Organ(new_uid("r"))),
        as_organ: false,
        linked_claims: Vec::new(),
        include_contacts: true,
        receipt_sources: Vec::new(),
        contact_subjects: Vec::new(),
    };
    Review {
        nonce: new_uid("ask"),
        person: new_uid("r"),
        questions: draft.questions().unwrap(),
        draft,
        endpoints: vec![Endpoint {
            organ: new_uid("r"),
            endpoint: "a".repeat(64),
            label: "Selected source".into(),
            profile_revision: 0,
        }],
        profiles: Vec::new(),
        reviewed_at: 1000,
    }
}

#[test]
fn outgoing_questions_do_not_contain_reader_policy_but_the_review_binds_it() {
    let original = review();
    let mut changed = original.clone();
    changed.draft.query.weights.giver = DecimalValue::parse_inferred("10").unwrap();
    assert_eq!(changed.draft.questions().unwrap(), original.questions);
    assert_ne!(changed.hash().unwrap(), original.hash().unwrap());
    let question = serde_json::to_string(&original.questions).unwrap();
    assert!(!question.contains(&original.person));
    assert!(!question.contains("weights"));
    assert!(!question.contains("endpoints"));
    changed.draft.as_organ = true;
    assert_ne!(changed.hash().unwrap(), original.hash().unwrap());
}

#[test]
fn review_rejects_duplicate_recipients_and_edited_public_questions() {
    let original = review();
    original.hash().unwrap();
    let mut changed = original.clone();
    changed.endpoints.push(changed.endpoints[0].clone());
    assert!(changed.hash().is_err());
    changed = original.clone();
    changed.questions.clear();
    assert!(changed.hash().is_err());
    changed = original.clone();
    changed.endpoints[0].endpoint = "unreviewed-address".into();
    assert!(changed.hash().is_err());
    changed = original.clone();
    changed.draft.linked_claims = (0..5)
        .map(|_| Series {
            issuer: Identity::Person(new_uid("r")),
            claim: new_uid("r"),
            snapshot: None,
        })
        .collect();
    assert!(changed.hash().is_err());
}

#[test]
fn additional_public_context_needs_an_exact_new_question_review_and_keeps_private_anchors_local() {
    let original = review();
    let mut expanded = original.clone();
    expanded.draft.receipt_sources.push(ReceiptSource {
        origin: new_uid("r"),
        occurrence: canonical_hash("lince.test.source.v1", &"public occurrence").unwrap(),
    });
    expanded
        .draft
        .contact_subjects
        .push(Identity::Organ(new_uid("r")));
    assert!(expanded.hash().is_err());
    expanded.questions = expanded.draft.questions().unwrap();
    assert_ne!(expanded.hash().unwrap(), original.hash().unwrap());
    assert_eq!(expanded.questions.len(), original.questions.len() + 2);
    let encoded = serde_json::to_string(&expanded.questions).unwrap();
    assert!(!encoded.contains(&original.person));
    expanded
        .draft
        .query
        .exclude_everywhere
        .push(expanded.draft.contact_subjects[0].clone());
    assert!(expanded.draft.questions().is_err());
    expanded.draft.query.exclude_everywhere.clear();
    let subject = expanded.draft.contact_subjects[0].clone();
    expanded.draft.contact_subjects.push(subject);
    assert!(expanded.draft.questions().is_err());
}
