use std::collections::BTreeSet;
use std::io::{self, Write};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;
use sqlx::SqliteConnection;

use crate::StoreError;

pub const MAX_OUTCOME_BYTES: usize = 1024 * 1024;
pub const MAX_AFFECTED_RECORDS: usize = 4096;
pub const MAX_AFFECTED_RECORD_BYTES: usize = MAX_AFFECTED_RECORDS * 28;
pub const MAX_ACCEPTED_AT_BYTES: usize = 64;

#[derive(Debug, Clone, Copy)]
pub struct OperationReceiptKey<'a> {
    pub organ_uid: &'a str,
    pub person_uid: &'a str,
    pub operation_uid: &'a str,
}

#[derive(Debug, Clone)]
pub struct NewOperationReceipt<'a> {
    pub key: OperationReceiptKey<'a>,
    pub payload_digest: &'a [u8; 32],
    pub outcome: &'a Value,
    pub affected_record_uids: &'a [String],
    pub accepted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationReceipt {
    pub organ_uid: String,
    pub person_uid: String,
    pub operation_uid: String,
    pub payload_digest: [u8; 32],
    pub outcome: Value,
    pub affected_record_uids: Vec<String>,
    pub accepted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiptWrite {
    Stored(OperationReceipt),
    Existing(OperationReceipt),
}

struct BoundedWriter {
    bytes: Vec<u8>,
}

impl BoundedWriter {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let length = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("Operation receipt outcome size overflow"))?;
        if length > MAX_OUTCOME_BYTES {
            return Err(io::Error::other(
                "Operation receipt outcome exceeds its byte limit",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn protocol(message: impl Into<String>) -> StoreError {
    sqlx::Error::Protocol(message.into())
}

fn validate_uid(uid: &str, prefix: &str, label: &str) -> Result<(), StoreError> {
    if !nucleus::valid_uid(uid, prefix) {
        return Err(protocol(format!(
            "Operation receipt requires a canonical {label} uid"
        )));
    }
    Ok(())
}

fn validate_key(key: OperationReceiptKey<'_>) -> Result<(), StoreError> {
    validate_uid(key.organ_uid, "r", "Organ")?;
    validate_uid(key.person_uid, "r", "Person")?;
    validate_uid(key.operation_uid, "op", "operation")
}

async fn validate_scope_identity_on(
    connection: &mut SqliteConnection,
    uid: &str,
    expected_kind: &str,
) -> Result<(), StoreError> {
    let row = sqlx::query_as::<_, (String, Option<String>, i64)>(
        "SELECT typeof(kind),
                CASE
                    WHEN typeof(kind) = 'text' AND length(CAST(kind AS BLOB)) <= 32
                    THEN kind
                END,
                deleted_at IS NULL
           FROM record
          WHERE uid = ?",
    )
    .bind(uid)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or_else(|| protocol("Operation receipt scope identity is missing"))?;
    if row.0 != "text" || row.1.as_deref() != Some(expected_kind) {
        return Err(protocol(
            "Operation receipt scope identity has the wrong kind",
        ));
    }
    if row.2 == 0 {
        return Err(protocol("Operation receipt scope identity is deleted"));
    }
    Ok(())
}

async fn validate_scope_on(
    connection: &mut SqliteConnection,
    key: OperationReceiptKey<'_>,
) -> Result<(), StoreError> {
    validate_key(key)?;
    validate_scope_identity_on(connection, key.organ_uid, "organ").await?;
    validate_scope_identity_on(connection, key.person_uid, "person").await
}

fn encode_outcome(outcome: &Value) -> Result<String, StoreError> {
    if !outcome.is_object() {
        return Err(protocol("Operation receipt outcome must be a JSON object"));
    }
    let mut writer = BoundedWriter::new();
    serde_json::to_writer(&mut writer, outcome).map_err(|error| {
        protocol(format!(
            "Operation receipt outcome is not serialisable: {error}"
        ))
    })?;
    String::from_utf8(writer.bytes)
        .map_err(|error| protocol(format!("Operation receipt outcome is not UTF-8: {error}")))
}

fn decode_outcome(outcome: &str) -> Result<Value, StoreError> {
    let value: Value = serde_json::from_str(outcome).map_err(|error| {
        protocol(format!(
            "Operation receipt outcome is not valid JSON: {error}"
        ))
    })?;
    if !value.is_object() {
        return Err(protocol("Operation receipt outcome is not a JSON object"));
    }
    Ok(value)
}

fn canonical_time(value: &DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Nanos, true)
}

fn decode_time(value: &str) -> Result<DateTime<Utc>, StoreError> {
    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|error| {
            protocol(format!(
                "Operation receipt acceptance time is invalid: {error}"
            ))
        })?
        .with_timezone(&Utc);
    if canonical_time(&parsed) != value {
        return Err(protocol(
            "Operation receipt acceptance time is not canonical UTC",
        ));
    }
    Ok(parsed)
}

async fn validate_affected_on(
    connection: &mut SqliteConnection,
    affected_record_uids: &[String],
) -> Result<Vec<String>, StoreError> {
    if affected_record_uids.len() > MAX_AFFECTED_RECORDS {
        return Err(protocol(
            "Operation receipt exceeds its affected Record count limit",
        ));
    }
    let mut unique = BTreeSet::new();
    for uid in affected_record_uids {
        validate_uid(uid, "r", "affected Record")?;
        if !unique.insert(uid.clone()) {
            return Err(protocol(
                "Operation receipt affected Record identities must be unique",
            ));
        }
    }
    for uid in &unique {
        let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM record WHERE uid = ?")
            .bind(uid)
            .fetch_optional(&mut *connection)
            .await?
            .is_some();
        if !exists {
            return Err(protocol(
                "Operation receipt names a missing affected Record",
            ));
        }
    }
    Ok(unique.into_iter().collect())
}

async fn read_affected_on(
    connection: &mut SqliteConnection,
    key: OperationReceiptKey<'_>,
) -> Result<Vec<String>, StoreError> {
    let bounds = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        "SELECT COUNT(*),
                COALESCE(SUM(length(CAST(record_uid AS BLOB))), 0),
                COALESCE(MAX(length(CAST(record_uid AS BLOB))), 0),
                COALESCE(SUM(CASE WHEN typeof(record_uid) = 'text' THEN 0 ELSE 1 END), 0)
           FROM operation_receipt_record
          WHERE organ_uid = ? AND person_uid = ? AND operation_uid = ?",
    )
    .bind(key.organ_uid)
    .bind(key.person_uid)
    .bind(key.operation_uid)
    .fetch_one(&mut *connection)
    .await?;
    let count = usize::try_from(bounds.0)
        .map_err(|_| protocol("Operation receipt has an invalid affected Record count"))?;
    let total_bytes = usize::try_from(bounds.1)
        .map_err(|_| protocol("Operation receipt has an invalid affected Record byte count"))?;
    let largest_bytes = usize::try_from(bounds.2)
        .map_err(|_| protocol("Operation receipt has an invalid affected Record uid length"))?;
    if count > MAX_AFFECTED_RECORDS {
        return Err(protocol(
            "Operation receipt exceeds its affected Record count limit",
        ));
    }
    if total_bytes > MAX_AFFECTED_RECORD_BYTES || largest_bytes > 28 {
        return Err(protocol(
            "Operation receipt exceeds its affected Record byte limit",
        ));
    }
    if bounds.3 != 0 {
        return Err(protocol(
            "Operation receipt affected Record uid has an invalid storage type",
        ));
    }
    let rows = sqlx::query_scalar::<_, Option<String>>(
        "SELECT CASE
                    WHEN typeof(record_uid) = 'text'
                     AND length(CAST(record_uid AS BLOB)) <= 28
                    THEN record_uid
                END
           FROM operation_receipt_record
          WHERE organ_uid = ? AND person_uid = ? AND operation_uid = ?
          ORDER BY record_uid",
    )
    .bind(key.organ_uid)
    .bind(key.person_uid)
    .bind(key.operation_uid)
    .fetch_all(&mut *connection)
    .await?;
    if rows.len() != count {
        return Err(protocol(
            "Operation receipt affected Record count changed during its read",
        ));
    }
    let mut affected = Vec::with_capacity(count);
    for uid in rows {
        let uid =
            uid.ok_or_else(|| protocol("Operation receipt affected Record uid is unreadable"))?;
        validate_uid(&uid, "r", "affected Record")?;
        let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM record WHERE uid = ?")
            .bind(&uid)
            .fetch_optional(&mut *connection)
            .await?
            .is_some();
        if !exists {
            return Err(protocol(
                "Operation receipt names a missing affected Record",
            ));
        }
        affected.push(uid);
    }
    Ok(affected)
}

pub async fn get_on(
    connection: &mut SqliteConnection,
    key: OperationReceiptKey<'_>,
) -> Result<Option<OperationReceipt>, StoreError> {
    validate_scope_on(connection, key).await?;
    let row = sqlx::query_as::<
        _,
        (
            String,
            i64,
            String,
            i64,
            String,
            i64,
            Option<Vec<u8>>,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT typeof(payload_digest), length(payload_digest),
                typeof(outcome), length(CAST(outcome AS BLOB)),
                typeof(accepted_at), length(CAST(accepted_at AS BLOB)),
                CASE
                    WHEN typeof(payload_digest) = 'blob' AND length(payload_digest) = 32
                    THEN payload_digest
                END,
                CASE
                    WHEN typeof(outcome) = 'text' AND length(CAST(outcome AS BLOB)) <= ?
                    THEN outcome
                END,
                CASE
                    WHEN typeof(accepted_at) = 'text'
                     AND length(CAST(accepted_at AS BLOB)) <= ?
                    THEN accepted_at
                END
           FROM operation_receipt
          WHERE organ_uid = ? AND person_uid = ? AND operation_uid = ?",
    )
    .bind(i64::try_from(MAX_OUTCOME_BYTES).expect("outcome limit fits SQLite"))
    .bind(i64::try_from(MAX_ACCEPTED_AT_BYTES).expect("time limit fits SQLite"))
    .bind(key.organ_uid)
    .bind(key.person_uid)
    .bind(key.operation_uid)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row.0 != "blob" || row.1 != 32 {
        return Err(protocol("Operation receipt payload digest is corrupt"));
    }
    if row.2 != "text" {
        return Err(protocol(
            "Operation receipt outcome has an invalid storage type",
        ));
    }
    let outcome_bytes = usize::try_from(row.3)
        .map_err(|_| protocol("Operation receipt outcome has an invalid byte length"))?;
    if outcome_bytes > MAX_OUTCOME_BYTES {
        return Err(protocol("Operation receipt outcome exceeds its byte limit"));
    }
    if row.4 != "text" {
        return Err(protocol(
            "Operation receipt acceptance time has an invalid storage type",
        ));
    }
    let accepted_at_bytes = usize::try_from(row.5)
        .map_err(|_| protocol("Operation receipt acceptance time has an invalid byte length"))?;
    if accepted_at_bytes > MAX_ACCEPTED_AT_BYTES {
        return Err(protocol(
            "Operation receipt acceptance time exceeds its byte limit",
        ));
    }
    let digest = row
        .6
        .ok_or_else(|| protocol("Operation receipt payload digest is unreadable"))?
        .try_into()
        .map_err(|_| protocol("Operation receipt payload digest is not 32 bytes"))?;
    let outcome = decode_outcome(
        row.7
            .as_deref()
            .ok_or_else(|| protocol("Operation receipt outcome is unreadable"))?,
    )?;
    let accepted_at = decode_time(
        row.8
            .as_deref()
            .ok_or_else(|| protocol("Operation receipt acceptance time is unreadable"))?,
    )?;
    let affected_record_uids = read_affected_on(connection, key).await?;
    Ok(Some(OperationReceipt {
        organ_uid: key.organ_uid.to_owned(),
        person_uid: key.person_uid.to_owned(),
        operation_uid: key.operation_uid.to_owned(),
        payload_digest: digest,
        outcome,
        affected_record_uids,
        accepted_at,
    }))
}

pub async fn store_on(
    connection: &mut SqliteConnection,
    new: NewOperationReceipt<'_>,
) -> Result<ReceiptWrite, StoreError> {
    validate_scope_on(connection, new.key).await?;
    if let Some(existing) = get_on(connection, new.key).await? {
        if existing.payload_digest == *new.payload_digest {
            return Ok(ReceiptWrite::Existing(existing));
        }
        return Err(protocol(
            "Operation identity is already bound to different content",
        ));
    }
    let outcome = encode_outcome(new.outcome)?;
    let affected_record_uids = validate_affected_on(connection, new.affected_record_uids).await?;
    let accepted_at_text = canonical_time(&new.accepted_at);
    if accepted_at_text.len() > MAX_ACCEPTED_AT_BYTES {
        return Err(protocol(
            "Operation receipt acceptance time exceeds its byte limit",
        ));
    }
    sqlx::query(
        "INSERT INTO operation_receipt
            (organ_uid, person_uid, operation_uid, payload_digest, outcome, accepted_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(new.key.organ_uid)
    .bind(new.key.person_uid)
    .bind(new.key.operation_uid)
    .bind(new.payload_digest.as_slice())
    .bind(&outcome)
    .bind(&accepted_at_text)
    .execute(&mut *connection)
    .await?;
    for record_uid in &affected_record_uids {
        sqlx::query(
            "INSERT INTO operation_receipt_record
                (organ_uid, person_uid, operation_uid, record_uid)
             VALUES (?, ?, ?, ?)",
        )
        .bind(new.key.organ_uid)
        .bind(new.key.person_uid)
        .bind(new.key.operation_uid)
        .bind(record_uid)
        .execute(&mut *connection)
        .await?;
    }
    Ok(ReceiptWrite::Stored(OperationReceipt {
        organ_uid: new.key.organ_uid.to_owned(),
        person_uid: new.key.person_uid.to_owned(),
        operation_uid: new.key.operation_uid.to_owned(),
        payload_digest: *new.payload_digest,
        outcome: new.outcome.clone(),
        affected_record_uids,
        accepted_at: new.accepted_at,
    }))
}
