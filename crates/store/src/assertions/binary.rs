use std::collections::BTreeSet;

use nucleus::DecimalValue;
use serde_json::to_string;
use sqlx::{AssertSqlSafe, QueryBuilder, Row, SqlitePool, query, sqlite::SqliteRow};

use super::model::read_quantity;
use crate::StoreError;

/// An active Assertion with a resolved Record at both ends.
pub struct BinaryAssertionRow {
    pub uid: String,
    pub subject_uid: String,
    pub object_uid: String,
    pub predicate_uid: String,
    pub predicate: String,
    pub quantity: Option<DecimalValue>,
    pub unit_uid: Option<String>,
    pub created_at: String,
}

fn map_binary(row: SqliteRow) -> Result<BinaryAssertionRow, StoreError> {
    let quantity = read_quantity(&row)?;
    Ok(BinaryAssertionRow {
        uid: row.get("uid"),
        subject_uid: row.get("subject_uid"),
        object_uid: row.get("object_uid"),
        predicate_uid: row.get("predicate_uid"),
        predicate: row.get("predicate"),
        quantity,
        unit_uid: row.get("unit_uid"),
        created_at: row.get("created_at"),
    })
}

const BINARY_SELECT: &str =
    "SELECT a.uid, a.subject_uid, a.target_uid AS object_uid, a.predicate_uid,
            c.canonical_name AS predicate, a.quantity_mantissa,
            a.quantity_scale, a.unit_uid, a.created_at
       FROM record_assertion_link a JOIN concept c ON c.uid = a.predicate_uid
      WHERE a.target_uid IS NOT NULL AND a.retracted_at IS NULL";

/// Lists binary Assertions for a predicate UID or vocabulary feature.
pub async fn binary_of_predicate(
    pool: &SqlitePool,
    predicate_uid: &str,
) -> Result<Vec<BinaryAssertionRow>, StoreError> {
    let sql = format!(
        "{BINARY_SELECT} AND (a.predicate_uid = ? OR a.predicate_uid IN (SELECT c.concept_uid FROM vocabulary_feature f JOIN vocabulary_role_concept c ON c.role_uid=f.role_uid WHERE f.feature=?)) ORDER BY a.created_at, a.uid"
    );
    query(AssertSqlSafe(sql.as_str()))
        .bind(predicate_uid)
        .bind(predicate_uid.strip_prefix("feature:"))
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map_binary)
        .collect()
}

/// Lists binary Assertions for the given predicate UIDs in creation order.
pub async fn binary_of_predicates(
    pool: &SqlitePool,
    predicate_uids: &[String],
) -> Result<Vec<BinaryAssertionRow>, StoreError> {
    binary_selection(pool, predicate_uids, None).await
}

/// Lists binary Assertions for the given subjects and predicate UIDs.
pub async fn binary_for_subjects(
    pool: &SqlitePool,
    predicate_uids: &[String],
    subjects: &[&str],
) -> Result<Vec<BinaryAssertionRow>, StoreError> {
    if subjects.is_empty() {
        return Ok(Vec::new());
    }
    let subjects = to_string(subjects).map_err(|error| StoreError::Protocol(error.to_string()))?;
    binary_selection(pool, predicate_uids, Some(&subjects)).await
}

async fn binary_selection(
    pool: &SqlitePool,
    predicate_uids: &[String],
    subjects: Option<&str>,
) -> Result<Vec<BinaryAssertionRow>, StoreError> {
    let unique: BTreeSet<_> = predicate_uids.iter().collect();
    let unique: Vec<_> = unique.into_iter().collect();
    let mut out = Vec::new();
    for batch in unique.chunks(400) {
        let mut query = QueryBuilder::new(format!("{BINARY_SELECT} AND a.predicate_uid IN ("));
        let mut values = query.separated(",");
        for uid in batch {
            values.push_bind(*uid);
        }
        query.push(")");
        if let Some(subjects) = subjects {
            query
                .push(" AND a.subject_uid IN (SELECT value FROM json_each(")
                .push_bind(subjects)
                .push("))");
        }
        for row in query.build().fetch_all(pool).await? {
            out.push(map_binary(row)?);
        }
    }
    out.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.uid.cmp(&b.uid))
    });
    Ok(out)
}

/// Lists active binary Assertions in creation order, breaking ties by UID.
pub async fn all_binary(pool: &SqlitePool) -> Result<Vec<BinaryAssertionRow>, StoreError> {
    let sql = format!("{BINARY_SELECT} ORDER BY a.created_at, a.uid");
    query(AssertSqlSafe(sql.as_str()))
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map_binary)
        .collect()
}

/// Lists the predicates in use by active binary Assertions.
pub async fn used_binary_predicates(
    pool: &SqlitePool,
) -> Result<Vec<(String, String)>, StoreError> {
    Ok(query(
        "SELECT c.uid, c.canonical_name
           FROM concept c JOIN record_assertion a ON a.predicate_uid = c.uid
          WHERE a.target_uid IS NOT NULL AND a.retracted_at IS NULL
          GROUP BY c.uid, c.canonical_name ORDER BY c.canonical_name, c.uid",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| (row.get("uid"), row.get("canonical_name")))
    .collect())
}
