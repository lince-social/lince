//! Persistence for client-signed Action authorization evidence.

use chrono::{DateTime, Utc};
use nucleus::Fact;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;

#[derive(Debug, Clone)]
pub struct NewSignedActionIntent<'a> {
    pub session_id: &'a str,
    pub session_challenge: &'a str,
    pub sequence: u64,
    pub message_id: &'a str,
    pub actor_person_uid: &'a str,
    pub key_id: &'a str,
    pub action_base64: &'a str,
    pub signature: &'a str,
}

#[derive(Debug, Clone)]
pub struct StoredSignedActionIntent {
    pub uid: String,
    pub session_id: String,
    pub session_challenge: String,
    pub sequence: u64,
    pub message_id: String,
    pub actor_person_uid: String,
    pub key_id: String,
    pub action_base64: String,
    pub signature: String,
    pub status: String,
}

pub async fn insert_pending(
    pool: &SqlitePool,
    input: NewSignedActionIntent<'_>,
    now: DateTime<Utc>,
) -> Result<StoredSignedActionIntent, StoreError> {
    let sequence = i64::try_from(input.sequence)
        .map_err(|_| sqlx::Error::Protocol("Action intent sequence exceeds SQLite range".into()))?;
    let uid = nucleus::new_uid("sai");
    sqlx::query(
        "INSERT INTO signed_action_intent
            (uid, session_id, session_challenge, sequence, message_id,
             actor_person_uid, key_id, action_base64, signature, received_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(input.session_id)
    .bind(input.session_challenge)
    .bind(sequence)
    .bind(input.message_id)
    .bind(input.actor_person_uid)
    .bind(input.key_id)
    .bind(input.action_base64)
    .bind(input.signature)
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(StoredSignedActionIntent {
        uid,
        session_id: input.session_id.into(),
        session_challenge: input.session_challenge.into(),
        sequence: input.sequence,
        message_id: input.message_id.into(),
        actor_person_uid: input.actor_person_uid.into(),
        key_id: input.key_id.into(),
        action_base64: input.action_base64.into(),
        signature: input.signature.into(),
        status: "pending".into(),
    })
}

pub async fn mark_committed(
    pool: &SqlitePool,
    intent_uid: &str,
    fact_uids: &[String],
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    let intent =
        sqlx::query("SELECT actor_person_uid, status FROM signed_action_intent WHERE uid = ?")
            .bind(intent_uid)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
    let actor: String = intent.get("actor_person_uid");
    let status: String = intent.get("status");
    if status == "failed" {
        return Err(sqlx::Error::Protocol(
            "failed Action intent cannot be committed".into(),
        ));
    }
    for fact_uid in fact_uids {
        let fact_actor: Option<String> =
            sqlx::query_scalar("SELECT actor_uid FROM fact WHERE uid = ?")
                .bind(fact_uid)
                .fetch_optional(&mut *tx)
                .await?
                .flatten();
        if fact_actor.as_deref() != Some(actor.as_str()) {
            return Err(sqlx::Error::Protocol(
                "Action intent can link only Facts attributed to its Person".into(),
            ));
        }
        let linked: Option<String> =
            sqlx::query_scalar("SELECT intent_uid FROM fact_action_intent WHERE fact_uid = ?")
                .bind(fact_uid)
                .fetch_optional(&mut *tx)
                .await?;
        match linked.as_deref() {
            Some(existing) if existing != intent_uid => {
                return Err(sqlx::Error::Protocol(
                    "Fact is already attributed to a different Action intent".into(),
                ));
            }
            Some(_) => {}
            None => {
                sqlx::query("INSERT INTO fact_action_intent (fact_uid, intent_uid) VALUES (?, ?)")
                    .bind(fact_uid)
                    .bind(intent_uid)
                    .execute(&mut *tx)
                    .await?;
            }
        }
    }
    sqlx::query(
        "UPDATE signed_action_intent
         SET status = 'committed', error_code = NULL, error_message = NULL, finished_at = ?
         WHERE uid = ?",
    )
    .bind(now.to_rfc3339())
    .bind(intent_uid)
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// Link an unsigned semantic Fact while its authorizing intent is still
/// pending in the same higher-level action. Call this inside the semantic
/// transaction before returning the Fact to the engine.
pub async fn link_pending_fact(
    tx: &mut Transaction<'_, Sqlite>,
    intent_uid: &str,
    fact: &Fact,
) -> Result<(), StoreError> {
    let row =
        sqlx::query("SELECT actor_person_uid, status FROM signed_action_intent WHERE uid = ?")
            .bind(intent_uid)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
    let status: String = row.get("status");
    let actor: String = row.get("actor_person_uid");
    if status != "pending" || fact.actor_uid.as_deref() != Some(actor.as_str()) {
        return Err(sqlx::Error::Protocol(
            "pending Action intent does not authorize this Fact actor".into(),
        ));
    }
    sqlx::query("INSERT INTO fact_action_intent (fact_uid, intent_uid) VALUES (?, ?)")
        .bind(&fact.uid)
        .bind(intent_uid)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn mark_failed(
    pool: &SqlitePool,
    intent_uid: &str,
    error_code: Option<&str>,
    error_message: &str,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE signed_action_intent
         SET status = 'failed', error_code = ?, error_message = ?, finished_at = ?
         WHERE uid = ? AND status = 'pending'",
    )
    .bind(error_code)
    .bind(error_message)
    .bind(now.to_rfc3339())
    .bind(intent_uid)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn for_fact(
    pool: &SqlitePool,
    fact_uid: &str,
) -> Result<Option<StoredSignedActionIntent>, StoreError> {
    Ok(sqlx::query(
        "SELECT sai.uid, sai.session_id, sai.session_challenge, sai.sequence,
                sai.message_id, sai.actor_person_uid, sai.key_id,
                sai.action_base64, sai.signature, sai.status
         FROM fact_action_intent fai
         JOIN signed_action_intent sai ON sai.uid = fai.intent_uid
         WHERE fai.fact_uid = ? AND sai.status = 'committed'",
    )
    .bind(fact_uid)
    .fetch_optional(pool)
    .await?
    .map(map_row))
}

pub async fn fact_has_committed_intent(
    pool: &SqlitePool,
    fact_uid: &str,
) -> Result<bool, StoreError> {
    sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM fact_action_intent fai
             JOIN signed_action_intent sai ON sai.uid = fai.intent_uid
             WHERE fai.fact_uid = ? AND sai.status = 'committed'
         )",
    )
    .bind(fact_uid)
    .fetch_one(pool)
    .await
}

pub async fn published_keys_for_actor(
    pool: &SqlitePool,
    actor_uid: &str,
) -> Result<Vec<(String, String)>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT key_id, public_key FROM identity_key
         WHERE actor_uid = ? ORDER BY key_id",
    )
    .bind(actor_uid)
    .fetch_all(pool)
    .await?)
}

fn map_row(row: sqlx::sqlite::SqliteRow) -> StoredSignedActionIntent {
    StoredSignedActionIntent {
        uid: row.get("uid"),
        session_id: row.get("session_id"),
        session_challenge: row.get("session_challenge"),
        sequence: row.get::<i64, _>("sequence") as u64,
        message_id: row.get("message_id"),
        actor_person_uid: row.get("actor_person_uid"),
        key_id: row.get("key_id"),
        action_base64: row.get("action_base64"),
        signature: row.get("signature"),
        status: row.get("status"),
    }
}
