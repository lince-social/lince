use super::*;
use crate::property_rule::Property;
use serde_json::json;

fn rule() -> Rule {
    Rule {
        revision: 1, enabled: true, match_all: true,
        conditions: vec![PropertyRule { property: Property::ApplicationId, value: "org.example.Editor".into() }],
        properties: Properties { width: Some(Dimension::Fraction([1, 2])), floating: Some(false), ..Default::default() },
    }
}

#[test]
fn sparse_override_keeps_explicit_false_and_exact_requested_dimensions() {
    let base = Properties { workspace: Some(42), floating: Some(true), focus: Some(true), width: Some(Dimension::Fraction([1, 3])), ..Default::default() };
    let override_ = Properties { floating: Some(false), focus: Some(false), height: Some(Dimension::Pixels(720)), ..Default::default() };
    let result = override_.over(&base);
    assert_eq!(result.workspace, Some(42));
    assert_eq!(result.width, Some(Dimension::Fraction([1, 3])));
    assert_eq!(result.height, Some(Dimension::Pixels(720)));
    assert_eq!(result.floating, Some(false));
    assert_eq!(result.focus, Some(false));
    assert_eq!(result.width.unwrap().resolve(900.0), 300.0);
    assert_eq!(base.floating, Some(true));
}

#[test]
fn area_conditions_and_application_metadata_share_exact_matching_without_inferred_authority() {
    let mut rule = rule();
    rule.conditions.push(PropertyRule { property: Property::Title, value: "Owner document".into() });
    assert!(rule.matches(&json!({"head":"Owner document","app_id":"org.example.Editor"})));
    assert!(!rule.matches(&json!({"head":"Other","app_id":"org.example.Editor"})));
    assert!(!rule.matches(&json!({"head":"Owner document","app_id":"org.example.editor"})));
    rule.match_all = false;
    assert!(rule.matches(&json!({"head":"Other","app_id":"org.example.Editor"})));
    rule.enabled = false;
    assert!(!rule.matches(&json!({"head":"Owner document","app_id":"org.example.Editor"})));
    let quantity = PropertyRule { property: Property::Quantity, value: "0.10".into() };
    assert!(quantity.matches(&json!({"quantity":"0.1"})));
    assert!(!quantity.matches(&json!({"quantity":"0.1000000001"})));
}

#[test]
fn known_extensions_validate_before_reporting_a_revision() {
    let mut value = serde_json::to_value(rule()).unwrap();
    assert_eq!(revision(RULE_NAMESPACE, &value).unwrap(), Some(1));
    value["properties"]["program"] = json!("untrusted-command");
    assert!(revision(RULE_NAMESPACE, &value).is_err());
    assert_eq!(revision("org.example.unrelated", &value).unwrap(), None);
    let value = json!({"revision":1,"properties":{"focus":false}});
    assert_eq!(revision(WINDOW_NAMESPACE, &value).unwrap(), Some(1));
    assert!(revision(WINDOW_NAMESPACE, &json!({"revision":0,"properties":{}})).is_err());
}

#[test]
fn malformed_and_oversized_preferences_never_match() {
    for dimension in [Dimension::Fraction([0, 1]), Dimension::Fraction([1, 0]), Dimension::Fraction([2, 1]), Dimension::Pixels(0), Dimension::Pixels(100_001)] {
        assert!(Properties { width: Some(dimension), ..Default::default() }.validate().is_err());
    }
    for output in [String::new(), "bad\noutput".into(), "x".repeat(513)] {
        assert!(Properties { output: Some(output), ..Default::default() }.validate().is_err());
    }
    let mut invalid = rule();
    invalid.conditions[0].value = "x".repeat(4097);
    assert!(invalid.validate().is_err());
    assert!(!invalid.matches(&json!({"app_id":invalid.conditions[0].value})));
    invalid = rule();
    invalid.conditions = vec![invalid.conditions[0].clone(); 33];
    assert!(invalid.validate().is_err());
    invalid.conditions = vec![PropertyRule { property: Property::Title, value: "x".repeat(4096) }; 32];
    assert!(invalid.validate().is_err());
}
