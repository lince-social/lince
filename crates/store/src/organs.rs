use chrono::Utc;
use nucleus::RecordKind;
use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};

use crate::StoreError;

pub const LOCAL_ORGAN_SLUG: &str = "local-organ";
const LOCAL_ORGAN_EXTENSION: &str = "lince.organ";

#[derive(Debug, Clone, PartialEq)]
pub struct OrganRecord {
    pub uid: String,
    pub slug: Option<String>,
    pub head: String,
    pub body: String,
    pub base_url: String,
    pub local: bool,
}

pub async fn ensure_local(pool: &SqlitePool, base_url: &str) -> Result<OrganRecord, StoreError> {
    let base_url = normalize_base_url(base_url);
    let now = Utc::now().to_rfc3339();
    let existing_uid = sqlx::query_scalar::<_, String>("SELECT uid FROM record WHERE slug = ?")
        .bind(LOCAL_ORGAN_SLUG)
        .fetch_optional(pool)
        .await?;
    let uid = match existing_uid {
        Some(uid) => {
            sqlx::query(
                "UPDATE record
                    SET kind = ?,
                        head = ?,
                        body = ?,
                        updated_at = ?
                  WHERE uid = ?",
            )
            .bind(RecordKind::Organ.as_str())
            .bind("Local Lince")
            .bind(&base_url)
            .bind(&now)
            .bind(&uid)
            .execute(pool)
            .await?;
            uid
        }
        None => {
            let uid = nucleus::new_uid("r");
            sqlx::query(
                "INSERT INTO record (uid, slug, kind, head, body, quantity, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&uid)
            .bind(LOCAL_ORGAN_SLUG)
            .bind(RecordKind::Organ.as_str())
            .bind("Local Lince")
            .bind(&base_url)
            .bind(1.0_f64)
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await?;
            uid
        }
    };

    let fds = json!({
        "baseUrl": base_url,
        "aliases": ["http://127.0.0.1", "http://localhost"],
        "local": true
    });
    crate::records::set_extension(pool, &uid, LOCAL_ORGAN_EXTENSION, &fds).await?;
    local(pool).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn local(pool: &SqlitePool) -> Result<Option<OrganRecord>, StoreError> {
    let row = sqlx::query(
        "
        SELECT r.uid, r.slug, r.head, r.body, e.fds
        FROM record r
        LEFT JOIN record_extension e
          ON e.record_uid = r.uid AND e.namespace = ?
        WHERE r.slug = ? AND r.kind = ?
        LIMIT 1
        ",
    )
    .bind(LOCAL_ORGAN_EXTENSION)
    .bind(LOCAL_ORGAN_SLUG)
    .bind(RecordKind::Organ.as_str())
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| {
        let fds = row
            .get::<Option<String>, _>("fds")
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .unwrap_or_else(|| json!({}));
        let body = row.get::<String, _>("body");
        let base_url = fds
            .get("baseUrl")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| body.clone());
        let local = fds.get("local").and_then(Value::as_bool).unwrap_or(true);
        OrganRecord {
            uid: row.get("uid"),
            slug: row.get("slug"),
            head: row.get("head"),
            body,
            base_url,
            local,
        }
    }))
}

fn normalize_base_url(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_string()
}

// ------------------------------------------------- contacts (blueprint XV)

#[derive(Debug, Clone)]
pub struct Contact {
    pub record_uid: String,
    pub slug: Option<String>,
    pub head: String,
    pub base_url: String,
    pub trust: String, // unknown | known | blocked
    pub proximity: u32,
    pub sync_out: bool,
    pub sync_in: bool,
}

