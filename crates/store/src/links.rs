//! Link repository (blueprint Part IV). Identity is the TRIPLE (from, kind, to),
//! so the same two records carry many links of different kinds — `@part-of` and
//! `@before` between the same pair coexist, each kind its own graph.

use chrono::Utc;
use nucleus::graph::Edge;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

pub async fn add(
    pool: &SqlitePool,
    from_uid: &str,
    kind_uid: &str,
    to_uid: &str,
    quantity: Option<f64>,
) -> Result<String, StoreError> {
    let uid = nucleus::new_uid("l");
    sqlx::query(
        "INSERT INTO link (uid, from_uid, kind_uid, to_uid, quantity, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(from_uid)
    .bind(kind_uid)
    .bind(to_uid)
    .bind(quantity)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(uid)
}

pub async fn remove(
    pool: &SqlitePool,
    from_uid: &str,
    kind_uid: &str,
    to_uid: &str,
) -> Result<bool, StoreError> {
    let res = sqlx::query("DELETE FROM link WHERE from_uid = ? AND kind_uid = ? AND to_uid = ?")
        .bind(from_uid)
        .bind(kind_uid)
        .bind(to_uid)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
}

/// All edges of one kind, as the pure-graph shape `nucleus::graph` walks.
pub async fn edges_of_kind(pool: &SqlitePool, kind_uid: &str) -> Result<Vec<Edge>, StoreError> {
    Ok(sqlx::query("SELECT from_uid, to_uid, quantity FROM link WHERE kind_uid = ?")
        .bind(kind_uid)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| Edge {
            from: r.get("from_uid"),
            to: r.get("to_uid"),
            quantity: r.get("quantity"),
        })
        .collect())
}

/// Edges of every kind in a set (e.g. `@precedes` and all its child kinds).
pub async fn edges_of_kinds(
    pool: &SqlitePool,
    kind_uids: &[String],
) -> Result<Vec<Edge>, StoreError> {
    let mut out = Vec::new();
    for kind in kind_uids {
        out.extend(edges_of_kind(pool, kind).await?);
    }
    Ok(out)
}
