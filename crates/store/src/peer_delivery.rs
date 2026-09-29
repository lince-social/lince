use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Status {
    pub cell_uid: String,
    pub node_id: String,
    pub attempted_at: String,
    pub succeeded_at: Option<String>,
    pub error: Option<String>,
    pub covered_seq: i64,
    pub pending: i64,
    pub addresses: Vec<String>,
}

pub async fn list(pool: &SqlitePool, organ: &str) -> Result<Vec<Status>, crate::StoreError> {
    sqlx::query("SELECT p.*, (SELECT COUNT(*) FROM sync_op s WHERE s.organ_uid = p.organ_uid AND s.replica_root IS NULL AND s.seq > p.covered_seq) AS pending FROM peer_delivery p WHERE p.organ_uid = ? ORDER BY p.cell_uid")
        .bind(organ).fetch_all(pool).await?.into_iter().map(|row| {
            let addresses: String = row.get("addresses");
            Ok(Status {
                cell_uid: row.get("cell_uid"),
                node_id: row.get("node_id"),
                attempted_at: row.get("attempted_at"),
                succeeded_at: row.get("succeeded_at"),
                error: row.get("error"),
                covered_seq: row.get("covered_seq"),
                pending: row.get("pending"),
                addresses: serde_json::from_str(&addresses).map_err(|error| sqlx::Error::Protocol(error.to_string()))?,
            })
        }).collect()
}

pub async fn note(
    pool: &SqlitePool,
    organ: &str,
    cell: &str,
    node: &str,
    result: Result<(i64, Vec<String>), String>,
) -> Result<(), crate::StoreError> {
    let now = nucleus::execution::now().to_rfc3339();
    let (success, covered, addresses, error) = match result {
        Ok((covered, addresses)) => (
            Some(now.clone()),
            covered,
            serde_json::json!(addresses).to_string(),
            None,
        ),
        Err(error) => (None, 0, "[]".into(), Some(error)),
    };
    sqlx::query("INSERT INTO peer_delivery (organ_uid, cell_uid, node_id, attempted_at, succeeded_at, covered_seq, addresses, error) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (organ_uid, cell_uid, node_id) DO UPDATE SET attempted_at = excluded.attempted_at, succeeded_at = COALESCE(excluded.succeeded_at, peer_delivery.succeeded_at), covered_seq = CASE WHEN excluded.succeeded_at IS NULL THEN peer_delivery.covered_seq ELSE excluded.covered_seq END, addresses = CASE WHEN excluded.addresses = '[]' THEN peer_delivery.addresses ELSE excluded.addresses END, error = excluded.error")
        .bind(organ).bind(cell).bind(node).bind(now).bind(success).bind(covered).bind(addresses).bind(error)
        .execute(pool).await?;
    Ok(())
}
