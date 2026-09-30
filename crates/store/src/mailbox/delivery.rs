use sqlx::{Row, SqlitePool};

use super::{HeldBundle, map_bundle};
use crate::StoreError;

#[derive(Debug, PartialEq, Eq)]
pub enum Deposit {
    Stored,
    NotRegistered,
    Full,
    Conflict,
}

pub async fn deposit(
    pool: &SqlitePool,
    bundle: &HeldBundle,
    global_limit: i64,
) -> Result<Deposit, StoreError> {
    if bundle.bytes != bundle.body.len() as i64 {
        return Ok(Deposit::Conflict);
    }
    let mut tx = crate::write_tx(pool).await?;
    let quota: Option<i64> =
        sqlx::query_scalar("SELECT quota_bytes FROM mailbox_registration WHERE organ_uid=?")
            .bind(&bundle.to_organ)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(quota) = quota else {
        return Ok(Deposit::NotRegistered);
    };
    let now = nucleus::execution::now().to_rfc3339();
    sqlx::query("DELETE FROM mailbox_completion WHERE expires_at<=?")
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    if let Some(row) = sqlx::query("SELECT body_hash,to_organ FROM mailbox_completion WHERE uid=?")
        .bind(&bundle.uid)
        .fetch_optional(&mut *tx)
        .await?
    {
        return Ok(
            if row.get::<String, _>("body_hash")
                == nucleus::fact::sha256_hex(bundle.body.as_bytes())
                && row.get::<String, _>("to_organ") == bundle.to_organ
            {
                Deposit::Stored
            } else {
                Deposit::Conflict
            },
        );
    }
    if let Some(row) = sqlx::query("SELECT body,to_organ FROM mailbox_bundle WHERE uid=?")
        .bind(&bundle.uid)
        .fetch_optional(&mut *tx)
        .await?
    {
        return Ok(
            if row.get::<String, _>("body") == bundle.body
                && row.get::<String, _>("to_organ") == bundle.to_organ
            {
                Deposit::Stored
            } else {
                Deposit::Conflict
            },
        );
    }
    let held: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(bytes),0) FROM mailbox_bundle WHERE to_organ=? AND expires_at>?",
    )
    .bind(&bundle.to_organ)
    .bind(&now)
    .fetch_one(&mut *tx)
    .await?;
    let total: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(bytes),0) FROM mailbox_bundle")
        .fetch_one(&mut *tx)
        .await?;
    let envelopes: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM mailbox_bundle)+(SELECT COUNT(*) FROM mailbox_completion)",
    )
    .fetch_one(&mut *tx)
    .await?;
    if envelopes >= 50_000
        || bundle.bytes <= 0
        || held.saturating_add(bundle.bytes) > quota
        || total.saturating_add(bundle.bytes) > global_limit
    {
        return Ok(Deposit::Full);
    }
    sqlx::query("INSERT INTO mailbox_bundle(uid,to_organ,from_organ,from_cell,from_node,body,bytes,received_at,expires_at) VALUES (?,?,?,?,?,?,?,?,?)")
        .bind(&bundle.uid).bind(&bundle.to_organ).bind(&bundle.from_organ).bind(&bundle.from_cell)
        .bind(&bundle.from_node).bind(&bundle.body).bind(bundle.bytes).bind(&bundle.received_at)
        .bind(&bundle.expires_at).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Deposit::Stored)
}

pub async fn advance_roster(
    pool: &SqlitePool,
    organ: &str,
    version: i64,
    payload: &str,
) -> Result<bool, StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    if let Some(row) =
        sqlx::query("SELECT version,payload FROM mailbox_roster_floor WHERE organ_uid=?")
            .bind(organ)
            .fetch_optional(&mut *tx)
            .await?
    {
        let floor: i64 = row.get("version");
        if version < floor || (version == floor && row.get::<String, _>("payload") != payload) {
            return Ok(false);
        }
    }
    sqlx::query("INSERT INTO mailbox_roster_floor(organ_uid,version,payload) VALUES (?,?,?) ON CONFLICT(organ_uid) DO UPDATE SET version=excluded.version,payload=excluded.payload WHERE excluded.version>mailbox_roster_floor.version")
        .bind(organ).bind(version).bind(payload).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn roster_floor(
    pool: &SqlitePool,
    organ: &str,
) -> Result<Option<(i64, String)>, StoreError> {
    Ok(
        sqlx::query("SELECT version,payload FROM mailbox_roster_floor WHERE organ_uid=?")
            .bind(organ)
            .fetch_optional(pool)
            .await?
            .map(|row| (row.get("version"), row.get("payload"))),
    )
}

