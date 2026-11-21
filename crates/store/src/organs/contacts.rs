use nucleus::execution::now;
use serde_json::{from_str, to_string};
use sqlx::{Row, SqlitePool, query, sqlite::SqliteRow};

use super::{
    Delivery,
    local::{insert_organ, normalize_base_url},
};
use crate::{StoreError, records};

/// The stored Organ contact, including sync settings and delivery cursors.
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

/// Ensures the contact and its backing Organ Record exist.
pub async fn add_contact(
    pool: &SqlitePool,
    uid: &str,
    slug: Option<&str>,
    head: &str,
    base_url: &str,
    proximity: u32,
) -> Result<String, StoreError> {
    let base_url = normalize_base_url(base_url);
    let now = now().to_rfc3339();
    if records::get(pool, uid).await?.is_none() {
        let slug_taken = match slug {
            Some(slug) => records::resolve(pool, slug).await?.is_some(),
            None => false,
        };
        insert_organ(
            uid,
            if slug_taken { None } else { slug },
            head,
            base_url,
            &now,
        )
        .execute(pool)
        .await?;
    }
    query(
        "INSERT OR IGNORE INTO organ_contact (record_uid, trust, proximity)
         VALUES (?, 'unknown', ?)",
    )
    .bind(uid)
    .bind(i64::from(proximity))
    .execute(pool)
    .await?;
    Ok(uid.to_string())
}

