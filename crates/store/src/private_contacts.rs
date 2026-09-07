use chrono::DateTime;
use sqlx::{Acquire, Sqlite, SqliteConnection, Transaction};

use crate::StoreError;
use crate::session_access::{self, ContactTrust, PeerContact};

pub const MAX_LOGIN_TIMESTAMP_BYTES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactBinding {
    pub peer: PeerContact,
    pub person_uid: Option<String>,
}

fn refusal(message: &str) -> StoreError {
    sqlx::Error::Protocol(message.into())
}

fn record_uid(uid: &str) -> Result<(), StoreError> {
    if nucleus::valid_uid(uid, "r") {
        Ok(())
    } else {
        Err(refusal(
            "private contact requires a canonical Record identity",
        ))
    }
}

fn node_id(value: &str) -> Result<(), StoreError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(refusal(
            "private contact requires a canonical transport NodeId",
        ))
    }
}

async fn generation_on(connection: &mut SqliteConnection, uid: &str) -> Result<i64, StoreError> {
    sqlx::query_scalar::<_, Option<i64>>(
        "SELECT CASE WHEN typeof(organ_uid) = 'text' AND typeof(generation) = 'integer'
                           AND generation > 0 THEN generation END
           FROM organ_login_generation WHERE organ_uid = ?",
    )
    .bind(uid)
    .fetch_optional(connection)
    .await?
    .flatten()
    .ok_or_else(|| refusal("private contact generation is missing or corrupt"))
}

async fn login_on(
    connection: &mut SqliteConnection,
    organ_uid: &str,
) -> Result<Option<String>, StoreError> {
    let row = sqlx::query_as::<_, (i64, i64, Option<String>, Option<String>)>(
        "WITH bounded AS (
             SELECT COUNT(*) AS matches,
                    COALESCE(SUM(CASE WHEN typeof(organ_uid) = 'text'
                         AND typeof(person_uid) = 'text' AND length(CAST(person_uid AS BLOB)) = 28
                         AND typeof(created_at) = 'text' AND length(CAST(created_at AS BLOB)) BETWEEN 1 AND ?
                         THEN 0 ELSE 1 END), 0) AS invalid
               FROM organ_login WHERE CAST(organ_uid AS TEXT) = ?
         )
         SELECT b.matches, b.invalid, l.person_uid, l.created_at FROM bounded b
           LEFT JOIN organ_login l ON b.matches = 1 AND b.invalid = 0 AND l.organ_uid = ? LIMIT 1",
    )
    .bind(MAX_LOGIN_TIMESTAMP_BYTES as i64)
    .bind(organ_uid)
    .bind(organ_uid)
    .fetch_one(&mut *connection)
    .await?;
    if row == (0, 0, None, None) {
        return Ok(None);
    }
    let (1, 0, Some(person), Some(created_at)) = row else {
        return Err(refusal(
            "private contact login has invalid or oversized stored data",
        ));
    };
    record_uid(&person)?;
    if DateTime::parse_from_rfc3339(&created_at).is_err() {
        return Err(refusal("private contact login timestamp is invalid"));
    }
    let valid = sqlx::query_scalar::<_, i64>(
        "SELECT CASE WHEN typeof(uid) = 'text' AND typeof(kind) = 'text' AND kind = 'person'
                           AND (deleted_at IS NULL OR typeof(deleted_at) = 'text')
                      THEN 1 ELSE 0 END FROM record WHERE uid = ?",
    )
    .bind(&person)
    .fetch_optional(&mut *connection)
    .await?;
    if valid != Some(1) {
        return Err(refusal(
            "private contact login refers to a missing or invalid Person",
        ));
    }
    Ok(Some(person))
}

async fn binding_on(
    connection: &mut SqliteConnection,
    organ_uid: &str,
) -> Result<ContactBinding, StoreError> {
    record_uid(organ_uid)?;
    let row = sqlx::query_as::<_, (i64, i64, Option<String>)>(
        "WITH bounded AS (
             SELECT COUNT(*) AS matches,
                    COALESCE(SUM(CASE WHEN typeof(record_uid) = 'text'
                         AND typeof(node_id) = 'text' AND length(CAST(node_id AS BLOB)) = 64
                         THEN 0 ELSE 1 END), 0) AS invalid
               FROM organ_contact WHERE CAST(record_uid AS TEXT) = ?
         )
         SELECT b.matches, b.invalid, c.node_id FROM bounded b
           LEFT JOIN organ_contact c ON b.matches = 1 AND b.invalid = 0 AND c.record_uid = ? LIMIT 1",
    )
    .bind(organ_uid)
    .bind(organ_uid)
    .fetch_one(&mut *connection)
    .await?;
    let (1, 0, Some(node)) = row else {
        return Err(refusal(
            "private contact is missing or has invalid stored identity",
        ));
    };
    let peer = session_access::peer_contact_on(connection, &node)
        .await?
        .ok_or_else(|| refusal("private contact has no current peer binding"))?;
    if peer.organ_uid != organ_uid || peer.trust == ContactTrust::Blocked {
        return Err(refusal("private contact binding is unavailable"));
    }
    let person_uid = login_on(connection, organ_uid).await?;
    Ok(ContactBinding { peer, person_uid })
}

