use nucleus::DecimalValue;
use protein_authority::query::Predicate;
use serde_json::{Value, from_value, json, to_value};

fn assert_round_trip(raw: Value) {
    let parsed: Predicate = from_value(raw.clone()).expect("valid decimal predicate");
    assert_eq!(
        to_value(parsed).expect("serializable decimal predicate"),
        raw
    );
}

#[test]
fn every_quantity_predicate_uses_a_bare_canonical_string() {
    for name in [
        "quantity_lt",
        "quantity_lte",
        "quantity_gt",
        "quantity_gte",
        "quantity_eq",
    ] {
        assert_round_trip(json!({(name): "-12.340"}));
    }
}

#[test]
fn nested_boolean_quantity_predicates_round_trip_exactly() {
    assert_round_trip(json!({
        "all": [
            {"quantity_gt": "-1"},
            {"any": [
                {"quantity_lt": "2.00"},
                {"quantity_gte": "3.000"}
            ]},
            {"not": {"quantity_eq": "1.0"}}
        ]
    }));
}

#[test]
fn every_scale_and_i128_extreme_round_trip() {
    for scale in 0..=18 {
        for mantissa in [i128::MIN, i128::MAX] {
            let value =
                DecimalValue::from_mantissa(scale, mantissa).expect("supported decimal scale");
            assert_round_trip(json!({"quantity_eq": value.to_string()}));
        }
    }
}

#[test]
fn non_string_operands_refuse() {
    for value in [
        json!(0),
        json!(0.0),
        json!(true),
        Value::Null,
        json!([]),
        json!({}),
    ] {
        assert!(from_value::<Predicate>(json!({"quantity_eq": value})).is_err());
    }
}

#[test]
fn malformed_and_noncanonical_decimal_strings_refuse() {
    for value in [
        "",
        " 1",
        "1 ",
        "+1",
        "01",
        "00.0",
        "-0",
        "-0.0",
        ".1",
        "1.",
        "1e0",
        "1E+0",
        "0.0000000000000000000",
        "170141183460469231731687303715884105728",
        "-170141183460469231731687303715884105729",
    ] {
        assert!(
            from_value::<Predicate>(json!({"quantity_eq": value})).is_err(),
            "accepted {value:?}"
        );
    }
}

#[test]
fn decimal_string_limit_refuses_before_parsing() {
    let oversized = "1".repeat(65);
    let error =
        from_value::<Predicate>(json!({"quantity_eq": oversized})).expect_err("65-byte operand");
    assert!(error.to_string().contains("byte limit"));
}