/// Stores the contact trust classification.
pub async fn set_trust(pool: &SqlitePool, organ_uid: &str, trust: &str) -> Result<(), StoreError> {
    query("UPDATE organ_contact SET trust = ? WHERE record_uid = ?")
        .bind(trust)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Sets contact proximity and schedules its shared Protein for reconsideration.
pub async fn set_proximity(
    pool: &SqlitePool,
    organ_uid: &str,
    proximity: u32,
) -> Result<(), StoreError> {
    query("UPDATE organ_contact SET proximity = ?, share_seen_seq = NULL WHERE record_uid = ?")
        .bind(i64::from(proximity))
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Sets independent outgoing and incoming sync permissions.
pub async fn set_sync_policy(
    pool: &SqlitePool,
    organ_uid: &str,
    sync_out: bool,
    sync_in: bool,
) -> Result<(), StoreError> {
    query("UPDATE organ_contact SET sync_out = ?, sync_in = ? WHERE record_uid = ?")
        .bind(i64::from(sync_out))
        .bind(i64::from(sync_in))
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Sets the outbound property scope and advances its version.
pub async fn set_contact_scope(
    pool: &SqlitePool,
    organ_uid: &str,
    fields: Option<&[String]>,
) -> Result<(), StoreError> {
    write_scope(
        pool,
        organ_uid,
        fields,
        "UPDATE organ_contact SET scope_fields = ?, scope_version = scope_version + 1
          WHERE record_uid = ?",
    )
    .await
}

/// Sets the inbound property scope and advances its version.
pub async fn set_contact_accept_scope(
    pool: &SqlitePool,
    organ_uid: &str,
    fields: Option<&[String]>,
) -> Result<(), StoreError> {
    write_scope(
        pool,
        organ_uid,
        fields,
        "UPDATE organ_contact SET accept_fields = ?, accept_version = accept_version + 1
          WHERE record_uid = ?",
    )
    .await
}

async fn write_scope(
    pool: &SqlitePool,
    organ_uid: &str,
    fields: Option<&[String]>,
    sql: &'static str,
) -> Result<(), StoreError> {
    let encoded = fields
        .map(to_string)
        .transpose()
        .map_err(|error| StoreError::Protocol(error.to_string()))?;
    query(sql)
        .bind(encoded)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub(super) fn parse_scope(raw: Option<&str>) -> (Option<Vec<String>>, Option<String>) {
    let Some(raw) = raw else {
        return (None, None);
    };
    match from_str(raw) {
        Ok(fields) => (Some(fields), None),
        Err(_) => (None, Some(raw.to_owned())),
    }
}

pub(super) fn read_proximity(row: &SqliteRow) -> Result<u32, StoreError> {
    u32::try_from(row.try_get::<i64, _>("proximity")?)
        .map_err(|_| StoreError::Protocol("Stored Organ proximity is out of range".into()))
}

macro_rules! contact_select {
    ($tail:literal) => {
        concat!(
            "SELECT c.*, r.slug, r.head, r.body FROM organ_contact c
                 JOIN record r ON r.uid = c.record_uid ",
            $tail
        )
    };
}

fn map_contact(row: SqliteRow) -> Result<Contact, StoreError> {
    let (scope_fields, scope_unreadable) = parse_scope(row.try_get("scope_fields")?);
    let (accept_fields, accept_unreadable) = parse_scope(row.try_get("accept_fields")?);
    Ok(Contact {
        record_uid: row.try_get("record_uid")?,
        scope_fields,
        scope_unreadable,
        scope_version: row.try_get("scope_version")?,
        accept_fields,
        accept_unreadable,
        accept_version: row.try_get("accept_version")?,
        slug: row.try_get("slug")?,
        head: row.try_get("head")?,
        base_url: row.try_get("body")?,
        trust: row.try_get("trust")?,
        proximity: read_proximity(&row)?,
        sync_out: row.try_get::<i64, _>("sync_out")? != 0,
        sync_in: row.try_get::<i64, _>("sync_in")? != 0,
        last_synced_seq: row.try_get("last_synced_seq")?,
        peer_acked_seq: row.try_get("peer_acked_seq")?,
        mode: row.try_get("mode")?,
        catchup_interval_secs: row.try_get("catchup_interval_secs")?,
        share_protein: row.try_get("share_protein")?,
        share_seen_seq: row.try_get("share_seen_seq")?,
        node_id: row.try_get("node_id")?,
        pending_introduction: row.try_get::<i64, _>("pending_introduction")? != 0,
        unreachable_since: row.try_get("unreachable_since")?,
        awaiting_roster_since: row.try_get("awaiting_roster_since")?,
        mailed_at: row.try_get("mailed_at")?,
    })
}

/// Marks whether the contact still needs an introduction.
pub async fn set_pending_introduction(
    pool: &SqlitePool,
    organ_uid: &str,
    pending: bool,
) -> Result<(), StoreError> {
    query("UPDATE organ_contact SET pending_introduction = ? WHERE record_uid = ?")
        .bind(i64::from(pending))
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Lists contacts awaiting an introduction with known transport nodes.
pub async fn pending_introductions(pool: &SqlitePool) -> Result<Vec<Contact>, StoreError> {
    query(contact_select!(
        "WHERE c.pending_introduction = 1 AND c.node_id IS NOT NULL"
    ))
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_contact)
    .collect()
}

/// Removes the contact and its dependent local storage.
pub async fn forget_contact(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    for child in [
        "DELETE FROM fact_concept WHERE fact_uid IN (SELECT uid FROM fact WHERE record_uid = ?)",
        "DELETE FROM fact_concept_event WHERE fact_uid IN (SELECT uid FROM fact WHERE record_uid = ?)",
        "DELETE FROM fact_action_intent WHERE fact_uid IN (SELECT uid FROM fact WHERE record_uid = ?)",
        "DELETE FROM fact_remote_command WHERE fact_uid IN (SELECT uid FROM fact WHERE record_uid = ?)",
        "DELETE FROM fact WHERE record_uid = ?",
        "DELETE FROM record_extension WHERE record_uid = ?",
        "DELETE FROM record_doc WHERE record_uid = ?",
        "DELETE FROM organ_vocabulary WHERE organ_uid = ?",
        "DELETE FROM identity_key WHERE actor_uid = ?",
        "DELETE FROM sync_outbox WHERE contact_organ = ?",
        "DELETE FROM organ_contact WHERE record_uid = ?",
        "DELETE FROM record WHERE uid = ?",
    ] {
        query(child).bind(organ_uid).execute(pool).await?;
    }
    Ok(())
}

/// Updates the contact's displayed Record title.
pub async fn rename_contact(
    pool: &SqlitePool,
    organ_uid: &str,
    head: &str,
) -> Result<(), StoreError> {
    query("UPDATE record SET head = ?, updated_at = ? WHERE uid = ?")
        .bind(head)
        .bind(now().to_rfc3339())
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Sets or clears the contact's transport node ID.
pub async fn set_node_id(
    pool: &SqlitePool,
    organ_uid: &str,
    node_id: Option<&str>,
) -> Result<(), StoreError> {
    query("UPDATE organ_contact SET node_id = ? WHERE record_uid = ?")
        .bind(node_id)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Looks up a contact by transport node ID.
pub async fn contact_by_node_id(
    pool: &SqlitePool,
    node_id: &str,
) -> Result<Option<Contact>, StoreError> {
    query(contact_select!("WHERE c.node_id = ?"))
        .bind(node_id)
        .fetch_optional(pool)
        .await?
        .map(map_contact)
        .transpose()
}

impl Contact {
    /// Decodes the saved delivery mode with an automatic fallback.
    pub fn delivery(&self) -> Delivery {
        Delivery::from_mode(&self.mode)
    }
}

/// Sets the contact's shared Protein and resets the observed cursor.
pub async fn set_contact_share_protein(
    pool: &SqlitePool,
    organ_uid: &str,
    protein: Option<&str>,
) -> Result<(), StoreError> {
    query(
        "UPDATE organ_contact SET share_protein = ?, share_seen_seq = NULL
          WHERE record_uid = ?",
    )
    .bind(protein)
    .bind(organ_uid)
    .execute(pool)
    .await?;
    Ok(())
}

/// Reads one Organ contact and its current settings.
pub async fn contact(pool: &SqlitePool, organ_uid: &str) -> Result<Option<Contact>, StoreError> {
    query(contact_select!("WHERE c.record_uid = ?"))
        .bind(organ_uid)
        .fetch_optional(pool)
        .await?
        .map(map_contact)
        .transpose()
}

/// Lists Organ contacts in backing Record creation order.
pub async fn contacts(pool: &SqlitePool) -> Result<Vec<Contact>, StoreError> {
    query(contact_select!("ORDER BY r.created_at"))
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map_contact)
        .collect()
}