pub async fn get_on(
    transaction: &mut Transaction<'_, Sqlite>,
    organ_uid: &str,
) -> Result<ContactBinding, StoreError> {
    binding_on(transaction, organ_uid).await
}

async fn create_inner(
    connection: &mut SqliteConnection,
    hosted_organ_uid: &str,
    contact_organ_uid: &str,
    node: &str,
) -> Result<ContactBinding, StoreError> {
    let valid = sqlx::query_scalar::<_, i64>(
        "SELECT CASE WHEN typeof(r.uid) = 'text' AND typeof(r.kind) = 'text' AND r.kind = 'organ'
                           AND r.deleted_at IS NULL AND typeof(r.organ_uid) = 'text' AND r.organ_uid = ?
                           AND typeof(h.uid) = 'text' AND typeof(h.kind) = 'text' AND h.kind = 'organ'
                           AND h.deleted_at IS NULL
                      THEN 1 ELSE 0 END
           FROM record r JOIN record h ON h.uid = ? WHERE r.uid = ?",
    )
    .bind(hosted_organ_uid)
    .bind(hosted_organ_uid)
    .bind(contact_organ_uid)
    .fetch_optional(&mut *connection)
    .await?;
    if valid != Some(1) {
        return Err(refusal(
            "private contact requires a live company-origin Organ description",
        ));
    }
    let exists: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM organ_contact WHERE CAST(record_uid AS TEXT) = ?")
            .bind(contact_organ_uid)
            .fetch_one(&mut *connection)
            .await?;
    if exists != 0 || login_on(connection, contact_organ_uid).await?.is_some() {
        return Err(refusal(
            "private contact creation requires an absent contact and login",
        ));
    }
    if session_access::peer_contact_on(connection, node)
        .await?
        .is_some()
    {
        return Err(refusal("private contact NodeId is already bound"));
    }
    let before = generation_on(connection, contact_organ_uid).await?;
    before
        .checked_add(1)
        .ok_or_else(|| refusal("private contact generation is exhausted"))?;
    let inserted = sqlx::query(
        "INSERT INTO organ_contact (
             record_uid, node_id, trust, proximity, sync_out, sync_in, last_synced_seq,
             mode, catchup_interval_secs, pending_introduction, peer_acked_seq,
             scope_fields, scope_version, accept_fields, accept_version, closed_by_default,
             share_protein, share_seen_seq, unreachable_since, mailed_at, awaiting_roster_since
         ) VALUES (?, ?, 'unknown', 1, 0, 0, 0, 'direct', 30, 0, 0, '[]', 0, '[]', 0, 1,
                   NULL, NULL, NULL, NULL, NULL)",
    )
    .bind(contact_organ_uid)
    .bind(node)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    let closed = sqlx::query_scalar::<_, i64>(
        "SELECT CASE WHEN typeof(proximity) = 'integer' AND proximity = 1
                           AND typeof(sync_out) = 'integer' AND sync_out = 0
                           AND typeof(sync_in) = 'integer' AND sync_in = 0
                           AND typeof(last_synced_seq) = 'integer' AND last_synced_seq = 0
                           AND typeof(peer_acked_seq) = 'integer' AND peer_acked_seq = 0
                           AND typeof(mode) = 'text' AND mode = 'direct'
                           AND typeof(catchup_interval_secs) = 'integer' AND catchup_interval_secs = 30
                           AND typeof(pending_introduction) = 'integer' AND pending_introduction = 0
                           AND typeof(scope_fields) = 'text' AND scope_fields = '[]'
                           AND typeof(scope_version) = 'integer' AND scope_version = 0
                           AND typeof(accept_fields) = 'text' AND accept_fields = '[]'
                           AND typeof(accept_version) = 'integer' AND accept_version = 0
                           AND typeof(closed_by_default) = 'integer' AND closed_by_default = 1
                           AND share_protein IS NULL AND share_seen_seq IS NULL
                           AND unreachable_since IS NULL AND mailed_at IS NULL AND awaiting_roster_since IS NULL
                      THEN 1 ELSE 0 END FROM organ_contact WHERE record_uid = ?",
    ).bind(contact_organ_uid).fetch_one(&mut *connection).await?;
    let result = binding_on(connection, contact_organ_uid).await?;
    if inserted != 1
        || closed != 1
        || result.peer.node_id != node
        || result.peer.trust != ContactTrust::Unknown
        || result.peer.generation <= before
        || result.person_uid.is_some()
        || session_access::granted_login_on(connection, contact_organ_uid, node)
            .await?
            .is_some()
    {
        return Err(refusal(
            "private contact creation did not preserve its exact binding",
        ));
    }
    Ok(result)
}

