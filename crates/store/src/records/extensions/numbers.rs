use std::collections::{BTreeMap, HashMap};

use nucleus::DecimalValue;
use serde_json::{Value, from_str, json, to_string, value::RawValue};
use sqlx::{Row, SqlitePool};

use super::{
    MAX_EXTENSION_BYTES, MAX_EXTENSION_KEYS, SQLITE_BYTE_LIMIT, protocol, validate_extension_name,
};
use crate::{StoreError, exact::zero};

fn extension_decimal(raw: &str) -> Result<DecimalValue, StoreError> {
    if !raw.starts_with(|character: char| character == '-' || character.is_ascii_digit()) {
        return Err(protocol("Extension property is not a JSON number"));
    }
    let (coefficient, exponent) = raw.split_once(['e', 'E']).unwrap_or((raw, "0"));
    let number = DecimalValue::parse_inferred(coefficient)
        .map_err(|error| protocol(format!("Extension number is unsupported: {error}")))?;
    let exponent: i32 = exponent
        .parse()
        .map_err(|_| protocol("Extension number exponent is out of range"))?;
    let mut scale = i64::from(number.scale()) - i64::from(exponent);
    let mut mantissa = number.mantissa();
    if mantissa == 0 {
        return Ok(zero());
    }
    while scale > 0 && mantissa % 10 == 0 {
        mantissa /= 10;
        scale -= 1;
    }
    if scale < 0 {
        let factor = u32::try_from(-scale)
            .ok()
            .and_then(|shift| 10_i128.checked_pow(shift));
        mantissa = factor
            .and_then(|factor| mantissa.checked_mul(factor))
            .ok_or_else(|| protocol("Extension number is too large"))?;
        scale = 0;
    }
    let scale = u8::try_from(scale)
        .map_err(|_| protocol("Extension number has too many decimal places"))?;
    DecimalValue::from_mantissa(scale, mantissa)
        .map_err(|error| protocol(format!("Extension number is unsupported: {error}")))
}

fn extension_raw_fields(raw: &str) -> Result<BTreeMap<String, &RawValue>, StoreError> {
    if raw.len() > MAX_EXTENSION_BYTES {
        return Err(protocol("Record extension exceeds its byte limit"));
    }
    let fields: BTreeMap<String, &RawValue> = from_str(raw)
        .map_err(|error| protocol(format!("Extension is not a JSON object: {error}")))?;
    if fields.len() > MAX_EXTENSION_KEYS {
        return Err(protocol("Record extension exceeds its key count limit"));
    }
    Ok(fields)
}

/// Reads an exact decimal from a top-level extension property.
///
/// # Errors
/// Rejects invalid or missing namespace and property names, unsupported
/// numbers, and failed database reads.
pub async fn extension_number(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
    property: &str,
) -> Result<DecimalValue, StoreError> {
    validate_extension_name(namespace, "namespace", true)?;
    validate_extension_name(property, "property", false)?;
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT substr(fds, 1, ?) FROM record_extension WHERE record_uid = ? AND namespace = ?",
    )
    .bind(SQLITE_BYTE_LIMIT + 1)
    .bind(record_uid)
    .bind(namespace)
    .fetch_optional(pool)
    .await?;
    let raw = raw.ok_or_else(|| protocol(format!("Record extension {namespace} is missing")))?;
    let fields = extension_raw_fields(&raw)?;
    let raw = fields.get(property).ok_or_else(|| {
        protocol(format!(
            "Extension property {namespace}.{property} is missing"
        ))
    })?;
    extension_decimal(raw.get())
        .map_err(|error| protocol(format!("Cannot read {namespace}.{property}: {error}")))
}

/// Lists exact numeric properties of a Record's extensions.
pub async fn numeric_extensions(
    pool: &SqlitePool,
    record_uid: &str,
) -> Result<Vec<Value>, StoreError> {
    Ok(numeric_extensions_for_records(pool, &[record_uid])
        .await?
        .remove(record_uid)
        .unwrap_or_default())
}

/// Lists exact numeric properties grouped by Record, ordered by namespace and key.
///
/// # Errors
/// Returns an error for an oversized or malformed extension object or a failed
/// database read. Properties that cannot be read as exact decimals are skipped.
pub async fn numeric_extensions_for_records(
    pool: &SqlitePool,
    records: &[&str],
) -> Result<HashMap<String, Vec<Value>>, StoreError> {
    let mut output = HashMap::<String, Vec<Value>>::new();
    if records.is_empty() {
        return Ok(output);
    }
    let selected = to_string(records).map_err(|error| protocol(error.to_string()))?;
    let rows = sqlx::query(
        "SELECT record_uid, namespace, substr(fds, 1, ?) AS fds FROM record_extension
         WHERE record_uid IN (SELECT value FROM json_each(?)) ORDER BY record_uid, namespace",
    )
    .bind(SQLITE_BYTE_LIMIT + 1)
    .bind(selected)
    .fetch_all(pool)
    .await?;
    for row in rows {
        let readings = output.entry(row.get("record_uid")).or_default();
        let namespace: String = row.get("namespace");
        for (property, raw) in extension_raw_fields(row.get::<&str, _>("fds"))? {
            if let Ok(number) = extension_decimal(raw.get()) {
                readings.push(json!({"namespace": namespace, "property": property, "value": number.to_string()}));
            }
        }
    }
    Ok(output)
}

/// Reads one exact decimal from a bounded JSON extension path.
pub fn extension_numeric_at(raw: &str, path: &[String]) -> Result<DecimalValue, StoreError> {
    ExtensionNumbers::new(raw).get(path)
}

/// Borrows extension JSON and caches parsed objects across numeric lookups.
pub struct ExtensionNumbers<'a> {
    raw: &'a str,
    fields: Option<BTreeMap<String, ExtensionNumbers<'a>>>,
}

impl<'a> ExtensionNumbers<'a> {
    /// Creates a lookup without parsing the document.
    pub fn new(raw: &'a str) -> Self {
        Self { raw, fields: None }
    }

    /// Reads the exact decimal at the path, parsing and caching objects as needed.
    ///
    /// # Errors
    /// Rejects out-of-bounds paths, missing fields, and unsupported objects or numbers.
    pub fn get(&mut self, path: &[String]) -> Result<DecimalValue, StoreError> {
        if path.is_empty()
            || path.len() > 8
            || path.iter().any(|part| part.is_empty() || part.len() > 128)
            || path.iter().map(String::len).sum::<usize>() > 512
        {
            return Err(protocol("Numeric extension paths exceed their bounds"));
        }
        let mut value = self;
        for part in path {
            let fields = match &mut value.fields {
                Some(fields) => fields,
                slot @ None => slot.insert(
                    extension_raw_fields(value.raw)?
                        .into_iter()
                        .map(|(key, raw)| (key, Self::new(raw.get())))
                        .collect(),
                ),
            };
            value = fields
                .get_mut(part)
                .ok_or_else(|| protocol("Numeric extension field is unavailable"))?;
        }
        extension_decimal(value.raw)
    }
}
