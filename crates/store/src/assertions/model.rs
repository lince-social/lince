use nucleus::DecimalValue;
use serde_json::{Map, Value, json};
use sqlx::{Row, sqlite::SqliteRow};

use crate::{
    StoreError,
    exact::{decimal_columns, read_decimal},
};

pub(super) fn assertion_op_fields(
    subject_uid: &str,
    predicate_uid: &str,
    object_uid: Option<&str>,
    role: &str,
    quantity: AssertionQuantity<'_>,
    asserted_by: Option<&str>,
    created_at: &str,
) -> Map<String, Value> {
    let (mantissa, scale) = quantity_columns(quantity.quantity);
    [
        ("subject_uid", json!(subject_uid)),
        ("predicate_uid", json!(predicate_uid)),
        ("object_uid", json!(object_uid)),
        ("role", json!(role)),
        ("quantity_mantissa", json!(mantissa)),
        ("quantity_scale", json!(scale)),
        ("unit_uid", json!(quantity.unit_uid)),
        ("asserted_by", json!(asserted_by)),
        ("created_at", json!(created_at)),
    ]
    .into_iter()
    .map(|(key, value)| (key.into(), value))
    .collect()
}

pub(super) fn quantity_columns(quantity: Option<DecimalValue>) -> (Option<String>, Option<i64>) {
    quantity.map_or((None, None), |value| {
        let (mantissa, scale) = decimal_columns(value);
        (Some(mantissa), Some(scale))
    })
}

pub(super) fn read_quantity(row: &SqliteRow) -> Result<Option<DecimalValue>, StoreError> {
    row.try_get::<Option<&str>, _>("quantity_mantissa")?
        .map(|_| read_decimal(row, "quantity"))
        .transpose()
}

/// Distinguishes ordinary relations from unary Record identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertionRole {
    Ordinary,
    Identity,
}

impl AssertionRole {
    /// Returns the role's stored spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ordinary => "ordinary",
            Self::Identity => "identity",
        }
    }
}

/// Stored Assertion with its exact quantity, provenance, and retraction state.
#[derive(Debug, Clone, PartialEq)]
pub struct AssertionRow {
    pub uid: String,
    pub subject_uid: String,
    pub predicate_uid: String,
    pub predicate_revision: i64,
    pub object_uid: Option<String>,
    pub object_uri: Option<String>,
    pub resolved_object_uid: Option<String>,
    pub role: String,
    pub quantity: Option<DecimalValue>,
    pub unit_uid: Option<String>,
    pub unit_revision: Option<i64>,
    pub asserted_by: Option<String>,
    pub created_at: String,
    pub retracted_at: Option<String>,
    pub retracted_by: Option<String>,
}

pub(super) fn map(row: SqliteRow) -> Result<AssertionRow, StoreError> {
    let quantity = read_quantity(&row)?;
    let uid = row.try_get("uid")?;
    let subject_uid = row.try_get("subject_uid")?;
    let predicate_uid = row.try_get("predicate_uid")?;
    let predicate_revision = row.try_get("predicate_revision")?;
    let object_uid = row.try_get("object_uid")?;
    let object_uri: Option<String> = row.try_get("object_uri")?;
    let resolved_object_uid = if object_uri.is_some() {
        match row.try_get("target_uid") {
            Err(sqlx::Error::ColumnNotFound(_)) => None,
            value => value?,
        }
    } else {
        None
    };
    Ok(AssertionRow {
        uid,
        subject_uid,
        predicate_uid,
        predicate_revision,
        object_uid,
        object_uri,
        resolved_object_uid,
        role: row.try_get("role")?,
        quantity,
        unit_uid: row.try_get("unit_uid")?,
        unit_revision: row.try_get("unit_revision")?,
        asserted_by: row.try_get("asserted_by")?,
        created_at: row.try_get("created_at")?,
        retracted_at: row.try_get("retracted_at")?,
        retracted_by: row.try_get("retracted_by")?,
    })
}

/// Borrowed fields for authoring an Assertion.
pub struct NewAssertion<'a> {
    pub subject_uid: &'a str,
    pub predicate_uid: &'a str,
    pub object_uid: Option<&'a str>,
    pub role: AssertionRole,
    pub quantity: Option<DecimalValue>,
    pub unit_uid: Option<&'a str>,
    pub asserted_by: Option<&'a str>,
}

/// An optional exact quantity and its optional unit.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AssertionQuantity<'a> {
    pub quantity: Option<DecimalValue>,
    pub unit_uid: Option<&'a str>,
}
