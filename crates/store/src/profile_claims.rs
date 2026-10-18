use crate::StoreError;
use chrono::{DateTime, Utc};
use nucleus::{
    Cause, Fact, NewFact, RecordKind,
    trust_analysis::claims::{Document, NAMESPACE, Statement},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Sqlite, SqliteConnection, Transaction};

fn invalid(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Saved {
    pub record: String,
    pub owner: String,
    pub document: Document,
    pub fact: Fact,
}

pub async fn get_on(connection: &mut SqliteConnection, record: &str) -> Result<Saved, StoreError> {
    let (owner,series,statement_hash,document_hash,issuer_kind,issuer,claim,subject_kind,subject,revision,payload,fact,generation):(String,String,String,String,String,String,String,String,String,i64,String,String,Option<String>)=sqlx::query_as("SELECT owner_person_uid,series_hash,statement_hash,document_hash,issuer_kind,issuer_uid,claim_uid,subject_kind,subject_uid,revision,document_json,fact_uid,generation_recipe_uid FROM profile_claim_record WHERE record_uid=?")
        .bind(record).fetch_optional(&mut *connection).await?.ok_or_else(||invalid("Profile claim is unavailable"))?;
    let document = Document::parse(payload.as_bytes()).map_err(invalid)?;
    let fact = crate::facts::get_on(connection, &fact)
        .await?
        .ok_or_else(|| invalid("Original profile-claim audit Fact is unavailable"))?;
    let body = &document.statement;
    let audit_actor = if let Some(recipe) = generation {
        generation_actor_on(connection, &recipe, &owner, &document, false).await?
    } else {
        owner.clone()
    };
    if body.series().map_err(invalid)?.as_str() != series
        || body.id().map_err(invalid)?.as_str() != statement_hash
        || document.id().map_err(invalid)?.as_str() != document_hash
        || body.issuer.kind() != issuer_kind
        || body.issuer.uid() != issuer
        || body.claim != claim
        || body.descriptor.subject.kind() != subject_kind
        || body.descriptor.subject.uid() != subject
        || body.revision != revision as u64
        || fact.record_uid != record
        || fact.actor_uid.as_deref() != Some(&audit_actor)
        || fact.payload.as_deref() != Some(&payload)
        || !fact.delta.is_zero()
        || !nucleus::fact::verify_chain_step(&fact)
    {
        return Err(invalid(
            "Profile claim differs from its immutable source and audit Fact",
        ));
    }
    Ok(Saved {
        record: record.into(),
        owner,
        document,
        fact,
    })
}

fn request_json(
    person: &str,
    request: &Value,
    expected: Option<u64>,
) -> Result<String, StoreError> {
    let bytes =
        nucleus::karma::canonical_json_bytes(&(person, request, expected)).map_err(invalid)?;
    if bytes.len() > 65536 {
        return Err(invalid("Profile-claim request exceeds 64 KiB"));
    }
    String::from_utf8(bytes).map_err(invalid)
}

pub async fn replay_on(
    connection: &mut SqliteConnection,
    person: &str,
    id: &str,
    request: &Value,
    expected: Option<u64>,
) -> Result<Option<Saved>, StoreError> {
    let previous:Option<(String,String,String)>=sqlx::query_as("SELECT owner_person_uid,request_json,record_uid FROM profile_claim_request WHERE request_id=?").bind(id).fetch_optional(&mut *connection).await?;
    if let Some((owner, previous, record)) = previous {
        if owner != person || previous != request_json(person, request, expected)? {
            return Err(invalid(
                "Profile-claim request ID was used with different fields or owner",
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
        "SELECT record_uid FROM profile_claim_record WHERE owner_person_uid=? AND document_hash=?",
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

pub async fn latest_on(
    connection: &mut SqliteConnection,
    person: &str,
    statement: &Statement,
) -> Result<u64, StoreError> {
    let revision:i64=sqlx::query_scalar("SELECT COALESCE(MAX(revision),0) FROM profile_claim_record WHERE owner_person_uid=? AND series_hash=?")
        .bind(person).bind(statement.series().map_err(invalid)?.as_str()).fetch_one(connection).await?;
    u64::try_from(revision).map_err(invalid)
}

pub async fn current_on(
    connection: &mut SqliteConnection,
    person: &str,
    statement: &Statement,
) -> Result<bool, StoreError> {
    if latest_on(connection, person, statement).await? != statement.revision {
        return Ok(false);
    }
    let meanings:i64=sqlx::query_scalar("SELECT COUNT(DISTINCT statement_hash) FROM profile_claim_record WHERE owner_person_uid=? AND series_hash=? AND revision=?")
        .bind(person).bind(statement.series().map_err(invalid)?.as_str()).bind(statement.revision as i64).fetch_one(connection).await?;
    Ok(meanings == 1)
}

pub struct Input {
    pub record: String,
    pub person: String,
    pub request_id: String,
    pub request: Value,
    pub document: Document,
    pub expected_revision: Option<u64>,
    pub authorization_intent: Option<String>,
    pub generation_recipe: Option<String>,
}

pub struct Commit {
    pub saved: Saved,
    pub created: bool,
}

async fn generation_actor_on(
    connection: &mut SqliteConnection,
    recipe: &str,
    person: &str,
    document: &Document,
    current: bool,
) -> Result<String, StoreError> {
    use nucleus::trust_analysis::claims::{
        Authorship,
        recipes::{EvaluationState, Mode},
    };
    let saved = crate::claim_recipes::get_on(connection, recipe).await?;
    let Authorship::Generated { permit, .. } = &document.statement.authorship else {
        return Err(invalid(
            "Only a generated output can use a recipe's publishing authority",
        ));
    };
    if saved.owner != person || permit.as_ref() != &saved.recipe.permit {
        return Err(invalid(
            "The output differs from its original reviewed recipe",
        ));
    }
    if current {
        let state = crate::claim_recipes::state_on(connection, &saved).await?;
        let revoking = state.mode == Mode::Revoked
            && matches!(
                &document.statement.authorship,
                Authorship::Generated {
                    state: EvaluationState::Revoked,
                    ..
                }
            );
        if !crate::claim_recipes::current_on(connection, &saved).await?
            || (state.mode != Mode::Enabled && !revoking)
        {
            return Err(invalid(
                "This recipe no longer authorizes automatic publication",
            ));
        }
    }
    Ok(saved.recipe.definition.terms.delegate.organ)
}

pub async fn save<F>(
    mut tx: Transaction<'static, Sqlite>,
    input: Input,
    now: DateTime<Utc>,
    sign: F,
) -> Result<(Commit, Transaction<'static, Sqlite>), StoreError>
where
    F: FnOnce(&str) -> Option<String>,
{
    input.document.validate().map_err(invalid)?;
    if !nucleus::valid_uid(&input.record, "r")
        || input.request_id.is_empty()
        || input.request_id.len() > 256
        || input.request_id.chars().any(char::is_control)
    {
        return Err(invalid(
            "Choose a canonical Record and a request ID of 1–256 characters",
        ));
    }
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
            return Err(invalid("A replay cannot replace its original signed claim"));
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
        return Err(invalid("Retain claims for an active Person"));
    }
    if let Some(saved) = find_on(&mut tx, &input.person, &input.document).await? {
        sqlx::query("INSERT INTO profile_claim_request(request_id,owner_person_uid,request_json,record_uid) VALUES(?,?,?,?)").bind(&input.request_id).bind(&input.person).bind(request).bind(&saved.record).execute(&mut *tx).await?;
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
        if latest_on(&mut tx, &input.person, statement).await? != expected
            || expected.checked_add(1) != Some(statement.revision)
        {
            return Err(invalid("This claim changed; review its current revision"));
        }
        if statement.issued_at > now.timestamp() || statement.expires_at <= now.timestamp() {
            return Err(invalid("Review a currently effective claim term"));
        }
    }
    let organ: String = sqlx::query_scalar(
        "SELECT uid FROM record WHERE kind='organ' AND slug=? AND deleted_at IS NULL",
    )
    .bind(crate::organs::LOCAL_ORGAN_SLUG)
    .fetch_one(&mut *tx)
    .await?;
    crate::records::create_with_uid_on(&mut tx,&input.record,crate::records::NewRecord {slug:None,kind:RecordKind::Plain,head:&statement.title,body:"An attributed claim. A signature identifies its issuer; supporting evidence is checked independently.",quantity:crate::exact::zero()},&organ,None).await?;
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
    let payload = serde_json::to_string(&input.document).map_err(invalid)?;
    let audit_actor = if let Some(recipe) = &input.generation_recipe {
        generation_actor_on(&mut tx, recipe, &input.person, &input.document, true).await?
    } else {
        input.person.clone()
    };
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: input.record.clone(),
            delta: crate::exact::zero(),
            at: None,
            actor_uid: Some(audit_actor),
            cause: Cause::user_edit(),
            payload: Some(payload.clone()),
        },
        &crate::facts::last_hash(&mut tx).await?,
        now,
    );
    fact.signature = sign(&fact.hash);
    if input.generation_recipe.is_some() && fact.signature.is_none() {
        return Err(invalid(
            "Generated outputs require their publishing Cell's own signed audit Fact",
        ));
    }
    if fact.signature.is_none() && input.authorization_intent.is_none() {
        return Err(invalid(
            "Retain a claim under its holder's signature or verified Action intent",
        ));
    }
    crate::facts::insert(&mut tx, &fact).await?;
    if let Some(intent) = &input.authorization_intent {
        crate::action_intents::link_pending_fact(&mut tx, intent, &fact).await?;
    }
    sqlx::query("INSERT INTO profile_claim_record(record_uid,owner_person_uid,series_hash,statement_hash,document_hash,issuer_kind,issuer_uid,claim_uid,subject_kind,subject_uid,revision,document_json,fact_uid,created_at,generation_recipe_uid) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&input.record).bind(&input.person).bind(statement.series().map_err(invalid)?.as_str()).bind(statement.id().map_err(invalid)?.as_str()).bind(input.document.id().map_err(invalid)?.as_str())
        .bind(statement.issuer.kind()).bind(statement.issuer.uid()).bind(&statement.claim).bind(statement.descriptor.subject.kind()).bind(statement.descriptor.subject.uid()).bind(statement.revision as i64).bind(&payload).bind(&fact.uid).bind(now.to_rfc3339()).bind(&input.generation_recipe).execute(&mut *tx).await?;
    crate::records::set_extension_on(
        &mut tx,
        &input.record,
        NAMESPACE,
        &serde_json::to_value(&input.document).map_err(invalid)?,
    )
    .await?;
    sqlx::query("INSERT INTO profile_claim_request(request_id,owner_person_uid,request_json,record_uid) VALUES(?,?,?,?)").bind(&input.request_id).bind(&input.person).bind(request).bind(&input.record).execute(&mut *tx).await?;
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
