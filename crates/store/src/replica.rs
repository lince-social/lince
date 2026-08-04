//! Individual replica (Ontology §11 "Threads"): per-record-per-contact sync.
//!
//! `sync_out`/`sync_in` on `organ_contact` are per-CONTACT and mean the whole
//! visible feed. This is the other axis: THIS Record syncs to THESE peers and
//! to nobody else, and does not ride the general feed at all. A conversation
//! with one contact is exactly that, which is why messaging needs no delivery
//! subsystem of its own — it is Records, synced with one peer.
//!
//! The rule the whole module exists to enforce: a grant on the root covers
//! every Record inside it, and that containment is resolved ONCE at creation
//! (`record.replica_root`), never by walking Assertions at send time.

use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

/// A grant's state. Revoked is not a state — it is the absence of the row.
pub const OFFERED: &str = "offered";
pub const ACCEPTED: &str = "accepted";

/// Offer `root` to `contact`. Nothing flows yet: an offer is an invitation,
/// and ops move only once the receiver has accepted.
pub async fn offer(pool: &SqlitePool, root: &str, contact: &str) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO replica_grant (root_record, contact_organ, state, created_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(root_record, contact_organ) DO NOTHING",
    )
    .bind(root)
    .bind(contact)
    .bind(OFFERED)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Accept (or, on the sharer's side, confirm) a grant. Idempotent.
pub async fn accept(pool: &SqlitePool, root: &str, contact: &str) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO replica_grant (root_record, contact_organ, state, created_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(root_record, contact_organ) DO UPDATE SET state = excluded.state",
    )
    .bind(root)
    .bind(contact)
    .bind(ACCEPTED)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Revoke reach. This is the ONLY thing that stops a peer's ops being
/// accepted, and it is deliberately separate from deleting the Record:
///
///   * a purely local delete would leave the peer still pushing ops for a uid
///     that is gone, and import must DROP those, never resurrect the record;
///   * `tombstone` is a SYNCED op kind, so a delete that emitted one would
///     delete THEIR copy of the conversation too, which contradicts both
///     parties keeping a copy.
///
/// So deletion = local removal + this call. Their copy survives, their ops
/// stop being accepted, and nothing is reached into on their Cell — §12's
/// honest split between revoke (hard, local, guaranteed) and forget (a
/// request the remote may honour).
pub async fn revoke(pool: &SqlitePool, root: &str, contact: &str) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM replica_grant WHERE root_record = ? AND contact_organ = ?")
        .bind(root)
        .bind(contact)
        .execute(pool)
        .await?;
    // Queued ops for a revoked grant must not be delivered by a drain that is
    // already in flight.
    sqlx::query(
        "DELETE FROM sync_outbox
          WHERE contact_organ = ?
            AND uid IN (SELECT uid FROM record WHERE replica_root = ?)",
    )
    .bind(contact)
    .bind(root)
    .execute(pool)
    .await?;
    Ok(())
}

/// Make a Record its own individual-replica root.
///
/// The one legitimate post-creation stamp, and it is safe for exactly one
/// reason: a root is stamped the instant it is created, before anything is
/// inside it and before it has said anything. Its own create-time ops carry a
/// title the local user typed, not conversation content. Any OTHER
/// post-creation stamp reopens the window this design closes — a Record that
/// logged ops onto the general feed and only then became private has already
/// been served to every `sync_in` contact.
pub async fn make_own_root(pool: &SqlitePool, record_uid: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE record SET replica_root = uid WHERE uid = ? AND replica_root IS NULL")
        .bind(record_uid)
        .execute(pool)
        .await?;
    // Its create-time ops were logged before the stamp existed, so bring them
    // into the root too — otherwise the title would ride the general feed
    // while everything else stayed private.
    sqlx::query("UPDATE sync_op SET replica_root = ? WHERE tbl = 'record' AND uid = ?")
        .bind(record_uid)
        .bind(record_uid)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM sync_outbox WHERE tbl = 'record' AND uid = ?")
        .bind(record_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn state(
    pool: &SqlitePool,
    root: &str,
    contact: &str,
) -> Result<Option<String>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT state FROM replica_grant WHERE root_record = ? AND contact_organ = ?",
    )
    .bind(root)
    .bind(contact)
    .fetch_optional(pool)
    .await?)
}

/// Whether ops for `root` may move to/from `contact`. The single predicate the
/// outbox, the feed and the import gate all ask.
pub async fn is_accepted(pool: &SqlitePool, root: &str, contact: &str) -> Result<bool, StoreError> {
    Ok(state(pool, root, contact).await?.as_deref() == Some(ACCEPTED))
}

/// Every root this contact may see — used to serve their catch-up.
pub async fn roots_for_contact(
    pool: &SqlitePool,
    contact: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT root_record FROM replica_grant WHERE contact_organ = ? AND state = ?",
    )
    .bind(contact)
    .bind(ACCEPTED)
    .fetch_all(pool)
    .await?)
}

/// The root governing a Record, or `None` when it rides the general feed.
pub async fn root_of(pool: &SqlitePool, record_uid: &str) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query_scalar::<_, Option<String>>("SELECT replica_root FROM record WHERE uid = ?")
            .bind(record_uid)
            .fetch_optional(pool)
            .await?
            .flatten(),
    )
}

/// The root governing an OP, resolved from whatever row it targets.
///
/// Scope is deliberately narrow: only `record` and `record_assertion` rows can
/// be individually replicated today, which is everything a conversation needs
/// (a Record per message, Assertions joining them). Anything else resolves to
/// `None` and rides the general feed, which is the safe direction to fail —
/// a Record that should have been private and is not would be a leak, whereas
/// this errs toward "not eligible for a private channel".
pub async fn root_for_op(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
) -> Result<Option<String>, StoreError> {
    match tbl {
        "record" => root_of(pool, uid).await,
        "record_assertion" => Ok(sqlx::query_scalar::<_, Option<String>>(
            "SELECT r.replica_root FROM record_assertion a
               JOIN record r ON r.uid = a.subject_uid
              WHERE a.uid = ?",
        )
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .flatten()),
        _ => Ok(None),
    }
}

/// The root two Assertion endpoints agree on, for an Assertion about to be
/// created.
///
/// If either side is inside a root, the Assertion belongs to that root —
/// otherwise an Assertion whose subject is a general-feed Record and whose
/// object is a private message would put the private uid on the general feed.
/// If the two sides sit in DIFFERENT roots, there is no correct answer and the
/// caller must refuse: joining two separately-shared conversations would
/// silently widen both.
pub async fn root_for_link(
    pool: &SqlitePool,
    subject_uid: &str,
    object_uid: &str,
) -> Result<Result<Option<String>, &'static str>, StoreError> {
    let subject = root_of(pool, subject_uid).await?;
    let object = root_of(pool, object_uid).await?;
    Ok(match (subject, object) {
        (Some(a), Some(b)) if a != b => Err("cross-root link"),
        (Some(a), _) => Ok(Some(a)),
        (None, Some(b)) => Ok(Some(b)),
        (None, None) => Ok(None),
    })
}

/// Records inside a root, oldest first — a conversation's contents.
pub async fn records_in_root(pool: &SqlitePool, root: &str) -> Result<Vec<String>, StoreError> {
    Ok(
        sqlx::query("SELECT uid FROM record WHERE replica_root = ? ORDER BY created_at")
            .bind(root)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|row| row.get::<String, _>("uid"))
            .collect(),
    )
}
