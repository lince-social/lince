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

/// A conversation this Cell shares with one contact, in whatever state the
/// grant is in.
#[derive(Debug, Clone)]
pub struct SharedConversation {
    pub uid: String,
    pub head: String,
    /// `offered` or `accepted`. Both mean a conversation EXISTS — a surface
    /// asking "do I already have one with them" must not treat an offer that
    /// has not been answered yet as nothing.
    pub state: String,
}

/// The conversations shared with `contact`, newest first.
///
/// Deliberately not `roots_for_contact`: that one answers "what may flow", so
/// it filters to `accepted`. This one answers "what already exists", and an
/// unanswered offer is the case that most needs an answer of yes — re-offering
/// it would mint a second conversation next to the pending one.
///
/// The kind filter is not decoration: `replica_grant.root_record` is any root,
/// and `make_own_root` is called for records that are not conversations.
pub async fn conversations_with(
    pool: &SqlitePool,
    contact: &str,
) -> Result<Vec<SharedConversation>, StoreError> {
    let rows = sqlx::query(
        "SELECT r.uid, r.head, g.state
           FROM replica_grant g
           JOIN record r ON r.uid = g.root_record
          WHERE g.contact_organ = ? AND r.kind = 'conversation'
          ORDER BY r.created_at DESC",
    )
    .bind(contact)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| SharedConversation {
            uid: row.get("uid"),
            head: row.get("head"),
            state: row.get("state"),
        })
        .collect())
}

/// The root governing a Record, or `None` when it rides the general feed.
/// `root_for_op` inside a caller-owned transaction.
///
/// Exists because the in-memory pool has ONE connection, so a pool query while
/// a transaction is open deadlocks — the same reason `log_local_tx` exists at
/// all. Kept beside its pool twin so the two cannot drift: they must answer
/// identically, or the same write lands on the general feed or off it
/// depending only on which caller logged it.
pub async fn root_for_op_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    tbl: &str,
    uid: &str,
) -> Result<Option<String>, StoreError> {
    Ok(match tbl {
        "record" => sqlx::query_scalar::<_, Option<String>>(
            "SELECT replica_root FROM record WHERE uid = ?",
        )
        .bind(uid)
        .fetch_optional(&mut **tx)
        .await?
        .flatten(),
        "record_assertion" => sqlx::query_scalar::<_, Option<String>>(
            "SELECT r.replica_root FROM record_assertion a
               JOIN record r ON r.uid = a.subject_uid
              WHERE a.uid = ?",
        )
        .bind(uid)
        .fetch_optional(&mut **tx)
        .await?
        .flatten(),
        _ => None,
    })
}

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

/// Whether a message INSIDE `root` references `record` (Ontology §11, C6).
///
/// This is what authorises a live reference read, and it is the reason such a
/// read is not a general "give me that uid" oracle: the pointer has to exist,
/// in a conversation the asker was granted, before the §12 gate is even asked
/// what the Record looks like.
///
/// A RETRACTED reference does not count. Removing the mention is the only way
/// to take back a pointer once it has been posted, so treating a retracted one
/// as live would make that gesture do nothing.
pub async fn root_references_record(
    pool: &SqlitePool,
    root: &str,
    record: &str,
) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM record_assertion a
           JOIN record m ON m.uid = a.subject_uid
          WHERE a.object_uid = ? AND m.replica_root = ?
            AND a.retracted_at IS NULL
          LIMIT 1",
    )
    .bind(record)
    .bind(root)
    .fetch_optional(pool)
    .await?
    .is_some())
}

/// One read of a live reference, from the OWNER's side (Ontology §11, C6).
///
/// Recorded because it is observable whether or not we record it: the read is a
/// live request against this Cell, so the alternative is not privacy, it is an
/// invisible side effect. Collapsed to a count and a last-read time rather than
/// a row per read — a conversation left open in a tab would otherwise turn a
/// receipt into a surveillance log, and "when did they last look" is the
/// question anyone actually has.
pub async fn note_reference_read(
    pool: &SqlitePool,
    reader_organ: &str,
    record: &str,
    root: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO reference_read (reader_organ, record_uid, root_record, reads, last_read_at)
         VALUES (?, ?, ?, 1, ?)
         ON CONFLICT(reader_organ, record_uid, root_record)
         DO UPDATE SET reads = reads + 1, last_read_at = excluded.last_read_at",
    )
    .bind(reader_organ)
    .bind(record)
    .bind(root)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Who has read a Record of ours through a reference, most recent first.
