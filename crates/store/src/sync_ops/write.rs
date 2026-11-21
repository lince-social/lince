use nucleus::{
    execution::now,
    hlc::{adopt_own, next},
    performance::{Span, add},
};
use serde_json::{Map, Value, from_str};
use sqlx::{
    Executor, Row, Sqlite, SqliteConnection, SqlitePool, Transaction, query,
    query::{Query, QueryAs},
    query_as, query_scalar,
    sqlite::{SqliteArguments, SqliteQueryResult},
};

use super::OpKind;
use crate::{StoreError, cells::LOCAL_CELL_SLUG, record_changes};

const INSERT: &str = "INSERT OR IGNORE INTO sync_op
    (tbl, uid, field, kind, value, hlc, actor_cell, organ_uid, replica_root)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)";

const ENQUEUE_GRANT: &str =
    "INSERT INTO sync_outbox (contact_organ, tbl, uid, field, kind, seq, queued_at)
    SELECT g.contact_organ, ?, ?, ?, ?, ?, ?
      FROM replica_grant g
      JOIN organ_contact c ON c.record_uid = g.contact_organ
     WHERE g.root_record = ? AND g.state = 'accepted'
       AND c.trust != 'blocked' AND g.contact_organ != ?
    ON CONFLICT(contact_organ, tbl, uid, field, kind)
    DO UPDATE SET seq = excluded.seq, queued_at = excluded.queued_at";

const ENQUEUE: &str =
    "INSERT INTO sync_outbox (contact_organ, tbl, uid, field, kind, seq, queued_at)
    SELECT record_uid, ?, ?, ?, ?, ?, ?
      FROM organ_contact
     WHERE sync_out = 1 AND trust != 'blocked' AND record_uid != ?
    ON CONFLICT(contact_organ, tbl, uid, field, kind)
    DO UPDATE SET seq = excluded.seq, queued_at = excluded.queued_at";

/// Appends an incoming or locally stamped operation using the supplied routing context.
/// Incoming duplicate identities return `None` and are never queued for forwarding.
///
/// # Errors
/// Returns an error if local identity allocation is exhausted or a database write fails.
#[expect(
    clippy::too_many_arguments,
    reason = "The public append API accepts the wire fields and routing context"
)]
pub async fn append(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
    field: &str,
    kind: OpKind,
    value: Option<&str>,
    hlc: i64,
    actor_cell: &str,
    organ_uid: &str,
    from_contact: Option<&str>,
    replica_root: Option<&str>,
) -> Result<Option<i64>, StoreError> {
    let op = LocalOperation {
        tbl,
        uid,
        field,
        kind,
        value,
        actor_cell,
        organ_uid,
        replica_root,
    };
    if from_contact.is_some() {
        let result = op.insert_query(hlc).execute(pool).await?;
        return Ok(inserted_seq(result));
    }
    let seq = op.insert(pool, Some(hlc)).await?;
    if kind != OpKind::Snapshot {
        op.enqueue_query(seq, &now().to_rfc3339())
            .execute(pool)
            .await?;
    }
    Ok(Some(seq))
}

fn inserted_seq(result: SqliteQueryResult) -> Option<i64> {
    (result.rows_affected() != 0).then(|| result.last_insert_rowid())
}

const LOCAL_CONTEXT: &str = "
    SELECT device.uid, device.organ_uid,
           CASE
             WHEN ?1 IN ('record', 'record_extension') THEN
               (SELECT replica_root FROM record WHERE uid = ?2)
             WHEN ?1 = 'record_assertion' THEN
               (SELECT r.replica_root FROM record_assertion a
                  JOIN record r ON r.uid = a.subject_uid WHERE a.uid = ?2)
             WHEN ?1 = 'fact' THEN
               (SELECT r.replica_root FROM fact f
                  JOIN record r ON r.uid = f.record_uid WHERE f.uid = ?2)
             ELSE NULL
           END AS replica_root
      FROM record device
     WHERE device.slug = ?3 AND device.kind = 'device' LIMIT 1";

struct LocalContext {
    actor_cell: String,
    organ_uid: String,
    root: Option<String>,
}

async fn local_context_on<'e>(
    executor: impl Executor<'e, Database = Sqlite>,
    tbl: &str,
    uid: &str,
) -> Result<Option<LocalContext>, StoreError> {
    let _read = Span::new("sync-local-context-read-await");
    add("sync-local-context-reads", 1);
    Ok(query(LOCAL_CONTEXT)
        .bind(tbl)
        .bind(uid)
        .bind(LOCAL_CELL_SLUG)
        .fetch_optional(executor)
        .await?
        .map(|row| LocalContext {
            actor_cell: row.get("uid"),
            organ_uid: row.get("organ_uid"),
            root: row.get("replica_root"),
        }))
}

const LOCAL_STAMP_ATTEMPTS: usize = 8;

struct LocalOperation<'a> {
    tbl: &'a str,
    uid: &'a str,
    field: &'a str,
    kind: OpKind,
    value: Option<&'a str>,
    actor_cell: &'a str,
    organ_uid: &'a str,
    replica_root: Option<&'a str>,
}

