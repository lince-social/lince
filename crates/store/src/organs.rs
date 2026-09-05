use chrono::Utc;
use nucleus::RecordKind;
use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};

use crate::StoreError;

pub const LOCAL_ORGAN_SLUG: &str = "local-organ";

const CELL_SURFACE_CONFIG: &str = "lince.cell.surface";

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
    let mut base_url = normalize_base_url(base_url);
    if base_url.is_empty() {
        if let Some(existing) = local(pool).await? {
            base_url = existing.base_url;
        }
    }
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
                        updated_at = ?
                  WHERE uid = ?",
            )
            .bind(RecordKind::Organ.as_str())
            .bind(&now)
            .bind(&uid)
            .execute(pool)
            .await?;
            uid
        }
        None => {
            let uid = nucleus::new_uid("r");
            sqlx::query(
                "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                                     organ_uid, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, '1', 0, ?, ?, ?)",
            )
            .bind(&uid)
            .bind(LOCAL_ORGAN_SLUG)
            .bind(RecordKind::Organ.as_str())
            .bind("Local Lince")
            .bind("")
            .bind(&uid)
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await?;
            uid
        }
    };

    crate::cells::ensure_local(pool, &uid, "this cell").await?;
    crate::cells::set_config(pool, CELL_SURFACE_CONFIG, &surface(&base_url)).await?;
    local(pool).await?.ok_or(sqlx::Error::RowNotFound)
}

pub async fn holds_own_records(pool: &SqlitePool) -> Result<bool, StoreError> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM record WHERE slug IS NULL OR slug NOT IN (?, ?)")
            .bind(LOCAL_ORGAN_SLUG)
            .bind(crate::cells::LOCAL_CELL_SLUG)
            .fetch_one(pool)
            .await?;
    Ok(count > 0)
}

pub async fn adopt_identity(
    pool: &SqlitePool,
    joined_organ_uid: &str,
    base_url: &str,
) -> Result<OrganRecord, StoreError> {
    let Some(current) = local(pool).await? else {
        return Err(sqlx::Error::Protocol(
            "this Cell has no Organ to replace".into(),
        ));
    };
    if current.uid == joined_organ_uid {
        return Ok(current);
    }
    if holds_own_records(pool).await? {
        return Err(sqlx::Error::Protocol(
            "this Cell already holds Records of its own, so joining another \
                      Organ would have to merge two identities. Enrol a device that \
                      has not been used yet."
                .into(),
        ));
    }
    let cell = crate::cells::local(pool)
        .await?
        .ok_or_else(|| sqlx::Error::Protocol("this Cell has no Cell Record".into()))?;

    let mut tx = crate::write_tx(pool).await?;
    sqlx::query("DELETE FROM sync_op").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM record_extension WHERE record_uid = ?")
        .bind(&current.uid)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE record SET organ_uid = ? WHERE slug = ?")
        .bind(joined_organ_uid)
        .bind(crate::cells::LOCAL_CELL_SLUG)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM record WHERE uid = ?")
        .bind(&current.uid)
        .execute(&mut *tx)
        .await?;
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                             organ_uid, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, '1', 0, ?, ?, ?)",
    )
    .bind(joined_organ_uid)
    .bind(LOCAL_ORGAN_SLUG)
    .bind(RecordKind::Organ.as_str())
    .bind(&current.head)
    .bind("")
    .bind(joined_organ_uid)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO record_extension (record_uid, namespace, fds) VALUES (?, ?, ?)
         ON CONFLICT(record_uid, namespace)
         DO UPDATE SET fds = excluded.fds, version = version + 1",
    )
    .bind(&cell.uid)
    .bind(CELL_SURFACE_CONFIG)
    .bind(surface(&normalize_base_url(base_url)).to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    local(pool).await?.ok_or(sqlx::Error::RowNotFound.into())
}

pub async fn local(pool: &SqlitePool) -> Result<Option<OrganRecord>, StoreError> {
    let row = sqlx::query(
        "SELECT r.uid, r.slug, r.head, r.body
           FROM record r
          WHERE r.slug = ? AND r.kind = ?
          LIMIT 1",
    )
    .bind(LOCAL_ORGAN_SLUG)
    .bind(RecordKind::Organ.as_str())
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };
    let fds = crate::cells::config(pool, CELL_SURFACE_CONFIG)
        .await?
        .unwrap_or_else(|| json!({}));
    Ok(Some(OrganRecord {
        uid: row.get("uid"),
        slug: row.get("slug"),
        head: row.get("head"),
        body: row.get("body"),
        base_url: fds
            .get("baseUrl")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        local: fds.get("local").and_then(Value::as_bool).unwrap_or(true),
    }))
}

