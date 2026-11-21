//! Private storage and exact replay for signed contact disclosures.

use std::fmt::{self, Debug, Formatter};

use chrono::{DateTime, Utc};
use nucleus::{
    Cause, Fact, NewFact, RecordKind,
    fact::{seal, verify_chain_step},
    karma::canonical_json_bytes,
    trust_analysis::contacts::{Document, MAX_DOCUMENT_BYTES, NAMESPACE, Statement},
    valid_uid,
    visibility::Data,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, to_string, to_value};
use sqlx::{
    FromRow, Sqlite, SqliteConnection, SqlitePool, Transaction, query, query_as, query_scalar,
};

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

const MAX_REQUEST_BYTES: usize = 64 * 1024;

fn invalid(message: impl ToString) -> StoreError {
    StoreError::Protocol(message.to_string())
}

fn request_json(
    person: &str,
    request: &Value,
    expected: Option<u64>,
) -> Result<String, StoreError> {
    let bytes = canonical_json_bytes(&(person, request, expected)).map_err(invalid)?;
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(invalid("Contact disclosure request exceeds 64 KiB"));
    }
    String::from_utf8(bytes).map_err(invalid)
}

/// Immutable disclosure and the Fact that records its approval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Saved {
    /// Record containing the disclosure.
    pub record: String,
    /// Person who stored the disclosure.
    pub owner: String,
    /// Original signed contact document.
    pub document: Document,
    /// Original zero-quantity Fact bound to the document.
    pub fact: Fact,
}

#[derive(FromRow)]
struct Stored {
    owner: String,
    relation: String,
    statement: String,
    hash: String,
    issuer_kind: String,
    issuer: String,
    contact_kind: String,
    contact: String,
    revision: i64,
    issued_at: i64,
    expires_at: i64,
    withdrawn: bool,
    payload: String,
    fact: String,
}

/// Reads a disclosure and checks it against its original signed evidence.
/// # Errors
/// Returns an error if the disclosure or original Fact is absent, unreadable, or inconsistent.
pub async fn get_on(connection: &mut SqliteConnection, record: &str) -> Result<Saved, StoreError> {
    let row: Stored = query_as(
        "SELECT owner_person_uid AS owner,relation_hash AS relation,statement_hash AS statement,document_hash AS hash,issuer_kind,issuer_uid AS issuer,contact_kind,contact_uid AS contact,revision,issued_at,expires_at,withdrawn,document_json AS payload,fact_uid AS fact FROM contact_disclosure_record WHERE record_uid=?",
    )
        .bind(record)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| invalid("Contact disclosure is unavailable"))?;
    let document = Document::parse(row.payload.as_bytes()).map_err(invalid)?;
    let fact = facts::get_on(connection, &row.fact)
        .await?
        .ok_or_else(|| invalid("Original contact disclosure Fact is unavailable"))?;
    let statement = &document.statement;
    if document.id().map_err(invalid)?.as_str() != row.hash
        || statement.id().map_err(invalid)?.as_str() != row.statement
        || statement.relation().map_err(invalid)?.as_str() != row.relation
        || statement.issuer.kind() != row.issuer_kind
        || statement.issuer.uid() != row.issuer
        || statement.contact.kind() != row.contact_kind
        || statement.contact.uid() != row.contact
        || i64::try_from(statement.revision).ok() != Some(row.revision)
        || statement.issued_at != row.issued_at
        || statement.expires_at != row.expires_at
        || statement.withdrawn != row.withdrawn
        || fact.record_uid != record
        || fact.actor_uid.as_deref() != Some(&row.owner)
        || fact.payload.as_deref() != Some(&row.payload)
        || !fact.delta.is_zero()
        || !verify_chain_step(&fact)
    {
        return Err(invalid(
            "Contact disclosure differs from its original signed evidence",
        ));
    }
    Ok(Saved {
        record: record.into(),
        owner: row.owner,
        document,
        fact,
    })
}

/// Reads a verified disclosure using a connection from the pool.
/// # Errors
/// Returns an error if a connection cannot be acquired or the disclosure cannot be verified.
pub async fn get(pool: &SqlitePool, record: &str) -> Result<Saved, StoreError> {
    get_on(&mut *pool.acquire().await?, record).await
}

/// Replays an identical request by the original Person.
/// # Errors
/// Returns an error if the request ID conflicts or its original disclosure cannot be verified.
pub async fn replay_on(
    connection: &mut SqliteConnection,
    person: &str,
    id: &str,
    request: &Value,
    expected_revision: Option<u64>,
) -> Result<Option<Saved>, StoreError> {
    replay_request(connection, person, id, || {
        request_json(person, request, expected_revision)
    })
    .await
}