pub async fn reference_reads(
    pool: &SqlitePool,
    record: &str,
) -> Result<Vec<(String, i64, String)>, StoreError> {
    Ok(sqlx::query(
        "SELECT reader_organ, reads, last_read_at FROM reference_read
          WHERE record_uid = ? ORDER BY last_read_at DESC",
    )
    .bind(record)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| {
        (
            row.get("reader_organ"),
            row.get("reads"),
            row.get("last_read_at"),
        )
    })
    .collect())
}

/// Remove a conversation from THIS Cell, locally and without logging anything
/// (Ontology §11, C6).
///
/// The other half of `revoke`, which that function's own comment already
/// describes: deletion is local removal plus revocation. Both halves are needed
/// and neither is enough — revoking alone leaves the conversation sitting in
/// the list, and removing alone leaves their ops still welcome, so the thread
/// would quietly repopulate on the next sync.
///
/// UNLOGGED, deliberately, and this is the part that would be wrong the
/// obvious way: `tombstone` is a SYNCED op kind, so deleting these Records
/// through the ordinary path would emit tombstones that travel down the grant
/// channel and delete THEIR copy too. Nobody agreed to that. Deleting a
/// conversation ends it here; their copy is theirs, and §12's revoke/forget
/// split says plainly that we cannot reach into their Cell.
///
/// What remains afterwards is exactly one thing: they may send an INVITE to
/// open a new conversation, one pending at a time — which is a knock, not a
/// channel.
pub async fn delete_root_locally(pool: &SqlitePool, root: &str) -> Result<u64, StoreError> {
    // Grants first. If this failed halfway, the safe half to have done is the
    // one that stops accepting their ops — the opposite order could leave a
    // deleted conversation still importing.
    sqlx::query("DELETE FROM replica_grant WHERE root_record = ?")
        .bind(root)
        .execute(pool)
        .await?;
    sqlx::query(
        "DELETE FROM sync_outbox WHERE uid IN (SELECT uid FROM record WHERE replica_root = ?)",
    )
    .bind(root)
    .execute(pool)
    .await?;
    // The ops go too. They are only ever served on the grant channel for this
    // root, so with the grants gone they can reach nobody — and keeping them
    // would leave the conversation reconstructible from our own log by a
    // rebuild, which is not what "deleted" means to the person who asked.
    sqlx::query("DELETE FROM sync_op WHERE replica_root = ?")
        .bind(root)
        .execute(pool)
        .await?;
    // What HANGS OFF those Records has to go first, or the foreign keys refuse
    // the delete. Both directions of an Assertion are covered: a reference
    // posted in this conversation has its subject inside the root and its
    // object out on the general feed, and deleting only by subject would leave
    // the reverse case behind for whatever adds one next.
    sqlx::query(
        "DELETE FROM record_assertion
          WHERE subject_uid IN (SELECT uid FROM record WHERE replica_root = ?)
             OR object_uid IN (SELECT uid FROM record WHERE replica_root = ?)",
    )
    .bind(root)
    .bind(root)
    .execute(pool)
    .await?;
    sqlx::query(
        "DELETE FROM fact WHERE record_uid IN (SELECT uid FROM record WHERE replica_root = ?)",
    )
    .bind(root)
    .execute(pool)
    .await?;
    // Read receipts for Records inside the conversation. Keeping them would
    // leave a record of who read what in a conversation that no longer exists,
    // which is the opposite of what deleting it means.
    sqlx::query(
        "DELETE FROM reference_read
          WHERE record_uid IN (SELECT uid FROM record WHERE replica_root = ?)
             OR root_record = ?",
    )
    .bind(root)
    .bind(root)
    .execute(pool)
    .await?;
    // The root Record itself carries `replica_root = uid`, so this one
    // statement covers the conversation, its threads and its messages.
    let removed = sqlx::query("DELETE FROM record WHERE replica_root = ?")
        .bind(root)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(removed)
}
