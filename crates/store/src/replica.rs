use sqlx::{Row, SqlitePool};

use crate::StoreError;

pub const OFFERED: &str = "offered";
pub const ACCEPTED: &str = "accepted";

pub async fn offer(pool: &SqlitePool, root: &str, contact: &str) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO replica_grant (root_record, contact_organ, state, created_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(root_record, contact_organ) DO NOTHING",
    )
    .bind(root)
    .bind(contact)
    .bind(OFFERED)
    .bind(nucleus::execution::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn accept(pool: &SqlitePool, root: &str, contact: &str) -> Result<(), StoreError> {
    let mut tx=crate::write_tx(pool).await?;
    let now=nucleus::execution::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO replica_grant (root_record, contact_organ, state, created_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(root_record, contact_organ) DO UPDATE SET state = excluded.state",
    )
    .bind(root)
    .bind(contact)
    .bind(ACCEPTED)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO sync_outbox(contact_organ,tbl,uid,field,kind,seq,queued_at) SELECT ?,o.tbl,o.uid,o.field,o.kind,o.seq,? FROM sync_op o WHERE o.replica_root=? AND o.organ_uid=(SELECT uid FROM record WHERE slug=? AND kind='organ' AND deleted_at IS NULL) AND o.kind!='snapshot' AND EXISTS(SELECT 1 FROM organ_contact WHERE record_uid=? AND trust!='blocked') ORDER BY o.seq ON CONFLICT(contact_organ,tbl,uid,field,kind) DO UPDATE SET seq=MAX(sync_outbox.seq,excluded.seq),queued_at=excluded.queued_at")
        .bind(contact).bind(&now).bind(root).bind(crate::organs::LOCAL_ORGAN_SLUG).bind(contact).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn revoke(pool: &SqlitePool, root: &str, contact: &str) -> Result<(), StoreError> {
    let mut tx=crate::write_tx(pool).await?;
    sqlx::query("DELETE FROM replica_grant WHERE root_record = ? AND contact_organ = ?")
        .bind(root)
        .bind(contact)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "DELETE FROM sync_outbox
          WHERE contact_organ = ?
            AND seq IN (SELECT seq FROM sync_op WHERE replica_root = ?)",
    )
    .bind(contact)
    .bind(root)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn make_own_root(pool: &SqlitePool, record_uid: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE record SET replica_root = uid WHERE uid = ? AND replica_root IS NULL")
        .bind(record_uid)
        .execute(pool)
        .await?;
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

pub async fn is_accepted(pool: &SqlitePool, root: &str, contact: &str) -> Result<bool, StoreError> {
    Ok(state(pool, root, contact).await?.as_deref() == Some(ACCEPTED))
}

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

#[derive(Debug, Clone)]
pub struct SharedConversation {
    pub uid: String,
    pub head: String,
    pub state: String,
}

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

pub async fn root_for_op_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    tbl: &str,
    uid: &str,
) -> Result<Option<String>, StoreError> {
    Ok(match tbl {
        "record" | "record_extension" => {
            sqlx::query_scalar::<_, Option<String>>("SELECT replica_root FROM record WHERE uid = ?")
                .bind(uid)
                .fetch_optional(&mut **tx)
                .await?
                .flatten()
        }
        "record_assertion" => sqlx::query_scalar::<_, Option<String>>(
            "SELECT r.replica_root FROM record_assertion a
               JOIN record r ON r.uid = a.subject_uid
              WHERE a.uid = ?",
        )
        .bind(uid)
        .fetch_optional(&mut **tx)
        .await?
        .flatten(),
        "fact" => sqlx::query_scalar::<_, Option<String>>(
            "SELECT r.replica_root FROM fact f JOIN record r ON r.uid=f.record_uid WHERE f.uid=?",
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

pub async fn root_for_op(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
) -> Result<Option<String>, StoreError> {
    match tbl {
        "record" | "record_extension" => root_of(pool, uid).await,
        "record_assertion" => Ok(sqlx::query_scalar::<_, Option<String>>(
            "SELECT r.replica_root FROM record_assertion a
               JOIN record r ON r.uid = a.subject_uid
              WHERE a.uid = ?",
        )
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .flatten()),
        "fact" => Ok(sqlx::query_scalar::<_, Option<String>>(
            "SELECT r.replica_root FROM fact f JOIN record r ON r.uid=f.record_uid WHERE f.uid=?",
        )
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .flatten()),
        _ => Ok(None),
    }
}

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
    .bind(nucleus::execution::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

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

pub async fn delete_root_locally(pool: &SqlitePool, root: &str) -> Result<u64, StoreError> {
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
    sqlx::query("DELETE FROM sync_op WHERE replica_root = ?")
        .bind(root)
        .execute(pool)
        .await?;
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
    sqlx::query(
        "DELETE FROM reference_read
          WHERE record_uid IN (SELECT uid FROM record WHERE replica_root = ?)
             OR root_record = ?",
    )
    .bind(root)
    .bind(root)
    .execute(pool)
    .await?;
    let removed = sqlx::query("DELETE FROM record WHERE replica_root = ?")
        .bind(root)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(removed)
}
