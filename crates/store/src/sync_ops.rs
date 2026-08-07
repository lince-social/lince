//! Sync op log (Ontology §11): the outbound feed of field-level operations.
//!
//! Every local write on a syncable table calls `log_local`/`log_local_tx`
//! right where the SQL write happens, so the log and the read model can never
//! disagree. A Cell with no local organ yet (early bootstrap, most unit tests)
//! has no sync identity — logging is silently skipped, never an error.
//!
//! Applying a REMOTE op must NOT go through these helpers: the import path
//! appends the op with its ORIGINAL identity (actor_organ, hlc) via `append`,
//! so downstream contacts can relay it.

use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Set,
    Tombstone,
    Fact,
    Crdt,
}

impl OpKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::Tombstone => "tombstone",
            Self::Fact => "fact",
            Self::Crdt => "crdt",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "set" => Self::Set,
            "tombstone" => Self::Tombstone,
            "fact" => Self::Fact,
            "crdt" => Self::Crdt,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OpRow {
    pub seq: i64,
    pub tbl: String,
    pub uid: String,
    pub field: String,
    pub kind: String,
    pub value: Option<String>,
    pub hlc: i64,
    pub actor_organ: String,
    /// The individual-replica root this op belongs to, or `None` for the
    /// general feed. Decides WHICH channel the op may leave on.
    pub replica_root: Option<String>,
}

fn map(row: sqlx::sqlite::SqliteRow) -> OpRow {
    OpRow {
        seq: row.get("seq"),
        tbl: row.get("tbl"),
        uid: row.get("uid"),
        field: row.get("field"),
        kind: row.get("kind"),
        value: row.get("value"),
        hlc: row.get("hlc"),
        actor_organ: row.get("actor_organ"),
        replica_root: row.get("replica_root"),
    }
}

const INSERT: &str = "INSERT OR IGNORE INTO sync_op
    (tbl, uid, field, kind, value, hlc, actor_organ, replica_root)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?)";

/// Queue one op for every contact holding an ACCEPTED grant on its root — the
/// individual-replica counterpart of `ENQUEUE`. A Record inside a root never
/// rides the general feed, so exactly one of the two statements runs per op.
///
/// `trust != 'blocked'` matters as much here as on the general feed: blocked
/// is terminal everywhere (Ontology §2), and a grant does not survive it.
const ENQUEUE_GRANT: &str =
    "INSERT INTO sync_outbox (contact_organ, tbl, uid, field, seq, queued_at)
    SELECT g.contact_organ, ?, ?, ?, ?, ?
      FROM replica_grant g
      JOIN organ_contact c ON c.record_uid = g.contact_organ
     WHERE g.root_record = ? AND g.state = 'accepted'
       AND c.trust != 'blocked' AND g.contact_organ != ?
    ON CONFLICT(contact_organ, tbl, uid, field)
    DO UPDATE SET seq = excluded.seq, queued_at = excluded.queued_at";

/// Queue one op for every sync-out contact (except an optional relay source),
/// replacing any older queued op on the same (contact, tbl, uid, field) —
/// the bounded outbox (Ontology §11 "Reactive deltas").
const ENQUEUE: &str = "INSERT INTO sync_outbox (contact_organ, tbl, uid, field, seq, queued_at)
    SELECT record_uid, ?, ?, ?, ?, ?
      FROM organ_contact
     WHERE sync_out = 1 AND trust != 'blocked' AND record_uid != ?
    ON CONFLICT(contact_organ, tbl, uid, field)
    DO UPDATE SET seq = excluded.seq, queued_at = excluded.queued_at";

