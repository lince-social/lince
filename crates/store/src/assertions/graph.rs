use std::{
    collections::{HashSet, VecDeque},
    iter::repeat_n,
};

use nucleus::{DecimalValue, graph::Edge};
use sqlx::{AssertSqlSafe, Row, SqlitePool, query};

use super::binary_of_predicate;
use crate::StoreError;

/// Projects binary Assertions for a predicate or feature as graph edges.
pub async fn edges_of_predicate(
    pool: &SqlitePool,
    predicate_uid: &str,
) -> Result<Vec<Edge>, StoreError> {
    Ok(binary_of_predicate(pool, predicate_uid)
        .await?
        .into_iter()
        .map(|row| Edge {
            from: row.subject_uid,
            to: row.object_uid,
            quantity: row.quantity.map(DecimalValue::to_f64),
        })
        .collect())
}

/// Concatenates graph edges in caller-supplied predicate order.
pub async fn edges_of_predicates(
    pool: &SqlitePool,
    predicate_uids: &[String],
) -> Result<Vec<Edge>, StoreError> {
    let mut out = Vec::new();
    for predicate_uid in predicate_uids {
        out.extend(edges_of_predicate(pool, predicate_uid).await?);
    }
    Ok(out)
}

/// Visits distinct incoming relatives in breadth-first order. Optionally includes
/// the root before its descendants.
///
/// # Errors
/// Propagates database errors while reading the family links.
pub async fn record_descendants(
    pool: &SqlitePool,
    root_uid: &str,
    family: &[String],
    include_self: bool,
) -> Result<Vec<String>, StoreError> {
    if family.is_empty() {
        return Ok(if include_self {
            vec![root_uid.to_string()]
        } else {
            Vec::new()
        });
    }
    let placeholders = repeat_n("?", family.len()).collect::<Vec<_>>().join(", ");
    let sql = format!(
        "SELECT subject_uid FROM record_assertion_link
          WHERE target_uid = ? AND predicate_uid IN ({placeholders})"
    );

    let mut seen: HashSet<String> = HashSet::from([root_uid.to_string()]);
    let mut queue: VecDeque<String> = VecDeque::from([root_uid.to_string()]);
    let mut out = Vec::new();
    while let Some(current) = queue.pop_front() {
        let mut query = query(AssertSqlSafe(sql.as_str())).bind(&current);
        for uid in family {
            query = query.bind(uid);
        }
        for row in query.fetch_all(pool).await? {
            let uid: String = row.get("subject_uid");
            if seen.insert(uid.clone()) {
                queue.push_back(uid);
            }
        }
        if include_self || current != root_uid {
            out.push(current);
        }
    }
    Ok(out)
}
