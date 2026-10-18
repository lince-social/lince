use crate::StoreError;
use chrono::{DateTime, Utc};
use nucleus::{
    Cause, Fact, NewFact, RecordKind,
    trust_analysis::claims::recipes::{Mode, NAMESPACE, Recipe},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Sqlite, SqliteConnection, Transaction};

fn invalid(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Saved {
    pub record: String,
    pub owner: String,
    pub recipe: Recipe,
    pub fact: Fact,
}

#[derive(Debug, Clone, Serialize)]
pub struct State {
    pub mode: Mode,
    pub revision: u64,
    pub status: String,
    pub checked_at: Option<String>,
    pub next_at: Option<String>,
    pub input_hash: Option<String>,
    pub output: Option<String>,
}

pub async fn get_on(connection: &mut SqliteConnection, record: &str) -> Result<Saved, StoreError> {
    let (owner,uid,revision,hash,payload,fact): (String,String,i64,String,String,String) = sqlx::query_as("SELECT owner_person_uid,recipe_uid,revision,recipe_hash,recipe_json,fact_uid FROM profile_claim_recipe_record WHERE record_uid=?")
        .bind(record).fetch_optional(&mut *connection).await?.ok_or_else(|| invalid("Publication recipe is unavailable"))?;
    let recipe: Recipe = serde_json::from_str(&payload).map_err(invalid)?;
    recipe.validate().map_err(invalid)?;
    let fact = crate::facts::get_on(connection, &fact)
        .await?
        .ok_or_else(|| invalid("The recipe's original review Fact is unavailable"))?;
    if recipe.definition.controller != owner
        || recipe.definition.recipe != uid
        || recipe.definition.revision != revision as u64
        || recipe.definition.id().map_err(invalid)?.as_str() != hash
        || fact.actor_uid.as_deref() != Some(&owner)
        || fact.record_uid != record
        || fact.payload.as_deref() != Some(&payload)
        || !fact.delta.is_zero()
        || !nucleus::fact::verify_chain_step(&fact)
    {
        return Err(invalid(
            "The recipe differs from its immutable authorization and original Fact",
        ));
    }
    Ok(Saved {
        record: record.into(),
        owner,
        recipe,
        fact,
    })
}

pub async fn latest_on(
    connection: &mut SqliteConnection,
    owner: &str,
    recipe: &str,
) -> Result<u64, StoreError> {
    let revision: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(revision),0) FROM profile_claim_recipe_record WHERE owner_person_uid=? AND recipe_uid=?")
        .bind(owner).bind(recipe).fetch_one(connection).await?;
    u64::try_from(revision).map_err(invalid)
}

pub async fn current_on(
    connection: &mut SqliteConnection,
    saved: &Saved,
) -> Result<bool, StoreError> {
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM record WHERE uid=? AND deleted_at IS NULL)",
    )
    .bind(&saved.record)
    .fetch_one(&mut *connection)
    .await?;
    Ok(active
        && latest_on(connection, &saved.owner, &saved.recipe.definition.recipe).await?
            == saved.recipe.definition.revision)
}

fn encoded(owner: &str, request: &Value) -> Result<String, StoreError> {
    let bytes = nucleus::karma::canonical_json_bytes(&(owner, request)).map_err(invalid)?;
    if bytes.len() > 131072 {
        return Err(invalid("Recipe request exceeds 128 KiB"));
    }
    String::from_utf8(bytes).map_err(invalid)
}

pub async fn replay_on(
    connection: &mut SqliteConnection,
    owner: &str,
    id: &str,
    request: &Value,
) -> Result<Option<Saved>, StoreError> {
    let previous: Option<(String,String,String)> = sqlx::query_as("SELECT owner_person_uid,request_json,record_uid FROM profile_claim_recipe_request WHERE request_id=?").bind(id).fetch_optional(&mut *connection).await?;
    if let Some((person, fields, record)) = previous {
        if person != owner || fields != encoded(owner, request)? {
            return Err(invalid(
                "This recipe request ID already binds different fields or a different owner",
            ));
        }
        return get_on(connection, &record).await.map(Some);
    }
    Ok(None)
}