/// Append an op with an explicit identity — the import/relay path. Returns the
/// local seq, or `None` when the identity already exists (idempotent import).
/// The op is queued for every sync-out contact except `relay_exclude` (the
/// contact it just came from must not be echoed).
pub async fn append(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
    field: &str,
    kind: OpKind,
    value: Option<&str>,
    hlc: i64,
    actor_organ: &str,
    relay_exclude: Option<&str>,
    replica_root: Option<&str>,
) -> Result<Option<i64>, StoreError> {
    let res = sqlx::query(INSERT)
        .bind(tbl)
        .bind(uid)
        .bind(field)
        .bind(kind.as_str())
        .bind(value)
        .bind(hlc)
        .bind(actor_organ)
        .bind(replica_root)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Ok(None);
    }
    let seq = res.last_insert_rowid();
    let now = chrono::Utc::now().to_rfc3339();
    // Relaying onward follows the same split as a local write: an op inside a
    // root goes to that root's grant holders and never to the general feed.
    match replica_root {
        Some(root) => {
            sqlx::query(ENQUEUE_GRANT)
                .bind(tbl)
                .bind(uid)
                .bind(field)
                .bind(seq)
                .bind(&now)
                .bind(root)
                .bind(relay_exclude.unwrap_or(""))
                .execute(pool)
                .await?;
        }
        None => {
            sqlx::query(ENQUEUE)
                .bind(tbl)
                .bind(uid)
                .bind(field)
                .bind(seq)
                .bind(&now)
                .bind(relay_exclude.unwrap_or(""))
                .execute(pool)
                .await?;
        }
    }
    Ok(Some(seq))
}

/// The uid of the local organ, if the Cell has bootstrapped one.
async fn local_organ_uid_tx(
    tx: &mut Transaction<'_, Sqlite>,
) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query("SELECT uid FROM record WHERE slug = ? AND kind = 'organ' LIMIT 1")
            .bind(crate::organs::LOCAL_ORGAN_SLUG)
            .fetch_optional(&mut **tx)
            .await?
            .map(|row| row.get("uid")),
    )
}

async fn local_organ_uid(pool: &SqlitePool) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query("SELECT uid FROM record WHERE slug = ? AND kind = 'organ' LIMIT 1")
            .bind(crate::organs::LOCAL_ORGAN_SLUG)
            .fetch_optional(pool)
            .await?
            .map(|row| row.get("uid")),
    )
}

/// Log a LOCAL write as an op: actor is the local organ, stamp is the Cell's
/// HLC. No local organ = no sync identity = skip (never an error).
pub async fn log_local(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
    field: &str,
    kind: OpKind,
    value: Option<String>,
) -> Result<(), StoreError> {
    let Some(actor) = local_organ_uid(pool).await? else {
        return Ok(());
    };
    // Resolved BEFORE the op is written, from the row it targets. Because
    // `record.replica_root` is stamped at creation and immutable, this is the
    // same answer forever — which is what lets every later reader trust the
    // copy on the op instead of re-deriving it.
    let root = crate::replica::root_for_op(pool, tbl, uid).await?;
    let res = sqlx::query(INSERT)
        .bind(tbl)
        .bind(uid)
        .bind(field)
        .bind(kind.as_str())
        .bind(&value)
        .bind(nucleus::hlc::next())
        .bind(actor)
        .bind(&root)
        .execute(pool)
        .await?;
    if res.rows_affected() > 0 {
        let seq = res.last_insert_rowid();
        let now = chrono::Utc::now().to_rfc3339();
        match &root {
            // Individually replicated: grant holders only, and NOT the general
            // feed. Riding both would be the leak the whole axis exists to
            // prevent.
            Some(root) => {
                sqlx::query(ENQUEUE_GRANT)
                    .bind(tbl)
                    .bind(uid)
                    .bind(field)
                    .bind(seq)
                    .bind(&now)
                    .bind(root)
                    .bind("")
                    .execute(pool)
                    .await?;
            }
            None => {
                sqlx::query(ENQUEUE)
                    .bind(tbl)
                    .bind(uid)
                    .bind(field)
                    .bind(seq)
                    .bind(&now)
                    .bind("")
                    .execute(pool)
                    .await?;
            }
        }
    }
    Ok(())
}