pub async fn create_on(
    transaction: &mut Transaction<'_, Sqlite>,
    hosted_organ_uid: &str,
    contact_organ_uid: &str,
    node: &str,
) -> Result<ContactBinding, StoreError> {
    record_uid(hosted_organ_uid)?;
    record_uid(contact_organ_uid)?;
    node_id(node)?;
    if hosted_organ_uid == contact_organ_uid {
        return Err(refusal(
            "private contact cannot adopt the hosted Organ identity",
        ));
    }
    let mut savepoint = transaction.begin().await?;
    match create_inner(&mut savepoint, hosted_organ_uid, contact_organ_uid, node).await {
        Ok(result) => {
            savepoint.commit().await?;
            Ok(result)
        }
        Err(error) => {
            savepoint.rollback().await?;
            Err(error)
        }
    }
}

enum LoginChange<'a> {
    Grant(&'a str),
    Replace(&'a str),
    Revoke,
}

async fn change_inner(
    connection: &mut SqliteConnection,
    organ_uid: &str,
    expected_generation: i64,
    change: LoginChange<'_>,
) -> Result<ContactBinding, StoreError> {
    let current = binding_on(connection, organ_uid).await?;
    if current.peer.generation != expected_generation {
        return Err(refusal("private contact binding generation conflict"));
    }
    let proposed = match change {
        LoginChange::Grant(person) if current.person_uid.is_none() => Some(person),
        LoginChange::Replace(person) if current.person_uid.is_some() => Some(person),
        LoginChange::Revoke if current.person_uid.is_some() => None,
        _ => {
            return Err(refusal(
                "private login operation does not match current presence",
            ));
        }
    };
    if let Some(person) = proposed {
        session_access::person_generation_on(connection, person).await?;
        if current.person_uid.as_deref() == Some(person) {
            session_access::granted_login_on(connection, organ_uid, &current.peer.node_id)
                .await?
                .filter(|authentication| authentication.person_uid() == person)
                .ok_or_else(|| refusal("unchanged private login is not current"))?;
            return Ok(current);
        }
    }
    expected_generation
        .checked_add(1)
        .ok_or_else(|| refusal("private contact generation is exhausted"))?;
    let changed = match change {
        LoginChange::Grant(person) => {
            sqlx::query(
                "INSERT INTO organ_login (organ_uid, person_uid, created_at) VALUES (?, ?, ?)",
            )
            .bind(organ_uid)
            .bind(person)
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&mut *connection)
            .await?
        }
        LoginChange::Replace(person) => {
            sqlx::query("UPDATE organ_login SET person_uid = ? WHERE organ_uid = ?")
                .bind(person)
                .bind(organ_uid)
                .execute(&mut *connection)
                .await?
        }
        LoginChange::Revoke => {
            sqlx::query("DELETE FROM organ_login WHERE organ_uid = ?")
                .bind(organ_uid)
                .execute(&mut *connection)
                .await?
        }
    };
    let result = binding_on(connection, organ_uid).await?;
    if changed.rows_affected() != 1
        || result.person_uid.as_deref() != proposed
        || result.peer.node_id != current.peer.node_id
        || result.peer.trust != current.peer.trust
        || result.peer.generation <= current.peer.generation
    {
        return Err(refusal(
            "private login mutation did not preserve its exact binding",
        ));
    }
    let authentication =
        session_access::granted_login_on(connection, organ_uid, &result.peer.node_id).await?;
    if authentication
        .as_ref()
        .map(|authentication| authentication.person_uid())
        != proposed
    {
        return Err(refusal(
            "private login mutation failed current authentication validation",
        ));
    }
    Ok(result)
}

async fn change_on(
    transaction: &mut Transaction<'_, Sqlite>,
    organ_uid: &str,
    expected_generation: i64,
    change: LoginChange<'_>,
) -> Result<ContactBinding, StoreError> {
    record_uid(organ_uid)?;
    if expected_generation <= 0 {
        return Err(refusal(
            "private login requires a positive expected generation",
        ));
    }
    let mut savepoint = transaction.begin().await?;
    match change_inner(&mut savepoint, organ_uid, expected_generation, change).await {
        Ok(result) => {
            savepoint.commit().await?;
            Ok(result)
        }
        Err(error) => {
            savepoint.rollback().await?;
            Err(error)
        }
    }
}

pub async fn grant_on(
    transaction: &mut Transaction<'_, Sqlite>,
    organ_uid: &str,
    expected_generation: i64,
    person_uid: &str,
) -> Result<ContactBinding, StoreError> {
    change_on(
        transaction,
        organ_uid,
        expected_generation,
        LoginChange::Grant(person_uid),
    )
    .await
}

pub async fn replace_on(
    transaction: &mut Transaction<'_, Sqlite>,
    organ_uid: &str,
    expected_generation: i64,
    person_uid: &str,
) -> Result<ContactBinding, StoreError> {
    change_on(
        transaction,
        organ_uid,
        expected_generation,
        LoginChange::Replace(person_uid),
    )
    .await
}

pub async fn revoke_on(
    transaction: &mut Transaction<'_, Sqlite>,
    organ_uid: &str,
    expected_generation: i64,
) -> Result<ContactBinding, StoreError> {
    change_on(
        transaction,
        organ_uid,
        expected_generation,
        LoginChange::Revoke,
    )
    .await
}
