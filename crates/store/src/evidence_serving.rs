//! Private approvals for serving minimal signed evidence.

use std::fmt::{self, Debug, Formatter};

use chrono::{DateTime, Utc};
use nucleus::{
    Cause, Fact, NewFact, RecordKind,
    fact::{seal, verify_chain_step},
    karma::canonical_json_bytes,
    trust_analysis::serving::{self, Review},
    valid_uid,
    visibility::Data,
};
use serde_json::{Value, from_str, to_string, to_value};
use sqlx::{Sqlite, SqliteConnection, Transaction, query, query_as, query_scalar};

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

const MAX_REQUEST_BYTES: usize = 950_000;
const MAX_REVIEW_BYTES: usize = 900_000;

type ServingRow = (
    String,
    String,
    String,
    i64,
    String,
    i64,
    String,
    String,
    String,
    bool,
    bool,
);

fn invalid(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}

/// A serving approval verified against its immutable review and original Fact.
pub struct Saved {
    /// Approval Record identity.
    pub record: String,
    /// Person who authorized the approval.
    pub owner: String,
    /// Exact reviewed artifact and serving scope.
    pub review: Review,
    /// Original zero-quantity approval Fact.
    pub fact: Fact,
    /// Whether the approval Record remains a live plain Record.
    pub active: bool,
    /// Whether this is the latest approval for the owner and source.
    pub current: bool,
}

impl Debug for Saved {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Saved")
            .field("record", &self.record)
            .field("active", &self.active)
            .field("current", &self.current)
            .finish_non_exhaustive()
    }
}

/// Reads and checks the stored approval, including its active and current flags.
/// # Errors
/// Returns an error if the approval or its original Fact is missing or differs from the reviewed evidence.
pub async fn get_on(connection: &mut SqliteConnection, record: &str) -> Result<Saved, StoreError> {
    let (owner, source, kind, version, artifact, expires, hash, payload, fact, active, current): ServingRow =
        query_as("SELECT a.owner_person_uid,a.source_record_uid,a.source_kind,a.source_version,a.artifact_hash,a.expires_at,a.review_hash,a.review_json,a.fact_uid,r.deleted_at IS NULL AND r.kind='plain',NOT EXISTS(SELECT 1 FROM record_evidence_serving n WHERE n.owner_person_uid=a.owner_person_uid AND n.source_record_uid=a.source_record_uid AND n.source_kind=a.source_kind AND n.sequence>a.sequence) FROM record_evidence_serving a JOIN record r ON r.uid=a.record_uid WHERE a.record_uid=?")
            .bind(record)
            .fetch_optional(&mut *connection)
            .await?
            .ok_or_else(|| invalid("Evidence serving permission is unavailable"))?;
    let review: Review = from_str(&payload).map_err(invalid)?;
    let fact = facts::get_on(connection, &fact)
        .await?
        .ok_or_else(|| invalid("Original serving approval Fact is unavailable"))?;
    if review.person != owner
        || review.request.source.record != source
        || review.request.source.kind.as_str() != kind
        || i64::try_from(review.source_version).ok() != Some(version)
        || review.artifact.hash().map_err(invalid)?.as_str() != artifact
        || review.expires_at != expires
        || review.hash().map_err(invalid)?.as_str() != hash
        || fact.record_uid != record
        || fact.actor_uid.as_deref() != Some(owner.as_str())
        || fact.payload.as_deref() != Some(payload.as_str())
        || !fact.delta.is_zero()
        || !verify_chain_step(&fact)
    {
        return Err(invalid(
            "Serving permission differs from its original reviewed approval",
        ));
    }
    Ok(Saved {
        record: record.into(),
        owner,
        review,
        fact,
        active,
        current,
    })
}

fn request_json(person: &str, request: &Value) -> Result<String, StoreError> {
    let bytes = canonical_json_bytes(&(person, request)).map_err(invalid)?;
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(invalid("Serving request exceeds its byte limit"));
    }
    String::from_utf8(bytes).map_err(invalid)
}

