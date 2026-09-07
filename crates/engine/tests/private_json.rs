use engine::private_json::{self, Error, Limits};
use engine::private_requests::{RequestError, RequestLimits};
use serde_json::{Value, json};

fn decode(input: &str) -> Result<Value, Error> {
    private_json::decode(input.as_bytes(), &Limits::default())
}

#[test]
fn private_json_rejects_duplicate_decoded_keys_at_every_depth() {
    for input in [
        r#"{"same":1,"same":2}"#,
        r#"{"same":1,"s\u0061me":2}"#,
        r#"{"outer":{"same":1,"same":2}}"#,
        r#"[{"same":1,"s\u0061me":2}]"#,
    ] {
        assert_eq!(decode(input), Err(Error::InvalidJson));
    }
}

#[test]
fn private_json_rejects_trailing_and_malformed_input() {
    for input in [
        "{}{}",
        "null true",
        "",
        "{",
        r#"{"x":NaN}"#,
        r#"{"x":Infinity}"#,
    ] {
        assert_eq!(decode(input), Err(Error::InvalidJson));
    }
    assert_eq!(
        private_json::decode(&[b'{', b'}', 0xff], &Limits::default()),
        Err(Error::InvalidJson)
    );
}

#[test]
fn private_json_preserves_exact_integer_limits_and_refuses_overflow() {
    let value = decode(
        r#"[-9223372036854775808,9223372036854775807,18446744073709551615,9007199254740993]"#,
    )
    .unwrap();
    let values = value.as_array().unwrap();
    assert_eq!(values[0].as_i64(), Some(i64::MIN));
    assert_eq!(values[1].as_i64(), Some(i64::MAX));
    assert_eq!(values[2].as_u64(), Some(u64::MAX));
    assert_eq!(values[3].as_u64(), Some(9_007_199_254_740_993));
    for input in ["18446744073709551616", "-9223372036854775809"] {
        assert_eq!(decode(input), Err(Error::InvalidJson));
    }
}

#[test]
fn private_json_uses_finite_correctly_rounded_decimal_numbers() {
    for (token, expected_bits) in [
        ("0.1", 0x3fb9_9999_9999_999a),
        ("1.7976931348623157e308", 0x7fef_ffff_ffff_ffff),
        ("1.7976931348623158e308", 0x7fef_ffff_ffff_ffff),
        ("2.2250738585072014e-308", 0x0010_0000_0000_0000),
        ("5e-324", 0x0000_0000_0000_0001),
        ("-0.0", 0x8000_0000_0000_0000),
        ("125e-2", 0x3ff4_0000_0000_0000),
    ] {
        let value = decode(token).unwrap();
        assert_eq!(value.as_f64().unwrap().to_bits(), expected_bits);
        let bytes = private_json::canonical_bytes(&value, &Limits::default()).unwrap();
        let roundtrip = private_json::decode(&bytes, &Limits::default()).unwrap();
        assert_eq!(roundtrip.as_f64().unwrap().to_bits(), expected_bits);
    }
    for token in ["1e309", "-1e309"] {
        assert_eq!(decode(token), Err(Error::InvalidJson));
    }
    let canonical = |token: &str| {
        private_json::canonical_bytes(&decode(token).unwrap(), &Limits::default()).unwrap()
    };
    assert_ne!(canonical("1"), canonical("1.0"));
    assert_eq!(canonical("1.0"), canonical("1e0"));
    assert_eq!(canonical("-0"), canonical("0"));
    assert_ne!(canonical("-0.0"), canonical("0.0"));
}

