use std::cmp::Ordering;
use std::collections::HashSet;

use nucleus::DecimalValue;
use nucleus::karma::MAX_DECIMAL_SCALE;

fn decimal(scale: u8, mantissa: i128) -> DecimalValue {
    DecimalValue::from_mantissa(scale, mantissa).unwrap()
}

fn arithmetic_cmp(left: DecimalValue, right: DecimalValue) -> Ordering {
    let scale = left.scale().max(right.scale());
    let left_factor = 10_i128
        .checked_pow(u32::from(scale - left.scale()))
        .unwrap();
    let right_factor = 10_i128
        .checked_pow(u32::from(scale - right.scale()))
        .unwrap();
    left.mantissa()
        .checked_mul(left_factor)
        .unwrap()
        .cmp(&right.mantissa().checked_mul(right_factor).unwrap())
}

#[test]
fn numeric_equality_does_not_change_structural_identity_or_serialization() {
    let one = decimal(0, 1);
    let one_point_zero = decimal(1, 10);
    let one_point_zero_zero = decimal(2, 100);
    assert_eq!(one.exact_numeric_cmp(one_point_zero), Ordering::Equal);
    assert_eq!(
        one_point_zero.exact_numeric_cmp(one_point_zero_zero),
        Ordering::Equal
    );
    assert_ne!(one, one_point_zero);
    assert_eq!(one.cmp(&one_point_zero), Ordering::Less);
    assert_eq!(HashSet::from([one, one_point_zero]).len(), 2);
    let one_json = serde_json::to_value(one).unwrap();
    let one_point_zero_json = serde_json::to_value(one_point_zero).unwrap();
    assert_ne!(one_json, one_point_zero_json);
    assert_eq!(
        serde_json::from_value::<DecimalValue>(one_json).unwrap(),
        one
    );
    assert_eq!(
        serde_json::from_value::<DecimalValue>(one_point_zero_json).unwrap(),
        one_point_zero
    );
}

#[test]
fn every_supported_scale_compares_equal_for_equivalent_values_and_zero() {
    let zero = decimal(0, 0);
    let one = decimal(0, 1);
    let negative_one = decimal(0, -1);
    for scale in 0..=MAX_DECIMAL_SCALE {
        let factor = 10_i128.pow(u32::from(scale));
        assert_eq!(zero.exact_numeric_cmp(decimal(scale, 0)), Ordering::Equal);
        assert_eq!(
            one.exact_numeric_cmp(decimal(scale, factor)),
            Ordering::Equal
        );
        assert_eq!(
            negative_one.exact_numeric_cmp(decimal(scale, -factor)),
            Ordering::Equal
        );
    }
}

#[test]
fn signs_extremes_and_alignment_overflow_are_ordered_exactly() {
    let minimum = decimal(0, i128::MIN);
    let maximum = decimal(0, i128::MAX);
    let scaled_minimum = decimal(MAX_DECIMAL_SCALE, i128::MIN);
    let scaled_maximum = decimal(MAX_DECIMAL_SCALE, i128::MAX);
    assert_eq!(minimum.exact_numeric_cmp(maximum), Ordering::Less);
    assert_eq!(minimum.exact_numeric_cmp(scaled_minimum), Ordering::Less);
    assert_eq!(maximum.exact_numeric_cmp(scaled_maximum), Ordering::Greater);
    assert_eq!(
        scaled_minimum.exact_numeric_cmp(decimal(0, -1)),
        Ordering::Less
    );
    assert_eq!(
        scaled_maximum.exact_numeric_cmp(decimal(0, 1)),
        Ordering::Greater
    );
    assert_eq!(
        decimal(18, -1).exact_numeric_cmp(decimal(0, 0)),
        Ordering::Less
    );
    assert_eq!(
        decimal(18, 1).exact_numeric_cmp(decimal(0, 0)),
        Ordering::Greater
    );
    assert!(minimum.rescale(MAX_DECIMAL_SCALE).is_none());
    assert!(maximum.rescale(MAX_DECIMAL_SCALE).is_none());
}

#[test]
fn quantities_too_close_for_binary64_remain_distinct() {
    let lower = decimal(MAX_DECIMAL_SCALE, i128::MAX - 1);
    let higher = decimal(MAX_DECIMAL_SCALE, i128::MAX);
    assert_eq!(lower.to_f64(), higher.to_f64());
    assert_eq!(lower.exact_numeric_cmp(higher), Ordering::Less);
    assert_eq!(higher.exact_numeric_cmp(lower), Ordering::Greater);

    let exact_integer = decimal(0, 9_007_199_254_740_992);
    let next_integer = decimal(0, 9_007_199_254_740_993);
    assert_eq!(exact_integer.to_f64(), next_integer.to_f64());
    assert_eq!(
        exact_integer.exact_numeric_cmp(next_integer),
        Ordering::Less
    );
}

#[test]
fn comparison_is_antisymmetric_and_transitive_across_scales() {
    let values = [
        decimal(0, i128::MIN),
        decimal(18, i128::MIN),
        decimal(0, -10),
        decimal(1, -1),
        decimal(18, -1),
        decimal(18, 0),
        decimal(18, 1),
        decimal(1, 1),
        decimal(0, 10),
        decimal(18, i128::MAX),
        decimal(0, i128::MAX),
    ];
    for left in values {
        for right in values {
            assert_eq!(
                left.exact_numeric_cmp(right),
                right.exact_numeric_cmp(left).reverse()
            );
            for last in values {
                if left.exact_numeric_cmp(right) != Ordering::Greater
                    && right.exact_numeric_cmp(last) != Ordering::Greater
                {
                    assert_ne!(left.exact_numeric_cmp(last), Ordering::Greater);
                }
            }
        }
    }
}

#[test]
fn bounded_checked_arithmetic_oracle_agrees() {
    let mantissas = [
        -1_000_000_i128,
        -12_345,
        -10,
        -1,
        0,
        1,
        9,
        10,
        12_345,
        1_000_000,
    ];
    for left_scale in 0..=6 {
        for right_scale in 0..=6 {
            for left_mantissa in mantissas {
                for right_mantissa in mantissas {
                    let left = decimal(left_scale, left_mantissa);
                    let right = decimal(right_scale, right_mantissa);
                    assert_eq!(left.exact_numeric_cmp(right), arithmetic_cmp(left, right));
                }
            }
        }
    }
}