pub async fn state_on(
    connection: &mut SqliteConnection,
    saved: &Saved,
) -> Result<State, StoreError> {
    let (revision, mode, fact): (i64,String,String) = sqlx::query_as("SELECT revision,mode,fact_uid FROM profile_claim_recipe_control WHERE record_uid=? ORDER BY revision DESC LIMIT 1").bind(&saved.record).fetch_one(&mut *connection).await?;
    let mode: Mode = serde_json::from_value(json!(mode)).map_err(invalid)?;
    let control = crate::facts::get_on(connection, &fact)
        .await?
        .ok_or_else(|| invalid("Recipe control proof is unavailable"))?;
    let expected = json!({"recipe":saved.record,"revision":revision,"mode":mode});
    let payload_ok = if revision == 0 {
        control.uid == saved.fact.uid
            && mode
                == if saved.recipe.definition.start_enabled {
                    Mode::Enabled
                } else {
                    Mode::Paused
                }
    } else {
        control
            .payload
            .as_deref()
            .map(serde_json::from_str::<Value>)
            .transpose()
            .map_err(invalid)?
            .as_ref()
            == Some(&expected)
    };
    if !payload_ok
        || control.actor_uid.as_deref() != Some(&saved.owner)
        || control.record_uid != saved.record
        || !control.delta.is_zero()
        || !nucleus::fact::verify_chain_step(&control)
    {
        return Err(invalid(
            "Recipe controls differ from their original signed authorization",
        ));
    }
    let (status,checked_at,next_at,input_hash,output): (String,Option<String>,Option<String>,Option<String>,Option<String>) = sqlx::query_as("SELECT status,checked_at,next_at,input_hash,output_record_uid FROM profile_claim_recipe_runtime WHERE record_uid=?").bind(&saved.record).fetch_one(&mut *connection).await?;
    if let Some(output) = &output {
        let bound:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM profile_claim_record WHERE record_uid=? AND generation_recipe_uid=? AND owner_person_uid=?)").bind(output).bind(&saved.record).bind(&saved.owner).fetch_one(&mut *connection).await?;
        if !bound {
            return Err(invalid(
                "The cached recipe output does not belong to its original recipe and controller",
            ));
        }
    }
    Ok(State {
        mode,
        revision: revision as u64,
        status,
        checked_at,
        next_at,
        input_hash,
        output,
    })
}

pub struct Input {
    pub record: String,
    pub person: String,
    pub request_id: String,
    pub request: Value,
    pub recipe: Recipe,
    pub expected_revision: u64,
    pub authorization_intent: Option<String>,
}
pub struct Commit {
    pub saved: Saved,
    pub created: bool,
}

async fn append<F>(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
    owner: &str,
    payload: String,
    intent: Option<&str>,
    now: DateTime<Utc>,
    sign: F,
) -> Result<Fact, StoreError>
where
    F: FnOnce(&str) -> Option<String>,
{
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: record.into(),
            delta: crate::exact::zero(),
            at: None,
            actor_uid: Some(owner.into()),
            cause: Cause::user_edit(),
            payload: Some(payload),
        },
        &crate::facts::last_hash(tx).await?,
        now,
    );
    fact.signature = sign(&fact.hash);
    if fact.signature.is_none() && intent.is_none() {
        return Err(invalid(
            "Authorize this recipe change with the controller's signature or verified Action intent",
        ));
    }
    crate::facts::insert(tx, &fact).await?;
    if let Some(intent) = intent {
        crate::action_intents::link_pending_fact(tx, intent, &fact).await?;
    }
    Ok(fact)
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
    input.recipe.validate().map_err(invalid)?;
    let definition = &input.recipe.definition;
    if definition.controller != input.person
        || !nucleus::valid_uid(&input.record, "r")
        || input.request_id.is_empty()
        || input.request_id.len() > 256
        || input.request_id.chars().any(char::is_control)
    {
        return Err(invalid(
            "Choose the actual controller, a canonical Record and a bounded request ID",
        ));
    }
    if let Some(saved) =
        replay_on(&mut tx, &input.person, &input.request_id, &input.request).await?
    {
        if saved.recipe != input.recipe {
            return Err(invalid("A replay cannot change its recipe permit"));
        }
        return Ok((
            Commit {
                saved,
                created: false,
            },
            tx,
        ));
    }
    if !crate::people::is_active_on(&mut tx, &input.person).await?
        || latest_on(&mut tx, &input.person, &definition.recipe).await? != input.expected_revision
        || input.expected_revision.checked_add(1) != Some(definition.revision)
        || definition.terms.issued_at > now.timestamp()
        || definition.terms.expires_at <= now.timestamp()
    {
        return Err(invalid(
            "The controller, current recipe revision or reviewed term changed",
        ));
    }
    let organ: String = sqlx::query_scalar(
        "SELECT uid FROM record WHERE slug=? AND kind='organ' AND deleted_at IS NULL",
    )
    .bind(crate::organs::LOCAL_ORGAN_SLUG)
    .fetch_one(&mut *tx)
    .await?;
    crate::records::create_with_uid_on(&mut tx,&input.record,crate::records::NewRecord {slug:None,kind:RecordKind::Plain,head:&definition.terms.title,body:"A private reviewed recipe for a bounded signed publication. Its private input policy is not exported.",quantity:crate::exact::zero()},&organ,None).await?;
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
    let payload = serde_json::to_string(&input.recipe).map_err(invalid)?;
    let fact = append(
        &mut tx,
        &input.record,
        &input.person,
        payload.clone(),
        input.authorization_intent.as_deref(),
        now,
        sign,
    )
    .await?;
    sqlx::query("INSERT INTO profile_claim_recipe_record(record_uid,owner_person_uid,recipe_uid,revision,recipe_hash,recipe_json,fact_uid,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(&input.record).bind(&input.person).bind(&definition.recipe).bind(definition.revision as i64).bind(definition.id().map_err(invalid)?.as_str()).bind(&payload).bind(&fact.uid).bind(now.to_rfc3339()).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO profile_claim_recipe_request(request_id,owner_person_uid,request_json,record_uid) VALUES(?,?,?,?)").bind(&input.request_id).bind(&input.person).bind(encoded(&input.person,&input.request)?).bind(&input.record).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO profile_claim_recipe_control(record_uid,revision,mode,fact_uid,request_id) VALUES(?,0,?,?,?)").bind(&input.record).bind(if definition.start_enabled {"enabled"} else {"paused"}).bind(&fact.uid).bind(&input.request_id).execute(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO profile_claim_recipe_runtime(record_uid,status,next_at) VALUES(?,'pending',?)",
    )
    .bind(&input.record)
    .bind(now.to_rfc3339())
    .execute(&mut *tx)
    .await?;
    crate::records::set_extension_on(
        &mut tx,
        &input.record,
        NAMESPACE,
        &serde_json::to_value(&input.recipe).map_err(invalid)?,
    )
    .await?;
    Ok((
        Commit {
            saved: Saved {
                record: input.record,
                owner: input.person,
                recipe: input.recipe,
                fact,
            },
            created: true,
        },
        tx,
    ))
}

