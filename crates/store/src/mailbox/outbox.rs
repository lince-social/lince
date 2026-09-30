use sqlx::{Row, SqlitePool};

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub intent: String,
    pub uid: String,
    pub to_organ: String,
    pub body: String,
    pub expires_at: String,
    pub requested_copies: i64,
    pub next_attempt: i64,
}

fn map(row: sqlx::sqlite::SqliteRow) -> Envelope {
    Envelope {
        intent: row.get("intent"),
        uid: row.get("uid"),
        to_organ: row.get("to_organ"),
        body: row.get("body"),
        expires_at: row.get("expires_at"),
        requested_copies: row.get("requested_copies"),
        next_attempt: row.get("next_attempt"),
    }
}

pub async fn get(pool: &SqlitePool, intent: &str) -> Result<Option<Envelope>, StoreError> {
    Ok(sqlx::query("SELECT * FROM mailbox_outbox WHERE intent=?")
        .bind(intent)
        .fetch_optional(pool)
        .await?
        .map(map))
}

pub async fn prepare(pool: &SqlitePool, candidate: &Envelope) -> Result<Envelope, StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    if let Some(row) = sqlx::query("SELECT * FROM mailbox_outbox WHERE intent=?")
        .bind(&candidate.intent)
        .fetch_optional(&mut *tx)
        .await?
    {
        let held = map(row);
        if held.to_organ != candidate.to_organ {
            return Err(sqlx::Error::Protocol(
                "Outgoing mail intent has another recipient".into(),
            ));
        }
        return Ok(held);
    }
    let now = nucleus::execution::now();
    sqlx::query("DELETE FROM mailbox_outbox WHERE expires_at<?")
        .bind((now - chrono::Duration::days(3)).to_rfc3339())
        .execute(&mut *tx)
        .await?;
    let bytes: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(length(CAST(body AS BLOB))),0) FROM mailbox_outbox",
    )
    .fetch_one(&mut *tx)
    .await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mailbox_outbox")
        .fetch_one(&mut *tx)
        .await?;
    if candidate.intent.len() != 64
        || candidate.uid.len() > 128
        || candidate.to_organ.len() > 128
        || !(1..=2).contains(&candidate.requested_copies)
        || candidate.body.is_empty()
        || candidate.body.len() > 1024 * 1024
        || count >= 50_000
        || bytes.saturating_add(candidate.body.len() as i64) > 64 * 1024 * 1024
    {
        return Err(sqlx::Error::Protocol(
            "Invalid or full outgoing mail queue".into(),
        ));
    }
    sqlx::query("INSERT INTO mailbox_outbox(intent,uid,to_organ,body,created_at,expires_at,requested_copies) VALUES (?,?,?,?,?,?,?)")
        .bind(&candidate.intent).bind(&candidate.uid).bind(&candidate.to_organ).bind(&candidate.body)
        .bind(now.to_rfc3339()).bind(&candidate.expires_at).bind(candidate.requested_copies)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(candidate.clone())
}

pub async fn receipts(pool: &SqlitePool, uid: &str) -> Result<Vec<(String, String)>, StoreError> {
    Ok(sqlx::query("SELECT carrier_organ,carrier_node FROM mailbox_outbox_receipt WHERE uid=? ORDER BY accepted_at,carrier_node")
        .bind(uid).fetch_all(pool).await?.into_iter().map(|row| (row.get("carrier_organ"),row.get("carrier_node"))).collect())
}

pub async fn accepted(
    pool: &SqlitePool,
    uid: &str,
    organ: &str,
    node: &str,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    let recipient: String = sqlx::query_scalar("SELECT to_organ FROM mailbox_outbox WHERE uid=?")
        .bind(uid)
        .fetch_one(&mut *tx)
        .await?;
    let now = nucleus::execution::now().to_rfc3339();
    sqlx::query("INSERT INTO mailbox_outbox_receipt(uid,carrier_organ,carrier_node,accepted_at) VALUES (?,?,?,?) ON CONFLICT(uid,carrier_node) DO NOTHING")
        .bind(uid).bind(organ).bind(node).bind(&now).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO mail_left(uid,carrier_organ,carrier_node,to_organ,left_at) VALUES (?,?,?,?,?) ON CONFLICT(uid,carrier_node) DO NOTHING")
        .bind(uid).bind(organ).bind(node).bind(recipient).bind(now).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn attempted(
    pool: &SqlitePool,
    uid: &str,
    error: Option<&str>,
) -> Result<(), StoreError> {
    let jitter = uid
        .bytes()
        .fold(0u64, |sum, byte| sum.wrapping_add(byte as u64))
        % 7;
    sqlx::query("UPDATE mailbox_outbox SET attempts=MIN(attempts+1,1000000),error=?,next_attempt=?+MIN(3600,5*(1<<MIN(attempts,10)))+? WHERE uid=?")
        .bind(error).bind(nucleus::execution::now().timestamp()).bind(jitter as i64).bind(uid).execute(pool).await?;
    Ok(())
}

pub async fn pending(pool: &SqlitePool, limit: i64) -> Result<Vec<Envelope>, StoreError> {
    let now = nucleus::execution::now();
    Ok(sqlx::query("SELECT o.* FROM mailbox_outbox o WHERE o.expires_at>? AND o.next_attempt<=? AND (SELECT COUNT(*) FROM mailbox_outbox_receipt r WHERE r.uid=o.uid)<o.requested_copies ORDER BY o.next_attempt,o.created_at,o.uid LIMIT ?")
        .bind(now.to_rfc3339()).bind(now.timestamp()).bind(limit.clamp(1,32)).fetch_all(pool).await?.into_iter().map(map).collect())
}

pub async fn retry_recipient(pool: &SqlitePool, recipient: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE mailbox_outbox SET next_attempt=0 WHERE to_organ=?")
        .bind(recipient)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn status(pool: &SqlitePool) -> Result<Vec<serde_json::Value>, StoreError> {
    let now = nucleus::execution::now().to_rfc3339();
    Ok(sqlx::query("SELECT o.*, (SELECT COUNT(*) FROM mailbox_outbox_receipt r WHERE r.uid=o.uid) AS copies FROM mailbox_outbox o ORDER BY o.created_at DESC LIMIT 100")
        .fetch_all(pool).await?.into_iter().map(|row| {
            let copies: i64 = row.get("copies");
            let requested: i64 = row.get("requested_copies");
            let expiry: String = row.get("expires_at");
            let error: Option<String> = row.get("error");
            let state = if expiry<=now {"expired"} else if copies>=requested {"accepted"} else if copies>0 {"partially accepted"} else {"pending"};
            serde_json::json!({"uid":row.get::<String,_>("uid"),"to_organ":row.get::<String,_>("to_organ"),"state":state,
                "copies":copies,"requested_copies":requested,"error":error,"expires_at":expiry,
                "attempts":row.get::<i64,_>("attempts")})
        }).collect())
}
