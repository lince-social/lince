use std::collections::BTreeMap;

use nucleus::{DecimalValue, RecordKind, valid_uid};
use serde_json::{Map, Value, from_str};
use sqlx::{Row, sqlite::SqliteRow};

use super::{AssertionRole, LinguaVisibility, VisibilityGrant, protocol};
use crate::{StoreError, exact::parse_decimal, linguas::LOCAL_UID};

pub(super) fn record_kind(value: &str) -> Result<RecordKind, StoreError> {
    RecordKind::parse(value)
        .ok_or_else(|| protocol(format!("invalid stored Record kind `{value}`")))
}

pub(super) fn assertion_role(value: &str) -> Result<AssertionRole, StoreError> {
    match value {
        "ordinary" => Ok(AssertionRole::Ordinary),
        "identity" => Ok(AssertionRole::Identity),
        _ => Err(protocol(format!("invalid stored Assertion role `{value}`"))),
    }
}

pub(super) fn lingua_visibility(value: &str) -> Result<LinguaVisibility, StoreError> {
    match value {
        "private" => Ok(LinguaVisibility::Private),
        "shared" => Ok(LinguaVisibility::Shared),
        "public" => Ok(LinguaVisibility::Public),
        _ => Err(protocol(format!(
            "invalid stored Lingua visibility `{value}`"
        ))),
    }
}

pub(super) fn visibility_grant(value: &str) -> Result<VisibilityGrant, StoreError> {
    match value {
        "visible" => Ok(VisibilityGrant::Visible),
        "hidden" => Ok(VisibilityGrant::Hidden),
        _ => Err(protocol(format!(
            "invalid stored visibility grant `{value}`"
        ))),
    }
}

pub(super) fn require_uid(uid: &str, prefix: &str, label: &str) -> Result<(), StoreError> {
    if !valid_uid(uid, prefix) {
        return Err(protocol(format!("invalid stored {label} identity `{uid}`")));
    }
    Ok(())
}

pub(super) fn require_lingua_uid(uid: &str) -> Result<(), StoreError> {
    if uid != LOCAL_UID && !valid_uid(uid, "g") {
        return Err(protocol(format!("invalid stored Lingua identity `{uid}`")));
    }
    Ok(())
}

pub(super) fn require_record_kind(
    records: &BTreeMap<&str, RecordKind>,
    uid: &str,
    accepted: &[RecordKind],
    label: &str,
) -> Result<(), StoreError> {
    let kind = records
        .get(uid)
        .ok_or_else(|| protocol(format!("{label} references a missing Record `{uid}`")))?;
    if !accepted.contains(kind) {
        return Err(protocol(format!(
            "{label} references a Record of the wrong kind"
        )));
    }
    Ok(())
}

pub(super) fn parse_role_identity(value: &str) -> Result<i64, StoreError> {
    if matches!(value.as_bytes().first(), Some(b'1'..=b'9'))
        && let Ok(id) = value.parse()
    {
        return Ok(id);
    }
    Err(protocol(
        "visibility Role identity is not a positive canonical integer",
    ))
}

pub(super) fn object(value: &str, label: &str) -> Result<Map<String, Value>, StoreError> {
    match from_str(value)
        .map_err(|error| protocol(format!("{label} is not valid JSON: {error}")))?
    {
        Value::Object(fields) => Ok(fields),
        _ => Err(protocol(format!("{label} is not a JSON object"))),
    }
}

pub(super) fn read_quantity(row: &SqliteRow) -> Result<DecimalValue, StoreError> {
    let mantissa: &str = row.try_get("quantity_mantissa")?;
    let scale = row.try_get("quantity_scale")?;
    let digits = mantissa.strip_prefix('-').unwrap_or(mantissa);
    if digits.is_empty()
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
        || (digits.len() > 1 && digits.starts_with('0'))
        || mantissa == "-0"
    {
        return Err(protocol("stored quantity mantissa is not canonical"));
    }
    parse_decimal(mantissa, scale)
}