pub async fn for_device(
    pool: &SqlitePool,
    organ: &str,
    node: &str,
    limit: i64,
) -> Result<Vec<HeldBundle>, StoreError> {
    Ok(sqlx::query(r#"WITH device AS (
        SELECT json_extract(c.value,'$.sealing_key.key_id') AS key_id,
            'x25519:cell:' || json_extract(c.value,'$.cell_uid') || ':' AS prefix
        FROM mailbox_roster_floor f, json_each(f.payload,'$.roster.cells') c
        WHERE f.organ_uid=? AND json_extract(c.value,'$.node_id')=?
            AND EXISTS(SELECT 1 FROM json_each(c.value,'$.capabilities') cap WHERE cap.value='write')
    ) SELECT * FROM (
        SELECT page.*, SUM(2*length(CAST(body AS BLOB))+2048) OVER (ORDER BY received_at,uid) AS frame_bytes
        FROM (SELECT b.* FROM mailbox_bundle b
            WHERE b.to_organ=? AND b.expires_at>?
                AND NOT EXISTS(SELECT 1 FROM mailbox_device_ack a WHERE a.uid=b.uid AND a.node_id=?)
                AND EXISTS(SELECT 1 FROM device d,
                    json_each(CASE WHEN json_valid(b.body) THEN b.body ELSE '{}' END,'$.recipients') r
                    WHERE json_extract(r.value,'$.key_id')=d.key_id
                        OR substr(json_extract(r.value,'$.key_id'),1,length(d.prefix))=d.prefix)
            ORDER BY b.received_at,b.uid LIMIT ?) page
    ) WHERE frame_bytes<=?"#)
        .bind(organ).bind(node).bind(organ).bind(nucleus::execution::now().to_rfc3339()).bind(node).bind(limit.clamp(1,256))
        .bind(16*1024*1024-4096)
        .fetch_all(pool).await?.into_iter().map(map_bundle).collect())
}

