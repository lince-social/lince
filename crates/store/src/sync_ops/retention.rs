use sqlx::{AssertSqlSafe, SqlitePool, query, query_scalar};

use crate::StoreError;

/// The retention floor and counts of prunable and protected operations below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PruneReport {
    pub floor: i64,
    pub removed: i64,
    pub retained: i64,
}

/// Finds the lowest acknowledgement among nonblocked contacts eligible for
/// general sync or holding a replica grant.
pub async fn retention_floor(pool: &SqlitePool) -> Result<Option<i64>, StoreError> {
    query_scalar(
        "SELECT MIN(c.peer_acked_seq)
           FROM organ_contact c
          WHERE c.trust != 'blocked'
            AND (c.sync_out = 1
                 OR EXISTS (SELECT 1 FROM replica_grant g
                             WHERE g.contact_organ = c.record_uid))",
    )
    .fetch_one(pool)
    .await
}

/// Reports prunable rows under the retention floor. Executes the deletion
/// when dry-run is false.
///
/// # Errors
/// Propagates database errors from checking acknowledgements, counting, or deleting.
pub async fn prune(pool: &SqlitePool, dry_run: bool) -> Result<PruneReport, StoreError> {
    let Some(floor) = retention_floor(pool).await? else {
        return Ok(PruneReport {
            floor: 0,
            removed: 0,
            retained: 0,
        });
    };

    const PRUNABLE: &str = "seq <= ?
           AND NOT (tbl = 'record' AND field = 'property:quantity')
           AND tbl != 'record_assertion'
           AND seq NOT IN (SELECT seq FROM sync_outbox)
           AND (EXISTS (SELECT 1 FROM sync_op n
                         WHERE n.tbl = sync_op.tbl
                           AND n.uid = sync_op.uid
                           AND n.field = sync_op.field
                           AND n.kind = sync_op.kind
                           AND n.replica_root IS sync_op.replica_root
                           AND n.seq > sync_op.seq)
                OR (sync_op.kind = 'crdt'
                    AND EXISTS (SELECT 1 FROM sync_op s
                                 WHERE s.kind = 'snapshot'
                                   AND s.tbl = sync_op.tbl
                                   AND s.uid = sync_op.uid
                                   AND s.replica_root IS sync_op.replica_root
                                   AND s.seq > sync_op.seq)))";

    let removable: i64 = query_scalar(AssertSqlSafe(format!(
        "SELECT COUNT(*) AS n FROM sync_op WHERE {PRUNABLE}"
    )))
    .bind(floor)
    .fetch_one(pool)
    .await?;
    let at_or_below: i64 = query_scalar("SELECT COUNT(*) AS n FROM sync_op WHERE seq <= ?")
        .bind(floor)
        .fetch_one(pool)
        .await?;

    if !dry_run && removable > 0 {
        query(AssertSqlSafe(format!(
            "DELETE FROM sync_op WHERE {PRUNABLE}"
        )))
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