/// Register a remote organ contact: an organ record carrying the REMOTE
/// organ's own uid (identity replicates by uid, blueprint XV.2 — introduction
/// hands it over) + the contact sidecar. Idempotent by uid; a colliding slug
/// is dropped (slugs are local suggestions, never identity).
pub async fn add_contact(
    pool: &SqlitePool,
    uid: &str,
    slug: Option<&str>,
    head: &str,
    base_url: &str,
    proximity: u32,
) -> Result<String, StoreError> {
    let base_url = normalize_base_url(base_url);
    let now = Utc::now().to_rfc3339();
    if crate::records::get(pool, uid).await?.is_none() {
        let slug_taken = match slug {
            Some(slug) => crate::records::resolve(pool, slug).await?.is_some(),
            None => false,
        };
        sqlx::query(
            "INSERT INTO record (uid, slug, kind, head, body, quantity, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uid)
        .bind(if slug_taken { None } else { slug })
        .bind(RecordKind::Organ.as_str())
        .bind(head)
        .bind(&base_url)
        .bind(1.0_f64)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
    }
    sqlx::query(
        "INSERT OR IGNORE INTO organ_contact (record_uid, trust, proximity) VALUES (?, 'known', ?)",
    )
    .bind(uid)
    .bind(proximity as i64)
    .execute(pool)
    .await?;
    Ok(uid.to_string())
}

pub async fn set_trust(pool: &SqlitePool, organ_uid: &str, trust: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET trust = ? WHERE record_uid = ?")
        .bind(trust)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_sync_policy(
    pool: &SqlitePool,
    organ_uid: &str,
    sync_out: bool,
    sync_in: bool,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET sync_out = ?, sync_in = ? WHERE record_uid = ?")
        .bind(sync_out as i64)
        .bind(sync_in as i64)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

fn map_contact(r: sqlx::sqlite::SqliteRow) -> Contact {
    Contact {
        record_uid: r.get("record_uid"),
        slug: r.get("slug"),
        head: r.get("head"),
        base_url: r.get("body"),
        trust: r.get("trust"),
        proximity: r.get::<i64, _>("proximity") as u32,
        sync_out: r.get::<i64, _>("sync_out") != 0,
        sync_in: r.get::<i64, _>("sync_in") != 0,
    }
}

pub async fn contact(pool: &SqlitePool, organ_uid: &str) -> Result<Option<Contact>, StoreError> {
    Ok(sqlx::query(
        "SELECT c.*, r.slug, r.head, r.body FROM organ_contact c
           JOIN record r ON r.uid = c.record_uid
          WHERE c.record_uid = ?",
    )
    .bind(organ_uid)
    .fetch_optional(pool)
    .await?
    .map(map_contact))
}

pub async fn contacts(pool: &SqlitePool) -> Result<Vec<Contact>, StoreError> {
    Ok(sqlx::query(
        "SELECT c.*, r.slug, r.head, r.body FROM organ_contact c
           JOIN record r ON r.uid = c.record_uid
          ORDER BY r.created_at",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_contact)
    .collect())
}

// --------------------------------------------------- outbox (blueprint XV)

#[derive(Debug, Clone)]
pub struct OutboxRow {
    pub uid: String,
    pub organ_uid: String,
    pub payload: String,
    pub attempts: i64,
}

pub async fn outbox_enqueue(
    pool: &SqlitePool,
    organ_uid: &str,
    payload: &str,
) -> Result<String, StoreError> {
    let uid = nucleus::new_uid("o");
    sqlx::query(
        "INSERT INTO sync_outbox (uid, organ_uid, payload, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(organ_uid)
    .bind(payload)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(uid)
}

/// Queued or previously-failed rows, oldest first — failures retry.
pub async fn outbox_due(pool: &SqlitePool) -> Result<Vec<OutboxRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT uid, organ_uid, payload, attempts FROM sync_outbox
          WHERE status IN ('queued', 'failed') ORDER BY created_at",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| OutboxRow {
        uid: r.get("uid"),
        organ_uid: r.get("organ_uid"),
        payload: r.get("payload"),
        attempts: r.get("attempts"),
    })
    .collect())
}

pub async fn outbox_mark(pool: &SqlitePool, uid: &str, sent: bool) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE sync_outbox SET status = ?, attempts = attempts + 1, sent_at = ? WHERE uid = ?",
    )
    .bind(if sent { "sent" } else { "failed" })
    .bind(sent.then(|| Utc::now().to_rfc3339()))
    .bind(uid)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record a rejected import row (blueprint XI.1: reject the row, keep the
/// package, remember why).
pub async fn quarantine(
    pool: &SqlitePool,
    from_organ: &str,
    reason: &str,
    payload: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO sync_quarantine (uid, from_organ, reason, payload, at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(nucleus::new_uid("qr"))
    .bind(from_organ)
    .bind(reason)
    .bind(payload)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn quarantine_count(pool: &SqlitePool) -> Result<i64, StoreError> {
    Ok(sqlx::query("SELECT COUNT(1) AS n FROM sync_quarantine")
        .fetch_one(pool)
        .await?
        .get("n"))
}