pub struct Control<'a> {
    pub id: &'a str,
    pub expected: u64,
    pub mode: Mode,
    pub request: &'a Value,
    pub intent: Option<&'a str>,
}

pub async fn control<F>(
    tx: &mut Transaction<'_, Sqlite>,
    saved: &Saved,
    change: Control<'_>,
    now: DateTime<Utc>,
    sign: F,
) -> Result<Option<Fact>, StoreError>
where
    F: FnOnce(&str) -> Option<String>,
{
    let Control {
        id,
        expected,
        mode,
        request,
        intent,
    } = change;
    if replay_on(tx, &saved.owner, id, request).await?.is_some() {
        return Ok(None);
    }
    let current = state_on(tx, saved).await?;
    if !current_on(tx, saved).await?
        || current.revision != expected
        || current.mode == Mode::Revoked
        || expected >= i64::MAX as u64
    {
        return Err(invalid(
            "This recipe control changed, was superseded or has been revoked",
        ));
    }
    let revision = expected + 1;
    let fact = append(
        tx,
        &saved.record,
        &saved.owner,
        serde_json::to_string(&json!({"recipe":saved.record,"revision":revision,"mode":mode}))
            .map_err(invalid)?,
        intent,
        now,
        sign,
    )
    .await?;
    sqlx::query("INSERT INTO profile_claim_recipe_control(record_uid,revision,mode,fact_uid,request_id) VALUES(?,?,?,?,?)").bind(&saved.record).bind(revision as i64).bind(mode.as_str()).bind(&fact.uid).bind(id).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO profile_claim_recipe_request(request_id,owner_person_uid,request_json,record_uid) VALUES(?,?,?,?)").bind(id).bind(&saved.owner).bind(encoded(&saved.owner,request)?).bind(&saved.record).execute(&mut **tx).await?;
    sqlx::query("UPDATE profile_claim_recipe_runtime SET status=?,next_at=? WHERE record_uid=?")
        .bind(if mode == Mode::Revoked {
            "revocation_pending"
        } else {
            mode.as_str()
        })
        .bind(now.to_rfc3339())
        .bind(&saved.record)
        .execute(&mut **tx)
        .await?;
    Ok(Some(fact))
}

pub async fn status_on(
    connection: &mut SqliteConnection,
    record: &str,
    status: &str,
    now: DateTime<Utc>,
    next: DateTime<Utc>,
    input: Option<&str>,
    output: Option<&str>,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE profile_claim_recipe_runtime SET status=?,checked_at=?,next_at=?,input_hash=COALESCE(?,input_hash),output_record_uid=COALESCE(?,output_record_uid) WHERE record_uid=?").bind(status).bind(now.to_rfc3339()).bind(next.to_rfc3339()).bind(input).bind(output).bind(record).execute(connection).await?;
    Ok(())
}
