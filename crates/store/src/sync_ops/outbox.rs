use nucleus::execution::now;
use sqlx::{Row, SqlitePool, query, query_as, sqlite::SqliteRow};

use super::op_in_scope;
use crate::StoreError;

/// An outgoing operation awaiting delivery to a particular Organ.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OutboxRow {
    pub contact_organ: String,
    pub tbl: String,
    pub uid: String,
    pub field: String,
    pub kind: String,
    pub seq: i64,
    pub attempts: i64,
}

/// Lists pending deliveries in contact and sequence order.
pub async fn outbox_due(pool: &SqlitePool) -> Result<Vec<OutboxRow>, StoreError> {
    query_as("SELECT * FROM sync_outbox ORDER BY contact_organ, seq")
        .fetch_all(pool)
        .await
}

/// Acknowledges this exact queued sequence. A newer replacement survives.
pub async fn outbox_delete(pool: &SqlitePool, row: &OutboxRow) -> Result<(), StoreError> {
    query(
        "DELETE FROM sync_outbox
          WHERE contact_organ = ? AND tbl = ? AND uid = ? AND field = ? AND kind = ?
            AND seq = ?",
    )
    .bind(&row.contact_organ)
    .bind(&row.tbl)
    .bind(&row.uid)
    .bind(&row.field)
    .bind(&row.kind)
    .bind(row.seq)
    .execute(pool)
    .await?;
    Ok(())
}

/// Increments attempts on all pending deliveries for this contact.
pub async fn outbox_bump_attempts(
    pool: &SqlitePool,
    contact_organ: &str,
) -> Result<(), StoreError> {
    query("UPDATE sync_outbox SET attempts = attempts + 1 WHERE contact_organ = ?")
        .bind(contact_organ)
        .execute(pool)
        .await?;
    Ok(())
}

/// Removes queued deliveries for this contact.
pub async fn outbox_clear_contact(
    pool: &SqlitePool,
    contact_organ: &str,
) -> Result<(), StoreError> {
    query("DELETE FROM sync_outbox WHERE contact_organ = ?")
        .bind(contact_organ)
        .execute(pool)
        .await?;
    Ok(())
}

/// Replays a Record's general-feed operations in sequence order. The return
/// value counts scanned operations, even if the contact is ineligible.
pub async fn enqueue_record_for_contact(
    pool: &SqlitePool,
    contact_organ: &str,
    record_uid: &str,
) -> Result<usize, StoreError> {
    let rows = query(
        "SELECT tbl, uid, field, kind, seq FROM sync_op
          WHERE replica_root IS NULL
            AND (
              (tbl = 'record' AND uid = ?1)
              OR (tbl = 'fact' AND uid IN (SELECT uid FROM fact WHERE record_uid = ?1))
              OR (tbl = 'record_assertion' AND uid IN (
                    SELECT uid FROM record_assertion
                     WHERE subject_uid = ?1 OR object_uid = ?1))
            )
          -- Seq order, and it is load-bearing rather than tidy: the genesis
          -- `quantity` op is an OPENING value and every later change adds to
          -- it, so a replay that arrived out of order would still converge but
          -- a truncated one would not.
          ORDER BY seq",
    )
    .bind(record_uid)
    .fetch_all(pool)
    .await?;
    let now = now().to_rfc3339();
    let scanned = rows.len();
    for row in rows {
        Pending::from_row(&row)
            .enqueue(pool, contact_organ, &now)
            .await?;
    }
    Ok(scanned)
}

/// Queues general-feed operation tips newly admitted by the widened scope.
/// The return value counts the rows inserted or updated.
pub async fn enqueue_widened_for_contact(
    pool: &SqlitePool,
    contact_organ: &str,
    before: Option<&[String]>,
    after: Option<&[String]>,
) -> Result<u64, StoreError> {
    let rows = query(
        "SELECT o.tbl, o.uid, o.field, o.kind, MAX(o.seq) AS seq
           FROM sync_op o
          WHERE o.replica_root IS NULL
          GROUP BY o.tbl, o.uid, o.field, o.kind",
    )
    .fetch_all(pool)
    .await?;
    let now = now().to_rfc3339();
    let mut queued = 0;
    for row in rows {
        let pending = Pending::from_row(&row);
        if pending.in_scope(after) && !pending.in_scope(before) {
            queued += pending.enqueue(pool, contact_organ, &now).await?;
        }
    }
    Ok(queued)
}

struct Pending<'a> {
    tbl: &'a str,
    uid: &'a str,
    field: &'a str,
    kind: &'a str,
    seq: i64,
}

impl<'a> Pending<'a> {
    fn from_row(row: &'a SqliteRow) -> Self {
        Self {
            tbl: row.get("tbl"),
            uid: row.get("uid"),
            field: row.get("field"),
            kind: row.get("kind"),
            seq: row.get("seq"),
        }
    }

    fn in_scope(&self, scope: Option<&[String]>) -> bool {
        op_in_scope(self.tbl, self.kind, self.field, scope)
    }

    async fn enqueue(
        &self,
        pool: &SqlitePool,
        contact: &str,
        now: &str,
    ) -> Result<u64, StoreError> {
        Ok(query(
            "INSERT INTO sync_outbox (contact_organ, tbl, uid, field, kind, seq, queued_at)
             SELECT ?, ?, ?, ?, ?, ?, ?
               FROM organ_contact
              WHERE record_uid = ? AND sync_out = 1 AND trust != 'blocked'
             ON CONFLICT(contact_organ, tbl, uid, field, kind)
             DO UPDATE SET seq = excluded.seq, queued_at = excluded.queued_at",
        )
        .bind(contact)
        .bind(self.tbl)
        .bind(self.uid)
        .bind(self.field)
        .bind(self.kind)
        .bind(self.seq)
        .bind(now)
        .bind(contact)
        .execute(pool)
        .await?
        .rows_affected())
    }
}
