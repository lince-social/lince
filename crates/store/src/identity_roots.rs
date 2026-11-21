//! Reviewed Organ root recognition and its retained signed history.
use crate::{
    StoreError,
    action_intents::link_pending_fact,
    data_visibility::ensure_private_for_person_on,
    exact::zero,
    facts,
    organs::LOCAL_ORGAN_SLUG,
    people::is_active_on,
    records::{NewRecord, create_with_uid_on, set_extension_on},
    sync_ops::{OpKind, log_local_tx},
};
use chrono::{DateTime, Utc};
use nucleus::{
    Cause, Fact, NewFact, RecordKind,
    fact::{seal, verify_chain_step},
    identity::roots::{self, KeyStatus, Review, State},
    karma::canonical_json_bytes,
    valid_uid,
    visibility::Data,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, from_str, to_string, to_value};
use sqlx::{Sqlite, SqliteConnection, Transaction, query, query_as, query_scalar};
use std::fmt::{self, Debug, Formatter};
const MAX_REQUEST_BYTES: usize = 64 * 1024;
const EVIDENCE_MISMATCH: &str =
    "Root recognition differs from its original reviewed decision and audit Fact";
type RecognitionRow = (String, i64, String, String, String, String, String, bool);
fn invalid(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}

/// Root recognition and the original Fact bound to the decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Saved {
    /// Record containing the recognition.
    pub record: String,
    /// Person who approved the recognition.
    pub owner: String,
    /// Organ’s recognition revision.
    pub revision: u64,
    /// Original reviewed decision.
    pub review: Review,
    /// Original signed approval Fact.
    pub fact: Fact,
    /// Whether the Record is live and still has the expected kind.
    pub active: bool,
}

/// Reads a recognition and verifies its original decision and approval Fact.
/// # Errors
/// Returns an error if the decision or original Fact is unavailable, unreadable, or inconsistent.
pub async fn get_on(connection: &mut SqliteConnection, record: &str) -> Result<Saved, StoreError> {
    let (organ, revision, owner, key, hash, payload, fact, active): RecognitionRow = query_as("SELECT a.organ_uid,a.revision,a.owner_person_uid,a.public_key,a.review_hash,a.review_json,a.fact_uid,r.deleted_at IS NULL AND r.kind='plain' FROM organ_root_recognition a JOIN record r ON r.uid=a.record_uid WHERE a.record_uid=?")
        .bind(record)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(||invalid("Root recognition is unavailable"))?;
    let review: Review = from_str(&payload).map_err(invalid)?;
    let fact = facts::get_on(connection, &fact)
        .await?
        .ok_or_else(|| invalid("Original root recognition Fact is unavailable"))?;
    let revision = u64::try_from(revision).map_err(|_| invalid(EVIDENCE_MISMATCH))?;
    if review.proof.organ != organ
        || review.person != owner
        || review.proof.public_key != key
        || review.previous.revision.checked_add(1) != Some(revision)
        || review.hash().map_err(invalid)?.as_str() != hash
        || fact.record_uid != record
        || fact.actor_uid.as_deref() != Some(&owner)
        || fact.payload.as_deref() != Some(&payload)
        || !fact.delta.is_zero()
        || !verify_chain_step(&fact)
    {
        return Err(invalid(EVIDENCE_MISMATCH));
    }
    Ok(Saved {
        record: record.into(),
        owner,
        revision,
        review,
        fact,
        active,
    })
}

/// Reads the highest recognition revision for this Organ.
/// # Errors
/// Returns an error if the recognition index or its latest decision cannot be read.
pub async fn current_on(
    connection: &mut SqliteConnection,
    organ: &str,
) -> Result<Option<Saved>, StoreError> {
    let record: Option<String> = query_scalar("SELECT record_uid FROM organ_root_recognition WHERE organ_uid=? ORDER BY revision DESC LIMIT 1")
        .bind(organ)
        .fetch_optional(&mut *connection)
        .await?;
    match record {
        Some(record) => get_on(connection, &record).await.map(Some),
        None => Ok(None),
    }
}

