//! This device (Ontology §11 "Profile vs device surfaces").
//!
//! The Organ is the published identity — what a contact saves, what a QR
//! encodes, what survives every device change. The Cell is one machine of it.
//! Before the split one row did both jobs, which is why `organs::local()` read
//! as "who am I" and "who authored this" interchangeably at every call site.
//!
//! A Cell is a `device`-kind Record so surfaces and Protein can see it without
//! a second mechanism, and it is inserted RAW — no `log_local`, so no op, so
//! it never travels. A Cell reaches other people exactly one way: as an entry
//! in the signed roster. Local-only settings (File Sync paths, cache sizes,
//! storage config) belong on this Record precisely because it does not sync.

use chrono::Utc;
use nucleus::RecordKind;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

pub const LOCAL_CELL_SLUG: &str = "local-cell";

#[derive(Debug, Clone, PartialEq)]
pub struct CellRecord {
    pub uid: String,
    /// The Organ this Cell is a member of.
    pub organ_uid: String,
    pub label: String,
}

fn map(row: sqlx::sqlite::SqliteRow) -> CellRecord {
    CellRecord {
        uid: row.get("uid"),
        organ_uid: row.get("organ_uid"),
        label: row.get("head"),
    }
}

/// This Cell, creating it on first call. `organ_uid` is the Organ it belongs
/// to, which must already exist — a Cell with no Organ has nothing to be a
/// member OF, and its ops would have no published identity to carry.
pub async fn ensure_local(
    pool: &SqlitePool,
    organ_uid: &str,
    label: &str,
) -> Result<CellRecord, StoreError> {
    if let Some(existing) = local(pool).await? {
        return Ok(existing);
    }
    let uid = nucleus::new_uid("r");
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                             organ_uid, created_at, updated_at)
         VALUES (?, ?, ?, ?, '', '1', 0, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(LOCAL_CELL_SLUG)
    .bind(RecordKind::Device.as_str())
    .bind(label)
    .bind(organ_uid)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    local(pool).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn local(pool: &SqlitePool) -> Result<Option<CellRecord>, StoreError> {
    Ok(sqlx::query(
        "SELECT uid, organ_uid, head FROM record WHERE slug = ? AND kind = ? LIMIT 1",
    )
    .bind(LOCAL_CELL_SLUG)
    .bind(RecordKind::Device.as_str())
    .fetch_optional(pool)
    .await?
    .map(map))
}

/// Rename this device. The label is what a roster entry shows a contact, and
/// what the owner picks a Cell out of a list by.
pub async fn set_label(pool: &SqlitePool, label: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE record SET head = ?, updated_at = ? WHERE slug = ? AND kind = ?")
        .bind(label)
        .bind(Utc::now().to_rfc3339())
        .bind(LOCAL_CELL_SLUG)
        .bind(RecordKind::Device.as_str())
        .execute(pool)
        .await?;
    Ok(())
}

/// Read a LOCAL-ONLY config namespace off this Cell's Record.
///
/// Cell config never syncs and never logs an op, which is what makes it
/// usable by a Cell that may not write (Ontology §11, C4). A relay holds
/// `relay_capabilities()` and the database refuses any op it authors — so if
/// its own discovery settings were an ordinary logged write on the shared
/// Organ Record, a relay could not configure itself at all. That is not a
/// hypothetical: it is what the write-capability trigger surfaced the day it
/// was added.
pub async fn config(
    pool: &SqlitePool,
    namespace: &str,
) -> Result<Option<serde_json::Value>, StoreError> {
    let Some(cell) = local(pool).await? else {
        return Ok(None);
    };
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT fds FROM record_extension WHERE record_uid = ? AND namespace = ?",
    )
    .bind(&cell.uid)
    .bind(namespace)
    .fetch_optional(pool)
    .await?;
    Ok(raw.and_then(|raw| serde_json::from_str(&raw).ok()))
}

/// Write a local-only config namespace. RAW — no op, so it never travels and
/// never needs a write capability.
pub async fn set_config(
    pool: &SqlitePool,
    namespace: &str,
    fds: &serde_json::Value,
) -> Result<(), StoreError> {
    let cell = local(pool)
        .await?
        .ok_or_else(|| sqlx::Error::Protocol("this Cell has no Cell Record".into()))?;
    sqlx::query(
        "INSERT INTO record_extension (record_uid, namespace, fds) VALUES (?, ?, ?)
         ON CONFLICT(record_uid, namespace)
         DO UPDATE SET fds = excluded.fds, version = version + 1",
    )
    .bind(&cell.uid)
    .bind(namespace)
    .bind(fds.to_string())
    .execute(pool)
    .await?;
    Ok(())
}