/// `log_local` inside a caller-owned transaction (the in-memory pool has one
/// connection, so a pool query while a tx is open would deadlock).
pub async fn log_local_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tbl: &str,
    uid: &str,
    field: &str,
    kind: OpKind,
    value: Option<String>,
) -> Result<(), StoreError> {
    let Some(actor) = local_organ_uid_tx(tx).await? else {
        return Ok(());
    };
    let res = sqlx::query(INSERT)
        .bind(tbl)
        .bind(uid)
        .bind(field)
        .bind(kind.as_str())
        .bind(&value)
        .bind(nucleus::hlc::next())
        .bind(actor)
        .execute(&mut **tx)
        .await?;
    if res.rows_affected() > 0 {
        sqlx::query(ENQUEUE)
            .bind(tbl)
            .bind(uid)
            .bind(field)
            .bind(res.last_insert_rowid())
            .bind(chrono::Utc::now().to_rfc3339())
            .bind("")
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// Ops past a checkpoint, oldest first — the catch-up feed. One indexed
/// rowid-range query; an empty answer means converged.
/// The GENERAL feed only: `replica_root IS NULL`.
///
/// This is enforcement point two of three. An individually-replicated Record
/// must never appear here, or a contact with plain `sync_in` would receive a
/// conversation they were never granted — which is the exact failure the axis
/// exists to prevent, and it would be invisible because the catch-up feed is
/// served automatically.
pub async fn after(pool: &SqlitePool, seq: i64, limit: i64) -> Result<Vec<OpRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM sync_op WHERE seq > ? AND replica_root IS NULL ORDER BY seq LIMIT ?",
    )
    .bind(seq)
    .bind(limit)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map)
    .collect())
}

/// The catch-up feed for ONE grant root. The caller must have already checked
/// that the requesting contact holds an accepted grant on `root` — this
/// function selects, it does not authorize.
pub async fn after_in_root(
    pool: &SqlitePool,
    root: &str,
    seq: i64,
    limit: i64,
) -> Result<Vec<OpRow>, StoreError> {
    Ok(
        sqlx::query(
            "SELECT * FROM sync_op WHERE seq > ? AND replica_root = ? ORDER BY seq LIMIT ?",
        )
        .bind(seq)
        .bind(root)
        .bind(limit)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map)
        .collect(),
    )
}

/// Ops targeting one (table, uid, field) — merge/inspection helper.
pub async fn for_field(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
    field: &str,
) -> Result<Vec<OpRow>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM sync_op WHERE tbl = ? AND uid = ? AND field = ? ORDER BY seq")
            .bind(tbl)
            .bind(uid)
            .bind(field)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(map)
            .collect(),
    )
}

/// The stored HLC of a field's current value: the max stamp among its ops.
/// This is the LWW compare on import — no field-HLC sidecar exists anywhere.
pub async fn latest_hlc_for_field(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
    field: &str,
) -> Result<Option<i64>, StoreError> {
    Ok(
        sqlx::query("SELECT MAX(hlc) AS hlc FROM sync_op WHERE tbl = ? AND uid = ? AND field = ?")
            .bind(tbl)
            .bind(uid)
            .bind(field)
            .fetch_one(pool)
            .await?
            .get::<Option<i64>, _>("hlc"),
    )
}

/// Max HLC in the whole log — boot seeds the Cell clock past it.
pub async fn max_hlc(pool: &SqlitePool) -> Result<Option<i64>, StoreError> {
    Ok(sqlx::query("SELECT MAX(hlc) AS hlc FROM sync_op")
        .fetch_one(pool)
        .await?
        .get::<Option<i64>, _>("hlc"))
}

/// Latest assigned seq (0 for an empty log) — the serve-side cursor head.
pub async fn max_seq(pool: &SqlitePool) -> Result<i64, StoreError> {
    Ok(
        sqlx::query("SELECT COALESCE(MAX(seq), 0) AS seq FROM sync_op")
            .fetch_one(pool)
            .await?
            .get("seq"),
    )
}

/// What `prune` did, or would do. Returned rather than logged so a caller can
/// show it before committing to anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PruneReport {
    /// The highest seq every synced contact has confirmed receiving.
    pub floor: i64,
    /// Ops deleted (or deletable, for a dry run).
    pub removed: i64,
    /// Ops at or below the floor that were KEPT because something still needs
    /// them — a queued outbox row, or a `crdt` op not yet folded into a
    /// snapshot.
    pub retained: i64,
}

