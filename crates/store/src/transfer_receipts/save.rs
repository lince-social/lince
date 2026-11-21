//! Commit receipt versions, audit Facts, and request IDs in one transaction.

use chrono::{DateTime, Utc};
use nucleus::{
    Cause, NewFact, RecordKind,
    fact::seal,
    karma::CanonicalHash,
    transfer::receipt::{Command, Document, NAMESPACE, ScopeRequest},
    valid_uid,
    visibility::Data,
};
use serde_json::json;
use sqlx::{Sqlite, Transaction};

use super::{
    Saved, get_on, invalid,
    read::find_uid_on,
    request::{lookup, record_request, request_json},
    source,
};
use crate::{
    StoreError, action_intents,
    data_visibility::ensure_private_for_person_on,
    exact::zero,
    facts,
    organs::LOCAL_ORGAN_SLUG,
    people::is_active_on,
    receipt_events,
    records::{self, NewRecord},
    sync_ops::{OpKind, log_local_tx},
};

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
    if !is_active_on(&mut tx, &input.person).await? {
        return Err(invalid("Receipt author must be an active Person"));
    }
    if let Some((previous, uid, version)) = lookup(&mut tx, &input.request_id).await? {
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
    let existing = find_uid_on(&mut tx, &input.person, input.document.kind(), &identity).await?;
    let saved_at = now.to_rfc3339();
    let (uid, version) = if let Some(uid) = existing {
        let previous = get_on(&mut tx, &uid, None).await?;
        if input.record != uid
            || input
                .expected
                .as_ref()
                .is_none_or(|(record, version)| record != &uid || *version != previous.version)
        {
            return Err(invalid("Receipt changed; review its current version"));
        }
        if previous.document == input.document {
            receipt_events::index_on(&mut tx, &uid, previous.version, &previous.document).await?;
            record_request(
                &mut tx,
                &input.request_id,
                &input.person,
                &request,
                &uid,
                previous.version,
            )
            .await?;
            return Ok((
                Commit {
                    saved: previous,
                    created: false,
                },
                tx,
            ));
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
        create_receipt(&mut tx, &input, &identity, &saved_at).await?;
        (input.record.clone(), 1)
    };
    if let Some(source) = &input.local_source {
        source::record_on(&mut tx, &uid, &input.document, source).await?;
    }
    let previous_hash = facts::last_hash(&mut tx).await?;
    let mut fact = seal(
        NewFact {
            uid: None,
            record_uid: uid.clone(),
            delta: zero(),
            at: None,
            actor_uid: Some(input.person.clone()),
            cause: Cause::user_edit(),
            payload: Some(payload),
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
    facts::insert(&mut tx, &fact).await?;
    if let Some(intent) = &input.authorization_intent {
        action_intents::link_pending_fact(&mut tx, intent, &fact).await?;
    }
    sqlx::query(
        "INSERT INTO transfer_receipt_version
         (record_uid,version,document_hash,document_json,fact_uid,created_at) VALUES(?,?,?,?,?,?)",
    )
    .bind(&uid)
    .bind(version as i64)
    .bind(hash.as_str())
    .bind(fact.payload.as_deref())
    .bind(&fact.uid)
    .bind(&saved_at)
    .execute(&mut *tx)
    .await?;
    receipt_events::index_on(&mut tx, &uid, version, &input.document).await?;
    records::set_extension_on(&mut tx, &uid, NAMESPACE, &json!(input.document)).await?;
    record_request(
        &mut tx,
        &input.request_id,
        &input.person,
        &request,
        &uid,
        version,
    )
    .await?;
    let saved = Saved {
        record: uid,
        owner: input.person,
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

async fn create_receipt(
    tx: &mut Transaction<'_, Sqlite>,
    input: &Input,
    identity: &CanonicalHash,
    saved_at: &str,
) -> Result<(), StoreError> {
    let uid = &input.record;
    let organ: String = sqlx::query_scalar(
        "SELECT uid FROM record WHERE kind='organ' AND slug=? AND deleted_at IS NULL",
    )
    .bind(LOCAL_ORGAN_SLUG)
    .fetch_one(&mut **tx)
    .await?;
    let head = match &input.document {
        Document::Authority(authority) if authority.participation.is_some() => {
            "Original participation approval"
        }
        Document::Authority(_) => "Receipt signing authority",
        Document::Review(_) => "Receipt publication review",
        Document::Publication(_) => "Transfer receipt",
        Document::ProofFragment(_) => "Receipt proof fragment",
    };
    records::create_with_uid_on(
        tx, uid, NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head,
            body: "Receipt evidence and publication review. Imported evidence does not execute Transfers.",
            quantity: zero(),
        }, &organ, None,
    ).await?;
    if let Some(policy) =
        ensure_private_for_person_on(tx, uid, Data::Record, Some(&input.person)).await?
    {
        log_local_tx(
            tx,
            "data_visibility",
            uid,
            "record",
            OpKind::Set,
            Some(serde_json::to_string(&policy).map_err(invalid)?),
        )
        .await?;
    }
    let source = input.document.scope().map(|scope| &scope.source);
    sqlx::query(
        "INSERT INTO transfer_receipt_record
         (record_uid,owner_person_uid,kind,identity_hash,source_origin,source_occurrence,expires_at,created_at)
         VALUES(?,?,?,?,?,?,?,?)",
    )
    .bind(uid)
    .bind(&input.person)
    .bind(input.document.kind())
    .bind(identity.as_str())
    .bind(source.map(|source| &source.origin))
    .bind(source.map(|source| source.occurrence.as_str()))
    .bind(input.document.expires_at())
    .bind(saved_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
