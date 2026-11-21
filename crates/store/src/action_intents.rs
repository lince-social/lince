//! Persistence and attribution of authenticated signed Action intents.

use std::fmt::{self, Debug, Formatter};

use chrono::{DateTime, Utc};
use nucleus::{Fact, new_uid};
use sqlx::{
    Row, Sqlite, SqliteConnection, SqlitePool, Transaction, query, query_as, query_scalar,
    sqlite::SqliteRow,
};

use crate::{StoreError, write_tx};

/// Borrowed signed Action envelope ready to be persisted.
#[derive(Clone)]
pub struct NewSignedActionIntent<'a> {
    /// Authenticated session identity.
    pub session_id: &'a str,
    /// Challenge bound into the signed envelope.
    pub session_challenge: &'a str,
    /// Sequence within the authenticated session.
    pub sequence: u64,
    /// Client message identity within the session.
    pub message_id: &'a str,
    /// Person to whom the Action is attributed.
    pub actor_person_uid: &'a str,
    /// Published key that signed the Action.
    pub key_id: &'a str,
    /// Exact encoded Action payload.
    pub action_base64: &'a str,
    /// Exact signature supplied with the Action.
    pub signature: &'a str,
}

/// A persisted Action envelope and its dispatch status.
#[derive(Clone)]
pub struct StoredSignedActionIntent {
    /// Local identity of the stored intent.
    pub uid: String,
    /// Authenticated session identity.
    pub session_id: String,
    /// Challenge bound into the signed envelope.
    pub session_challenge: String,
    /// Sequence within the authenticated session.
    pub sequence: u64,
    /// Client message identity within the session.
    pub message_id: String,
    /// Person to whom the Action is attributed.
    pub actor_person_uid: String,
    /// Published key that signed the Action.
    pub key_id: String,
    /// Exact encoded Action payload.
    pub action_base64: String,
    /// Exact signature supplied with the Action.
    pub signature: String,
    /// Stored dispatch status.
    pub status: String,
}

impl Debug for NewSignedActionIntent<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NewSignedActionIntent")
            .field("sequence", &self.sequence)
            .finish_non_exhaustive()
    }
}

