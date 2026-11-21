//! Batched contact state for presentation and nearby peer lists.

use std::collections::HashMap;

use nucleus::performance::{Span, add};
use serde_json::to_string;
use sqlx::{Row, SqlitePool, query, query_as};

use super::{
    Delivery,
    contacts::{parse_scope, read_proximity},
    quarantined_for_many,
};
use crate::{
    StoreError, organ_access,
    visibility::{HiddenRecordRow, hidden_records_from_organs},
};

/// Delivery, sync scope, and access information for a displayed contact.
pub struct Contact {
    pub trust: String,
    pub proximity: u32,
    pub pending_introduction: bool,
    pub node_id: Option<String>,
    pub delivery: Delivery,
    pub awaiting_roster_since: Option<String>,
    pub mailed_at: Option<String>,
    pub peer_acked_seq: i64,
    pub sync_out: bool,
    pub sync_in: bool,
    pub scope_fields: Option<Vec<String>>,
    pub scope_version: i64,
    pub accept_fields: Option<Vec<String>>,
    pub accept_version: i64,
    pub scope_unreadable: Option<String>,
    pub accept_unreadable: Option<String>,
    pub organ_access: Option<organ_access::Status>,
    pub delivery_status: DeliveryStatus,
    pub quarantined: Vec<(String, String, String)>,
    pub hidden_records: Vec<HiddenRecordRow>,
}

/// Summary of mailbox and direct delivery queues.
#[derive(Debug, Default)]
pub struct DeliveryStatus {
    pub saved_mail: i64,
    pub mailbox_stored: i64,
    pub last_mailbox_storage: Option<String>,
    pub queued_operations: i64,
}

