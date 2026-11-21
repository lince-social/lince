// non-lean-yet
// Scope: fn exact_error, fn zero, fn one, fn integer.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::missing_panics_doc] State the panic conditions for exported functions. Sites
//   10, 14, 18.
// - [Clippy: clippy::expect_used] Check the stated invariant and handle reachable failures. Sites
//   11, 15, 19.
// - [Clippy: clippy::map_err_ignore] Preserve relevant causes and context at the boundary. Sites
//   46, 48.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 30, 43, 52, 58, 64.
// - [Clippy: clippy::useless_conversion] useless conversion to the same type: `sqlx::Error`. Sites
//   35, 38.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 10, 14,
//   18, 22, 26, 30, 43, 52, 58, 64.

use nucleus::DecimalValue;
use sqlx::Row;

use crate::StoreError;

fn exact_error(message: String) -> StoreError {
    StoreError::Decode(message.into())
}

pub fn zero() -> DecimalValue {
    DecimalValue::from_mantissa(0, 0).expect("scale 0 is always valid")
}

pub fn one() -> DecimalValue {
    DecimalValue::from_mantissa(0, 1).expect("scale 0 is always valid")
}

pub fn integer(value: i128) -> DecimalValue {
    DecimalValue::from_mantissa(0, value).expect("scale 0 is always valid")
}

pub fn from_f64(value: f64) -> DecimalValue {
    nucleus::fact::decimal_from_f64(value)
}

pub fn decimal_columns(value: DecimalValue) -> (String, i64) {
    (value.mantissa().to_string(), i64::from(value.scale()))
}

pub fn read_decimal(
    row: &sqlx::sqlite::SqliteRow,
    prefix: &str,
) -> Result<DecimalValue, StoreError> {
    let mantissa: &str = row.try_get(format!("{prefix}_mantissa").as_str())?;
    let scale: i64 = row.try_get(format!("{prefix}_scale").as_str())?;
    parse_decimal(mantissa, scale)
}

pub fn parse_decimal(mantissa: &str, scale: i64) -> Result<DecimalValue, StoreError> {
    let mantissa: i128 = mantissa
        .parse()
        .map_err(|_| exact_error(format!("mantissa {mantissa:?} is not an integer")))?;
    let scale = u8::try_from(scale)
        .map_err(|_| exact_error(format!("decimal scale {scale} out of range")))?;
    DecimalValue::from_mantissa(scale, mantissa).map_err(|err| exact_error(err.to_string()))
}

pub fn difference(target: DecimalValue, current: DecimalValue) -> Result<DecimalValue, StoreError> {
    target
        .aligned_sub(current)
        .ok_or_else(|| exact_error("quantity difference overflows i128".to_string()))
}

pub fn negate(value: DecimalValue) -> Result<DecimalValue, StoreError> {
    value
        .checked_neg()
        .ok_or_else(|| exact_error("quantity negation overflows i128".to_string()))
}

pub fn sum_exact<I: IntoIterator<Item = DecimalValue>>(
    values: I,
) -> Result<DecimalValue, StoreError> {
    let mut total = zero();
    for value in values {
        total = total
            .aligned_add(value)
            .ok_or_else(|| exact_error("quantity sum overflows i128".into()))?;
    }
    Ok(total)
}
