//! The blind mailbox's storage (Ontology C4).
//!
//! This module STORES. It does not open, verify or route anything: whether a
//! bundle's signature holds, whether a collector really is the recipient, and
//! whether a deposit is admissible all live in `engine::mailbox`, so there is
//! one place that decides and one place that remembers.
//!
//! Nothing here can turn a bundle into an op. That is the carrier's defining
//! property — it converges nothing — and it is structural: a `body` is a text
//! blob, and no function in this file returns anything the sync path consumes.

use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub organ_uid: String,
    pub root_key: String,
    pub label: String,
    pub quota_bytes: i64,
    pub registered_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldBundle {
    pub uid: String,
    pub to_organ: String,
    pub from_organ: String,
    pub from_cell: String,
    /// The node id the deposit arrived on — proven by the transport, unlike
    /// the two fields above, which the sender wrote into the bundle.
    pub from_node: String,
    pub body: String,
    pub bytes: i64,
    pub received_at: String,
    pub expires_at: String,
}

/// What a recipient is told about their own mail, and the most a carrier can
/// say: how many bundles, how much space, and when the oldest one expires.
///
/// Not who they are from. The carrier knows — it is written on the outside —
/// but reporting it here would make the metadata leak an ordinary part of the
/// product rather than a stated cost, and the recipient learns every sender
/// anyway the moment they collect.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Waiting {
    pub bundles: i64,
    pub bytes: i64,
    pub oldest_expires_at: Option<String>,
}

fn map_registration(row: sqlx::sqlite::SqliteRow) -> Registration {
    Registration {
        organ_uid: row.get("organ_uid"),
        root_key: row.get("root_key"),
        label: row.get("label"),
        quota_bytes: row.get("quota_bytes"),
        registered_at: row.get("registered_at"),
    }
}

fn map_bundle(row: sqlx::sqlite::SqliteRow) -> HeldBundle {
    HeldBundle {
        uid: row.get("uid"),
        to_organ: row.get("to_organ"),
        from_organ: row.get("from_organ"),
        from_cell: row.get("from_cell"),
        from_node: row.get("from_node"),
        body: row.get("body"),
        bytes: row.get("bytes"),
        received_at: row.get("received_at"),
        expires_at: row.get("expires_at"),
    }
}

/// Register a recipient, or update the terms of one already registered.
pub async fn register(
    pool: &SqlitePool,
    organ_uid: &str,
    root_key: &str,
    label: &str,
    quota_bytes: i64,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO mailbox_registration
           (organ_uid, root_key, label, quota_bytes, registered_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(organ_uid) DO UPDATE SET
           root_key = excluded.root_key,
           label = excluded.label,
           quota_bytes = excluded.quota_bytes",
    )
    .bind(organ_uid)
    .bind(root_key)
    .bind(label)
    .bind(quota_bytes)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Stop carrying for a recipient. Their held bundles go with them — the
/// cascade is deliberate, because a mailbox that keeps mail for someone it no
/// longer serves is storing data nobody will ever collect.
pub async fn deregister(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM mailbox_registration WHERE organ_uid = ?")
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn registration(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Option<Registration>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM mailbox_registration WHERE organ_uid = ?")
            .bind(organ_uid)
            .fetch_optional(pool)
            .await?
            .map(map_registration),
    )
}

pub async fn registrations(pool: &SqlitePool) -> Result<Vec<Registration>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM mailbox_registration ORDER BY label, organ_uid")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(map_registration)
            .collect(),
    )
}

/// How much of a recipient's quota is currently spent.
pub async fn held_bytes(pool: &SqlitePool, organ_uid: &str) -> Result<i64, StoreError> {
    Ok(
        sqlx::query("SELECT COALESCE(SUM(bytes), 0) AS held FROM mailbox_bundle WHERE to_organ = ?")
            .bind(organ_uid)
            .fetch_one(pool)
            .await?
            .get("held"),
    )
}

/// Store one sealed bundle. The caller has already decided it is admissible.
pub async fn deposit(pool: &SqlitePool, bundle: &HeldBundle) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO mailbox_bundle
           (uid, to_organ, from_organ, from_cell, from_node, body, bytes, received_at, expires_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&bundle.uid)
    .bind(&bundle.to_organ)
    .bind(&bundle.from_organ)
    .bind(&bundle.from_cell)
    .bind(&bundle.from_node)
    .bind(&bundle.body)
    .bind(bundle.bytes)
    .bind(&bundle.received_at)
    .bind(&bundle.expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// Everything held for one recipient, oldest first.
pub async fn for_recipient(
    pool: &SqlitePool,
    organ_uid: &str,
    limit: i64,
) -> Result<Vec<HeldBundle>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM mailbox_bundle WHERE to_organ = ? ORDER BY received_at, uid LIMIT ?",
    )
    .bind(organ_uid)
    .bind(limit)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_bundle)
    .collect())
}

