use protein::{Predicate, authority::RolePolicy};
use serde_json::{from_value, json, to_value};

#[test]
fn role_policy_uses_the_same_strict_nested_predicate_grammar() {
    let raw = json!({
        "read": {
            "all": [
                {"relation": {"kind": "assigned", "direction": "both"}},
                {"not": {"under": {"record": "r_private", "include_self": true}}}
            ]
        },
        "grants": []
    });
    let policy: RolePolicy = from_value(raw).expect("valid policy");
    let encoded = to_value(policy).expect("serializable policy");
    let decoded: RolePolicy = from_value(encoded).expect("serialized policy is valid");
    assert!(matches!(decoded.read, Predicate::All(_)));

    assert!(
        from_value::<RolePolicy>(json!({
            "read": {
                "all": [{
                    "any": [{
                        "not": {"near": {"of": "p_office", "meters": 10.0, "unexpected": true}}
                    }]
                }]
            },
            "grants": []
        }))
        .is_err()
    );
}
