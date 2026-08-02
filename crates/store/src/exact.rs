//! Exact decimals at the SQL boundary (blueprint E0.0).
//!
//! A quantity crosses into SQLite as the pair `(mantissa TEXT, scale INTEGER)`
//! and comes back as a `DecimalValue`. Two rules hold everywhere:
//!
//! - The mantissa is TEXT because it is an `i128`; SQLite's INTEGER is 64-bit
//!   and would truncate at scale 18 above ±9.22 units.
//! - Sums happen in Rust over `i128`, never as SQL `SUM()`. SQLite's numeric
//!   affinity would coerce the pair back into a float, which is the exact
//!   failure this representation exists to prevent.

use nucleus::DecimalValue;
use sqlx::Row;

use crate::StoreError;

/// A Ledger row whose exact pair will not parse is corruption, not a zero.
fn exact_error(message: String) -> StoreError {
    StoreError::Decode(message.into())
}

/// The zero quantity — scale 0, mantissa 0.
pub fn zero() -> DecimalValue {
    DecimalValue::from_mantissa(0, 0).expect("scale 0 is always valid")
}

/// The unit quantity — `quantity = 1` is the universal activation knob.
pub fn one() -> DecimalValue {
    DecimalValue::from_mantissa(0, 1).expect("scale 0 is always valid")
}

/// A whole-number quantity at scale 0 — activation flags (`0`, `1`, `-1`) and
/// counts, which are exact without going anywhere near a float.
pub fn integer(value: i128) -> DecimalValue {
    DecimalValue::from_mantissa(0, value).expect("scale 0 is always valid")
}

/// Convert a legacy `f64` amount. Same door as `nucleus::fact::decimal_from_f64`
/// and named the same way on purpose: every use marks a producer that E0.2/E0.3
/// still has to make exact.
pub fn from_f64(value: f64) -> DecimalValue {
    nucleus::fact::decimal_from_f64(value)
}

/// Bind form: `(mantissa_text, scale)`.
pub fn decimal_columns(value: DecimalValue) -> (String, i64) {
    (value.mantissa().to_string(), i64::from(value.scale()))
}

/// Read a decimal from a row's `<prefix>_mantissa` / `<prefix>_scale` columns.
/// A row that cannot be parsed is a corrupt Ledger, not a zero: say so.
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

/// `target - current` — the delta that moves a level to a target exactly.
pub fn difference(target: DecimalValue, current: DecimalValue) -> Result<DecimalValue, StoreError> {
    target
        .aligned_sub(current)
        .ok_or_else(|| exact_error("quantity difference overflows i128".to_string()))
}

/// `-value` — the delta that reverses a movement exactly.
pub fn negate(value: DecimalValue) -> Result<DecimalValue, StoreError> {
    value
        .checked_neg()
        .ok_or_else(|| exact_error("quantity negation overflows i128".to_string()))
}

/// Fold exact decimals of mixed scales. Alignment takes the finer of the two
/// scales, and every scale is `<= 18` by construction, so the running total is
/// always representable and no rounding step can enter the fold.
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
