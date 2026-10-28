use crate::StoreError;
use chrono::{DateTime, Utc};
use nucleus::trust_analysis::contacts::{Document, NAMESPACE};
use nucleus::{Cause, Fact, NewFact, RecordKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{SqliteConnection, SqlitePool};

fn invalid(message: impl ToString) -> StoreError {
    StoreError::Protocol(message.to_string())
}

fn request_json(
    person: &str,
    request: &Value,
    expected: Option<u64>,
) -> Result<String, StoreError> {
    let bytes =
        nucleus::karma::canonical_json_bytes(&(person, request, expected)).map_err(invalid)?;
    if bytes.len() > 65536 {
        return Err(invalid("Contact disclosure request exceeds 64 KiB"));
    }
    String::from_utf8(bytes).map_err(invalid)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Saved {
    pub record: String,
    pub owner: String,
    pub document: Document,
    pub fact: Fact,
}

#[derive(sqlx::FromRow)]
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

pub async fn get_on(connection: &mut SqliteConnection, record: &str) -> Result<Saved, StoreError> {
    let row: Stored = sqlx::query_as(
        "SELECT owner_person_uid AS owner,relation_hash AS relation,statement_hash AS statement,document_hash AS hash,issuer_kind,issuer_uid AS issuer,contact_kind,contact_uid AS contact,revision,issued_at,expires_at,withdrawn,document_json AS payload,fact_uid AS fact FROM contact_disclosure_record WHERE record_uid=?",
    ).bind(record).fetch_optional(&mut *connection).await?.ok_or_else(|| invalid("Contact disclosure is unavailable"))?;
    let document = Document::parse(row.payload.as_bytes()).map_err(invalid)?;
    let fact = crate::facts::get_on(connection, &row.fact)
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
        || statement.revision != row.revision as u64
        || statement.issued_at != row.issued_at
        || statement.expires_at != row.expires_at
        || statement.withdrawn != row.withdrawn
        || fact.record_uid != record
        || fact.actor_uid.as_deref() != Some(&row.owner)
        || fact.payload.as_deref() != Some(&row.payload)
        || !fact.delta.is_zero()
        || !nucleus::fact::verify_chain_step(&fact)
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

pub async fn get(pool: &SqlitePool, record: &str) -> Result<Saved, StoreError> {
    get_on(&mut *pool.acquire().await?, record).await
}

pub async fn replay_on(
    connection: &mut SqliteConnection,
    person: &str,
    id: &str,
    request: &Value,
    expected_revision: Option<u64>,
) -> Result<Option<Saved>, StoreError> {
    let previous: Option<(String, String, String)> = sqlx::query_as(
        "SELECT owner_person_uid,request_json,record_uid FROM contact_disclosure_request WHERE request_id=?",
    ).bind(id).fetch_optional(&mut *connection).await?;
    if let Some((owner, previous, record)) = previous {
        if owner != person || previous != request_json(person, request, expected_revision)? {
            return Err(invalid(
                "Contact request ID was used with different contents or author",
            ));
        }
        return get_on(connection, &record).await.map(Some);
    }
    Ok(None)
}

pub async fn find_on(
    connection: &mut SqliteConnection,
    person: &str,
    document: &Document,
) -> Result<Option<Saved>, StoreError> {
    let record: Option<String> = sqlx::query_scalar(
        "SELECT record_uid FROM contact_disclosure_record WHERE owner_person_uid=? AND document_hash=?",
    ).bind(person).bind(document.id().map_err(invalid)?.as_str()).fetch_optional(&mut *connection).await?;
    match record {
        Some(record) => get_on(connection, &record).await.map(Some),
        None => Ok(None),
    }
}

pub async fn latest_revision_on(
    connection: &mut SqliteConnection,
    person: &str,
    statement: &nucleus::trust_analysis::contacts::Statement,
) -> Result<u64, StoreError> {
    let revision: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(revision),0) FROM contact_disclosure_record WHERE owner_person_uid=? AND relation_hash=? AND issuer_kind=? AND issuer_uid=?",
    ).bind(person).bind(statement.relation().map_err(invalid)?.as_str()).bind(statement.issuer.kind()).bind(statement.issuer.uid()).fetch_one(connection).await?;
    u64::try_from(revision).map_err(invalid)
}

pub async fn current_candidates(
    pool: &SqlitePool,
    person: &str,
) -> Result<Vec<String>, StoreError> {
    current_candidates_on(&mut *pool.acquire().await?, person).await
}

pub async fn current_candidates_on(
    connection: &mut SqliteConnection,
    person: &str,
) -> Result<Vec<String>, StoreError> {
    sqlx::query_scalar(
        "SELECT c.record_uid FROM contact_disclosure_record c WHERE c.owner_person_uid=? AND c.revision=(SELECT MAX(n.revision) FROM contact_disclosure_record n WHERE n.owner_person_uid=c.owner_person_uid AND n.relation_hash=c.relation_hash AND n.issuer_kind=c.issuer_kind AND n.issuer_uid=c.issuer_uid) ORDER BY c.relation_hash,c.issuer_kind,c.issuer_uid,c.document_hash LIMIT 5001",
    ).bind(person).fetch_all(connection).await
}

pub struct Input {
    pub record: String,
    pub person: String,
    pub request_id: String,
    pub request: Value,
    pub document: Document,
    pub expected_revision: Option<u64>,
    pub authorization_intent: Option<String>,
}

pub struct Commit {
    pub saved: Saved,
    pub created: bool,
}

pub async fn save<F>(
    mut tx: sqlx::Transaction<'static, sqlx::Sqlite>,
    input: Input,
    now: DateTime<Utc>,
    sign: F,
) -> Result<(Commit, sqlx::Transaction<'static, sqlx::Sqlite>), StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    if !nucleus::valid_uid(&input.record, "r")
        || input.request_id.is_empty()
        || input.request_id.len() > 256
        || input.request_id.chars().any(char::is_control)
    {
        return Err(invalid("Choose a contact request ID of 1–256 characters"));
    }
    input.document.validate().map_err(invalid)?;
    let request = request_json(&input.person, &input.request, input.expected_revision)?;
    if let Some(saved) = replay_on(
        &mut tx,
        &input.person,
        &input.request_id,
        &input.request,
        input.expected_revision,
    )
    .await?
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
    if !crate::people::is_active_on(&mut tx, &input.person).await? {
        return Err(invalid(
            "Contact disclosure storage requires an active Person",
        ));
    }
    if let Some(saved) = find_on(&mut tx, &input.person, &input.document).await? {
        sqlx::query("INSERT INTO contact_disclosure_request(request_id,owner_person_uid,request_json,record_uid) VALUES(?,?,?,?)")
            .bind(&input.request_id).bind(&input.person).bind(request).bind(&saved.record).execute(&mut *tx).await?;
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
    let payload = serde_json::to_string(&input.document).map_err(invalid)?;
    if payload.len() > nucleus::trust_analysis::contacts::MAX_DOCUMENT_BYTES {
        return Err(invalid("Contact disclosure exceeds 32 KiB"));
    }
    let organ: String = sqlx::query_scalar(
        "SELECT uid FROM record WHERE kind='organ' AND slug=? AND deleted_at IS NULL",
    )
    .bind(crate::organs::LOCAL_ORGAN_SLUG)
    .fetch_one(&mut *tx)
    .await?;
    crate::records::create_with_uid_on(
        &mut tx,
        &input.record,
        crate::records::NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: if statement.withdrawn { "Contact disclosure withdrawal" } else { "Signed contact disclosure" },
            body: "Signed relationship evidence. Identity, visibility and reliability remain separate.",
            quantity: crate::exact::zero(),
        },
        &organ,
        None,
    ).await?;
    if let Some(policy) = crate::data_visibility::ensure_private_for_person_on(
        &mut tx,
        &input.record,
        nucleus::visibility::Data::Record,
        Some(&input.person),
    )
    .await?
    {
        crate::sync_ops::log_local_tx(
            &mut tx,
            "data_visibility",
            &input.record,
            "record",
            crate::sync_ops::OpKind::Set,
            Some(serde_json::to_string(&policy).map_err(invalid)?),
        )
        .await?;
    }
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: input.record.clone(),
            delta: crate::exact::zero(),
            at: None,
            actor_uid: Some(input.person.clone()),
            cause: Cause::user_edit(),
            payload: Some(payload.clone()),
        },
        &crate::facts::last_hash(&mut tx).await?,
        now,
    );
    fact.signature = sign(&fact.hash);
    if fact.signature.is_none() && input.authorization_intent.is_none() {
        return Err(invalid(
            "Contact evidence needs its owner's signing key or verified Action intent",
        ));
    }
    crate::facts::insert(&mut tx, &fact).await?;
    if let Some(intent) = &input.authorization_intent {
        crate::action_intents::link_pending_fact(&mut tx, intent, &fact).await?;
    }
    sqlx::query("INSERT INTO contact_disclosure_record(record_uid,owner_person_uid,relation_hash,statement_hash,document_hash,issuer_kind,issuer_uid,contact_kind,contact_uid,revision,issued_at,expires_at,withdrawn,document_json,fact_uid,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&input.record).bind(&input.person)
        .bind(statement.relation().map_err(invalid)?.as_str()).bind(statement.id().map_err(invalid)?.as_str()).bind(input.document.id().map_err(invalid)?.as_str())
        .bind(statement.issuer.kind()).bind(statement.issuer.uid()).bind(statement.contact.kind()).bind(statement.contact.uid())
        .bind(statement.revision as i64).bind(statement.issued_at).bind(statement.expires_at).bind(statement.withdrawn).bind(&payload).bind(&fact.uid).bind(now.to_rfc3339())
        .execute(&mut *tx).await?;
    crate::records::set_extension_on(
        &mut tx,
        &input.record,
        NAMESPACE,
        &serde_json::to_value(&input.document).map_err(invalid)?,
    )
    .await?;
    sqlx::query("INSERT INTO contact_disclosure_request(request_id,owner_person_uid,request_json,record_uid) VALUES(?,?,?,?)")
        .bind(&input.request_id).bind(&input.person).bind(request).bind(&input.record).execute(&mut *tx).await?;
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
