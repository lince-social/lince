use super::*;
use crate::{
    new_uid,
    transfer::outcome::Role,
    trust_analysis::{
        Identity, contacts,
        serving::{Artifact, MAX_ARTIFACT_BYTES, SourceKind},
    },
};

fn request() -> Request {
    Request {
        id: new_uid("ask"),
        question: Question::Contacts {
            subject: Identity::Person(new_uid("r")),
        },
        issued_at: 1000,
        deadline: 1015,
        after: None,
    }
}

#[test]
fn questions_exclude_reader_policy_and_bound_freshness_and_continuation() {
    let request = request();
    request.validate(1000).unwrap();
    assert!(request.validate(1015).is_err());
    assert!(request.validate(994).is_err());
    let mut changed = request.clone();
    changed.deadline += 1;
    assert!(changed.validate(1000).is_err());
    changed = request.clone();
    changed.after = Some("x".repeat(MAX_CURSOR_BYTES + 1));
    assert!(changed.validate(1000).is_err());
    let mut value = serde_json::to_value(&request).unwrap();
    value["weights"] = serde_json::json!({"giver":"100"});
    assert!(serde_json::from_value::<Request>(value).is_err());
    let mut value = serde_json::to_value(&request.question).unwrap();
    value["private_person"] = serde_json::json!(new_uid("r"));
    assert!(serde_json::from_value::<Question>(value).is_err());
    let invalid = Question::Activity {
        subject: Identity::Person(new_uid("r")),
        role: Role::Giver,
        measurements: Vec::new(),
        from: Some(1001),
        until: Some(1000),
    };
    assert!(invalid.validate().is_err());
}

#[test]
fn headers_are_bounded_hints_bound_to_the_original_question() {
    let request = request();
    let mut page = Page {
        id: request.id.clone(),
        question: request.question.hash().unwrap(),
        headers: Vec::new(),
        next: None,
        partial: false,
    };
    page.validate(&request, 1000).unwrap();
    page.headers.push(Header {
        hash: page.question.clone(),
        kind: SourceKind::Contact,
        bytes: 320,
        expires_at: 1020,
    });
    page.validate(&request, 1000).unwrap();
    let mut other = request.clone();
    other.id = new_uid("ask");
    assert!(page.validate(&other, 1000).is_err());
    other = request.clone();
    other.question = Question::Contacts {
        subject: Identity::Organ(new_uid("r")),
    };
    assert!(page.validate(&other, 1000).is_err());
    page.headers.push(page.headers[0].clone());
    assert!(page.validate(&request, 1000).is_err());
    page.headers.pop();
    page.headers[0].bytes = MAX_ARTIFACT_BYTES as u32 + 1;
    assert!(page.validate(&request, 1000).is_err());
    page.headers[0].bytes = 320;
    page.headers[0].expires_at = 1000;
    assert!(page.validate(&request, 1000).is_err());
}

#[test]
fn disclosed_contacts_do_not_turn_persons_into_organs_with_the_same_identifier() {
    let uid = new_uid("r");
    let artifact = Artifact::Contact(contacts::Document {
        statement: contacts::Statement {
            issuer: Identity::Person(uid.clone()),
            contact: Identity::Organ(new_uid("r")),
            revision: 1,
            issued_at: 1000,
            expires_at: 2000,
            withdrawn: false,
            audience: contacts::Audience::Public,
        },
        key_id: String::new(),
        public_key: String::new(),
        signature: String::new(),
    });
    assert!(
        Question::Contacts {
            subject: Identity::Person(uid.clone())
        }
        .matches(&artifact)
    );
    assert!(
        !Question::Contacts {
            subject: Identity::Organ(uid)
        }
        .matches(&artifact)
    );
}
