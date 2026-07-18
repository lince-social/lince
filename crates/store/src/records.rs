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
    /// The organ (a `kind='organ'` record) this record originated from —
    /// `None` means "no known origin" (e.g. created before this column, or
    /// never stamped). Lets Protein filter records by organ (`organ_eq` /
    /// `organ_in`) and lets Sync/File Sync select WHAT travels where by
    /// pointing at a Protein instead of a hardcoded rule.
    pub organ_uid: Option<String>,
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
        organ_uid: r.get("organ_uid"),
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
    // Stamp the origin organ (this Cell) so Protein/Sync can filter by it
    // later — centralized here so EVERY create path gets it (threads,
    // messages, saved Proteins, ...), not just the top-level CreateRecord
    // action. No local organ yet (early bootstrap, most unit tests, and the
    // organ-bootstrap insert itself, which bypasses this fn) = no stamp,
    // origin stays "unknown" rather than erroring.
    if let Some(organ) = crate::organs::local(pool).await? {
        set_organ_origin(pool, &uid, Some(&organ.uid)).await?;
    }
    get(pool, &uid).await.map(|r| r.expect("just inserted"))
}

pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<RecordRow>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM record WHERE uid = ? AND deleted_at IS NULL")
            .bind(uid)
            .fetch_optional(pool)
            .await?
            .map(map_row),
    )
}

/// Resolve `@token`: slug first, uid fallback.
pub async fn resolve(pool: &SqlitePool, token: &str) -> Result<Option<RecordRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM record WHERE (slug = ? OR uid = ?) AND deleted_at IS NULL LIMIT 1",
    )
    .bind(token)
    .bind(token)
    .fetch_optional(pool)
    .await?
    .map(map_row))
}

pub async fn quantity(pool: &SqlitePool, uid: &str) -> Result<Option<f64>, StoreError> {
    Ok(
        sqlx::query("SELECT quantity FROM record WHERE uid = ? AND deleted_at IS NULL")
            .bind(uid)
            .fetch_optional(pool)
            .await?
            .map(|r| r.get::<f64, _>("quantity")),
    )
}

/// ISO timestamp a record was created — threads/messages surface this so a
/// Record-style UI can show "when" without RecordRow carrying it
/// everywhere (most callers never need it).
pub async fn created_at(pool: &SqlitePool, uid: &str) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query("SELECT created_at FROM record WHERE uid = ? AND deleted_at IS NULL")
            .bind(uid)
            .fetch_optional(pool)
            .await?
            .map(|r| r.get::<String, _>("created_at")),
    )
}

/// HARD delete = tombstone (2026-07-17), DISTINCT from `deactivate` (quantity
/// -> 0). The row stays (uids/provenance stay resolvable in the Ledger's
/// history) but no read path returns it again; the UNIQUE slug is freed for
/// reuse. Facts are never touched — the hash chain stays verifiable.
pub async fn mark_deleted(pool: &SqlitePool, uid: &str) -> Result<bool, StoreError> {
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query(
        "UPDATE record SET deleted_at = ?, slug = NULL, updated_at = ?
         WHERE uid = ? AND deleted_at IS NULL",
    )
    .bind(&now)
    .bind(&now)
    .bind(uid)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Every record, oldest first — the Protein `source: record` base set.
pub async fn list_all(pool: &SqlitePool) -> Result<Vec<RecordRow>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM record WHERE deleted_at IS NULL ORDER BY created_at, uid")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(map_row)
            .collect(),
    )
}

/// Active Needs in stable tie-break order (oldest first) — the focus-queue
/// candidate set (blueprint Window 1b). Window-based urgency joins in later
/// with Promises; created_at is the final tie-break already.
pub async fn active_needs(pool: &SqlitePool) -> Result<Vec<RecordRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM record
         WHERE quantity < 0 AND kind = 'plain' AND deleted_at IS NULL
         ORDER BY created_at, uid",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_row)
    .collect())
}

/// (uid, quantity) of every record — checkpoint sweep input (blueprint II.2).
pub async fn all_levels(pool: &SqlitePool) -> Result<Vec<(String, f64)>, StoreError> {
    Ok(sqlx::query("SELECT uid, quantity FROM record WHERE deleted_at IS NULL")
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
    Ok(
        sqlx::query("SELECT fds FROM record_extension WHERE record_uid = ? AND namespace = ?")
            .bind(record_uid)
            .bind(namespace)
            .fetch_optional(pool)
            .await?
            .and_then(|r| serde_json::from_str(&r.get::<String, _>("fds")).ok()),
    )
}

/// Edit a record's text (head/title and/or body). Not the quantity cache, so a
/// plain `UPDATE` is allowed; provenance/live-refresh is the engine's job via an
/// annotation fact. `None` leaves a field untouched.
pub async fn set_text(
    pool: &SqlitePool,
    uid: &str,
    head: Option<&str>,
    body: Option<&str>,
) -> Result<(), StoreError> {
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query(
        "UPDATE record
           SET head = COALESCE(?, head),
               body = COALESCE(?, body),
               updated_at = ?
         WHERE uid = ?",
    )
    .bind(head)
    .bind(body)
    .bind(&now)
    .bind(uid)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}

/// Rename a record's slug (uniqueness is enforced by the `record.slug` UNIQUE
/// index; an empty slug clears it).
pub async fn set_slug(pool: &SqlitePool, uid: &str, slug: Option<&str>) -> Result<(), StoreError> {
    if let Some(slug) = slug {
        if !nucleus::valid_slug(slug) {
            return Err(sqlx::Error::Protocol(format!("invalid slug `{slug}`")));
        }
    }
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query("UPDATE record SET slug = ?, updated_at = ? WHERE uid = ?")
        .bind(slug)
        .bind(&now)
        .bind(uid)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}

/// Set (or clear, with `None`) the record's Lingua concept classification.
pub async fn set_concept(
    pool: &SqlitePool,
    uid: &str,
    concept_uid: Option<&str>,
) -> Result<(), StoreError> {
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query("UPDATE record SET concept_uid = ?, updated_at = ? WHERE uid = ?")
        .bind(concept_uid)
        .bind(&now)
        .bind(uid)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}

/// Set (or clear, with `None`) the record's origin organ — stamped locally on
/// creation (the local organ) or carried through Sync (the true origin, so
/// lineage survives relaying through an intermediate organ).
pub async fn set_organ_origin(
    pool: &SqlitePool,
    uid: &str,
    organ_uid: Option<&str>,
) -> Result<(), StoreError> {
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query("UPDATE record SET organ_uid = ?, updated_at = ? WHERE uid = ?")
        .bind(organ_uid)
        .bind(&now)
        .bind(uid)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}

/// Set (or clear, with `None`) the record's unit-of-measure concept.
pub async fn set_unit(
    pool: &SqlitePool,
    uid: &str,
    unit_uid: Option<&str>,
) -> Result<(), StoreError> {
    let now = Utc::now().to_rfc3339();
    let res = sqlx::query("UPDATE record SET unit_uid = ?, updated_at = ? WHERE uid = ?")
        .bind(unit_uid)
        .bind(&now)
        .bind(uid)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}

/// The single writer of the quantity cache — called only from engine::append
/// inside the fact transaction.
pub async fn bump_quantity(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    delta: f64,
    now_rfc3339: &str,
) -> Result<(), StoreError> {
    let res =
        sqlx::query("UPDATE record SET quantity = quantity + ?, updated_at = ? WHERE uid = ?")
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