#[test]
fn private_json_numeric_preflight_enforces_json_grammar_and_token_bounds() {
    for token in [
        "01", "-01", "+1", ".1", "1.", "1e", "1e+", "1e-", "1-2", "--1", "1.2.3", "1e2e3", "1 2",
    ] {
        assert_eq!(decode(token), Err(Error::InvalidJson));
    }
    assert_eq!(
        decode(&format!("0.{}", "0".repeat(127))),
        Err(Error::LimitExceeded)
    );
    assert_eq!(decode(r#""1e309 -01""#).unwrap(), json!("1e309 -01"));
}

#[test]
fn private_json_input_node_depth_and_string_limits_are_exact() {
    let mut limits = Limits {
        bytes: 4,
        ..Limits::default()
    };
    assert_eq!(private_json::decode(b"null", &limits), Ok(Value::Null));
    limits.bytes = 3;
    assert_eq!(
        private_json::decode(b"null", &limits),
        Err(Error::LimitExceeded)
    );

    let input = br#"{"a":[true]}"#;
    limits = Limits {
        nodes: 3,
        depth: 3,
        ..Limits::default()
    };
    assert!(private_json::decode(input, &limits).is_ok());
    limits.nodes = 2;
    assert_eq!(
        private_json::decode(input, &limits),
        Err(Error::LimitExceeded)
    );
    limits.nodes = 3;
    limits.depth = 2;
    assert_eq!(
        private_json::decode(input, &limits),
        Err(Error::LimitExceeded)
    );

    limits = Limits {
        string_bytes: 2,
        ..Limits::default()
    };
    assert_eq!(
        private_json::decode("\"é\"".as_bytes(), &limits),
        Ok(json!("é"))
    );
    limits.string_bytes = 1;
    assert_eq!(
        private_json::decode("\"é\"".as_bytes(), &limits),
        Err(Error::LimitExceeded)
    );
    assert_eq!(
        private_json::decode(br#"{"xx":null}"#, &limits),
        Err(Error::LimitExceeded)
    );
}

#[test]
fn private_json_canonical_bytes_sort_every_object_and_obey_the_exact_limit() {
    let value = decode(r#"{"z":1.25,"a":{"last":2,"first":1},"m":[{"y":0,"x":1}]}"#).unwrap();
    let expected = br#"{"a":{"first":1,"last":2},"m":[{"x":1,"y":0}],"z":1.25}"#;
    let mut limits = Limits {
        canonical_bytes: expected.len(),
        ..Limits::default()
    };
    assert_eq!(
        private_json::canonical_bytes(&value, &limits).unwrap(),
        expected
    );
    limits.canonical_bytes -= 1;
    assert_eq!(
        private_json::canonical_bytes(&value, &limits),
        Err(Error::LimitExceeded)
    );
}

#[test]
fn private_json_canonical_bytes_reject_programmatic_structural_overflow() {
    let mut deep = Value::Null;
    for _ in 0..Limits::default().depth {
        deep = Value::Array(vec![deep]);
    }
    assert_eq!(
        private_json::canonical_bytes(&deep, &Limits::default()),
        Err(Error::LimitExceeded)
    );

    let values = Value::Array(vec![Value::Null, Value::Null, Value::Null]);
    let limits = Limits {
        nodes: 3,
        ..Limits::default()
    };
    assert_eq!(
        private_json::canonical_bytes(&values, &limits),
        Err(Error::LimitExceeded)
    );

    let limits = Limits {
        string_bytes: 2,
        ..Limits::default()
    };
    assert_eq!(
        private_json::canonical_bytes(&json!("abc"), &limits),
        Err(Error::LimitExceeded)
    );
    assert_eq!(
        private_json::canonical_bytes(&json!({"abc": null}), &limits),
        Err(Error::LimitExceeded)
    );
}

#[test]
fn private_json_canonical_output_limit_is_distinct_from_input_limit() {
    let value = json!({"long": "already decoded"});
    let expected = br#"{"long":"already decoded"}"#;
    let mut limits = Limits {
        bytes: 1,
        canonical_bytes: expected.len(),
        ..Limits::default()
    };
    assert_eq!(
        private_json::canonical_bytes(&value, &limits).unwrap(),
        expected
    );
    limits.canonical_bytes -= 1;
    assert_eq!(
        private_json::canonical_bytes(&value, &limits),
        Err(Error::LimitExceeded)
    );
}

#[test]
fn private_json_ordinary_normalization_keeps_its_existing_low_node_behavior() {
    let input = br#"{"version":1,"expected_organ_uid":"r_01ARZ3NDEKTSV4RRFFQ69G5FA1","expected_person_uid":"r_01ARZ3NDEKTSV4RRFFQ69G5FA2","operation_uid":"op_01ARZ3NDEKTSV4RRFFQ69G5FAB","expected_revisions":[],"commands":[{"command":"create_record","uid":"r_01ARZ3NDEKTSV4RRFFQ69G5FA3","kind":"plain","head":"Knowledge","body":"","quantity":{"scale":0,"value":"1"}}]}"#;
    let input_value: Value = serde_json::from_slice(input).unwrap();
    let nodes = json_nodes(&input_value);
    let limits = RequestLimits {
        nodes,
        ..RequestLimits::default()
    };
    let request = engine::private_requests::decode(
        input,
        "r_01ARZ3NDEKTSV4RRFFQ69G5FA1",
        "r_01ARZ3NDEKTSV4RRFFQ69G5FA2",
        &limits,
    )
    .unwrap();
    let expected = br#"{"commands":[{"body":"","command":"create_record","extensions":[],"head":"Knowledge","kind":"plain","place_uid":null,"quantity":{"scale":0,"value":"1"},"slug":null,"uid":"r_01ARZ3NDEKTSV4RRFFQ69G5FA3","unit_uid":null}],"expected_organ_uid":"r_01ARZ3NDEKTSV4RRFFQ69G5FA1","expected_person_uid":"r_01ARZ3NDEKTSV4RRFFQ69G5FA2","expected_revisions":[],"operation_uid":"op_01ARZ3NDEKTSV4RRFFQ69G5FAB","version":1}"#;
    assert_eq!(request.canonical_bytes(), expected);
    let normalized: Value = serde_json::from_slice(expected).unwrap();
    assert!(json_nodes(&normalized) > nodes);
    let json_limits = Limits {
        nodes,
        ..Limits::default()
    };
    assert_eq!(
        private_json::canonical_bytes(&normalized, &json_limits),
        Err(Error::LimitExceeded)
    );
    let mut too_small = limits;
    too_small.nodes -= 1;
    assert_eq!(
        engine::private_requests::decode(
            input,
            "r_01ARZ3NDEKTSV4RRFFQ69G5FA1",
            "r_01ARZ3NDEKTSV4RRFFQ69G5FA2",
            &too_small,
        )
        .err(),
        Some(RequestError::LimitExceeded)
    );
}

fn json_nodes(value: &Value) -> usize {
    1 + match value {
        Value::Array(values) => values.iter().map(json_nodes).sum::<usize>(),
        Value::Object(values) => values.values().map(json_nodes).sum::<usize>(),
        _ => 0,
    }
}

#[test]
fn private_json_limits_only_narrow_hard_caps() {
    let setters: [fn(&mut Limits, usize); 5] = [
        |limits, value| limits.bytes = value,
        |limits, value| limits.canonical_bytes = value,
        |limits, value| limits.depth = value,
        |limits, value| limits.nodes = value,
        |limits, value| limits.string_bytes = value,
    ];
    for setter in setters {
        for value in [0, usize::MAX] {
            let mut limits = Limits::default();
            setter(&mut limits, value);
            assert_eq!(limits.validate(), Err(Error::InvalidLimits));
            assert_eq!(
                private_json::decode(b"null", &limits),
                Err(Error::InvalidLimits)
            );
            assert_eq!(
                private_json::canonical_bytes(&Value::Null, &limits),
                Err(Error::InvalidLimits)
            );
        }
    }
}

#[test]
fn private_json_errors_do_not_retain_or_display_input() {
    let error = decode(r#"{"SECRET_DO_NOT_ECHO":1,"SECRET_DO_NOT_ECHO":2}"#).unwrap_err();
    assert_eq!(error, Error::InvalidJson);
    assert!(!error.to_string().contains("SECRET_DO_NOT_ECHO"));
    assert!(!format!("{error:?}").contains("SECRET_DO_NOT_ECHO"));
}
