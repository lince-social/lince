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
    }
}

const INSERT: &str = "INSERT OR IGNORE INTO sync_op
    (tbl, uid, field, kind, value, hlc, actor_organ)
    VALUES (?, ?, ?, ?, ?, ?, ?)";

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
) -> Result<Option<i64>, StoreError> {
    let res = sqlx::query(INSERT)
        .bind(tbl)
        .bind(uid)
        .bind(field)
        .bind(kind.as_str())
        .bind(value)
        .bind(hlc)
        .bind(actor_organ)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Ok(None);
    }
    let seq = res.last_insert_rowid();
    sqlx::query(ENQUEUE)
        .bind(tbl)
        .bind(uid)
        .bind(field)
        .bind(seq)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(relay_exclude.unwrap_or(""))
        .execute(pool)
        .await?;
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
    let res = sqlx::query(INSERT)
        .bind(tbl)
        .bind(uid)
        .bind(field)
        .bind(kind.as_str())
        .bind(&value)
        .bind(nucleus::hlc::next())
        .bind(actor)
        .execute(pool)
        .await?;
    if res.rows_affected() > 0 {
        sqlx::query(ENQUEUE)
            .bind(tbl)
            .bind(uid)
            .bind(field)
            .bind(res.last_insert_rowid())
            .bind(chrono::Utc::now().to_rfc3339())
            .bind("")
            .execute(pool)
            .await?;
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
pub async fn after(pool: &SqlitePool, seq: i64, limit: i64) -> Result<Vec<OpRow>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM sync_op WHERE seq > ? ORDER BY seq LIMIT ?")
            .bind(seq)
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
    Ok(sqlx::query("SELECT COALESCE(MAX(seq), 0) AS seq FROM sync_op")
        .fetch_one(pool)
        .await?
        .get("seq"))
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
