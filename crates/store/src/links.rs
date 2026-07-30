//! Link repository (blueprint Part IV). Identity is the TRIPLE (from, kind, to),
//! so the same two records carry many links of different kinds — `@part-of` and
//! `@before` between the same pair coexist, each kind its own graph.

use chrono::Utc;
use nucleus::graph::Edge;
use sqlx::{Row, SqlitePool};

use crate::StoreError;
use crate::records::RecordRow;

#[derive(Debug, Clone, PartialEq)]
pub struct LinkRow {
    pub uid: String,
    pub from: String,
    pub to: String,
    pub kind_uid: String,
    pub kind: String,
    pub quantity: Option<f64>,
    pub created_at: String,
}

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

fn map_link_row(r: sqlx::sqlite::SqliteRow) -> LinkRow {
    LinkRow {
        uid: r.get("uid"),
        from: r.get("from_uid"),
        to: r.get("to_uid"),
        kind_uid: r.get("kind_uid"),
        kind: r.get("kind"),
        quantity: r.get("quantity"),
        created_at: r.get("created_at"),
    }
}

pub async fn links_of_kind(pool: &SqlitePool, kind_uid: &str) -> Result<Vec<LinkRow>, StoreError> {
    Ok(sqlx::query(
        "
        SELECT l.uid, l.from_uid, l.to_uid, l.kind_uid, c.canonical_name AS kind,
               l.quantity, l.created_at
          FROM link l
          JOIN concept c ON c.uid = l.kind_uid
         WHERE l.kind_uid = ?
         ORDER BY l.created_at, l.uid
        ",
    )
    .bind(kind_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_link_row)
    .collect())
}

pub async fn links_of_kinds(
    pool: &SqlitePool,
    kind_uids: &[String],
) -> Result<Vec<LinkRow>, StoreError> {
    let mut out = Vec::new();
    for kind_uid in kind_uids {
        out.extend(links_of_kind(pool, kind_uid).await?);
    }
    Ok(out)
}

/// Every link in the store, any kind — backs the links-include `*` wildcard
/// (Record's "show me ALL of this record's links").
pub async fn all_links(pool: &SqlitePool) -> Result<Vec<LinkRow>, StoreError> {
    Ok(sqlx::query(
        "
        SELECT l.uid, l.from_uid, l.to_uid, l.kind_uid, c.canonical_name AS kind,
               l.quantity, l.created_at
          FROM link l
          JOIN concept c ON c.uid = l.kind_uid
         ORDER BY l.created_at, l.uid
        ",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_link_row)
    .collect())
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

pub async fn remove_kind_within_set(
    pool: &SqlitePool,
    kind_uid: &str,
    record_uids: &[String],
) -> Result<u64, StoreError> {
    let mut affected = 0;
    for from in record_uids {
        for to in record_uids {
            if from == to {
                continue;
            }
            let res =
                sqlx::query("DELETE FROM link WHERE from_uid = ? AND kind_uid = ? AND to_uid = ?")
                    .bind(from)
                    .bind(kind_uid)
                    .bind(to)
                    .execute(pool)
                    .await?;
            affected += res.rows_affected();
        }
    }
    Ok(affected)
}

fn map_record_row(r: sqlx::sqlite::SqliteRow) -> Result<RecordRow, StoreError> {
    Ok(RecordRow {
        uid: r.get("uid"),
        slug: r.get("slug"),
        kind: r.get("kind"),
        head: r.get("head"),
        body: r.get("body"),
        quantity: crate::exact::read_decimal(&r, "quantity")?,
        concept_uid: r.get("concept_uid"),
        unit_uid: r.get("unit_uid"),
        place_uid: r.get("place_uid"),
        organ_uid: r.get("organ_uid"),
    })
}

/// Records linked as `record --kind--> target`.
pub async fn records_to(
    pool: &SqlitePool,
    kind_uid: &str,
    target_uid: &str,
) -> Result<Vec<RecordRow>, StoreError> {
    sqlx::query(
        "SELECT r.*
           FROM link l
           JOIN record r ON r.uid = l.from_uid
          WHERE l.kind_uid = ? AND l.to_uid = ?
          ORDER BY l.created_at, l.uid",
    )
    .bind(kind_uid)
    .bind(target_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_record_row)
    .collect()
}

/// Records linked as `source --kind--> record`.
pub async fn records_from(
    pool: &SqlitePool,
    source_uid: &str,
    kind_uid: &str,
) -> Result<Vec<RecordRow>, StoreError> {
    sqlx::query(
        "SELECT r.*
           FROM link l
           JOIN record r ON r.uid = l.to_uid
          WHERE l.from_uid = ? AND l.kind_uid = ?
          ORDER BY l.created_at, l.uid",
    )
    .bind(source_uid)
    .bind(kind_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_record_row)
    .collect()
}

/// All edges of one kind, as the pure-graph shape `nucleus::graph` walks.
pub async fn edges_of_kind(pool: &SqlitePool, kind_uid: &str) -> Result<Vec<Edge>, StoreError> {
    Ok(
        sqlx::query("SELECT from_uid, to_uid, quantity FROM link WHERE kind_uid = ?")
            .bind(kind_uid)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|r| Edge {
                from: r.get("from_uid"),
                to: r.get("to_uid"),
                quantity: r.get("quantity"),
            })
            .collect(),
    )
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
