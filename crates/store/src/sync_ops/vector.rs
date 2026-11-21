use nucleus::social::CONVERSATION_VECTOR_PREFIX;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool, query, query_as, query_scalar};

use super::{OpRow, VectorEntry};
use crate::{StoreError, transfer_replication::VECTOR_PREFIX};

/// Builds an Organ vector for general-feed operations. For the local Organ it
/// also includes retained conversation and Transfer streams.
///
/// # Errors
/// Propagates database errors and rejects oversized conversation vectors.
pub async fn version_vector_for_organ(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Vec<VectorEntry>, StoreError> {
    let mut vector = query_as(
        "SELECT actor_cell, MAX(hlc) AS max_hlc FROM sync_op
          WHERE organ_uid = ? AND replica_root IS NULL
          GROUP BY actor_cell ORDER BY actor_cell",
    )
    .bind(organ_uid)
    .fetch_all(pool)
    .await?;
    let ours: bool =
        query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE slug = 'local-organ' AND uid = ?)")
            .bind(organ_uid)
            .fetch_one(pool)
            .await?;
    if ours {
        let roots = query("SELECT s.replica_root,s.actor_cell,MAX(s.hlc) AS max_hlc FROM sync_op s JOIN record r ON r.uid=s.replica_root WHERE s.organ_uid=? AND r.kind IN ('conversation','message_draft') GROUP BY s.replica_root,s.actor_cell ORDER BY s.replica_root,s.actor_cell LIMIT 4097")
            .bind(organ_uid).fetch_all(pool).await?;
        if roots.len() + vector.len() > 4096 {
            return Err(StoreError::Protocol(
                "The retained own-conversation vector exceeds this device's 4096-entry sync limit"
                    .into(),
            ));
        }
        vector.extend(roots.into_iter().map(|row| VectorEntry {
            actor_cell: format!(
                "{}{}/{}",
                CONVERSATION_VECTOR_PREFIX,
                row.get::<&str, _>("replica_root"),
                row.get::<&str, _>("actor_cell")
            ),
            max_hlc: row.get("max_hlc"),
        }));
        let rows = query("SELECT cell_uid,MAX(sequence) AS sequence FROM transfer_sync_message WHERE organ_uid = ? GROUP BY cell_uid ORDER BY cell_uid")
            .bind(organ_uid).fetch_all(pool).await?;
        vector.extend(rows.into_iter().map(|row| VectorEntry {
            actor_cell: format!("{}{}", VECTOR_PREFIX, row.get::<&str, _>("cell_uid")),
            max_hlc: row.get("sequence"),
        }));
    }
    Ok(vector)
}

/// Reads up to 200 operations after the actor's clock in a local conversation stream.
pub async fn own_conversation_ops_after(
    pool: &SqlitePool,
    organ: &str,
    root: &str,
    actor: &str,
    covered: i64,
) -> Result<Vec<OpRow>, StoreError> {
    query_as(
        "SELECT * FROM sync_op
         WHERE organ_uid=? AND replica_root=? AND actor_cell=? AND hlc>?
         ORDER BY hlc,seq LIMIT 200",
    )
    .bind(organ)
    .bind(root)
    .bind(actor)
    .bind(covered)
    .fetch_all(pool)
    .await
}

fn filter_uncovered(query: &mut QueryBuilder<Sqlite>, theirs: &[VectorEntry]) {
    if theirs.is_empty() {
        return;
    }
    query.push(" AND NOT (");
    let mut clauses = query.separated(" OR ");
    for entry in theirs {
        clauses
            .push("(actor_cell = ")
            .push_bind_unseparated(&entry.actor_cell)
            .push_unseparated(" AND hlc <= ")
            .push_bind_unseparated(entry.max_hlc)
            .push_unseparated(")");
    }
    clauses.push_unseparated(")");
}

enum BatchScope {
    Organ,
    Siblings,
    Root,
}

async fn batch_missing(
    pool: &SqlitePool,
    scope: BatchScope,
    identity: &str,
    theirs: &[VectorEntry],
    limit: i64,
) -> Result<Vec<OpRow>, StoreError> {
    let mut query = QueryBuilder::new("SELECT * FROM sync_op WHERE ");
    match scope {
        BatchScope::Organ | BatchScope::Siblings => {
            query
                .push("organ_uid = ")
                .push_bind(identity)
                .push(" AND replica_root IS NULL");
        }
        BatchScope::Root => {
            query.push("replica_root = ").push_bind(identity);
        }
    }
    if matches!(scope, BatchScope::Siblings) {
        query.push(" AND NOT (kind = 'fact' AND EXISTS (
            SELECT 1 FROM fact f JOIN transfer_sync_journal j ON j.commit_sequence = f.commit_sequence
             WHERE f.uid = sync_op.uid AND j.cell_uid = sync_op.actor_cell
               AND j.organ_uid = sync_op.organ_uid))");
    }
    filter_uncovered(&mut query, theirs);
    query
        .push(" ORDER BY seq LIMIT ")
        .push_bind(limit.max(0))
        .build_query_as()
        .fetch_all(pool)
        .await
}

/// Reads general-feed operations beyond the peer's vector, in sequence order.
pub async fn ops_missing_from_vector(
    pool: &SqlitePool,
    organ_uid: &str,
    theirs: &[VectorEntry],
    limit: i64,
) -> Result<Vec<OpRow>, StoreError> {
    batch_missing(pool, BatchScope::Organ, organ_uid, theirs, limit).await
}

/// Reads missing general-feed operations, excluding Facts sent by the Transfer journal.
pub async fn sibling_ops_missing_from_vector(
    pool: &SqlitePool,
    organ_uid: &str,
    theirs: &[VectorEntry],
    limit: i64,
) -> Result<Vec<OpRow>, StoreError> {
    batch_missing(pool, BatchScope::Siblings, organ_uid, theirs, limit).await
}

/// Reads missing operations in one replica root, in sequence order.
pub async fn ops_missing_from_vector_in_root(
    pool: &SqlitePool,
    root: &str,
    theirs: &[VectorEntry],
    limit: i64,
) -> Result<Vec<OpRow>, StoreError> {
    batch_missing(pool, BatchScope::Root, root, theirs, limit).await
}

/// Builds a vector of maximum clocks for one replica root.
pub async fn version_vector_for_root(
    pool: &SqlitePool,
    root: &str,
) -> Result<Vec<VectorEntry>, StoreError> {
    query_as(
        "SELECT actor_cell, MAX(hlc) AS max_hlc FROM sync_op
          WHERE replica_root = ?
          GROUP BY actor_cell ORDER BY actor_cell",
    )
    .bind(root)
    .fetch_all(pool)
    .await
}

/// Finds the last sequence before the first general-feed operation the peer lacks.
pub async fn seq_covered_by_vector(
    pool: &SqlitePool,
    organ_uid: &str,
    theirs: &[VectorEntry],
) -> Result<i64, StoreError> {
    let mut query = QueryBuilder::new("SELECT MIN(seq) FROM sync_op WHERE organ_uid = ");
    query.push_bind(organ_uid).push(" AND replica_root IS NULL");
    filter_uncovered(&mut query, theirs);
    match query
        .build_query_scalar::<Option<i64>>()
        .fetch_one(pool)
        .await?
    {
        Some(first_missing) => Ok(first_missing - 1),
        None => Ok(query_scalar::<_, Option<i64>>(
            "SELECT MAX(seq) FROM sync_op WHERE organ_uid = ? AND replica_root IS NULL",
        )
        .bind(organ_uid)
        .fetch_one(pool)
        .await?
        .unwrap_or(0)),
    }
}