impl LocalOperation<'_> {
    fn insert_query(&self, stamp: i64) -> Query<'static, Sqlite, SqliteArguments> {
        query(INSERT)
            .bind(self.tbl)
            .bind(self.uid)
            .bind(self.field)
            .bind(self.kind.as_str())
            .bind(self.value)
            .bind(stamp)
            .bind(self.actor_cell)
            .bind(self.organ_uid)
            .bind(self.replica_root)
    }

    fn enqueue_query(&self, seq: i64, queued_at: &str) -> Query<'static, Sqlite, SqliteArguments> {
        let sql = if self.replica_root.is_some() {
            ENQUEUE_GRANT
        } else {
            ENQUEUE
        };
        let mut pending = query(sql)
            .bind(self.tbl)
            .bind(self.uid)
            .bind(self.field)
            .bind(self.kind.as_str())
            .bind(seq)
            .bind(queued_at);
        if let Some(root) = self.replica_root {
            pending = pending.bind(root);
        }
        pending.bind("")
    }

    async fn insert(&self, pool: &SqlitePool, first_hlc: Option<i64>) -> Result<i64, StoreError> {
        let _insert = Span::new("sync-local-operation-insert-await");
        let stamp = first_hlc.unwrap_or_else(next);
        let mut connection = pool.acquire().await?;
        self.try_insert_on(&mut connection, stamp)
            .await?
            .ok_or_else(|| {
                let actor_cell = self.actor_cell;
                sqlx::Error::Protocol(format!(
                    "could not mint a free op identity for cell {actor_cell} after \
                 {LOCAL_STAMP_ATTEMPTS} attempts"
                ))
            })
    }

    async fn try_insert_on(
        &self,
        connection: &mut SqliteConnection,
        first_hlc: i64,
    ) -> Result<Option<i64>, StoreError> {
        let mut stamp = first_hlc;
        for attempt in 0..LOCAL_STAMP_ATTEMPTS {
            if attempt > 0 {
                stamp = next();
            }
            add("sync-local-operation-insert-attempts", 1);
            let result = self.insert_query(stamp).execute(&mut *connection).await?;
            if let Some(seq) = inserted_seq(result) {
                return Ok(Some(seq));
            }
            let theirs: Option<i64> =
                query_scalar("SELECT MAX(hlc) FROM sync_op WHERE actor_cell = ?")
                    .bind(self.actor_cell)
                    .fetch_one(&mut *connection)
                    .await?;
            if let Some(theirs) = theirs {
                adopt_own(theirs);
            }
        }
        Ok(None)
    }
}

/// Logs an operation with local identity and enqueues it for eligible peers.
/// Store-backed callers that already hold a transaction use [`log_local_tx`].
///
/// # Errors
/// Propagates database failures, malformed Assertion payloads, and stamp exhaustion.
pub async fn log_local(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
    field: &str,
    kind: OpKind,
    value: Option<String>,
) -> Result<(), StoreError> {
    let value = if tbl == "record_assertion" && kind == OpKind::Set {
        with_assertion_versions(value, assertion_revisions(uid).fetch_optional(pool).await?)?
    } else {
        value
    };
    let Some(context) = local_context_on(pool, tbl, uid).await? else {
        return Ok(());
    };
    let op = LocalOperation {
        tbl,
        uid,
        field,
        kind,
        value: value.as_deref(),
        actor_cell: &context.actor_cell,
        organ_uid: &context.organ_uid,
        replica_root: context.root.as_deref(),
    };
    let seq = op.insert(pool, None).await?;
    if tbl == "record" && kind == OpKind::Set {
        record_changes::note_local(pool, uid, field).await?;
    }
    op.enqueue_query(seq, &now().to_rfc3339())
        .execute(pool)
        .await?;
    Ok(())
}

/// Logs and queues a local operation in the caller's transaction.
///
/// # Errors
/// Propagates database failures, malformed Assertion payloads, and stamp exhaustion.
pub async fn log_local_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tbl: &str,
    uid: &str,
    field: &str,
    kind: OpKind,
    value: Option<String>,
) -> Result<(), StoreError> {
    let value = if tbl == "record_assertion" && kind == OpKind::Set {
        with_assertion_versions(
            value,
            assertion_revisions(uid).fetch_optional(&mut **tx).await?,
        )?
    } else {
        value
    };
    let Some(context) = local_context_on(&mut **tx, tbl, uid).await? else {
        return Ok(());
    };
    let op = LocalOperation {
        tbl,
        uid,
        field,
        kind,
        value: value.as_deref(),
        actor_cell: &context.actor_cell,
        organ_uid: &context.organ_uid,
        replica_root: context.root.as_deref(),
    };
    let seq = {
        let _insert = Span::new("sync-local-operation-insert-await");
        op.try_insert_on(tx, next()).await?
    }
    .ok_or_else(|| {
        StoreError::Protocol(format!(
            "could not mint a free op identity for cell {}",
            context.actor_cell
        ))
    })?;
    if tbl == "record" && kind == OpKind::Set {
        record_changes::note_local_tx(tx, uid, field).await?;
    }
    op.enqueue_query(seq, &now().to_rfc3339())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

type AssertionRevisions = (i64, Option<i64>);

fn assertion_revisions(uid: &str) -> QueryAs<'_, Sqlite, AssertionRevisions, SqliteArguments> {
    query_as("SELECT predicate_revision,unit_revision FROM record_assertion WHERE uid=?").bind(uid)
}

fn with_assertion_versions(
    value: Option<String>,
    versions: Option<AssertionRevisions>,
) -> Result<Option<String>, StoreError> {
    let (Some(payload), Some((predicate, unit))) = (&value, versions) else {
        return Ok(value);
    };
    let mut fields =
        match from_str(payload).map_err(|error| StoreError::Protocol(error.to_string()))? {
            Value::Object(fields) => fields,
            // Earlier writers accept JSON null and add the revision fields.
            Value::Null => Map::new(),
            _ => {
                return Err(StoreError::Protocol(
                    "Assertion operation payload must be a JSON object".into(),
                ));
            }
        };
    fields.insert("predicate_revision".into(), predicate.into());
    fields.insert("unit_revision".into(), unit.into());
    Ok(Some(Value::Object(fields).to_string()))
}