/// Resolves the original and current roots with their stored revocation status.
/// # Errors
/// Returns an error if the identity, latest recognition, or revocations cannot be read.
pub async fn state_on(connection: &mut SqliteConnection, organ: &str) -> Result<State, StoreError> {
    let original: Option<String> = query_scalar(
        "SELECT public_key FROM identity_key WHERE actor_uid=? AND key_id='ed25519:root:v1'",
    )
    .bind(organ)
    .fetch_optional(&mut *connection)
    .await?;
    let saved = current_on(connection, organ).await?;
    let current = saved
        .as_ref()
        .map(|saved| saved.review.proof.public_key.clone())
        .or_else(|| original.clone());
    async fn status(
        connection: &mut SqliteConnection,
        organ: &str,
        key: Option<String>,
    ) -> Result<Option<KeyStatus>, StoreError> {
        if let Some(public_key) = key {
            let revoked: bool = query_scalar("SELECT EXISTS(SELECT 1 FROM identity_revocation WHERE organ_uid=? AND revoked_key=?)")
                .bind(organ)
                .bind(&public_key)
                .fetch_one(connection)
                .await?;
            Ok(Some(KeyStatus {
                public_key,
                revoked,
            }))
        } else {
            Ok(None)
        }
    }
    Ok(State {
        original: status(connection, organ, original).await?,
        current: status(connection, organ, current).await?,
        revision: saved.as_ref().map_or(0, |saved| saved.revision),
        available: saved.as_ref().is_none_or(|saved| saved.active),
    })
}

/// Lists verified recognitions in revision order.
/// # Errors
/// Returns an error if the bounded history or a recognition cannot be read.
pub async fn history_on(
    connection: &mut SqliteConnection,
    organ: &str,
) -> Result<Vec<Saved>, StoreError> {
    let records: Vec<String> = query_scalar("SELECT record_uid FROM organ_root_recognition WHERE organ_uid=? ORDER BY revision LIMIT 65")
        .bind(organ)
        .fetch_all(&mut *connection)
        .await?;
    if records.len() > usize::try_from(roots::MAX_ROOTS).map_err(invalid)? {
        return Err(invalid("Root recognition history exceeds its limit"));
    }
    let mut history = Vec::with_capacity(records.len());
    for record in records {
        history.push(get_on(connection, &record).await?);
    }
    Ok(history)
}
fn request_json(person: &str, request: &Value) -> Result<String, StoreError> {
    let bytes = canonical_json_bytes(&(person, request)).map_err(invalid)?;
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(invalid("Root recognition request exceeds 64 KiB"));
    }
    String::from_utf8(bytes).map_err(invalid)
}

/// Reads the original recognition for an identical request by the same Person.
/// # Errors
/// Returns an error if the request conflicts or the original decision cannot be read.
pub async fn replay_on(
    connection: &mut SqliteConnection,
    person: &str,
    id: &str,
    request: &Value,
) -> Result<Option<Saved>, StoreError> {
    let previous: Option<(String, String, String)> = query_as("SELECT owner_person_uid,request_json,record_uid FROM organ_root_recognition WHERE request_id=?")
        .bind(id)
        .fetch_optional(&mut *connection)
        .await?;
    if let Some((owner, previous, record)) = previous {
        if owner != person || previous != request_json(person, request)? {
            return Err(invalid(
                "Root recognition request ID already has different fields or owner",
            ));
        }
        return get_on(connection, &record).await.map(Some);
    }
    Ok(None)
}

/// Reviewed recognition and the original request to persist.
pub struct Input {
    /// New recognition Record identity.
    pub record: String,
    /// Identity used to find the original request.
    pub request_id: String,
    /// Original request used for replay comparison.
    pub request: Value,
    /// Reviewed key and the expected previous root state.
    pub review: Review,
    /// Verified pending Action intent if a client authorized the decision.
    pub authorization_intent: Option<String>,
}
impl Debug for Input {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Input")
            .field("record", &self.record)
            .finish_non_exhaustive()
    }
}

