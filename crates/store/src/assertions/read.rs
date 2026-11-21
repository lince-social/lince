use std::{
    collections::{BTreeMap, HashSet},
    iter::repeat_n,
};

use nucleus::DecimalValue;
use sqlx::{AssertSqlSafe, Row, SqlitePool, query};

use super::{
    AssertionRow,
    model::{map, read_quantity},
};
use crate::StoreError;

/// Reads an Assertion by UID, including retracted Assertions.
pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<AssertionRow>, StoreError> {
    query("SELECT * FROM record_assertion_link WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .map(map)
        .transpose()
}

/// Lists active Assertions in creation order, breaking ties by UID.
pub async fn list_active(pool: &SqlitePool) -> Result<Vec<AssertionRow>, StoreError> {
    query("SELECT * FROM record_assertion_link WHERE retracted_at IS NULL ORDER BY created_at, uid")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map)
        .collect()
}

/// Lists distinct active Assertions incident to the selected Records, ordered by UID.
pub async fn incident_to(
    pool: &SqlitePool,
    record_uids: &[impl AsRef<str>],
) -> Result<Vec<AssertionRow>, StoreError> {
    let mut assertions = BTreeMap::new();
    for chunk in record_uids.chunks(400) {
        let holes = repeat_n("?", chunk.len()).collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT * FROM record_assertion_link WHERE retracted_at IS NULL AND (subject_uid IN ({holes}) OR target_uid IN ({holes})) ORDER BY uid"
        );
        let mut query = query(AssertSqlSafe(sql.as_str()));
        for uid in chunk.iter().chain(chunk.iter()) {
            query = query.bind(uid.as_ref());
        }
        for row in query.fetch_all(pool).await? {
            let assertion = map(row)?;
            assertions.insert(assertion.uid.clone(), assertion);
        }
    }
    Ok(assertions.into_values().collect())
}

/// Finds the Record's current identity predicate.
pub async fn identity_concept(
    pool: &SqlitePool,
    subject_uid: &str,
) -> Result<Option<String>, StoreError> {
    Ok(query(
        "SELECT predicate_uid FROM record_assertion
          WHERE subject_uid = ? AND role = 'identity' AND retracted_at IS NULL",
    )
    .bind(subject_uid)
    .fetch_optional(pool)
    .await?
    .map(|row| row.get("predicate_uid")))
}

/// Lists the distinct active predicate UIDs of a Record.
pub async fn concepts_for_record(
    pool: &SqlitePool,
    subject_uid: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(query(
        "SELECT DISTINCT predicate_uid FROM record_assertion
          WHERE subject_uid = ? AND retracted_at IS NULL
          ORDER BY predicate_uid",
    )
    .bind(subject_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| row.get("predicate_uid"))
    .collect())
}

/// Filters active Assertions to the supplied predicate UIDs.
pub async fn active_of_predicates(
    pool: &SqlitePool,
    predicate_uids: &[String],
) -> Result<Vec<AssertionRow>, StoreError> {
    let wanted: HashSet<&str> = predicate_uids.iter().map(String::as_str).collect();
    Ok(list_active(pool)
        .await?
        .into_iter()
        .filter(|row| wanted.contains(row.predicate_uid.as_str()))
        .collect())
}

/// Projects active Assertions for the selected subjects in display order.
pub async fn for_subjects(
    pool: &SqlitePool,
    subject_uids: &[String],
) -> Result<Vec<ProjectedAssertion>, StoreError> {
    if subject_uids.is_empty() {
        return Ok(Vec::new());
    }
    let holes = repeat_n("?", subject_uids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT a.uid, a.subject_uid, a.object_uid, a.object_uri, a.target_uid, a.predicate_uid, a.role,
                c.canonical_name AS predicate, a.quantity_mantissa,
                a.quantity_scale, a.unit_uid
           FROM record_assertion_link a JOIN concept c ON c.uid = a.predicate_uid
          WHERE a.retracted_at IS NULL AND a.subject_uid IN ({holes})
          ORDER BY a.subject_uid, c.canonical_name, a.object_uid, a.uid"
    );
    let mut query = query(AssertSqlSafe(sql.as_str()));
    for uid in subject_uids {
        query = query.bind(uid);
    }
    query
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| {
            let quantity = read_quantity(&row)?;
            Ok(ProjectedAssertion {
                uid: row.get("uid"),
                role: row.get("role"),
                subject_uid: row.get("subject_uid"),
                predicate_uid: row.get("predicate_uid"),
                predicate: row.get("predicate"),
                object_uid: row.get("object_uid"),
                object_uri: row.get("object_uri"),
                resolved_object_uid: if row.get::<Option<&str>, _>("object_uri").is_some() {
                    row.get("target_uid")
                } else {
                    None
                },
                quantity,
                unit_uid: row.get("unit_uid"),
            })
        })
        .collect()
}

/// An Assertion with its predicate name and resolved endpoint for display.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectedAssertion {
    pub uid: String,
    pub role: String,
    pub subject_uid: String,
    pub predicate_uid: String,
    pub predicate: String,
    pub object_uid: Option<String>,
    pub object_uri: Option<String>,
    pub resolved_object_uid: Option<String>,
    pub quantity: Option<DecimalValue>,
    pub unit_uid: Option<String>,
}