/// The retention floor: the lowest `peer_acked_seq` across contacts we still
/// owe ops to. `None` means there is no floor and nothing may be pruned.
///
/// Blocked contacts are excluded — we will never send to them again, so a
/// blocked peer stuck at seq 0 must not freeze retention forever. Contacts
/// that receive nothing from us (`sync_out = 0` and no accepted grant) are
/// excluded for the same reason.
pub async fn retention_floor(pool: &SqlitePool) -> Result<Option<i64>, StoreError> {
    let row = sqlx::query(
        "SELECT MIN(c.peer_acked_seq) AS floor, COUNT(*) AS n
           FROM organ_contact c
          WHERE c.trust != 'blocked'
            AND (c.sync_out = 1
                 OR EXISTS (SELECT 1 FROM replica_grant g
                             WHERE g.contact_organ = c.record_uid))",
    )
    .fetch_one(pool)
    .await?;
    let contacts: i64 = row.get("n");
    if contacts == 0 {
        // No one to relay to. Pruning would be safe for THEM and unsafe for
        // anyone added later: there is no bootstrap-from-snapshot path yet, so
        // a new contact can only be brought up to date by replaying the log.
        // Keep everything and let the caller decide.
        return Ok(None);
    }
    Ok(Some(row.get::<i64, _>("floor")))
}

/// Drop ops every synced contact has already received.
///
/// Deliberately NOT automatic, and this is the reason: a contact that falls
/// behind the pruned floor recovers by re-bootstrapping from a serve-time
/// snapshot, and that path **is not built** (Ontology §11c, "Replica bootstrap
/// and initial snapshot"). Until it exists, pruning is a one-way trade of
/// recoverability for disk, so it is an explicit operation a human asks for
/// rather than a loop that quietly runs at boot.
///
/// **Only SUPERSEDED ops are prunable**, and that one rule is what makes a
/// replica bootstrap possible with no bootstrap protocol at all. An op is
/// superseded when a NEWER op exists for the same `(tbl, uid, field)` on the
/// same channel — so the log always retains, for every live field, the op that
/// established its current value. The surviving log is therefore current state
/// plus recent history, and a contact added long after a prune builds a
/// complete replica by replaying from zero like any other.
///
/// It also keeps the LWW memory intact. Import decides "is this op older than
/// what I hold?" by reading the highest HLC for that field OUT OF THIS LOG
/// (`latest_hlc_for_field`). Pruning a field's newest op would erase that
/// memory, and a later-arriving stale value would then look new and overwrite
/// current data. Keeping the tip keeps the comparison honest.
///
/// Growth is still bounded, because history is what accumulates: a field
/// rewritten ten thousand times keeps one op. The log becomes O(live state)
/// rather than O(edit history), which was the actual goal.
///
/// Two further exceptions, both load-bearing:
///
/// 1. **An op still referenced by `sync_outbox`.** The outbox holds at most one
///    row per (contact, tbl, uid, field) and points into the log by `seq`, so a
///    queued row for an offline contact can easily sit below another contact's
///    checkpoint. Deleting it turns a pending delivery into a silent no-op.
/// 2. **`crdt` ops are never pruned.** Each one is the cumulative tail since
///    the last stored SNAPSHOT, and that snapshot lives in `record_doc` — which
///    is local, not in the log. A peer replaying from zero has no snapshot, so
///    dropping any crdt op would lose the text written before it with no way to
///    recover it. Compaction-gated crdt pruning needs the serve path to ship
///    `record_doc.snapshot` as part of a bootstrap, and that needs a synthesized
///    op identity — `(actor_organ, hlc)` is the unique index import dedupes on,
///    so inventing one is not free. Deferred rather than guessed at.
pub async fn prune(pool: &SqlitePool, dry_run: bool) -> Result<PruneReport, StoreError> {
    let Some(floor) = retention_floor(pool).await? else {
        return Ok(PruneReport {
            floor: 0,
            removed: 0,
            retained: 0,
        });
    };

    // One predicate, used for both the count and the delete, so a dry run can
    // never disagree with what the delete would do.
    //
    // `n.field = sync_op.field` matches exactly, empty string included: record
    // tombstones, assertions and crdt ops all use `field = ''`, so treating it
    // as a wildcard would let a tombstone and a crdt op on one uid look like
    // the same target and silently supersede each other.
    //
    // `IS` rather than `=` on `replica_root` because it is NULL for the general
    // feed and `=` never matches NULL. Comparing within one channel matters:
    // the general feed and a grant channel are served separately, so an op must
    // only be considered superseded by something a peer would receive ALONGSIDE
    // it.
    const PRUNABLE: &str = "seq <= ?
           AND kind != 'crdt'
           AND seq NOT IN (SELECT seq FROM sync_outbox)
           AND EXISTS (SELECT 1 FROM sync_op n
                        WHERE n.tbl = sync_op.tbl
                          AND n.uid = sync_op.uid
                          AND n.field = sync_op.field
                          AND n.replica_root IS sync_op.replica_root
                          AND n.seq > sync_op.seq)";

    let removable: i64 =
        sqlx::query(&format!("SELECT COUNT(*) AS n FROM sync_op WHERE {PRUNABLE}"))
            .bind(floor)
            .fetch_one(pool)
            .await?
            .get("n");
    let at_or_below: i64 = sqlx::query("SELECT COUNT(*) AS n FROM sync_op WHERE seq <= ?")
        .bind(floor)
        .fetch_one(pool)
        .await?
        .get("n");

    if !dry_run && removable > 0 {
        sqlx::query(&format!("DELETE FROM sync_op WHERE {PRUNABLE}"))
            .bind(floor)
            .execute(pool)
            .await?;
    }
    Ok(PruneReport {
        floor,
        removed: removable,
        retained: at_or_below - removable,
    })
}

