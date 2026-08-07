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
            // The head is NOT rewritten here. "Local Lince" is a first-boot
            // default, and re-stamping it every start would silently undo any
            // name the user gave this Cell — leaving every Cell in the world
            // called the same thing, which is precisely what makes a contact
            // row indistinguishable from this Cell's own in a list.
            sqlx::query(
                "UPDATE record
                    SET kind = ?,
                        body = ?,
                        updated_at = ?
                  WHERE uid = ?",
            )
            .bind(RecordKind::Organ.as_str())
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
                "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                                     created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, '1', 0, ?, ?)",
            )
            .bind(&uid)
            .bind(LOCAL_ORGAN_SLUG)
            .bind(RecordKind::Organ.as_str())
            .bind("Local Lince")
            .bind(&base_url)
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
    /// Their op-log seq as we last acknowledged it (catch-up checkpoint).
    pub last_synced_seq: i64,
    /// How far this contact has RECEIVED our own log — the retention floor.
    /// The mirror image of `last_synced_seq`, and not interchangeable with it:
    /// pruning against the wrong one deletes ops the peer never saw.
    pub peer_acked_seq: i64,
    /// `replica` (local rows, deltas + reconciliation) or `live` (Protein WS
    /// against the remote, zero local rows).
    pub mode: String,
    /// Seconds between catch-up pulls; 0 disables the cycle (reactive deltas
    /// and reconnect catch-up still run).
    pub catchup_interval_secs: i64,
    /// The last address a SIGNED exchange succeeded from — a cached hint,
    /// never identity (Ontology §11 "Peers"). Only the verified handshake
    /// writes it; discovery announces alone never do.
    pub last_seen_addr: Option<String>,
    /// Added from a code, with no connection yet to learn their real uid. The
    /// row is held under a uid derived from the NodeId until an Introduction
    /// replaces it; until then they cannot sync, because every batch they push
    /// is attributed to a uid this Cell does not know them by.
    pub pending_introduction: bool,
    /// The contact's iroh NodeId — the ONLY routing input (Ontology §11
    /// "Transport: iroh"). Under iroh the address is the key, so this both
    /// locates and authenticates. `None` for contacts made before the iroh
    /// path, which must be re-paired.
    pub node_id: Option<String>,
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
            "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                                 created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, '1', 0, ?, ?)",
        )
        .bind(uid)
        .bind(if slug_taken { None } else { slug })
        .bind(RecordKind::Organ.as_str())
        .bind(head)
        .bind(&base_url)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
    }
    // `unknown`, never `known`. Knowing someone's address is not deciding to
    // trust them, and `known` is what opens the sync ALPN (Ontology §11): a
    // default of `known` would mean every path that records a contact quietly
    // opens that door. Callers that HAVE made the decision — pairing, adopting
    // a code — say so with `set_trust`.
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
        last_synced_seq: r.get("last_synced_seq"),
        peer_acked_seq: r.get("peer_acked_seq"),
        mode: r.get("mode"),
        catchup_interval_secs: r.get("catchup_interval_secs"),
        last_seen_addr: r.get("last_seen_addr"),
        node_id: r.get("node_id"),
        pending_introduction: r.get::<i64, _>("pending_introduction") != 0,
    }
}

/// Mark a contact as still owing an Introduction, or clear it once one has
/// happened. Set only by adding from a code, cleared only by a connection.
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

/// Contacts added by code that no connection has confirmed yet. Only rows with
/// a NodeId are returned — without one there is nothing to dial, so there is
/// nothing a sync pass could do about them.
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

/// Drop a contact and the Record standing in for it.
///
/// Used to retire a placeholder once the real Organ has introduced itself, and
/// to forget someone deliberately. Safe precisely because `add_contact` writes
/// both rows with plain SQL rather than through the Record write path: nothing
/// was ever logged to the op log, so no peer was told about this uid and there
/// is no history to orphan.
///
/// The annotations go with it. Setting trust, proximity or feed direction
/// commits a Fact against this record, and `fact.record_uid` is a foreign key —
/// so leaving them behind does not preserve history, it just makes forgetting
/// fail with a constraint error. What is being deleted is this Cell's own notes
/// about a row that stands in for someone else's Organ; the Organ itself is
/// untouched, and nothing here was ever anyone else's to keep.
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

/// Bind a contact to the iroh NodeId that reaches them. Written by pairing
/// (QR, paste, or an introduction over an already-authenticated connection),
/// never inferred from an inbound connection: adopting the NodeId of whoever
/// dialed us is exactly how an impostor would claim a contact's row.
/// Rename a contact to what the LOCAL user calls them.
///
/// Plain SQL and no op, for the same reason `add_contact` writes its record
/// that way: this is our private label for someone else's Organ, and logging
/// it would push our name for them back to them and to every other contact.
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

/// Resolve an inbound connection's authenticated `remote_id()` to a contact.
/// This is the accept path's whole authorization input: iroh proved possession
/// of the private half during the QUIC/TLS handshake, so a hit here means the
/// peer IS that contact — no challenge, no signature, no replay window.
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

/// Record the address a signed exchange just succeeded from. The signature is
/// what authorizes the update — never trust-on-IP.
pub async fn set_last_seen_addr(
    pool: &SqlitePool,
    organ_uid: &str,
    addr: Option<&str>,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET last_seen_addr = ? WHERE record_uid = ?")
        .bind(addr)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Advance the catch-up checkpoint: the peer's op seq we have fully applied.
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

/// Advance the retention floor: how far this contact has received OUR log.
///
/// Monotonic by `MAX`, never assignment. A peer may legitimately ask for an
/// older `after` (a rebuild, a restored backup, two Cells of one Organ at
/// different points), and letting that move the floor BACKWARDS would be
/// harmless for correctness but would silently un-prune nothing while making
/// the floor meaningless. Moving it backwards is never useful; moving it
/// forwards on evidence is the whole point.
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

// The op-based bounded outbox lives in `crate::sync_ops` (queued by the op
// log itself; drained by `Engine::drain_outbox`).

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