impl Debug for StoredSignedActionIntent {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredSignedActionIntent")
            .field("uid", &self.uid)
            .field("sequence", &self.sequence)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

/// Stores the exact signed envelope before dispatch.
/// # Errors
/// Returns an error if the sequence cannot be represented or the envelope conflicts with stored data.
pub async fn insert_pending(
    pool: &SqlitePool,
    input: NewSignedActionIntent<'_>,
    now: DateTime<Utc>,
) -> Result<StoredSignedActionIntent, StoreError> {
    let sequence = i64::try_from(input.sequence)
        .map_err(|_| StoreError::Protocol("Action intent sequence exceeds SQLite range".into()))?;
    let uid = new_uid("sai");
    query(
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

/// Commits successful dispatch and attributes the listed Facts in one transaction.
/// # Errors
/// Returns an error if the intent is missing or failed, a Fact is incompatible, or the write fails.
pub async fn mark_committed(
    pool: &SqlitePool,
    intent_uid: &str,
    fact_uids: &[String],
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    let (actor, status) = actor_and_status(&mut tx, intent_uid).await?;
    if status == "failed" {
        return Err(StoreError::Protocol(
            "failed Action intent cannot be committed".into(),
        ));
    }
    for fact_uid in fact_uids {
        let fact_actor: Option<String> = query_scalar("SELECT actor_uid FROM fact WHERE uid = ?")
            .bind(fact_uid)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
        if fact_actor.as_deref() != Some(actor.as_str()) {
            return Err(StoreError::Protocol(
                "Action intent can link only Facts attributed to its Person".into(),
            ));
        }
        let linked: Option<String> =
            query_scalar("SELECT intent_uid FROM fact_action_intent WHERE fact_uid = ?")
                .bind(fact_uid)
                .fetch_optional(&mut *tx)
                .await?;
        match linked.as_deref() {
            Some(existing) if existing != intent_uid => {
                return Err(StoreError::Protocol(
                    "Fact is already attributed to a different Action intent".into(),
                ));
            }
            Some(_) => {}
            None => {
                associate(&mut tx, intent_uid, fact_uid).await?;
            }
        }
    }
    query(
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

/// Links a Fact to a pending intent on the caller’s transaction.
/// # Errors
/// Returns an error if the intent is absent, is no longer pending, or does not authorize the Fact actor.
pub async fn link_pending_fact(
    tx: &mut Transaction<'_, Sqlite>,
    intent_uid: &str,
    fact: &Fact,
) -> Result<(), StoreError> {
    let (actor, status) = actor_and_status(tx, intent_uid).await?;
    if status != "pending" || fact.actor_uid.as_deref() != Some(actor.as_str()) {
        return Err(StoreError::Protocol(
            "pending Action intent does not authorize this Fact actor".into(),
        ));
    }
    associate(tx, intent_uid, &fact.uid).await
}

/// Marks an intent failed while it is still pending.
/// # Errors
/// Returns an error if updating the intent fails.
pub async fn mark_failed(
    pool: &SqlitePool,
    intent_uid: &str,
    error_code: Option<&str>,
    error_message: &str,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    query(
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

/// Reads the committed signed Action that authorized the Fact.
/// # Errors
/// Returns an error if the associated intent cannot be read or decoded.
pub async fn for_fact(
    pool: &SqlitePool,
    fact_uid: &str,
) -> Result<Option<StoredSignedActionIntent>, StoreError> {
    query(
        "SELECT sai.uid, sai.session_id, sai.session_challenge, sai.sequence,
                sai.message_id, sai.actor_person_uid, sai.key_id,
                sai.action_base64, sai.signature, sai.status
         FROM fact_action_intent fai
         JOIN signed_action_intent sai ON sai.uid = fai.intent_uid
         WHERE fai.fact_uid = ? AND sai.status = 'committed'",
    )
    .bind(fact_uid)
    .try_map(map_row)
    .fetch_optional(pool)
    .await
}

/// Reports whether the Fact is linked to a committed signed Action.
/// # Errors
/// Returns an error if the attribution cannot be read.
pub async fn fact_has_committed_intent(
    pool: &SqlitePool,
    fact_uid: &str,
) -> Result<bool, StoreError> {
    query_scalar(
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

/// Lists the actor’s published key IDs and public keys in ID order.
/// # Errors
/// Returns an error if the key bindings cannot be read.
pub async fn published_keys_for_actor(
    pool: &SqlitePool,
    actor_uid: &str,
) -> Result<Vec<(String, String)>, StoreError> {
    query_as(
        "SELECT key_id, public_key FROM identity_key
         WHERE actor_uid = ? ORDER BY key_id",
    )
    .bind(actor_uid)
    .fetch_all(pool)
    .await
}

async fn actor_and_status(
    connection: &mut SqliteConnection,
    intent_uid: &str,
) -> Result<(String, String), StoreError> {
    query_as("SELECT actor_person_uid, status FROM signed_action_intent WHERE uid = ?")
        .bind(intent_uid)
        .fetch_one(connection)
        .await
}

async fn associate(
    connection: &mut SqliteConnection,
    intent_uid: &str,
    fact_uid: &str,
) -> Result<(), StoreError> {
    query("INSERT INTO fact_action_intent (fact_uid, intent_uid) VALUES (?, ?)")
        .bind(fact_uid)
        .bind(intent_uid)
        .execute(connection)
        .await?;
    Ok(())
}

fn map_row(row: SqliteRow) -> Result<StoredSignedActionIntent, StoreError> {
    Ok(StoredSignedActionIntent {
        uid: row.try_get("uid")?,
        session_id: row.try_get("session_id")?,
        session_challenge: row.try_get("session_challenge")?,
        sequence: u64::try_from(row.try_get::<i64, _>("sequence")?)
            .map_err(|error| StoreError::Protocol(error.to_string()))?,
        message_id: row.try_get("message_id")?,
        actor_person_uid: row.try_get("actor_person_uid")?,
        key_id: row.try_get("key_id")?,
        action_base64: row.try_get("action_base64")?,
        signature: row.try_get("signature")?,
        status: row.try_get("status")?,
    })
}
