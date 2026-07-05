//! Record repository. NOTE: `record.quantity` is written ONLY by the engine's
//! fact appender (`bump_quantity` below is called inside that transaction and
//! nowhere else — blueprint 0.3).

use chrono::Utc;
use nucleus::RecordKind;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;

#[derive(Debug, Clone)]
pub struct RecordRow {
    pub uid: String,
    pub slug: Option<String>,
    pub kind: String,
    pub head: String,
    pub body: String,
    pub quantity: f64,
    pub concept_uid: Option<String>,
    pub unit_uid: Option<String>,
    pub place_uid: Option<String>,
}

fn map_row(r: sqlx::sqlite::SqliteRow) -> RecordRow {
    RecordRow {
        uid: r.get("uid"),
        slug: r.get("slug"),
        kind: r.get("kind"),
        head: r.get("head"),
        body: r.get("body"),
        quantity: r.get("quantity"),
        concept_uid: r.get("concept_uid"),
        unit_uid: r.get("unit_uid"),
        place_uid: r.get("place_uid"),
    }
}

pub struct NewRecord<'a> {
    pub slug: Option<&'a str>,
    pub kind: RecordKind,
    pub head: &'a str,
    pub body: &'a str,
    pub quantity: f64,
}

pub async fn create(pool: &SqlitePool, new: NewRecord<'_>) -> Result<RecordRow, StoreError> {
    if let Some(slug) = new.slug {
        if !nucleus::valid_slug(slug) {
            return Err(sqlx::Error::Protocol(format!("invalid slug `{slug}`")));
        }
    }
    let uid = nucleus::new_uid("r");
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(new.slug)
    .bind(new.kind.as_str())
    .bind(new.head)
    .bind(new.body)
    .bind(new.quantity)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    get(pool, &uid).await.map(|r| r.expect("just inserted"))
}

pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<RecordRow>, StoreError> {
    Ok(sqlx::query("SELECT * FROM record WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .map(map_row))
}

/// Resolve `@token`: slug first, uid fallback.
pub async fn resolve(pool: &SqlitePool, token: &str) -> Result<Option<RecordRow>, StoreError> {
    Ok(sqlx::query("SELECT * FROM record WHERE slug = ? OR uid = ? LIMIT 1")
        .bind(token)
        .bind(token)
        .fetch_optional(pool)
        .await?
        .map(map_row))
}

pub async fn quantity(pool: &SqlitePool, uid: &str) -> Result<Option<f64>, StoreError> {
    Ok(sqlx::query("SELECT quantity FROM record WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .map(|r| r.get::<f64, _>("quantity")))
}

/// Every record, oldest first — the Protein `source: record` base set.
pub async fn list_all(pool: &SqlitePool) -> Result<Vec<RecordRow>, StoreError> {
    Ok(sqlx::query("SELECT * FROM record ORDER BY created_at, uid")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map_row)
        .collect())
}

/// Active Needs in stable tie-break order (oldest first) — the focus-queue
/// candidate set (blueprint Window 1b). Window-based urgency joins in later
/// with Promises; created_at is the final tie-break already.
pub async fn active_needs(pool: &SqlitePool) -> Result<Vec<RecordRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM record WHERE quantity < 0 AND kind = 'plain' ORDER BY created_at, uid",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_row)
    .collect())
}

/// (uid, quantity) of every record — checkpoint sweep input (blueprint II.2).
pub async fn all_levels(pool: &SqlitePool) -> Result<Vec<(String, f64)>, StoreError> {
    Ok(sqlx::query("SELECT uid, quantity FROM record")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| (r.get("uid"), r.get("quantity")))
        .collect())
}

/// Namespaced fds sidecar (blueprint I.2) — also where saved Proteins live
/// (`namespace = "lince.protein"`).
pub async fn set_extension(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
    fds: &serde_json::Value,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO record_extension (record_uid, namespace, fds) VALUES (?, ?, ?)
         ON CONFLICT(record_uid, namespace)
         DO UPDATE SET fds = excluded.fds, version = version + 1",
    )
    .bind(record_uid)
    .bind(namespace)
    .bind(fds.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_extension(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
) -> Result<Option<serde_json::Value>, StoreError> {
    Ok(sqlx::query("SELECT fds FROM record_extension WHERE record_uid = ? AND namespace = ?")
        .bind(record_uid)
        .bind(namespace)
        .fetch_optional(pool)
        .await?
        .and_then(|r| serde_json::from_str(&r.get::<String, _>("fds")).ok()))
}

/// The single writer of the quantity cache — called only from engine::append
/// inside the fact transaction.
pub async fn bump_quantity(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    delta: f64,
    now_rfc3339: &str,
) -> Result<(), StoreError> {
    let res = sqlx::query("UPDATE record SET quantity = quantity + ?, updated_at = ? WHERE uid = ?")
        .bind(delta)
        .bind(now_rfc3339)
        .bind(uid)
        .execute(&mut **tx)
        .await?;
    if res.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}