async fn replay_request<T: AsRef<str>>(
    connection: &mut SqliteConnection,
    person: &str,
    id: &str,
    encode_request: impl FnOnce() -> Result<T, StoreError>,
) -> Result<Option<Saved>, StoreError> {
    let previous: Option<(String, String, String)> = query_as(
        "SELECT owner_person_uid,request_json,record_uid FROM contact_disclosure_request WHERE request_id=?",
    )
        .bind(id)
        .fetch_optional(&mut *connection)
        .await?;
    if let Some((owner, previous, record)) = previous {
        if owner != person || previous != encode_request()?.as_ref() {
            return Err(invalid(
                "Contact request ID was used with different contents or author",
            ));
        }
        return get_on(connection, &record).await.map(Some);
    }
    Ok(None)
}

/// Finds the Person’s original copy of the exact signed document.
/// # Errors
/// Returns an error if the document or stored disclosure cannot be verified.
pub async fn find_on(
    connection: &mut SqliteConnection,
    person: &str,
    document: &Document,
) -> Result<Option<Saved>, StoreError> {
    let record: Option<String> = query_scalar(
        "SELECT record_uid FROM contact_disclosure_record WHERE owner_person_uid=? AND document_hash=?",
    )
        .bind(person)
        .bind(document.id().map_err(invalid)?.as_str())
        .fetch_optional(&mut *connection)
        .await?;
    match record {
        Some(record) => get_on(connection, &record).await.map(Some),
        None => Ok(None),
    }
}

/// Reads the highest revision for the owner, relation, and issuer.
/// # Errors
/// Returns an error if the relation or stored revision cannot be read.
pub async fn latest_revision_on(
    connection: &mut SqliteConnection,
    person: &str,
    statement: &Statement,
) -> Result<u64, StoreError> {
    let revision: i64 = query_scalar(
        "SELECT COALESCE(MAX(revision),0) FROM contact_disclosure_record WHERE owner_person_uid=? AND relation_hash=? AND issuer_kind=? AND issuer_uid=?",
    )
        .bind(person)
        .bind(statement.relation().map_err(invalid)?.as_str())
        .bind(statement.issuer.kind())
        .bind(statement.issuer.uid())
        .fetch_one(connection)
        .await?;
    u64::try_from(revision).map_err(invalid)
}

/// Lists current disclosure identities using a connection from the pool.
/// # Errors
/// Returns an error if the connection or disclosure index cannot be read.
pub async fn current_candidates(
    pool: &SqlitePool,
    person: &str,
) -> Result<Vec<String>, StoreError> {
    current_candidates_on(&mut *pool.acquire().await?, person).await
}

/// Lists the latest candidates per relation and issuer in stable index order.
/// # Errors
/// Returns an error if the disclosure index cannot be read.
pub async fn current_candidates_on(
    connection: &mut SqliteConnection,
    person: &str,
) -> Result<Vec<String>, StoreError> {
    query_scalar(
        "SELECT c.record_uid FROM contact_disclosure_record c WHERE c.owner_person_uid=? AND c.revision=(SELECT MAX(n.revision) FROM contact_disclosure_record n WHERE n.owner_person_uid=c.owner_person_uid AND n.relation_hash=c.relation_hash AND n.issuer_kind=c.issuer_kind AND n.issuer_uid=c.issuer_uid) ORDER BY c.relation_hash,c.issuer_kind,c.issuer_uid,c.document_hash LIMIT 5001",
    )
        .bind(person)
        .fetch_all(connection)
        .await
}

/// Data needed to store an original contact disclosure.
pub struct Input {
    /// Identity of the proposed disclosure Record.
    pub record: String,
    /// Person authorizing the disclosure.
    pub person: String,
    /// Identity used for exact request replay.
    pub request_id: String,
    /// Original request bound into replay validation.
    pub request: Value,
    /// Signed contact document to store.
    pub document: Document,
    /// Observed revision if the change requires a revision check.
    pub expected_revision: Option<u64>,
    /// Verified pending Action intent when supplied by the client.
    pub authorization_intent: Option<String>,
}

/// Outcome of a disclosure write or replay.
pub struct Commit {
    /// Original disclosure and its approval Fact.
    pub saved: Saved,
    /// Whether this operation created the disclosure.
    pub created: bool,
}

impl Debug for Input {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Input")
            .field("record", &self.record)
            .field("expected_revision", &self.expected_revision)
            .finish_non_exhaustive()
    }
}

impl Debug for Commit {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Commit")
            .field("record", &self.saved.record)
            .field("created", &self.created)
            .finish_non_exhaustive()
    }
}

