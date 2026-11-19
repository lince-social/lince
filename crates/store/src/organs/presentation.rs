use std::collections::HashMap;

use sqlx::{Row, SqlitePool};

use super::{Delivery, parse_scope};
use crate::{StoreError, organ_access, visibility::HiddenRecordRow};

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

#[derive(Default)]
pub struct DeliveryStatus {
    pub saved_mail: i64,
    pub mailbox_stored: i64,
    pub last_mailbox_storage: Option<String>,
    pub queued_operations: i64,
}

pub async fn for_records(
    pool: &SqlitePool,
    records: &[&str],
) -> Result<HashMap<String, Contact>, StoreError> {
    let _presentation = nucleus::performance::Span::new("store-contact-presentation-await");
    let mut output = HashMap::new();
    if records.is_empty() {
        return Ok(output);
    }
    let selected =
        serde_json::to_string(records).map_err(|error| StoreError::Protocol(error.to_string()))?;
    let rows = sqlx::query(
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
    nucleus::performance::add("contact-presentation-rows", rows.len() as u64);
    for row in rows {
        let (scope_fields, scope_unreadable) = parse_scope(row.get("scope_fields"));
        let (accept_fields, accept_unreadable) = parse_scope(row.get("accept_fields"));
        output.insert(
            row.get::<String, _>("record_uid"),
            Contact {
                trust: row.get("trust"),
                proximity: row.get::<i64, _>("proximity") as u32,
                pending_introduction: row.get::<i64, _>("pending_introduction") != 0,
                node_id: row.get("node_id"),
                delivery: Delivery::from_mode(row.get("mode")),
                awaiting_roster_since: row.get("awaiting_roster_since"),
                mailed_at: row.get("mailed_at"),
                peer_acked_seq: row.get("peer_acked_seq"),
                sync_out: row.get::<i64, _>("sync_out") != 0,
                sync_in: row.get::<i64, _>("sync_in") != 0,
                scope_fields,
                scope_version: row.get("scope_version"),
                accept_fields,
                accept_version: row.get("accept_version"),
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
        serde_json::to_string(&contacts).map_err(|error| StoreError::Protocol(error.to_string()))?
    };
    for (uid, status) in organ_access::get_many(pool, &contacts).await? {
        if let Some(contact) = output.get_mut(&uid) {
            contact.organ_access = Some(status);
        }
    }
    let rows = sqlx::query(
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
    for row in rows {
        if let Some(contact) = output.get_mut(row.get::<&str, _>("to_organ")) {
            contact.delivery_status.saved_mail = row.get("saved_mail");
            contact.delivery_status.mailbox_stored = row.get("mailbox_stored");
            contact.delivery_status.last_mailbox_storage = row.get("last_mailbox_storage");
        }
    }
    let rows = sqlx::query(
        "SELECT contact_organ, COUNT(*) AS queued_operations
         FROM sync_outbox
         WHERE contact_organ IN (SELECT value FROM json_each(?))
         GROUP BY contact_organ",
    )
    .bind(selected)
    .fetch_all(pool)
    .await?;
    for row in rows {
        if let Some(contact) = output.get_mut(row.get::<&str, _>("contact_organ")) {
            contact.delivery_status.queued_operations = row.get("queued_operations");
        }
    }
    for (uid, rows) in super::quarantined_for_many(pool, &contacts, 20).await? {
        if let Some(contact) = output.get_mut(&uid) {
            contact.quarantined = rows;
        }
    }
    for (uid, rows) in crate::visibility::hidden_records_from_organs(pool, &contacts).await? {
        if let Some(contact) = output.get_mut(&uid) {
            contact.hidden_records = rows;
        }
    }
    Ok(output)
}

pub struct Peer {
    pub head: String,
    pub trust: String,
}

pub async fn peers_by_node(
    pool: &SqlitePool,
    nodes: &[&str],
) -> Result<HashMap<String, Peer>, StoreError> {
    let _read = nucleus::performance::Span::new("store-nearby-presentation-await");
    let mut output = HashMap::new();
    if nodes.is_empty() {
        return Ok(output);
    }
    let selected =
        serde_json::to_string(nodes).map_err(|error| StoreError::Protocol(error.to_string()))?;
    let rows = sqlx::query(
        "SELECT c.node_id, r.head, c.trust
         FROM organ_contact c
         JOIN record r ON r.uid = c.record_uid
         WHERE c.node_id IN (SELECT value FROM json_each(?))",
    )
    .bind(selected)
    .fetch_all(pool)
    .await?;
    nucleus::performance::add("nearby-presentation-known-peers", rows.len() as u64);
    for row in rows {
        output.insert(
            row.get("node_id"),
            Peer {
                head: row.get("head"),
                trust: row.get("trust"),
            },
        );
    }
    Ok(output)
}
