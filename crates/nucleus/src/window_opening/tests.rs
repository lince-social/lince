use serde_json::{Value, json, to_value};

use super::{Dimension, Properties, RULE_NAMESPACE, Rule, WINDOW_NAMESPACE, revision};
use crate::property_rule::{Property, PropertyRule};

const APP_ID: &str = "org.example.Editor";

fn condition(property: Property, value: impl Into<String>) -> PropertyRule {
    PropertyRule {
        property,
        value: value.into(),
    }
}

fn rule() -> Rule {
    Rule {
        revision: 1,
        enabled: true,
        match_all: true,
        conditions: vec![condition(Property::ApplicationId, APP_ID)],
        properties: Properties {
            width: Some(Dimension::Fraction([1, 2])),
            floating: Some(false),
            ..Default::default()
        },
    }
}

#[test]
fn sparse_override_keeps_explicit_false_and_exact_requested_dimensions() {
    let base = Properties {
        workspace: Some(42),
        floating: Some(true),
        focus: Some(true),
        width: Some(Dimension::Fraction([1, 3])),
        ..Default::default()
    };
    let override_ = Properties {
        floating: Some(false),
        focus: Some(false),
        height: Some(Dimension::Pixels(720)),
        ..Default::default()
    };
    let result = override_.over(&base);
    assert_eq!(result.workspace, Some(42));
    assert_eq!(result.width, Some(Dimension::Fraction([1, 3])));
    assert_eq!(result.height, Some(Dimension::Pixels(720)));
    assert_eq!(result.floating, Some(false));
    assert_eq!(result.focus, Some(false));
    assert_eq!(result.width.map(|width| width.resolve(900.0)), Some(300.0));
    assert_eq!(base.floating, Some(true));
}

#[test]
fn area_conditions_and_application_metadata_share_exact_matching_without_inferred_authority() {
    let mut rule = rule();
    rule.conditions
        .push(condition(Property::Title, "Owner document"));
    for (head, app_id, expected) in [
        ("Owner document", APP_ID, true),
        ("Other", APP_ID, false),
        ("Owner document", "org.example.editor", false),
    ] {
        assert_eq!(
            rule.matches(&json!({"head":head,"app_id":app_id})),
            expected
        );
    }
    rule.match_all = false;
    assert!(rule.matches(&json!({"head":"Other","app_id":APP_ID})));
    rule.enabled = false;
    assert!(!rule.matches(&json!({"head":"Owner document","app_id":APP_ID})));
    let quantity = condition(Property::Quantity, "0.10");
    assert!(quantity.matches(&json!({"quantity":"0.1"})));
    assert!(!quantity.matches(&json!({"quantity":"0.1000000001"})));
}

#[test]
fn known_extensions_validate_before_reporting_a_revision() -> serde_json::Result<()> {
    let mut value = to_value(rule())?;
    assert_eq!(revision(RULE_NAMESPACE, &value), Ok(Some(1)));
    value
        .get_mut("properties")
        .and_then(Value::as_object_mut)
        .expect("the serialized opening rule has a properties object")
        .insert("program".into(), json!("untrusted-command"));
    assert!(revision(RULE_NAMESPACE, &value).is_err());
    assert_eq!(revision("org.example.unrelated", &value), Ok(None));
    let value = json!({"revision":1,"properties":{"focus":false}});
    assert_eq!(revision(WINDOW_NAMESPACE, &value), Ok(Some(1)));
    assert!(revision(WINDOW_NAMESPACE, &json!({"revision":0,"properties":{}})).is_err());
    Ok(())
}

#[test]
fn malformed_and_oversized_preferences_never_match() {
    for dimension in [
        Dimension::Fraction([0, 1]),
        Dimension::Fraction([1, 0]),
        Dimension::Fraction([2, 1]),
        Dimension::Pixels(0),
        Dimension::Pixels(100_001),
    ] {
        assert!(
            Properties {
                width: Some(dimension),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
    for output in [String::new(), "bad\noutput".into(), "x".repeat(513)] {
        assert!(
            Properties {
                output: Some(output),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
    let oversized_value = "x".repeat(4097);
    let invalid = Rule {
        conditions: vec![condition(Property::ApplicationId, &oversized_value)],
        ..rule()
    };
    assert!(invalid.validate().is_err());
    assert!(!invalid.matches(&json!({"app_id":oversized_value})));

    for conditions in [
        vec![condition(Property::ApplicationId, APP_ID); 33],
        vec![condition(Property::Title, "x".repeat(4096)); 32],
    ] {
        assert!(
            Rule {
                conditions,
                ..rule()
            }
            .validate()
            .is_err()
        );
    }
}