/// Appends an authorized private recognition and returns the transaction for commit.
/// # Errors
/// Returns an error if the reviewed state, authority, request, or required write fails validation.
pub async fn save<F>(
    mut tx: Transaction<'static, Sqlite>,
    input: Input,
    now: DateTime<Utc>,
    sign: F,
) -> Result<(Saved, Transaction<'static, Sqlite>), StoreError>
where
    F: FnOnce(&str) -> Option<String>,
{
    let review = &input.review;
    let hash = review.hash().map_err(invalid)?;
    if !valid_uid(&input.record, "r")
        || input.request_id.is_empty()
        || input.request_id.len() > 256
        || input.request_id.chars().any(char::is_control)
        || !is_active_on(&mut tx, &review.person).await?
    {
        return Err(invalid(
            "Choose an active controller, canonical Record and bounded request ID",
        ));
    }
    if state_on(&mut tx, &review.proof.organ).await? != review.previous {
        return Err(invalid("The reviewed root state changed"));
    }
    let payload = to_string(review).map_err(invalid)?;
    let local: String =
        query_scalar("SELECT uid FROM record WHERE kind='organ' AND slug=? AND deleted_at IS NULL")
            .bind(LOCAL_ORGAN_SLUG)
            .fetch_one(&mut *tx)
            .await?;
    create_with_uid_on(
        &mut tx,
        &input.record,
        NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "Reviewed Organ root recognition",
            body: "An administrator's independent identity decision. Original keys and signed history remain retained.",
            quantity: zero(),
        },
        &local,
        None,
    )
    .await?;
    if let Some(policy) =
        ensure_private_for_person_on(&mut tx, &input.record, Data::Record, Some(&review.person))
            .await?
    {
        log_local_tx(
            &mut tx,
            "data_visibility",
            &input.record,
            "record",
            OpKind::Set,
            Some(to_string(&policy).map_err(invalid)?),
        )
        .await?;
    }
    let mut fact = seal(
        NewFact {
            uid: None,
            record_uid: input.record.clone(),
            delta: zero(),
            at: None,
            actor_uid: Some(review.person.clone()),
            cause: Cause::user_edit(),
            payload: Some(payload),
        },
        &facts::last_hash(&mut tx).await?,
        now,
    );
    fact.signature = sign(&fact.hash);
    if fact.signature.is_none() && input.authorization_intent.is_none() {
        return Err(invalid(
            "Sign the recognition audit or provide its verified Action intent",
        ));
    }
    facts::insert(&mut tx, &fact).await?;
    if let Some(intent) = &input.authorization_intent {
        link_pending_fact(&mut tx, intent, &fact).await?;
    }
    let revision = review
        .previous
        .revision
        .checked_add(1)
        .ok_or_else(|| invalid("Root recognition revision exceeds its limit"))?;
    query("INSERT INTO organ_root_recognition(record_uid,organ_uid,revision,owner_person_uid,public_key,review_hash,review_json,request_id,request_json,fact_uid,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)")

        .bind(&input.record)
        .bind(&review.proof.organ)
        .bind(i64::try_from(revision).map_err(invalid)?)
        .bind(&review.person)
        .bind(&review.proof.public_key)
        .bind(hash.as_str())
        .bind(fact.payload.as_deref())
        .bind(&input.request_id)
        .bind(request_json(&review.person,&input.request)?)
        .bind(&fact.uid)
        .bind(now.to_rfc3339())
        .execute(&mut *tx)
        .await?;
    set_extension_on(
        &mut tx,
        &input.record,
        roots::NAMESPACE,
        &to_value(review).map_err(invalid)?,
    )
    .await?;
    Ok((
        Saved {
            record: input.record,
            owner: review.person.clone(),
            revision,
            review: input.review,
            fact,
            active: true,
        },
        tx,
    ))
}