/// Drop bundles the recipient has confirmed receiving.
///
/// Scoped to the recipient rather than taking bare uids: a collector that
/// could acknowledge any uid could delete other people's mail, and uids travel
/// over the wire.
pub async fn collected(
    pool: &SqlitePool,
    organ_uid: &str,
    uids: &[String],
) -> Result<u64, StoreError> {
    let mut dropped = 0;
    for uid in uids {
        dropped += sqlx::query("DELETE FROM mailbox_bundle WHERE uid = ? AND to_organ = ?")
            .bind(uid)
            .bind(organ_uid)
            .execute(pool)
            .await?
            .rows_affected();
    }
    Ok(dropped)
}

/// What a recipient may be told about their own waiting mail.
pub async fn waiting(pool: &SqlitePool, organ_uid: &str) -> Result<Waiting, StoreError> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS bundles,
                COALESCE(SUM(bytes), 0) AS bytes,
                MIN(expires_at) AS oldest
           FROM mailbox_bundle WHERE to_organ = ?",
    )
    .bind(organ_uid)
    .fetch_one(pool)
    .await?;
    Ok(Waiting {
        bundles: row.get("bundles"),
        bytes: row.get("bytes"),
        oldest_expires_at: row.get("oldest"),
    })
}

/// Delete everything past its retention date, leaving a notice per bundle so
/// the sender can be told. Returns how many were swept.
///
/// The two statements are one transaction because the notice is the only
/// reason the delete is acceptable: a crash between them would lose the mail
/// AND the record that it existed, which is precisely the silent failure the
/// retention rule exists to avoid.
pub async fn sweep_expired(pool: &SqlitePool) -> Result<u64, StoreError> {
    let now = Utc::now().to_rfc3339();
    let mut tx = crate::write_tx(pool).await?;
    sqlx::query(
        "INSERT OR IGNORE INTO mailbox_expiry_notice
           (uid, to_organ, from_organ, from_cell, from_node, bytes, received_at,
            expired_at, notified_at)
         SELECT uid, to_organ, from_organ, from_cell, from_node, bytes, received_at, ?, NULL
           FROM mailbox_bundle WHERE expires_at <= ?",
    )
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    let swept = sqlx::query("DELETE FROM mailbox_bundle WHERE expires_at <= ?")
        .bind(&now)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok(swept)
}

/// Move a held bundle's expiry date. A narrow seam, the same shape as
/// `organs::backdate_unreachable`: the retention window is thirty days, and a
/// test that waited it out would not be a test.
pub async fn backdate_expiry(
    pool: &SqlitePool,
    uid: &str,
    when: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE mailbox_bundle SET expires_at = ? WHERE uid = ?")
        .bind(when)
        .bind(uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Expiry notices whose sender has not yet been told.
pub async fn pending_notices(pool: &SqlitePool) -> Result<Vec<HeldBundle>, StoreError> {
    Ok(sqlx::query(
        "SELECT uid, to_organ, from_organ, from_cell, from_node, '' AS body, bytes,
                received_at, expired_at AS expires_at
           FROM mailbox_expiry_notice WHERE notified_at IS NULL ORDER BY expired_at",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_bundle)
    .collect())
}

/// The notices waiting for the sender on THIS connection, and nothing else.
///
/// Scoped by node id and by no other field, on purpose. `from_organ` and
/// `from_cell` were written by whoever left the bundle and a caller could
/// name either; the node id is the peer identity the transport proved. There
/// is deliberately no way for a caller to ask about somebody else's mail —
/// not by uid, not by Organ — so the answer can only ever repeat facts the
/// asker already had when they deposited.
pub async fn expiries_for_node(
    pool: &SqlitePool,
    from_node: &str,
) -> Result<Vec<HeldBundle>, StoreError> {
    Ok(sqlx::query(
        "SELECT uid, to_organ, from_organ, from_cell, from_node, '' AS body, bytes,
                received_at, expired_at AS expires_at
           FROM mailbox_expiry_notice WHERE from_node = ? ORDER BY expired_at",
    )
    .bind(from_node)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_bundle)
    .collect())
}

/// Stamp the notices just handed to a sender, so the operator's count reflects
/// what is still owed rather than what has ever expired.
pub async fn notices_handed(pool: &SqlitePool, from_node: &str) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE mailbox_expiry_notice SET notified_at = ?
          WHERE from_node = ? AND notified_at IS NULL",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(from_node)
    .execute(pool)
    .await?;
    Ok(())
}

/// Drop notices the sender acknowledged. Scoped to their node for the same
/// reason `collected` is scoped to the recipient: uids travel over the wire,
/// and an unscoped delete would let anyone erase what a carrier still owes
/// somebody else.
///
/// A delete rather than a flag. The carrier's ledger of who wrote to whom is
/// the most sensitive thing it holds, so the row's job ends when the sender
/// has it.
pub async fn notices_heard(
    pool: &SqlitePool,
    from_node: &str,
    uids: &[String],
) -> Result<u64, StoreError> {
    let mut dropped = 0;
    for uid in uids {
        dropped += sqlx::query("DELETE FROM mailbox_expiry_notice WHERE uid = ? AND from_node = ?")
            .bind(uid)
            .bind(from_node)
            .execute(pool)
            .await?
            .rows_affected();
    }
    Ok(dropped)
}

/// What the operator sees per recipient: how much is held and how many.
pub async fn carried_for(pool: &SqlitePool, organ_uid: &str) -> Result<Waiting, StoreError> {
    waiting(pool, organ_uid).await
}

/// An Organ that has asked to be carried and has not been answered.
#[derive(Debug, Clone)]
pub struct CarryRequest {
    pub organ_uid: String,
    pub root_key: String,
    /// What they call themselves. Untrusted, and shown beside the uid rather
    /// than instead of it.
    pub label: String,
    pub asked_at: String,
}

/// Record an ask, or refresh one already standing.
///
/// Idempotent by Organ: asking twice is one request, not two. A person whose
/// first ask went unanswered will ask again, and the answer to that is the
/// same row with a newer timestamp — not a second entry in a list the operator
/// then has to reconcile.
pub async fn ask_to_be_carried(
    pool: &SqlitePool,
    organ_uid: &str,
    root_key: &str,
    label: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO mailbox_request (organ_uid, root_key, label, asked_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(organ_uid) DO UPDATE SET
           root_key = excluded.root_key,
           label = excluded.label,
           asked_at = excluded.asked_at",
    )
    .bind(organ_uid)
    .bind(root_key)
    .bind(label)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn requests(pool: &SqlitePool) -> Result<Vec<CarryRequest>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM mailbox_request ORDER BY asked_at")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|r| CarryRequest {
                organ_uid: r.get("organ_uid"),
                root_key: r.get("root_key"),
                label: r.get("label"),
                asked_at: r.get("asked_at"),
            })
            .collect(),
    )
}

