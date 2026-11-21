use crate::{
    StoreError,
    exact::one,
    records::{NewRecord, create},
};
use nucleus::{
    RecordKind,
    execution::now,
    performance::{Span, add},
};
use serde_json::Value;
use sqlx::{Row, SqlitePool, query, query_as, query_scalar, sqlite::SqliteRow};
use std::collections::HashSet;

/// Creates a decision Record and saves its subject, kind, and options.
///
/// # Errors
///
/// Returns an error if the Record or decision details cannot be stored.
pub async fn create_decision(
    pool: &SqlitePool,
    subject_uid: &str,
    kind: &str,
    question: &str,
    options: &Value,
) -> Result<String, StoreError> {
    let rec = create(
        pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Decision,
            head: question,
            body: "",
            quantity: one(),
        },
    )
    .await?;
    query("INSERT INTO decision (record_uid, subject_uid, kind, options) VALUES (?, ?, ?, ?)")
        .bind(&rec.uid)
        .bind(subject_uid)
        .bind(kind)
        .bind(options.to_string())
        .execute(pool)
        .await?;
    Ok(rec.uid)
}

/// Creates a decision and saves its expiry timestamp.
///
/// # Errors
///
/// Returns an error if creation or the expiry update fails.
pub async fn create_decision_expiring(
    pool: &SqlitePool,
    subject_uid: &str,
    kind: &str,
    question: &str,
    options: &Value,
    expires_at: &str,
) -> Result<String, StoreError> {
    let uid = create_decision(pool, subject_uid, kind, question, options).await?;
    query("UPDATE decision SET expires_at = ? WHERE record_uid = ?")
        .bind(expires_at)
        .bind(&uid)
        .execute(pool)
        .await?;
    Ok(uid)
}

/// Reads open decision UIDs whose expiry precedes the supplied time.
///
/// # Errors
///
/// Returns an error if the read fails.
pub async fn expired_open_decisions(
    pool: &SqlitePool,
    now_rfc3339: &str,
) -> Result<Vec<String>, StoreError> {
    query_scalar(
        "SELECT d.record_uid FROM decision d
         JOIN record r ON r.uid = d.record_uid
         WHERE r.quantity_mantissa != '0' AND d.expires_at IS NOT NULL AND d.expires_at < ?",
    )
    .bind(now_rfc3339)
    .fetch_all(pool)
    .await
}

/// Collects subject UID and kind pairs from open decisions.
///
/// # Errors
///
/// Returns an error if the read fails.
pub async fn open_decision_subjects(
    pool: &SqlitePool,
) -> Result<HashSet<(String, String)>, StoreError> {
    let rows = query_as(
        "SELECT d.subject_uid, d.kind FROM decision d
         JOIN record r ON r.uid = d.record_uid WHERE r.quantity_mantissa != '0'",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Reads each open decision’s Record UID, kind, and question.
///
/// # Errors
///
/// Returns an error if the read fails.
pub async fn open_decisions(
    pool: &SqlitePool,
) -> Result<Vec<(String, String, String)>, StoreError> {
    query_as(
        "SELECT d.record_uid, d.kind, r.head FROM decision d
         JOIN record r ON r.uid = d.record_uid WHERE r.quantity_mantissa != '0'",
    )
    .fetch_all(pool)
    .await
}

/// Stored decision details and its current state.
#[derive(Debug, Clone)]
pub struct DecisionRow {
    /// Decision Record UID.
    pub record_uid: String,
    /// Subject Record UID.
    pub subject_uid: String,
    /// Decision type.
    pub kind: String,
    /// Question from the Record title.
    pub question: String,
    /// Decoded options.
    pub options: Value,
    /// Whether the Record quantity is nonzero.
    pub open: bool,
    /// Retained answer, if present.
    pub answer: Option<String>,
}

/// Lists decodable decisions in Record creation order.
///
/// # Errors
///
/// Returns an error if the read fails.
pub async fn list_decisions(pool: &SqlitePool) -> Result<Vec<DecisionRow>, StoreError> {
    Ok(query(
        "SELECT d.record_uid, d.subject_uid, d.kind, d.options, d.answer, r.head, r.quantity_mantissa
         FROM decision d JOIN record r ON r.uid = d.record_uid ORDER BY r.created_at",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .filter_map(|r| {
        let options = decision_options(&r)?;
        Some(DecisionRow {
            record_uid: r.try_get("record_uid").ok()?,
            subject_uid: r.try_get("subject_uid").ok()?,
            kind: r.try_get("kind").ok()?,
            question: r.try_get("head").ok()?,
            options,
            open: r.try_get::<&str, _>("quantity_mantissa").ok()? != "0",
            answer: r.try_get("answer").ok()?,
        })
    })
    .collect())
}

fn decision_options(row: &SqliteRow) -> Option<Value> {
    let raw: &str = row.try_get("options").ok()?;
    serde_json::from_str(raw).ok()
}

/// Details of one open decision.
#[derive(Debug)]
pub struct OpenDecision {
    /// Decision Record UID.
    pub record_uid: String,
    /// Subject Record UID.
    pub subject_uid: String,
    /// Decision type.
    pub kind: String,
    /// Question from the Record title.
    pub question: String,
    /// Decoded options.
    pub options: Value,
}

/// Lists decodable open decisions in Record creation order.
///
/// # Errors
///
/// Returns an error if the read fails.
pub async fn open_decision_rows(pool: &SqlitePool) -> Result<Vec<OpenDecision>, StoreError> {
    let _read = Span::new("store-open-decision-presentation-await");
    let rows = query(
        "SELECT d.record_uid, d.subject_uid, d.kind, d.options, r.head
         FROM decision d JOIN record r ON r.uid = d.record_uid
         WHERE r.quantity_mantissa <> '0' ORDER BY r.created_at",
    )
    .fetch_all(pool)
    .await?;
    let output: Vec<_> = rows
        .into_iter()
        .filter_map(|row| {
            let options = decision_options(&row)?;
            Some(OpenDecision {
                record_uid: row.try_get("record_uid").ok()?,
                subject_uid: row.try_get("subject_uid").ok()?,
                kind: row.try_get("kind").ok()?,
                question: row.try_get("head").ok()?,
                options,
            })
        })
        .collect();
    add("open-decision-presentation-rows", output.len() as u64);
    Ok(output)
}

/// Saves a decision answer and sets the decision timestamp.
///
/// # Errors
///
/// Returns an error if the update fails.
pub async fn answer_decision(
    pool: &SqlitePool,
    record_uid: &str,
    answer: &str,
) -> Result<(), StoreError> {
    query("UPDATE decision SET answer = ?, decided_at = ? WHERE record_uid = ?")
        .bind(answer)
        .bind(now().to_rfc3339())
        .bind(record_uid)
        .execute(pool)
        .await?;
    Ok(())
}
