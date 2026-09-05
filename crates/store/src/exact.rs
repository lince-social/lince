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
    let mantissa: String = row
        .try_get(format!("{prefix}_mantissa").as_str())
        .map_err(StoreError::from)?;
    let scale: i64 = row
        .try_get(format!("{prefix}_scale").as_str())
        .map_err(StoreError::from)?;
    parse_decimal(&mantissa, scale)
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