/// One op by its local seq — outbox hydration (a pruned seq returns None).
pub async fn get_by_seq(pool: &SqlitePool, seq: i64) -> Result<Option<OpRow>, StoreError> {
    Ok(sqlx::query("SELECT * FROM sync_op WHERE seq = ?")
        .bind(seq)
        .fetch_optional(pool)
        .await?
        .map(map))
}

/// The latest `set` stamp on any field of a row — what a record tombstone must
/// beat: "undelete is a newer write" means alive iff latest set > latest
/// tombstone.
pub async fn latest_set_hlc_for_row(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
) -> Result<Option<i64>, StoreError> {
    Ok(sqlx::query(
        "SELECT MAX(hlc) AS hlc FROM sync_op WHERE tbl = ? AND uid = ? AND kind = 'set'",
    )
    .bind(tbl)
    .bind(uid)
    .fetch_one(pool)
    .await?
    .get::<Option<i64>, _>("hlc"))
}

// ------------------------------------------------------- bounded outbox

#[derive(Debug, Clone)]
pub struct OutboxRow {
    pub contact_organ: String,
    pub tbl: String,
    pub uid: String,
    pub field: String,
    pub seq: i64,
    pub attempts: i64,
}

/// Every queued op, grouped by contact (stable order), oldest op first.
pub async fn outbox_due(pool: &SqlitePool) -> Result<Vec<OutboxRow>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM sync_outbox ORDER BY contact_organ, seq")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|r| OutboxRow {
                contact_organ: r.get("contact_organ"),
                tbl: r.get("tbl"),
                uid: r.get("uid"),
                field: r.get("field"),
                seq: r.get("seq"),
                attempts: r.get("attempts"),
            })
            .collect(),
    )
}

/// Delete one delivered outbox row — guarded by seq, so an op that was
/// replaced by a newer one while in flight stays queued for the next drain.
pub async fn outbox_delete(pool: &SqlitePool, row: &OutboxRow) -> Result<(), StoreError> {
    sqlx::query(
        "DELETE FROM sync_outbox
          WHERE contact_organ = ? AND tbl = ? AND uid = ? AND field = ? AND seq = ?",
    )
    .bind(&row.contact_organ)
    .bind(&row.tbl)
    .bind(&row.uid)
    .bind(&row.field)
    .bind(row.seq)
    .execute(pool)
    .await?;
    Ok(())
}

/// A failed send: bump attempts on everything queued for the contact; rows
/// stay queued and retry next drain.
pub async fn outbox_bump_attempts(
    pool: &SqlitePool,
    contact_organ: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE sync_outbox SET attempts = attempts + 1 WHERE contact_organ = ?")
        .bind(contact_organ)
        .execute(pool)
        .await?;
    Ok(())
}

/// Drop everything queued for a contact (it vanished or is blocked).
pub async fn outbox_clear_contact(
    pool: &SqlitePool,
    contact_organ: &str,
) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM sync_outbox WHERE contact_organ = ?")
        .bind(contact_organ)
        .execute(pool)
        .await?;
    Ok(())
}