/// Replays the original approval for an identical request by the same Person.
/// # Errors
/// Returns an error if the request ID denotes different contents or the original approval cannot be read.
pub async fn replay_on(
    connection: &mut SqliteConnection,
    person: &str,
    id: &str,
    request: &Value,
) -> Result<Option<Saved>, StoreError> {
    let row: Option<(String, String, String)> =
        query_as("SELECT owner_person_uid,request_json,record_uid FROM record_evidence_serving WHERE request_id=?")
            .bind(id)
            .fetch_optional(&mut *connection)
            .await?;
    if let Some((owner, previous, record)) = row {
        if owner != person || previous != request_json(person, request)? {
            return Err(invalid(
                "Serving request ID already has different fields or owner",
            ));
        }
        return get_on(connection, &record).await.map(Some);
    }
    Ok(None)
}

/// A reviewed serving approval and the exact client request that produced it.
pub struct Input {
    /// Identity to assign the new approval Record.
    pub record: String,
    /// Identity used to recognize an identical retry.
    pub request_id: String,
    /// Original request data bound into replay validation.
    pub request: Value,
    /// Reviewed source, scope, artifact, and expiry.
    pub review: Review,
    /// Verified pending intent if a client authorized the approval.
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

/// Writes an authorized private approval and returns the caller’s transaction for commit.
/// # Errors
/// Returns an error if the capacity, request, or authorization is invalid, or a required write fails.
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
    let payload = to_string(review).map_err(invalid)?;
    let request = request_json(&review.person, &input.request)?;
    let (count, bytes): (i64, i64) = query_as("SELECT COUNT(*),COALESCE(SUM(length(CAST(review_json AS BLOB))+length(CAST(request_json AS BLOB))),0) FROM record_evidence_serving")
        .fetch_one(&mut *tx)
        .await?;
    if usize::try_from(count).map_or(true, |count| count >= serving::MAX_PERMISSIONS)
        || usize::try_from(bytes).map_or(true, |bytes| {
            bytes
                .saturating_add(payload.len())
                .saturating_add(request.len())
                > serving::MAX_STORAGE_BYTES
        })
        || payload.len() > MAX_REVIEW_BYTES
        || !valid_uid(&input.record, "r")
        || input.request_id.is_empty()
        || input.request_id.len() > 256
        || input.request_id.chars().any(char::is_control)
        || !is_active_on(&mut tx, &review.person).await?
    {
        return Err(invalid(
            "Serving permission capacity, controller or request limit exceeded",
        ));
    }
    let local: String =
        query_scalar("SELECT uid FROM record WHERE slug=? AND kind='organ' AND deleted_at IS NULL")
            .bind(LOCAL_ORGAN_SLUG)
            .fetch_one(&mut *tx)
            .await?;
    create_with_uid_on(
        &mut tx,
        &input.record,
        NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "Evidence serving permission",
            body: "A private approval for a minimal signed document. Its source Record and accounting remain private.",
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
            record_uid: input.record,
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
            "Sign the serving approval or supply its verified client intent",
        ));
    }
    facts::insert(&mut tx, &fact).await?;
    if let Some(intent) = &input.authorization_intent {
        link_pending_fact(&mut tx, intent, &fact).await?;
    }
    query("INSERT INTO record_evidence_serving(record_uid,owner_person_uid,source_record_uid,source_kind,source_version,artifact_hash,expires_at,review_hash,review_json,request_id,request_json,fact_uid,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&fact.record_uid)
        .bind(&review.person)
        .bind(&review.request.source.record)
        .bind(review.request.source.kind.as_str())
        .bind(i64::try_from(review.source_version).map_err(invalid)?)
        .bind(review.artifact.hash().map_err(invalid)?.as_str())
        .bind(review.expires_at)
        .bind(hash.as_str())
        .bind(fact.payload.as_deref())
        .bind(&input.request_id)
        .bind(request)
        .bind(&fact.uid)
        .bind(now.to_rfc3339())
        .execute(&mut *tx)
        .await?;
    set_extension_on(
        &mut tx,
        &fact.record_uid,
        serving::NAMESPACE,
        &to_value(review).map_err(invalid)?,
    )
    .await?;
    let saved = get_on(&mut tx, &fact.record_uid).await?;
    Ok((saved, tx))
}