/// Stores authorized disclosure evidence or replays the original result.
/// # Errors
/// Returns an error if authority, revision, term, request, or a required write fails validation.
pub async fn save<F>(
    mut tx: Transaction<'static, Sqlite>,
    input: Input,
    now: DateTime<Utc>,
    sign: F,
) -> Result<(Commit, Transaction<'static, Sqlite>), StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    if !valid_uid(&input.record, "r")
        || input.request_id.is_empty()
        || input.request_id.len() > 256
        || input.request_id.chars().any(char::is_control)
    {
        return Err(invalid("Choose a contact request ID of 1–256 characters"));
    }
    input.document.validate().map_err(invalid)?;
    let request = request_json(&input.person, &input.request, input.expected_revision)?;
    if let Some(saved) =
        replay_request(&mut tx, &input.person, &input.request_id, || Ok(&request)).await?
    {
        if saved.document != input.document {
            return Err(invalid(
                "Contact request replay cannot replace its original evidence",
            ));
        }
        return Ok((
            Commit {
                saved,
                created: false,
            },
            tx,
        ));
    }
    if !is_active_on(&mut tx, &input.person).await? {
        return Err(invalid(
            "Contact disclosure storage requires an active Person",
        ));
    }
    if let Some(saved) = find_on(&mut tx, &input.person, &input.document).await? {
        register_request(
            &mut tx,
            &input.request_id,
            &input.person,
            &request,
            &saved.record,
        )
        .await?;
        return Ok((
            Commit {
                saved,
                created: false,
            },
            tx,
        ));
    }
    let statement = &input.document.statement;
    if let Some(expected) = input.expected_revision {
        if latest_revision_on(&mut tx, &input.person, statement).await? != expected
            || expected.checked_add(1) != Some(statement.revision)
        {
            return Err(invalid(
                "The contact disclosure changed; review its current revision",
            ));
        }
        if statement.issued_at > now.timestamp() || statement.expires_at <= now.timestamp() {
            return Err(invalid(
                "Review a currently effective contact disclosure term",
            ));
        }
    }
    let payload = to_string(&input.document).map_err(invalid)?;
    if payload.len() > MAX_DOCUMENT_BYTES {
        return Err(invalid("Contact disclosure exceeds 32 KiB"));
    }
    let organ: String =
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
            head: if statement.withdrawn { "Contact disclosure withdrawal" } else { "Signed contact disclosure" },
            body: "Signed relationship evidence. Identity, visibility and reliability remain separate.",
            quantity: zero(),
        },
        &organ,
        None,
    ).await?;
    if let Some(policy) =
        ensure_private_for_person_on(&mut tx, &input.record, Data::Record, Some(&input.person))
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
            actor_uid: Some(input.person.clone()),
            cause: Cause::user_edit(),
            payload: Some(payload),
        },
        &facts::last_hash(&mut tx).await?,
        now,
    );
    fact.signature = sign(&fact.hash);
    if fact.signature.is_none() && input.authorization_intent.is_none() {
        return Err(invalid(
            "Contact evidence needs its owner's signing key or verified Action intent",
        ));
    }
    facts::insert(&mut tx, &fact).await?;
    if let Some(intent) = &input.authorization_intent {
        link_pending_fact(&mut tx, intent, &fact).await?;
    }
    query("INSERT INTO contact_disclosure_record(record_uid,owner_person_uid,relation_hash,statement_hash,document_hash,issuer_kind,issuer_uid,contact_kind,contact_uid,revision,issued_at,expires_at,withdrawn,document_json,fact_uid,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")

        .bind(&input.record)
        .bind(&input.person)

        .bind(statement.relation().map_err(invalid)?.as_str())
        .bind(statement.id().map_err(invalid)?.as_str())
        .bind(input.document.id().map_err(invalid)?.as_str())

        .bind(statement.issuer.kind())
        .bind(statement.issuer.uid())
        .bind(statement.contact.kind())
        .bind(statement.contact.uid())

        .bind(i64::try_from(statement.revision).map_err(invalid)?)
        .bind(statement.issued_at)
        .bind(statement.expires_at)
        .bind(statement.withdrawn)
        .bind(fact.payload.as_deref())
        .bind(&fact.uid)
        .bind(now.to_rfc3339())
        .execute(&mut *tx).await?;
    set_extension_on(
        &mut tx,
        &input.record,
        NAMESPACE,
        &to_value(&input.document).map_err(invalid)?,
    )
    .await?;
    register_request(
        &mut tx,
        &input.request_id,
        &input.person,
        &request,
        &input.record,
    )
    .await?;
    Ok((
        Commit {
            saved: Saved {
                record: input.record,
                owner: input.person,
                document: input.document,
                fact,
            },
            created: true,
        },
        tx,
    ))
}

async fn register_request(
    connection: &mut SqliteConnection,
    id: &str,
    person: &str,
    request: &str,
    record: &str,
) -> Result<(), StoreError> {
    query("INSERT INTO contact_disclosure_request(request_id,owner_person_uid,request_json,record_uid) VALUES(?,?,?,?)")
        .bind(id)
        .bind(person)
        .bind(request)
        .bind(record)
        .execute(connection)
        .await?;
    Ok(())
}