pub async fn acknowledge(
    pool: &SqlitePool,
    organ: &str,
    node: &str,
    uids: &[String],
) -> Result<u64, StoreError> {
    if uids.len() > 256 {
        return Err(sqlx::Error::Protocol(
            "Too many delivery acknowledgements".into(),
        ));
    }
    let mut tx = crate::write_tx(pool).await?;
    let payload: String =
        sqlx::query_scalar("SELECT payload FROM mailbox_roster_floor WHERE organ_uid=?")
            .bind(organ)
            .fetch_one(&mut *tx)
            .await?;
    let roster: serde_json::Value =
        serde_json::from_str(&payload).map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    let devices: Vec<&serde_json::Value> = roster["roster"]["cells"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|cell| {
            cell["capabilities"]
                .as_array()
                .is_some_and(|caps| caps.iter().any(|cap| cap == "write"))
        })
        .collect();
    if !devices.iter().any(|cell| cell["node_id"] == node) {
        return Err(sqlx::Error::Protocol(
            "Device may no longer collect mail".into(),
        ));
    }
    let mut acknowledged = uids.to_vec();
    let previous: Vec<String> = sqlx::query_scalar("SELECT b.uid FROM mailbox_bundle b JOIN mailbox_device_ack a ON a.uid=b.uid WHERE b.to_organ=? AND a.node_id=? ORDER BY b.received_at,b.uid LIMIT ?")
        .bind(organ).bind(node).bind((256-uids.len()) as i64).fetch_all(&mut *tx).await?;
    acknowledged.extend(previous);
    let mut dropped = 0;
    for uid in &acknowledged {
        sqlx::query("INSERT INTO mailbox_device_ack(uid,node_id) SELECT uid,? FROM mailbox_bundle WHERE uid=? AND to_organ=? ON CONFLICT(uid,node_id) DO NOTHING")
            .bind(node).bind(uid).bind(organ).execute(&mut *tx).await?;
        let body: Option<String> =
            sqlx::query_scalar("SELECT body FROM mailbox_bundle WHERE uid=? AND to_organ=?")
                .bind(uid)
                .bind(organ)
                .fetch_optional(&mut *tx)
                .await?;
        let Some(body) = body else {
            continue;
        };
        let envelope: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
        let recipients = envelope["recipients"]
            .as_array()
            .ok_or_else(|| sqlx::Error::Protocol("Invalid stored recipient scope".into()))?;
        let addressed: Vec<&str> = devices
            .iter()
            .filter(|cell| {
                let prefix = format!(
                    "x25519:cell:{}:",
                    cell["cell_uid"].as_str().unwrap_or_default()
                );
                recipients.iter().any(|recipient| {
                    recipient["key_id"].as_str().is_some_and(|key| {
                        cell["sealing_key"]["key_id"].as_str() == Some(key)
                            || key.starts_with(&prefix)
                    })
                })
            })
            .filter_map(|cell| cell["node_id"].as_str())
            .collect();
        let mut complete = !addressed.is_empty();
        for member in &addressed {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM mailbox_device_ack WHERE uid=? AND node_id=?)",
            )
            .bind(uid)
            .bind(member)
            .fetch_one(&mut *tx)
            .await?;
            complete &= exists;
        }
        if complete {
            sqlx::query("INSERT INTO mailbox_completion(uid,to_organ,body_hash,expires_at,completed_at) SELECT uid,to_organ,?,expires_at,? FROM mailbox_bundle WHERE uid=? AND to_organ=? ON CONFLICT(uid) DO NOTHING")
                .bind(nucleus::fact::sha256_hex(body.as_bytes())).bind(nucleus::execution::now().to_rfc3339())
                .bind(uid).bind(organ).execute(&mut *tx).await?;
            dropped += sqlx::query("DELETE FROM mailbox_bundle WHERE uid=? AND to_organ=?")
                .bind(uid)
                .bind(organ)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        }
    }
    tx.commit().await?;
    Ok(dropped)
}

