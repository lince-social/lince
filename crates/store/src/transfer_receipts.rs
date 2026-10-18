use crate::StoreError;
use chrono::{DateTime, Utc};
use nucleus::transfer::receipt::{Command, Document, ScopeRequest};
use nucleus::{Cause, Fact, NewFact, RecordKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{SqliteConnection, SqlitePool};

fn invalid(message: impl ToString) -> StoreError {
    StoreError::Protocol(message.to_string())
}

fn request_json(person: &str, command: &Command) -> Result<String, StoreError> {
    let value = serde_json::to_string(&(person, command)).map_err(invalid)?;
    if value.len() > 850_000 {
        return Err(invalid("Receipt request exceeds its byte budget"));
    }
    Ok(value)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Saved {
    pub record: String,
    pub owner: String,
    pub version: u64,
    pub document: Document,
    pub fact: Fact,
}

pub async fn get_on(
    connection: &mut SqliteConnection,
    uid: &str,
    version: Option<u64>,
) -> Result<Saved, StoreError> {
    let version = version.map(i64::try_from).transpose().map_err(invalid)?;
    let (owner, version, payload, hash, fact_uid): (String, i64, String, String, String) = sqlx::query_as(
        "SELECT r.owner_person_uid,v.version,v.document_json,v.document_hash,v.fact_uid FROM transfer_receipt_record r JOIN transfer_receipt_version v ON v.record_uid=r.record_uid JOIN record d ON d.uid=r.record_uid WHERE r.record_uid=? AND d.deleted_at IS NULL AND (? IS NULL OR v.version=?) ORDER BY v.version DESC LIMIT 1",
    ).bind(uid).bind(version).bind(version).fetch_optional(&mut *connection).await?
        .ok_or_else(|| invalid("Receipt Record or version is unavailable"))?;
    let document = Document::parse(payload.as_bytes(), nucleus::execution::now().timestamp())
        .map_err(invalid)?;
    let fact = crate::facts::get_on(connection, &fact_uid)
        .await?
        .ok_or_else(|| invalid("Original receipt Fact is unavailable"))?;
    if fact.record_uid != uid
        || fact.actor_uid.as_deref() != Some(&owner)
        || fact.payload.as_deref() != Some(&payload)
        || !nucleus::fact::verify_chain_step(&fact)
        || document.hash().map_err(invalid)?.as_str() != hash
        || !fact.delta.is_zero()
    {
        return Err(invalid("Receipt version differs from its original Fact"));
    }
    Ok(Saved {
        record: uid.into(),
        owner,
        version: version as u64,
        document,
        fact,
    })
}

pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Saved, StoreError> {
    get_on(&mut *pool.acquire().await?, uid, None).await
}

pub async fn get_version(pool: &SqlitePool, uid: &str, version: u64) -> Result<Saved, StoreError> {
    if version == 0 {
        return Err(invalid("Choose a positive receipt version"));
    }
    get_on(&mut *pool.acquire().await?, uid, Some(version)).await
}

#[derive(Serialize, sqlx::FromRow)]
pub struct VersionHeader {
    pub version: i64,
    pub hash: String,
    pub saved_at: String,
}

pub async fn versions(
    pool: &SqlitePool,
    uid: &str,
    before: Option<i64>,
) -> Result<Vec<VersionHeader>, StoreError> {
    sqlx::query_as("SELECT v.version,v.document_hash AS hash,v.created_at AS saved_at FROM transfer_receipt_version v JOIN record d ON d.uid=v.record_uid WHERE v.record_uid=? AND d.deleted_at IS NULL AND (? IS NULL OR v.version<?) ORDER BY v.version DESC LIMIT 33").bind(uid).bind(before).bind(before).fetch_all(pool).await
}

pub async fn find(
    pool: &SqlitePool,
    person: &str,
    document: &Document,
) -> Result<Option<Saved>, StoreError> {
    let uid: Option<String> = sqlx::query_scalar("SELECT record_uid FROM transfer_receipt_record WHERE owner_person_uid=? AND kind=? AND identity_hash=?")
        .bind(person).bind(document.kind()).bind(document.identity().map_err(invalid)?.as_str()).fetch_optional(pool).await?;
    match uid {
        Some(uid) => get(pool, &uid).await.map(Some),
        None => Ok(None),
    }
}

pub async fn replay(
    pool: &SqlitePool,
    person: &str,
    request_id: &str,
    command: &Command,
) -> Result<Option<Saved>, StoreError> {
    let row: Option<(String, String, i64)> = sqlx::query_as(
        "SELECT request_json,record_uid,version FROM transfer_receipt_request WHERE request_id=?",
    )
    .bind(request_id)
    .fetch_optional(pool)
    .await?;
    if let Some((previous, uid, version)) = row {
        if previous != request_json(person, command)? {
            return Err(invalid(
                "Receipt request ID was used with different contents",
            ));
        }
        return get_on(&mut *pool.acquire().await?, &uid, Some(version as u64))
            .await
            .map(Some);
    }
    Ok(None)
}

pub async fn local_source(
    pool: &SqlitePool,
    uid: &str,
) -> Result<Option<ScopeRequest>, StoreError> {
    let value: Option<String> = sqlx::query_scalar(
        "SELECT scope_request_json FROM transfer_receipt_local_source WHERE record_uid=?",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await?;
    value
        .map(|value| serde_json::from_str(&value).map_err(invalid))
        .transpose()
}

pub async fn source_for_scope(
    pool: &SqlitePool,
    document: &Document,
) -> Result<Option<ScopeRequest>, StoreError> {
    let value: Option<String> = sqlx::query_scalar("SELECT s.scope_request_json FROM transfer_receipt_local_source s JOIN transfer_receipt_record r ON r.record_uid=s.record_uid WHERE r.kind='review' AND r.identity_hash=? ORDER BY r.rowid LIMIT 1")
        .bind(document.scope().ok_or_else(||invalid("Receipt scope is unavailable"))?.id().map_err(invalid)?.as_str()).fetch_optional(pool).await?;
    value
        .map(|value| serde_json::from_str(&value).map_err(invalid))
        .transpose()
}

pub struct Input {
    pub record: String,
    pub person: String,
    pub request_id: String,
    pub command: Command,
    pub document: Document,
    pub expected: Option<(String, u64)>,
    pub local_source: Option<ScopeRequest>,
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
        return Err(invalid("Choose a receipt request ID of 1–256 characters"));
    }
    input.document.validate(now.timestamp()).map_err(invalid)?;
    let request = request_json(&input.person, &input.command)?;
    let payload = serde_json::to_string(&input.document).map_err(invalid)?;
    if payload.len() > 800_000 {
        return Err(invalid("Receipt document exceeds its storage budget"));
    }
    let identity = input.document.identity().map_err(invalid)?;
    let hash = input.document.hash().map_err(invalid)?;
    if !crate::people::is_active_on(&mut tx, &input.person).await? {
        return Err(invalid("Receipt author must be an active Person"));
    }
    let replay: Option<(String, String, i64)> = sqlx::query_as(
        "SELECT request_json,record_uid,version FROM transfer_receipt_request WHERE request_id=?",
    )
    .bind(&input.request_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((previous, uid, version)) = replay {
        if previous != request {
            return Err(invalid(
                "Receipt request ID was used with different contents",
            ));
        }
        let saved = get_on(&mut tx, &uid, Some(version as u64)).await?;
        return Ok((
            Commit {
                saved,
                created: false,
            },
            tx,
        ));
    }
    let existing: Option<String> = sqlx::query_scalar("SELECT record_uid FROM transfer_receipt_record WHERE owner_person_uid=? AND kind=? AND identity_hash=?")
        .bind(&input.person).bind(input.document.kind()).bind(identity.as_str()).fetch_optional(&mut *tx).await?;
    let (uid, version) = if let Some(uid) = existing {
        let previous = get_on(&mut tx, &uid, None).await?;
        if input.record != uid || input.expected.as_ref() != Some(&(uid.clone(), previous.version))
        {
            return Err(invalid("Receipt changed; review its current version"));
        }
        if previous
            .document
            .merge(&input.document, now.timestamp())
            .map_err(invalid)?
            != input.document
        {
            return Err(invalid(
                "Receipt versions must retain earlier approvals and evidence",
            ));
        }
        if previous.document == input.document {
            crate::receipt_events::index_on(&mut tx, &uid, previous.version, &previous.document)
                .await?;
            sqlx::query("INSERT INTO transfer_receipt_request(request_id,person_uid,request_json,record_uid,version) VALUES(?,?,?,?,?)")
                .bind(&input.request_id).bind(&input.person).bind(request).bind(&uid).bind(previous.version as i64).execute(&mut *tx).await?;
            return Ok((
                Commit {
                    saved: previous,
                    created: false,
                },
                tx,
            ));
        }
        let version = previous
            .version
            .checked_add(1)
            .filter(|value| *value <= i64::MAX as u64)
            .ok_or_else(|| invalid("Receipt version overflow"))?;
        (uid, version)
    } else {
        if input.expected.is_some() {
            return Err(invalid("Receipt identity changed after review"));
        }
        let uid = input.record.clone();
        let organ: String = sqlx::query_scalar(
            "SELECT uid FROM record WHERE kind='organ' AND slug=? AND deleted_at IS NULL",
        )
        .bind(crate::organs::LOCAL_ORGAN_SLUG)
        .fetch_one(&mut *tx)
        .await?;
        crate::records::create_with_uid_on(&mut tx, &uid, crate::records::NewRecord {
            slug: None, kind: RecordKind::Plain,
            head: match &input.document { Document::Authority(authority) if authority.participation.is_some() => "Original participation approval", Document::Authority(_) => "Receipt signing authority", Document::Review(_) => "Receipt publication review", Document::Publication(_) => "Transfer receipt",Document::ProofFragment(_)=>"Receipt proof fragment" },
            body: "Receipt evidence and publication review. Imported evidence does not execute Transfers.",
            quantity: crate::exact::zero(),
        }, &organ, None).await?;
        if let Some(policy) = crate::data_visibility::ensure_private_for_person_on(
            &mut tx,
            &uid,
            nucleus::visibility::Data::Record,
            Some(&input.person),
        )
        .await?
        {
            crate::sync_ops::log_local_tx(
                &mut tx,
                "data_visibility",
                &uid,
                "record",
                crate::sync_ops::OpKind::Set,
                Some(serde_json::to_string(&policy).map_err(invalid)?),
            )
            .await?;
        }
        let scope = input.document.scope();
        sqlx::query("INSERT INTO transfer_receipt_record(record_uid,owner_person_uid,kind,identity_hash,source_origin,source_occurrence,expires_at,created_at) VALUES(?,?,?,?,?,?,?,?)")
            .bind(&uid).bind(&input.person).bind(input.document.kind()).bind(identity.as_str())
            .bind(scope.map(|scope| &scope.source.origin)).bind(scope.map(|scope| scope.source.occurrence.as_str()))
            .bind(input.document.expires_at()).bind(now.to_rfc3339()).execute(&mut *tx).await?;
        (uid, 1)
    };
    if let Some(source) = &input.local_source {
        let scope = input
            .document
            .scope()
            .ok_or_else(|| invalid("Authority certificates have no private occurrence mapping"))?;
        let origin: Option<String> = sqlx::query_scalar(
            "SELECT uid FROM record WHERE kind='organ' AND slug=? AND deleted_at IS NULL",
        )
        .bind(crate::organs::LOCAL_ORGAN_SLUG)
        .fetch_optional(&mut *tx)
        .await?;
        if origin.as_deref() != Some(scope.source.origin.as_str())
            || nucleus::transfer::receipt::Source::occurrence_id(
                &scope.source.origin,
                &source.occurrence,
            )
            .map_err(invalid)?
                != scope.source.occurrence
        {
            return Err(invalid("Private receipt mapping does not match its source"));
        }
        let current: Option<i64> = sqlx::query_scalar("SELECT t.revision FROM transfer_occurrence o JOIN transfer t ON t.record_uid=o.transfer_uid WHERE o.uid=?")
            .bind(&source.occurrence).fetch_optional(&mut *tx).await?;
        if current.and_then(|value| u64::try_from(value).ok()) != Some(source.expected_revision) {
            return Err(invalid("Source Transfer changed after receipt review"));
        }
        let value = serde_json::to_string(source).map_err(invalid)?;
        if value.len() > 32768 {
            return Err(invalid("Private receipt mapping exceeds its budget"));
        }
        sqlx::query("INSERT INTO transfer_receipt_local_source(record_uid,occurrence_uid,scope_request_json) VALUES(?,?,?) ON CONFLICT(record_uid) DO NOTHING")
            .bind(&uid).bind(&source.occurrence).bind(value).execute(&mut *tx).await?;
    }
    let previous_hash = crate::facts::last_hash(&mut tx).await?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: uid.clone(),
            delta: crate::exact::zero(),
            at: None,
            actor_uid: Some(input.person),
            cause: Cause::user_edit(),
            payload: Some(payload.clone()),
        },
        &previous_hash,
        now,
    );
    fact.signature = sign(&fact.hash);
    if fact.signature.is_none() && input.authorization_intent.is_none() {
        return Err(invalid(
            "Receipt history requires its author's signing key or verified Action intent",
        ));
    }
    crate::facts::insert(&mut tx, &fact).await?;
    if let Some(intent) = &input.authorization_intent {
        crate::action_intents::link_pending_fact(&mut tx, intent, &fact).await?;
    }
    sqlx::query("INSERT INTO transfer_receipt_version(record_uid,version,document_hash,document_json,fact_uid,created_at) VALUES(?,?,?,?,?,?)")
        .bind(&uid).bind(version as i64).bind(hash.as_str()).bind(&payload).bind(&fact.uid).bind(now.to_rfc3339()).execute(&mut *tx).await?;
    crate::receipt_events::index_on(&mut tx, &uid, version, &input.document).await?;
    crate::records::set_extension_on(
        &mut tx,
        &uid,
        nucleus::transfer::receipt::NAMESPACE,
        &json!(input.document),
    )
    .await?;
    sqlx::query("INSERT INTO transfer_receipt_request(request_id,person_uid,request_json,record_uid,version) VALUES(?,?,?,?,?)")
        .bind(&input.request_id).bind(&fact.actor_uid).bind(request).bind(&uid).bind(version as i64).execute(&mut *tx).await?;
    let saved = Saved {
        record: uid,
        owner: fact.actor_uid.clone().unwrap_or_default(),
        version,
        document: input.document,
        fact,
    };
    Ok((
        Commit {
            saved,
            created: true,
        },
        tx,
    ))
}

pub async fn list(pool: &SqlitePool, person: &str) -> Result<Value, StoreError> {
    let rows: Vec<(String, String, String, i64, i64)> = sqlx::query_as("SELECT r.record_uid,d.head,r.kind,r.expires_at,MAX(v.version) FROM transfer_receipt_record r JOIN record d ON d.uid=r.record_uid JOIN transfer_receipt_version v ON v.record_uid=r.record_uid WHERE r.owner_person_uid=? AND d.deleted_at IS NULL GROUP BY r.record_uid ORDER BY r.rowid DESC LIMIT 65")
        .bind(person).fetch_all(pool).await?;
    Ok(
        json!({"entries": rows.iter().take(64).map(|(uid, head, kind, expiry, version)| json!({"record":uid,"head":head,"kind":kind,"expires_at":expiry,"version":version})).collect::<Vec<_>>(),"partial":rows.len()>64}),
    )
}
