use crate::StoreError;
use chrono::{DateTime, Utc};
use nucleus::{
    Cause, Fact, NewFact, RecordKind,
    transfer::{
        outcome::{Content, Request},
        receipt_source::{self, RecordedClock, Snapshot, SourceRequest},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{SqliteConnection, SqlitePool};

fn invalid(message: impl ToString) -> StoreError {
    StoreError::Protocol(message.to_string())
}

fn encoded<T: Serialize>(value: &T, limit: usize) -> Result<String, StoreError> {
    let value = serde_json::to_string(value).map_err(invalid)?;
    if value.len() > limit {
        return Err(invalid("Source snapshot exceeds its byte budget"));
    }
    Ok(value)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Saved {
    pub snapshot: Snapshot,
    pub fact: Fact,
}

pub async fn get_on(connection: &mut SqliteConnection, uid: &str) -> Result<Saved, StoreError> {
    let (payload, fact): (String,String) = sqlx::query_as("SELECT s.payload_json,s.fact_uid FROM transfer_receipt_source_record s JOIN record r ON r.uid=s.record_uid WHERE s.record_uid=? AND r.deleted_at IS NULL").bind(uid).fetch_optional(&mut *connection).await?.ok_or_else(|| invalid("Source snapshot is unavailable"))?;
    if payload.len() > nucleus::transfer::outcome::MAX_BYTES {
        return Err(invalid("Source snapshot exceeds its byte budget"));
    }
    let snapshot: Snapshot = serde_json::from_str(&payload).map_err(invalid)?;
    let fact = crate::facts::get_on(connection, &fact)
        .await?
        .ok_or_else(|| invalid("Source snapshot Fact is unavailable"))?;
    if snapshot.domain != receipt_source::DOMAIN
        || snapshot.record != uid
        || fact.record_uid != uid
        || fact.actor_uid.as_deref() != Some(&snapshot.author)
        || fact.payload.as_deref() != Some(&payload)
        || !fact.delta.is_zero()
        || !nucleus::fact::verify_chain_step(&fact)
    {
        return Err(invalid("Source snapshot differs from its original Fact"));
    }
    Ok(Saved { snapshot, fact })
}

pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Saved, StoreError> {
    get_on(&mut *pool.acquire().await?, uid).await
}

pub async fn replay(
    pool: &SqlitePool,
    person: &str,
    id: &str,
    request: &SourceRequest,
) -> Result<Option<Saved>, StoreError> {
    let previous: Option<(String, String)> = sqlx::query_as(
        "SELECT record_uid,request_json FROM transfer_receipt_source_record WHERE request_id=?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    if let Some((uid, payload)) = previous {
        if payload != encoded(&(person, request), 65536)? {
            return Err(invalid(
                "Source request ID was used with different contents",
            ));
        }
        return get(pool, &uid).await.map(Some);
    }
    Ok(None)
}

pub struct Input {
    pub record: String,
    pub person: String,
    pub request_id: String,
    pub request: SourceRequest,
    pub source: nucleus::transfer::receipt::Source,
    pub measurement: nucleus::transfer::receipt::Measurement,
    pub clock: RecordedClock,
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
    if input.request_id.is_empty()
        || input.request_id.len() > 256
        || input.request_id.chars().any(char::is_control)
    {
        return Err(invalid("Choose a source request ID of 1–256 characters"));
    }
    let previous: Option<(String, String)> = sqlx::query_as(
        "SELECT record_uid,request_json FROM transfer_receipt_source_record WHERE request_id=?",
    )
    .bind(&input.request_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((uid, payload)) = previous {
        if payload != encoded(&(&input.person, &input.request), 65536)? {
            return Err(invalid(
                "Source request ID was used with different contents",
            ));
        }
        let saved = get_on(&mut tx, &uid).await?;
        return Ok((
            Commit {
                saved,
                created: false,
            },
            tx,
        ));
    }
    if !crate::people::is_active_on(&mut tx, &input.person).await? {
        return Err(invalid("Source author must be an active Person"));
    }
    let receipt = crate::transfer_receipts::get_on(&mut tx, &input.request.receipt, None).await?;
    if receipt.version != input.request.expected_version
        || receipt.owner != input.person
        || receipt.document.scope().map(|scope| &scope.source) != Some(&input.source)
    {
        return Err(invalid("Receipt changed after source review"));
    }
    let content = match &input.request.outcome {
        Request::BindNeed {
            selection,
            expected_measure,
            supersedes,
        } => {
            if input.request.public_need.is_some() {
                return Err(invalid(
                    "A Need snapshot cannot refer to another public Need",
                ));
            }
            let terms =
                crate::transfer_evidence::freeze_need_on(&mut tx, &input.person, selection).await?;
            if terms.measure != *expected_measure {
                return Err(invalid("Need meaning or unit changed after review"));
            }
            Content::NeedBinding {
                terms,
                supersedes: supersedes.clone(),
            }
        }
        Request::State {
            role,
            measure,
            amount,
            withdrawn,
            need_binding,
            supersedes,
        } => {
            if input.request.public_need.is_some() {
                measure.validate().map_err(invalid)?;
            } else {
                crate::transfer_evidence::check_measure_on(&mut tx, measure).await?;
            }
            if input.request.public_need.is_some() && need_binding.is_some() {
                return Err(invalid("Choose one exact private or public Need binding"));
            }
            if let Some(binding) = need_binding {
                let binding = get_on(&mut tx, binding).await?;
                if binding.snapshot.source != input.source
                    || !matches!(&binding.snapshot.content,Content::NeedBinding { terms,.. } if terms.measure==*measure)
                {
                    return Err(invalid(
                        "Statement must retain its source and exact Need measurement",
                    ));
                }
            }
            Content::Statement {
                role: *role,
                measure: measure.clone(),
                quantity: Request::amount(amount, *withdrawn).map_err(invalid)?,
                withdrawn: *withdrawn,
                need_binding: need_binding.clone(),
                supersedes: supersedes.clone(),
            }
        }
    };
    let key = receipt_source::source_key(&input.source).map_err(invalid)?;
    if content.supersedes().is_none() {
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer_receipt_source_record WHERE source_key=? AND author_person_uid=? AND kind=? AND measurement_key=? AND supersedes_uid IS NULL)").bind(key.as_str()).bind(&input.person).bind(content.kind()).bind(input.measurement.id().map_err(invalid)?.as_str()).fetch_one(&mut *tx).await?;
        if exists {
            return Err(invalid(
                "Correct an existing source snapshot instead of adding the same outcome again",
            ));
        }
    }
    if let Some(previous) = content.supersedes() {
        let previous = get_on(&mut tx, previous).await?;
        if previous.snapshot.source != input.source
            || previous.snapshot.author != input.person
            || previous.snapshot.content.kind() != content.kind()
            || previous.snapshot.content.measure() != content.measure()
            || previous.snapshot.measurement != input.measurement
        {
            return Err(invalid(
                "A source correction must retain its original source, author, role and measurement",
            ));
        }
    }
    let snapshot = Snapshot {
        domain: receipt_source::DOMAIN.into(),
        record: input.record.clone(),
        source: input.source,
        author: input.person.clone(),
        clock: input.clock,
        private_transfer: input.request.private_transfer.clone(),
        request_id: input.request_id.clone(),
        content,
        measurement: input.measurement,
        public_need: input.request.public_need.clone(),
    };
    let payload = encoded(&snapshot, 32768)?;
    crate::records::create_with_uid_on(&mut tx,&input.record,crate::records::NewRecord { slug:None,kind:RecordKind::Plain,head:if snapshot.content.kind()=="need_binding" { "Frozen receipt Need" } else { "Receipt outcome source" },body:"Private signed source snapshot. Sharing its minimal projection requires separate reviewed consent.",quantity:crate::exact::zero() },&snapshot.clock.organ,None).await?;
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
            "Source snapshot needs its author's signing key or verified Action intent",
        ));
    }
    crate::facts::insert(&mut tx, &fact).await?;
    if let Some(intent) = input.authorization_intent {
        crate::action_intents::link_pending_fact(&mut tx, &intent, &fact).await?;
    }
    let binding = if let Content::Statement { need_binding, .. } = &snapshot.content {
        need_binding.as_deref()
    } else {
        None
    };
    sqlx::query("INSERT INTO transfer_receipt_source_record(record_uid,source_key,author_person_uid,kind,measurement_key,supersedes_uid,need_binding_uid,fact_uid,request_id,request_json,payload_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&input.record).bind(key.as_str()).bind(&input.person).bind(snapshot.content.kind()).bind(snapshot.measurement.id().map_err(invalid)?.as_str()).bind(snapshot.content.supersedes()).bind(binding).bind(&fact.uid).bind(&input.request_id).bind(encoded(&(&input.person,&input.request),65536)?).bind(&payload).bind(now.to_rfc3339()).execute(&mut *tx).await?;
    crate::records::set_extension_on(
        &mut tx,
        &input.record,
        receipt_source::NAMESPACE,
        &json!(snapshot),
    )
    .await?;
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
    Ok((
        Commit {
            saved: Saved { snapshot, fact },
            created: true,
        },
        tx,
    ))
}

pub async fn list(
    pool: &SqlitePool,
    person: &str,
    source: &nucleus::transfer::receipt::Source,
) -> Result<Value, StoreError> {
    let key = receipt_source::source_key(source).map_err(invalid)?;
    let rows:Vec<String>=sqlx::query_scalar("SELECT s.record_uid FROM transfer_receipt_source_record s JOIN record r ON r.uid=s.record_uid WHERE s.author_person_uid=? AND s.source_key=? AND r.deleted_at IS NULL ORDER BY s.created_at DESC,s.rowid DESC LIMIT 65").bind(person).bind(key.as_str()).fetch_all(pool).await?;
    Ok(json!({"partial":rows.len()>64,"records":rows.into_iter().take(64).collect::<Vec<_>>()}))
}
