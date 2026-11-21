use serde_json::json;

use super::*;

fn question() -> Question {
    Question {
        prompt: "Choose the next step".into(),
        responder: "person-one".into(),
        schema: json!({"type":"object","properties":{"choice":{"type":"string","enum":["a","b"]},"count":{"type":"integer","minimum":1,"maximum":4}},"required":["choice"]}),
        state: State::Pending,
        answers: None,
        expires_ms: Some(1000),
    }
}

#[test]
fn answer_requires_exact_recipient_pending_state_and_unchanged_fields() {
    let original = question();
    let mut next = original.clone();
    next.state = State::Answered;
    next.answers = Some(json!({"choice":"a","count":2}));
    assert!(
        original
            .response(&next, "person-one", "author", 100)
            .is_ok()
    );
    assert!(
        original
            .response(&next, "person-two", "author", 100)
            .is_err()
    );
    assert!(
        original
            .response(&next, "person-one", "author", 1000)
            .is_err()
    );
    assert!(next.response(&next, "person-one", "author", 100).is_err());
    next.prompt = "Different question".into();
    assert!(
        original
            .response(&next, "person-one", "author", 100)
            .is_err()
    );
}

#[test]
fn form_validation_checks_types_choices_limits_patterns_and_duplicates() {
    let schema = question().schema;
    for answers in [
        json!({}),
        json!({"choice":"c"}),
        json!({"choice":"a","count":2.5}),
        json!({"choice":"a","extra":true}),
        json!({"choice":"a","count":5}),
    ] {
        assert!(validate_answers(&schema, &answers).is_err());
    }
    let schema = json!({"type":"object","properties":{"name":{"type":"string","minLength":2,"maxLength":8,"pattern":"^[a-z]+$"},"flags":{"type":"array","items":{"type":"string","enum":["a","b"]},"minItems":1,"maxItems":2}}});
    assert!(validate_answers(&schema, &json!({"name":"hi","flags":["a","b"]})).is_ok());
    for answers in [
        json!({"name":"X"}),
        json!({"name":"toolongname"}),
        json!({"flags":[]}),
        json!({"flags":["a","a"]}),
        json!({"flags":["c"]}),
    ] {
        assert!(validate_answers(&schema, &answers).is_err());
    }
}