fn surface(base_url: &str) -> Value {
    json!({
        "baseUrl": base_url,
        "aliases": ["http://127.0.0.1", "http://localhost"],
        "local": true
    })
}

fn normalize_base_url(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_string()
}

#[derive(Debug, Clone)]
pub struct Contact {
    pub record_uid: String,
    pub slug: Option<String>,
    pub head: String,
    pub base_url: String,
    pub trust: String,
    pub proximity: u32,
    pub sync_out: bool,
    pub sync_in: bool,
    pub last_synced_seq: i64,
    pub peer_acked_seq: i64,
    pub mode: String,
    pub catchup_interval_secs: i64,
    pub share_protein: Option<String>,
    pub share_seen_seq: Option<i64>,
    pub scope_fields: Option<Vec<String>>,
    pub scope_version: i64,
    pub scope_unreadable: Option<String>,
    pub accept_unreadable: Option<String>,
    pub accept_fields: Option<Vec<String>>,
    pub accept_version: i64,
    pub pending_introduction: bool,
    pub node_id: Option<String>,
    pub unreachable_since: Option<String>,
    pub awaiting_roster_since: Option<String>,
    pub mailed_at: Option<String>,
}

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
            "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                                 organ_uid, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, '1', 0, ?, ?, ?)",
        )
        .bind(uid)
        .bind(if slug_taken { None } else { slug })
        .bind(RecordKind::Organ.as_str())
        .bind(head)
        .bind(&base_url)
        .bind(uid)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
    }
    sqlx::query(
        "INSERT OR IGNORE INTO organ_contact (record_uid, trust, proximity)
         VALUES (?, 'unknown', ?)",
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

pub async fn set_proximity(
    pool: &SqlitePool,
    organ_uid: &str,
    proximity: u32,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET proximity = ? WHERE record_uid = ?")
        .bind(proximity as i64)
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

pub async fn set_contact_scope(
    pool: &SqlitePool,
    organ_uid: &str,
    fields: Option<&[String]>,
) -> Result<(), StoreError> {
    let encoded = match fields {
        Some(fields) => Some(
            serde_json::to_string(fields)
                .map_err(|error| sqlx::Error::Protocol(error.to_string()))?,
        ),
        None => None,
    };
    sqlx::query(
        "UPDATE organ_contact
            SET scope_fields = ?, scope_version = scope_version + 1
          WHERE record_uid = ?",
    )
    .bind(encoded)
    .bind(organ_uid)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_contact_accept_scope(
    pool: &SqlitePool,
    organ_uid: &str,
    fields: Option<&[String]>,
) -> Result<(), StoreError> {
    let encoded = match fields {
        Some(fields) => Some(
            serde_json::to_string(fields)
                .map_err(|error| sqlx::Error::Protocol(error.to_string()))?,
        ),
        None => None,
    };
    sqlx::query(
        "UPDATE organ_contact
            SET accept_fields = ?, accept_version = accept_version + 1
          WHERE record_uid = ?",
    )
    .bind(encoded)
    .bind(organ_uid)
    .execute(pool)
    .await?;
    Ok(())
}

fn parse_scope(raw: Option<String>) -> Option<Vec<String>> {
    raw.and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
}

fn unreadable_scope(raw: Option<String>) -> Option<String> {
    let raw = raw?;
    match serde_json::from_str::<Vec<String>>(&raw) {
        Ok(_) => None,
        Err(_) => Some(raw),
    }
}

fn map_contact(r: sqlx::sqlite::SqliteRow) -> Contact {
    Contact {
        record_uid: r.get("record_uid"),
        scope_fields: parse_scope(r.get("scope_fields")),
        scope_unreadable: unreadable_scope(r.get("scope_fields")),
        scope_version: r.get("scope_version"),
        accept_fields: parse_scope(r.get("accept_fields")),
        accept_unreadable: unreadable_scope(r.get("accept_fields")),
        accept_version: r.get("accept_version"),
        slug: r.get("slug"),
        head: r.get("head"),
        base_url: r.get("body"),
        trust: r.get("trust"),
        proximity: r.get::<i64, _>("proximity") as u32,
        sync_out: r.get::<i64, _>("sync_out") != 0,
        sync_in: r.get::<i64, _>("sync_in") != 0,
        last_synced_seq: r.get("last_synced_seq"),
        peer_acked_seq: r.get("peer_acked_seq"),
        mode: r.get("mode"),
        catchup_interval_secs: r.get("catchup_interval_secs"),
        share_protein: r.get("share_protein"),
        share_seen_seq: r.get("share_seen_seq"),
        node_id: r.get("node_id"),
        pending_introduction: r.get::<i64, _>("pending_introduction") != 0,
        unreachable_since: r.get("unreachable_since"),
        awaiting_roster_since: r.get("awaiting_roster_since"),
        mailed_at: r.get("mailed_at"),
    }
}

pub async fn mark_awaiting_roster(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE organ_contact SET awaiting_roster_since = ?
          WHERE record_uid = ? AND awaiting_roster_since IS NULL",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(organ_uid)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn awaiting_roster_longer_than(
    pool: &SqlitePool,
    organ_uid: &str,
    grace: chrono::Duration,
) -> Result<bool, StoreError> {
    let since: Option<String> =
        sqlx::query_scalar("SELECT awaiting_roster_since FROM organ_contact WHERE record_uid = ?")
            .bind(organ_uid)
            .fetch_optional(pool)
            .await?
            .flatten();
    let Some(since) = since else {
        return Ok(false);
    };
    let Ok(since) = chrono::DateTime::parse_from_rfc3339(&since) else {
        return Ok(false);
    };
    Ok(Utc::now().signed_duration_since(since.with_timezone(&Utc)) > grace)
}

pub async fn clear_awaiting_roster(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE organ_contact SET awaiting_roster_since = NULL
          WHERE record_uid = ? AND awaiting_roster_since IS NOT NULL",
    )
    .bind(organ_uid)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_unreachable(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE organ_contact SET unreachable_since = ?
          WHERE record_uid = ? AND unreachable_since IS NULL",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(organ_uid)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_reachable(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE organ_contact SET unreachable_since = NULL, mailed_at = NULL
          WHERE record_uid = ? AND (unreachable_since IS NOT NULL OR mailed_at IS NOT NULL)",
    )
    .bind(organ_uid)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_mailed(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET mailed_at = ? WHERE record_uid = ?")
        .bind(Utc::now().to_rfc3339())
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn mark_mailed_clear(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET mailed_at = NULL WHERE record_uid = ?")
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn backdate_unreachable(
    pool: &SqlitePool,
    organ_uid: &str,
    when: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET unreachable_since = ? WHERE record_uid = ?")
        .bind(when)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_pending_introduction(
    pool: &SqlitePool,
    organ_uid: &str,
    pending: bool,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET pending_introduction = ? WHERE record_uid = ?")
        .bind(if pending { 1_i64 } else { 0 })
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn pending_introductions(pool: &SqlitePool) -> Result<Vec<Contact>, StoreError> {
    Ok(sqlx::query(
        "SELECT c.*, r.slug, r.head, r.body FROM organ_contact c
           JOIN record r ON r.uid = c.record_uid
          WHERE c.pending_introduction = 1 AND c.node_id IS NOT NULL",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_contact)
    .collect())
}

pub async fn forget_contact(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    for child in [
        "DELETE FROM fact_concept WHERE fact_uid IN (SELECT uid FROM fact WHERE record_uid = ?)",
        "DELETE FROM fact_concept_event WHERE fact_uid IN (SELECT uid FROM fact WHERE record_uid = ?)",
        "DELETE FROM fact_action_intent WHERE fact_uid IN (SELECT uid FROM fact WHERE record_uid = ?)",
        "DELETE FROM fact_remote_command WHERE fact_uid IN (SELECT uid FROM fact WHERE record_uid = ?)",
        "DELETE FROM fact WHERE record_uid = ?",
        "DELETE FROM record_extension WHERE record_uid = ?",
        "DELETE FROM record_doc WHERE record_uid = ?",
    ] {
        sqlx::query(child).bind(organ_uid).execute(pool).await?;
    }
    sqlx::query("DELETE FROM identity_key WHERE actor_uid = ?")
        .bind(organ_uid)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM sync_outbox WHERE contact_organ = ?")
        .bind(organ_uid)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM organ_contact WHERE record_uid = ?")
        .bind(organ_uid)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM record WHERE uid = ?")
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn rename_contact(
    pool: &SqlitePool,
    organ_uid: &str,
    head: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE record SET head = ?, updated_at = ? WHERE uid = ?")
        .bind(head)
        .bind(Utc::now().to_rfc3339())
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_node_id(
    pool: &SqlitePool,
    organ_uid: &str,
    node_id: Option<&str>,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET node_id = ? WHERE record_uid = ?")
        .bind(node_id)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn contact_by_node_id(
    pool: &SqlitePool,
    node_id: &str,
) -> Result<Option<Contact>, StoreError> {
    Ok(sqlx::query(
        "SELECT c.*, r.slug, r.head, r.body FROM organ_contact c
           JOIN record r ON r.uid = c.record_uid
          WHERE c.node_id = ?",
    )
    .bind(node_id)
    .fetch_optional(pool)
    .await?
    .map(map_contact))
}

pub async fn set_last_synced_seq(
    pool: &SqlitePool,
    organ_uid: &str,
    seq: i64,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET last_synced_seq = ? WHERE record_uid = ?")
        .bind(seq)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn advance_peer_acked_seq(
    pool: &SqlitePool,
    organ_uid: &str,
    seq: i64,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE organ_contact SET peer_acked_seq = MAX(peer_acked_seq, ?) WHERE record_uid = ?",
    )
    .bind(seq)
    .bind(organ_uid)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Direct,
    Mailbox,
    Auto,
}

impl Delivery {
    pub fn as_str(self) -> &'static str {
        match self {
            Delivery::Direct => "direct",
            Delivery::Mailbox => "mailbox",
            Delivery::Auto => "auto",
        }
    }
}

impl Contact {
    pub fn delivery(&self) -> Delivery {
        match self.mode.as_str() {
            "direct" => Delivery::Direct,
            "mailbox" => Delivery::Mailbox,
            _ => Delivery::Auto,
        }
    }
}

pub async fn set_contact_share_protein(
    pool: &SqlitePool,
    organ_uid: &str,
    protein: Option<&str>,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE organ_contact SET share_protein = ?, share_seen_seq = NULL
          WHERE record_uid = ?",
    )
    .bind(protein)
    .bind(organ_uid)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_mode(pool: &SqlitePool, organ_uid: &str, mode: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET mode = ? WHERE record_uid = ?")
        .bind(mode)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_catchup_interval(
    pool: &SqlitePool,
    organ_uid: &str,
    secs: i64,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET catchup_interval_secs = ? WHERE record_uid = ?")
        .bind(secs)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
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

pub const QUARANTINE_PER_CONTACT: i64 = 200;

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
    sqlx::query(
        "DELETE FROM sync_quarantine
          WHERE from_organ = ?
            AND rowid NOT IN (SELECT rowid FROM sync_quarantine
                               WHERE from_organ = ?
                               ORDER BY rowid DESC LIMIT ?)",
    )
    .bind(from_organ)
    .bind(from_organ)
    .bind(QUARANTINE_PER_CONTACT)
    .execute(pool)
    .await?;
    crate::contact_rate::spend(pool, from_organ, crate::contact_rate::RateKind::Refusal).await?;
    trim_quarantine_to_budget(pool, from_organ).await?;
    Ok(())
}

async fn trim_quarantine_to_budget(pool: &SqlitePool, from_organ: &str) -> Result<(), StoreError> {
    let total = crate::budget::total(pool).await?;
    let Some(quota) = crate::budget::share(total, crate::budget::Area::Quarantine) else {
        return Ok(());
    };
    let rows: Vec<(i64, i64)> = sqlx::query(
        "SELECT rowid AS id, LENGTH(payload) AS n FROM sync_quarantine
          WHERE from_organ = ? ORDER BY rowid DESC",
    )
    .bind(from_organ)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| (row.get("id"), row.get("n")))
    .collect();
    let evicted = crate::budget::evict_plan(&rows, quota);
    if evicted.is_empty() {
        return Ok(());
    }
    let mut tx = crate::write_tx(pool).await?;
    for id in evicted {
        sqlx::query("DELETE FROM sync_quarantine WHERE rowid = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn quarantined_for(
    pool: &SqlitePool,
    from_organ: &str,
    limit: i64,
) -> Result<Vec<(String, String, String)>, StoreError> {
    Ok(sqlx::query(
        "SELECT reason, payload, at FROM sync_quarantine
          WHERE from_organ = ? ORDER BY rowid DESC LIMIT ?",
    )
    .bind(from_organ)
    .bind(limit)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| (row.get("reason"), row.get("payload"), row.get("at")))
    .collect())
}

pub async fn quarantine_by_contact(pool: &SqlitePool) -> Result<Vec<(String, i64)>, StoreError> {
    Ok(sqlx::query(
        "SELECT from_organ, COUNT(1) AS n FROM sync_quarantine
          GROUP BY from_organ ORDER BY n DESC",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| (row.get("from_organ"), row.get("n")))
    .collect())
}

pub async fn clear_quarantine(pool: &SqlitePool, from_organ: &str) -> Result<u64, StoreError> {
    Ok(
        sqlx::query("DELETE FROM sync_quarantine WHERE from_organ = ?")
            .bind(from_organ)
            .execute(pool)
            .await?
            .rows_affected(),
    )
}

pub async fn quarantine_count(pool: &SqlitePool) -> Result<i64, StoreError> {
    Ok(sqlx::query("SELECT COUNT(1) AS n FROM sync_quarantine")
        .fetch_one(pool)
        .await?
        .get("n"))
}