pub async fn receive(
    pool: &SqlitePool,
    uid: &str,
    carrier: &str,
    body: &str,
    body_hash: &str,
    expires_at: &str,
) -> Result<(), StoreError> {
    sweep_received(pool).await?;
    let mut tx = crate::write_tx(pool).await?;
    if let Some(old) =
        sqlx::query_scalar::<_, String>("SELECT body_hash FROM mailbox_inbox WHERE uid=?")
            .bind(uid)
            .fetch_optional(&mut *tx)
            .await?
    {
        if old != body_hash {
            return Err(sqlx::Error::Protocol(
                "Conflicting received envelope".into(),
            ));
        }
        return Ok(());
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mailbox_inbox")
        .fetch_one(&mut *tx)
        .await?;
    if count >= 50_000
        || uid.len() > 128
        || carrier.len() > 128
        || body.len() > 1024 * 1024
        || body_hash.len() != 64
    {
        return Err(sqlx::Error::Protocol(
            "Invalid or full recoverable mail inbox".into(),
        ));
    }
    let bytes:i64=sqlx::query_scalar("SELECT COALESCE(SUM(length(CAST(body AS BLOB))),0) FROM mailbox_inbox WHERE state!='processed'")
        .fetch_one(&mut *tx).await?;
    if bytes.saturating_add(body.len() as i64) > 64 * 1024 * 1024 {
        return Err(sqlx::Error::Protocol(
            "The recoverable mail inbox is full".into(),
        ));
    }
    sqlx::query("INSERT INTO mailbox_inbox(uid,carrier,body,body_hash,received_at,expires_at) VALUES (?,?,?,?,?,?)")
        .bind(uid).bind(carrier).bind(body).bind(body_hash).bind(nucleus::execution::now().to_rfc3339()).bind(expires_at)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn pending(pool: &SqlitePool) -> Result<Vec<(String, String)>, StoreError> {
    sweep_received(pool).await?;
    Ok(sqlx::query("SELECT uid,body FROM mailbox_inbox WHERE state='pending' AND next_attempt<=? ORDER BY received_at LIMIT 32")
        .bind(nucleus::execution::now().timestamp()).fetch_all(pool).await?.into_iter()
        .map(|row| (row.get("uid"),row.get("body"))).collect())
}

pub async fn sweep_received(pool: &SqlitePool) -> Result<(), StoreError> {
    let now = nucleus::execution::now();
    sqlx::query("UPDATE mailbox_inbox SET state='expired',body='',error='The saved envelope recovery window expired' WHERE state IN ('pending','quarantine') AND expires_at<?")
        .bind((now-chrono::Duration::days(3)).to_rfc3339()).execute(pool).await?;
    sqlx::query(
        "DELETE FROM mailbox_inbox WHERE state IN ('processed','expired') AND received_at<?",
    )
    .bind((now - chrono::Duration::days(33)).to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn processed(
    pool: &SqlitePool,
    uid: &str,
    error: Option<&str>,
) -> Result<(), StoreError> {
    let error = error.map(|value| value.chars().take(120).collect::<String>());
    sqlx::query("UPDATE mailbox_inbox SET body=CASE WHEN ? IS NULL THEN '' ELSE body END,state=CASE WHEN ? IS NULL THEN 'processed' WHEN attempts>=7 THEN 'quarantine' ELSE 'pending' END,error=?,attempts=attempts+1,next_attempt=?+MIN(3600,5*(1<<MIN(attempts,10))) WHERE uid=?")
        .bind(&error).bind(&error).bind(&error).bind(nucleus::execution::now().timestamp()).bind(uid).execute(pool).await?;
    Ok(())
}

pub async fn redeem_and_register(
    pool: &SqlitePool,
    token: &str,
    organ: &str,
    root_key: &str,
) -> Result<bool, StoreError> {
    let now = nucleus::execution::now().to_rfc3339();
    let mut tx = crate::write_tx(pool).await?;
    let claimed=sqlx::query("UPDATE mailbox_invite SET used_at=?,used_by=? WHERE token_hash=? AND used_at IS NULL AND expires_at>? RETURNING label,quota_bytes")
        .bind(&now).bind(organ).bind(token).bind(&now).fetch_optional(&mut *tx).await?;
    let Some(row) = claimed else {
        return Ok(false);
    };
    if sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM mailbox_registration WHERE organ_uid=? AND root_key!=?)",
    )
    .bind(organ)
    .bind(root_key)
    .fetch_one(&mut *tx)
    .await?
    {
        return Err(sqlx::Error::Protocol(
            "This mailbox is registered to another root key".into(),
        ));
    }
    sqlx::query("INSERT INTO mailbox_registration(organ_uid,root_key,label,quota_bytes,registered_at) VALUES (?,?,?,?,?) ON CONFLICT(organ_uid) DO UPDATE SET label=excluded.label,quota_bytes=excluded.quota_bytes")
        .bind(organ).bind(root_key).bind(row.get::<String,_>("label")).bind(row.get::<i64,_>("quota_bytes"))
        .bind(now).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn inbox_status(pool: &SqlitePool) -> Result<Vec<serde_json::Value>, StoreError> {
    sweep_received(pool).await?;
    let rows=sqlx::query("SELECT uid,carrier,state,attempts,error,received_at,expires_at FROM mailbox_inbox WHERE state!='processed' ORDER BY received_at LIMIT 100")
        .fetch_all(pool).await?;
    Ok(rows.into_iter().map(|row| serde_json::json!({
        "uid":row.get::<String,_>("uid"),"carrier":row.get::<String,_>("carrier"),
        "state":row.get::<String,_>("state"),"attempts":row.get::<i64,_>("attempts"),
        "error":row.get::<Option<String>,_>("error"),"received_at":row.get::<String,_>("received_at"),
        "expires_at":row.get::<String,_>("expires_at")
    })).collect())
}

pub async fn retry(pool: &SqlitePool, uid: &str) -> Result<bool, StoreError> {
    Ok(sqlx::query("UPDATE mailbox_inbox SET state='pending',attempts=0,next_attempt=0,error=NULL WHERE uid=? AND state IN ('pending','quarantine') AND expires_at>=?")
        .bind(uid).bind((nucleus::execution::now()-chrono::Duration::days(3)).to_rfc3339()).execute(pool).await?.rows_affected()==1)
}