/// Loads presentation details for the selected contact Records.
///
/// # Errors
/// Returns a database or decode error for unavailable or malformed stored data.
pub async fn for_records(
    pool: &SqlitePool,
    records: &[&str],
) -> Result<HashMap<String, Contact>, StoreError> {
    let _presentation = Span::new("store-contact-presentation-await");
    if records.is_empty() {
        return Ok(HashMap::new());
    }
    let selected = to_string(records).map_err(|error| StoreError::Protocol(error.to_string()))?;
    let rows = query(
        "SELECT c.record_uid, c.trust, c.proximity, c.pending_introduction,
                c.node_id, c.mode, c.awaiting_roster_since, c.mailed_at,
                c.peer_acked_seq, c.sync_out, c.sync_in, c.scope_fields,
                c.scope_version, c.accept_fields, c.accept_version
         FROM organ_contact c
         JOIN record r ON r.uid = c.record_uid
         WHERE c.record_uid IN (SELECT value FROM json_each(?))",
    )
    .bind(&selected)
    .fetch_all(pool)
    .await?;
    add("contact-presentation-rows", rows.len() as u64);
    let mut output = HashMap::with_capacity(rows.len());
    for row in rows {
        let (scope_fields, scope_unreadable) = parse_scope(row.try_get("scope_fields")?);
        let (accept_fields, accept_unreadable) = parse_scope(row.try_get("accept_fields")?);
        output.insert(
            row.try_get::<String, _>("record_uid")?,
            Contact {
                trust: row.try_get("trust")?,
                proximity: read_proximity(&row)?,
                pending_introduction: row.try_get::<i64, _>("pending_introduction")? != 0,
                node_id: row.try_get("node_id")?,
                delivery: Delivery::from_mode(row.try_get("mode")?),
                awaiting_roster_since: row.try_get("awaiting_roster_since")?,
                mailed_at: row.try_get("mailed_at")?,
                peer_acked_seq: row.try_get("peer_acked_seq")?,
                sync_out: row.try_get::<i64, _>("sync_out")? != 0,
                sync_in: row.try_get::<i64, _>("sync_in")? != 0,
                scope_fields,
                scope_version: row.try_get("scope_version")?,
                accept_fields,
                accept_version: row.try_get("accept_version")?,
                scope_unreadable,
                accept_unreadable,
                organ_access: None,
                delivery_status: DeliveryStatus::default(),
                quarantined: Vec::new(),
                hidden_records: Vec::new(),
            },
        );
    }
    if output.is_empty() {
        return Ok(output);
    }
    let contacts: Vec<_> = records
        .iter()
        .copied()
        .filter(|uid| output.contains_key(*uid))
        .collect();
    let selected = if contacts.len() == records.len() {
        selected
    } else {
        to_string(&contacts).map_err(|error| StoreError::Protocol(error.to_string()))?
    };
    for (uid, status) in organ_access::get_many(pool, &contacts).await? {
        if let Some(contact) = output.get_mut(&uid) {
            contact.organ_access = Some(status);
        }
    }
    let rows = query_as::<_, (String, i64, i64, Option<String>)>(
        "WITH deliveries AS (
             SELECT o.uid, o.to_organ,
                    COUNT(r.uid) > 0 AS stored, MAX(r.accepted_at) AS accepted_at
             FROM mailbox_outbox o
             LEFT JOIN mailbox_outbox_receipt r ON r.uid = o.uid
             WHERE o.to_organ IN (SELECT value FROM json_each(?))
             GROUP BY o.uid
         )
         SELECT to_organ, COUNT(*) AS saved_mail, SUM(stored) AS mailbox_stored,
                MAX(accepted_at) AS last_mailbox_storage
         FROM deliveries GROUP BY to_organ",
    )
    .bind(&selected)
    .fetch_all(pool)
    .await?;
    for (uid, saved, stored, accepted_at) in rows {
        if let Some(contact) = output.get_mut(&uid) {
            contact.delivery_status.saved_mail = saved;
            contact.delivery_status.mailbox_stored = stored;
            contact.delivery_status.last_mailbox_storage = accepted_at;
        }
    }
    let rows = query_as::<_, (String, i64)>(
        "SELECT contact_organ, COUNT(*) AS queued_operations
         FROM sync_outbox
         WHERE contact_organ IN (SELECT value FROM json_each(?))
         GROUP BY contact_organ",
    )
    .bind(selected)
    .fetch_all(pool)
    .await?;
    for (uid, queued) in rows {
        if let Some(contact) = output.get_mut(&uid) {
            contact.delivery_status.queued_operations = queued;
        }
    }
    for (uid, rows) in quarantined_for_many(pool, &contacts, 20).await? {
        if let Some(contact) = output.get_mut(&uid) {
            contact.quarantined = rows;
        }
    }
    for (uid, rows) in hidden_records_from_organs(pool, &contacts).await? {
        if let Some(contact) = output.get_mut(&uid) {
            contact.hidden_records = rows;
        }
    }
    Ok(output)
}

/// A nearby peer already recognized as a contact.
#[derive(Debug)]
pub struct Peer {
    pub head: String,
    pub trust: String,
}

/// Looks up known contacts for the selected transport node IDs.
pub async fn peers_by_node(
    pool: &SqlitePool,
    nodes: &[&str],
) -> Result<HashMap<String, Peer>, StoreError> {
    let _read = Span::new("store-nearby-presentation-await");
    if nodes.is_empty() {
        return Ok(HashMap::new());
    }
    let selected = to_string(nodes).map_err(|error| StoreError::Protocol(error.to_string()))?;
    let rows = query_as::<_, (String, String, String)>(
        "SELECT c.node_id, r.head, c.trust
         FROM organ_contact c
         JOIN record r ON r.uid = c.record_uid
         WHERE c.node_id IN (SELECT value FROM json_each(?))",
    )
    .bind(selected)
    .fetch_all(pool)
    .await?;
    add("nearby-presentation-known-peers", rows.len() as u64);
    Ok(rows
        .into_iter()
        .map(|(node, head, trust)| (node, Peer { head, trust }))
        .collect())
}