pub async fn request(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Option<CarryRequest>, StoreError> {
    Ok(requests(pool)
        .await?
        .into_iter()
        .find(|row| row.organ_uid == organ_uid))
}

/// Answer an ask by removing it. Accepting registers separately: the two are
/// not one statement, and an accept that failed to register must not also have
/// consumed the request.
pub async fn answer_request(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM mailbox_request WHERE organ_uid = ?")
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// One outstanding invite, as the operator's panel shows it. The token itself
/// is NOT here — it exists once, in the return value of `put_invite`.
#[derive(Debug, Clone)]
pub struct MailboxInvite {
    pub label: String,
    pub quota_bytes: i64,
    pub expires_at: String,
    pub created_at: String,
    pub used_at: Option<String>,
    pub used_by: Option<String>,
}

/// Store an invite by hash. The plaintext never reaches a row.
pub async fn put_invite(
    pool: &SqlitePool,
    token_hash: &str,
    label: &str,
    quota_bytes: i64,
    expires_at: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT OR REPLACE INTO mailbox_invite
           (token_hash, label, quota_bytes, expires_at, created_at, used_at, used_by)
         VALUES (?, ?, ?, ?, ?, NULL, NULL)",
    )
    .bind(token_hash)
    .bind(label)
    .bind(quota_bytes)
    .bind(expires_at)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Claim an invite for `organ_uid`, returning its terms if the claim won.
///
/// The UPDATE is the claim, exactly as for enrolment tokens: two Organs
/// racing the same code cannot both succeed, because only one of them changes
/// a row. Anything else — unknown, expired, already spent — returns `None`,
/// and the caller must not be able to tell those apart.
pub async fn redeem_invite(
    pool: &SqlitePool,
    token_hash: &str,
    organ_uid: &str,
) -> Result<Option<(String, i64)>, StoreError> {
    let now = Utc::now().to_rfc3339();
    let claimed = sqlx::query(
        "UPDATE mailbox_invite SET used_at = ?, used_by = ?
          WHERE token_hash = ? AND used_at IS NULL AND expires_at > ?",
    )
    .bind(&now)
    .bind(organ_uid)
    .bind(token_hash)
    .bind(&now)
    .execute(pool)
    .await?
    .rows_affected();
    if claimed == 0 {
        return Ok(None);
    }
    let row = sqlx::query("SELECT label, quota_bytes FROM mailbox_invite WHERE token_hash = ?")
        .bind(token_hash)
        .fetch_one(pool)
        .await?;
    Ok(Some((row.get("label"), row.get("quota_bytes"))))
}

/// Every invite ever issued, newest first, for the operator's panel.
pub async fn invites(pool: &SqlitePool) -> Result<Vec<MailboxInvite>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM mailbox_invite ORDER BY created_at DESC LIMIT 50")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|r| MailboxInvite {
                label: r.get("label"),
                quota_bytes: r.get("quota_bytes"),
                expires_at: r.get("expires_at"),
                created_at: r.get("created_at"),
                used_at: r.get("used_at"),
                used_by: r.get("used_by"),
            })
            .collect(),
    )
}
