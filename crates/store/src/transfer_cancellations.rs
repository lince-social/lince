use chrono::{DateTime, Utc};
use nucleus::{DecimalValue, Fact};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Row, Sqlite, SqliteConnection, SqlitePool, Transaction};

use crate::{StoreError, exact, transfer_accounting as accounting};

fn invalid(value: impl ToString) -> StoreError {
    StoreError::Protocol(value.to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proposal {
    pub transfer: String,
    pub occurrence: String,
    pub expected_revision: u64,
    pub quantity: DecimalValue,
    pub person: String,
    pub request_id: String,
    #[serde(skip)]
    pub authorization_intent_uid: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Application {
    pub transfer: String,
    pub cancellation: String,
    pub expected_revision: u64,
    pub person: String,
    pub request_id: String,
    #[serde(skip)]
    pub authorization_intent_uid: Option<String>,
}

pub struct Outcome {
    pub uid: String,
    pub fact: Option<Fact>,
}

async fn participant(
    tx: &mut Transaction<'_, Sqlite>,
    transfer: &str,
    person: &str,
    revision: u64,
) -> Result<(), StoreError> {
    let allowed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer t JOIN record r ON r.uid = t.record_uid JOIN record o ON o.slug = 'local-organ' AND o.uid = r.organ_uid JOIN transfer_party p ON p.transfer_uid = t.record_uid WHERE t.record_uid = ? AND p.actor_uid = ? AND t.revision = ?)")
        .bind(transfer).bind(person).bind(revision as i64).fetch_one(&mut **tx).await?;
    if !allowed {
        return Err(invalid(
            "cancellation requires an accepted participant and the current revision on the Transfer's origin",
        ));
    }
    Ok(())
}

async fn verify_remaining(
    tx: &mut Transaction<'_, Sqlite>,
    occurrences: &[String],
    expected: DecimalValue,
) -> Result<(), StoreError> {
    for occurrence in occurrences {
        let row = sqlx::query("SELECT o.quantity, o.disputed, o.system_disputed, p.state FROM transfer_occurrence o JOIN promise p ON p.uid = o.promise_uid WHERE o.uid = ?").bind(occurrence).fetch_one(&mut **tx).await?;
        if row.get::<bool, _>("disputed")
            || row.get::<bool, _>("system_disputed")
            || row.get::<String, _>("state") != "active"
        {
            return Err(invalid(
                "resolve the occurrence's hold before cancelling its remainder",
            ));
        }
        let settled = accounting::allocated_on(tx, occurrence, false).await?;
        let allocated = accounting::allocated_on(tx, occurrence, true).await?;
        if settled.exact_numeric_cmp(allocated).is_ne() {
            return Err(invalid(
                "finish pending settlement applications before cancelling the remainder",
            ));
        }
        let total =
            effective_total_on(tx, occurrence, accounting::amount(row.get("quantity"))?).await?;
        if exact::difference(total, settled)?
            .exact_numeric_cmp(expected)
            .is_ne()
        {
            return Err(invalid(
                "the remaining quantity changed; propose cancellation again",
            ));
        }
    }
    Ok(())
}

pub async fn propose<F>(
    pool: &SqlitePool,
    input: Proposal,
    now: DateTime<Utc>,
    sign: F,
) -> Result<Outcome, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    if !input.quantity.is_positive()
        || input.request_id.trim().is_empty()
        || input.request_id.len() > 200
        || input.expected_revision >= i64::MAX as u64
    {
        return Err(invalid(
            "cancellation needs a positive remaining quantity and request identifier",
        ));
    }
    let payload = serde_json::to_string(&input).map_err(invalid)?;
    let mut tx = crate::write_tx(pool).await?;
    if let Some((uid, previous)) = sqlx::query_as::<_, (String, String)>(
        "SELECT uid,payload FROM transfer_cancellation WHERE request_id = ?",
    )
    .bind(&input.request_id)
    .fetch_optional(&mut *tx)
    .await?
    {
        if previous != payload {
            return Err(invalid("cancellation request reused with different terms"));
        }
        tx.rollback().await?;
        return Ok(Outcome { uid, fact: None });
    }
    let reused: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer_revision WHERE idempotency_key = ?1) OR EXISTS(SELECT 1 FROM transfer_invitation_event WHERE idempotency_key = ?1) OR EXISTS(SELECT 1 FROM transfer_agreement_event WHERE idempotency_key = ?1) OR EXISTS(SELECT 1 FROM transfer_open_claim_pair WHERE idempotency_key = ?1) OR EXISTS(SELECT 1 FROM transfer_phase4_request WHERE idempotency_key = ?1) OR EXISTS(SELECT 1 FROM transfer_phase5_request WHERE idempotency_key = ?1) OR EXISTS(SELECT 1 FROM transfer_phase5_correction_request WHERE idempotency_key = ?1) OR EXISTS(SELECT 1 FROM transfer_phase6_bulk_request WHERE idempotency_key = ?1) OR EXISTS(SELECT 1 FROM transfer_correction_link WHERE idempotency_key = ?1) OR EXISTS(SELECT 1 FROM transfer_promise_successor WHERE idempotency_key = ?1) OR EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = ?1)")
        .bind(&input.request_id).fetch_one(&mut *tx).await?;
    if reused {
        return Err(invalid(
            "transfer request id was already used by another action",
        ));
    }
    participant(
        &mut tx,
        &input.transfer,
        &input.person,
        input.expected_revision,
    )
    .await?;
    let path: String = sqlx::query_scalar(
        "SELECT exchange_path_uid FROM transfer_occurrence WHERE uid = ? AND transfer_uid = ?",
    )
    .bind(&input.occurrence)
    .bind(&input.transfer)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| invalid("occurrence does not belong to this Transfer"))?;
    let occurrences: Vec<String> = sqlx::query_scalar("SELECT uid FROM transfer_occurrence WHERE exchange_path_uid = ? AND transfer_uid = ? ORDER BY uid").bind(&path).bind(&input.transfer).fetch_all(&mut *tx).await?;
    verify_remaining(&mut tx, &occurrences, input.quantity).await?;
    let people: Vec<String> = sqlx::query_scalar(
        "SELECT actor_uid FROM transfer_party WHERE transfer_uid = ? ORDER BY actor_uid",
    )
    .bind(&input.transfer)
    .fetch_all(&mut *tx)
    .await?;
    if people.is_empty() {
        return Err(invalid("cancellation needs its affected participants"));
    }
    let revision = crate::transfers::advance_transfer_revision(
        &mut tx,
        &input.transfer,
        input.expected_revision,
    )
    .await?
    .map_err(|_| invalid("Transfer revision changed"))?;
    crate::transfers::reset_agreements_for_revision(&mut tx, &input.transfer, revision, now)
        .await?;
    let uid = nucleus::new_uid("tc");
    let (mantissa, scale) = exact::decimal_columns(input.quantity);
    sqlx::query("INSERT INTO transfer_cancellation (uid,transfer_uid,revision,exchange_path_uid,quantity_mantissa,quantity_scale,occurrences,required_people,proposer_uid,request_id,payload,created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&uid).bind(&input.transfer).bind(revision).bind(path).bind(mantissa).bind(scale).bind(serde_json::to_string(&occurrences).map_err(invalid)?).bind(serde_json::to_string(&people).map_err(invalid)?).bind(&input.person).bind(&input.request_id).bind(payload).bind(now.to_rfc3339()).execute(&mut *tx).await?;
    let fact = crate::transfers::insert_revision_fact(
        &mut tx,
        &input.transfer,
        revision,
        input.expected_revision,
        &input.request_id,
        "propose-transfer-cancellation",
        Some(input.person),
        now,
        &sign,
    )
    .await?;
    if let Some(intent) = &input.authorization_intent_uid {
        crate::action_intents::link_pending_fact(&mut tx, intent, &fact).await?;
    } else if fact.signature.is_none() {
        return Err(invalid("cancellation requires the participant's signature"));
    }
    sqlx::query("UPDATE transfer_cancellation SET proposal_fact_uid = ? WHERE uid = ?")
        .bind(&fact.uid)
        .bind(&uid)
        .execute(&mut *tx)
        .await?;
    crate::records::bump_quantity(&mut tx, &input.transfer, exact::zero(), &now.to_rfc3339())
        .await?;
    crate::transfers::cancel_stale_item_delivery(&mut tx, &input.transfer, revision).await?;
    tx.commit().await?;
    Ok(Outcome {
        uid,
        fact: Some(fact),
    })
}

pub async fn apply<F>(
    pool: &SqlitePool,
    input: Application,
    now: DateTime<Utc>,
    sign: F,
) -> Result<Outcome, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    if input.request_id.trim().is_empty()
        || input.request_id.len() > 200
        || input.expected_revision > i64::MAX as u64
    {
        return Err(invalid(
            "cancellation needs a current revision and request identifier",
        ));
    }
    let payload = serde_json::to_string(&input).map_err(invalid)?;
    let mut tx = crate::write_tx(pool).await?;
    if let Some((uid,previous)) = sqlx::query_as::<_,(String,String)>("SELECT cancellation_uid,payload FROM transfer_cancellation_application WHERE request_id = ?").bind(&input.request_id).fetch_optional(&mut *tx).await? {
        if previous != payload { return Err(invalid("cancellation request reused with different terms")); }
        tx.rollback().await?;
        return Ok(Outcome {uid,fact:None});
    }
    participant(
        &mut tx,
        &input.transfer,
        &input.person,
        input.expected_revision,
    )
    .await?;
    let row = sqlx::query("SELECT * FROM transfer_cancellation WHERE uid = ? AND transfer_uid = ? AND revision = ? AND applied_fact_uid IS NULL").bind(&input.cancellation).bind(&input.transfer).bind(input.expected_revision as i64).fetch_optional(&mut *tx).await?.ok_or_else(||invalid("cancellation is stale or already completed"))?;
    let people: Vec<String> =
        serde_json::from_str(&row.get::<String, _>("required_people")).map_err(invalid)?;
    let occurrences: Vec<String> =
        serde_json::from_str(&row.get::<String, _>("occurrences")).map_err(invalid)?;
    let quantity = exact::read_decimal(&row, "quantity")?;
    let mut agreement_facts = Vec::new();
    for person in people {
        let agreed: Option<String> = sqlx::query_scalar("SELECT e.fact_uid FROM transfer_agreement a JOIN transfer_party p ON p.uid = a.party_uid JOIN transfer_agreement_event e ON e.transfer_uid = a.transfer_uid AND e.person_uid = p.actor_uid AND e.revision = a.revision AND e.to_level = 2 WHERE a.transfer_uid = ? AND p.actor_uid = ? AND a.revision = ? AND a.level = 2 ORDER BY e.created_at DESC,e.uid DESC LIMIT 1").bind(&input.transfer).bind(person).bind(input.expected_revision as i64).fetch_optional(&mut *tx).await?;
        agreement_facts.push(agreed.ok_or_else(|| {
            invalid("all affected participants must agree to the cancellation revision")
        })?);
    }
    verify_remaining(&mut tx, &occurrences, quantity).await?;
    let previous_hash = crate::facts::last_hash(&mut tx).await?;
    let mut fact = nucleus::fact::seal(nucleus::fact::NewFact {
        uid:None,record_uid:input.transfer.clone(),delta:exact::zero(),at:None,actor_uid:Some(input.person.clone()),cause:nucleus::Cause::user_edit(),
        payload:Some(json!({"action":"apply-transfer-cancellation","cancellation":input.cancellation,"revision":input.expected_revision,"quantity":quantity,"occurrences":occurrences,"agreement_facts":agreement_facts,"request_id":input.request_id}).to_string()),
    },&previous_hash,now);
    fact.signature = sign(&fact.hash);
    if fact.signature.is_none() && input.authorization_intent_uid.is_none() {
        return Err(invalid("cancellation requires the participant's signature"));
    }
    crate::facts::insert(&mut tx, &fact).await?;
    if let Some(intent) = &input.authorization_intent_uid {
        crate::action_intents::link_pending_fact(&mut tx, intent, &fact).await?;
    }
    let (mantissa, scale) = exact::decimal_columns(quantity);
    for occurrence in occurrences {
        sqlx::query("INSERT INTO transfer_occurrence_cancellation (occurrence_uid,cancellation_uid,quantity_mantissa,quantity_scale,fact_uid) VALUES (?,?,?,?,?)").bind(occurrence).bind(&input.cancellation).bind(&mantissa).bind(scale).bind(&fact.uid).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO transfer_cancellation_application (request_id,cancellation_uid,person_uid,fact_uid,payload) VALUES (?,?,?,?,?)").bind(&input.request_id).bind(&input.cancellation).bind(input.person).bind(&fact.uid).bind(payload).execute(&mut *tx).await?;
    sqlx::query("UPDATE transfer_cancellation SET applied_fact_uid = ? WHERE uid = ?")
        .bind(&fact.uid)
        .bind(&input.cancellation)
        .execute(&mut *tx)
        .await?;
    crate::records::bump_quantity(&mut tx, &input.transfer, exact::zero(), &now.to_rfc3339())
        .await?;
    tx.commit().await?;
    Ok(Outcome {
        uid: input.cancellation,
        fact: Some(fact),
    })
}

pub async fn cancelled(pool: &SqlitePool, occurrence: &str) -> Result<DecimalValue, StoreError> {
    cancelled_on(&mut *pool.acquire().await?, occurrence).await
}

pub async fn cancelled_on(
    connection: &mut SqliteConnection,
    occurrence: &str,
) -> Result<DecimalValue, StoreError> {
    sqlx::query("SELECT quantity_mantissa,quantity_scale FROM transfer_occurrence_cancellation WHERE occurrence_uid = ?").bind(occurrence).fetch_optional(&mut *connection).await?.map(|row|exact::read_decimal(&row,"quantity")).transpose().map(|value|value.unwrap_or_else(exact::zero))
}

pub async fn effective_total_on(
    connection: &mut SqliteConnection,
    occurrence: &str,
    total: DecimalValue,
) -> Result<DecimalValue, StoreError> {
    exact::difference(total, cancelled_on(connection, occurrence).await?)
}

pub async fn terms_on(
    connection: &mut SqliteConnection,
    transfer: &str,
) -> Result<Vec<nucleus::transfer::TransferCancellationTerms>, StoreError> {
    let rows = sqlx::query("SELECT c.* FROM transfer_cancellation c JOIN transfer t ON t.record_uid = c.transfer_uid WHERE c.transfer_uid = ? AND (c.revision = t.revision OR c.applied_fact_uid IS NOT NULL) ORDER BY c.uid").bind(transfer).fetch_all(&mut *connection).await?;
    rows.into_iter()
        .map(|row| {
            Ok(nucleus::transfer::TransferCancellationTerms {
                uid: row.get("uid"),
                exchange_path_uid: row.get("exchange_path_uid"),
                occurrences: serde_json::from_str(&row.get::<String, _>("occurrences"))
                    .map_err(invalid)?,
                quantity: exact::read_decimal(&row, "quantity")?,
                required_people: serde_json::from_str(&row.get::<String, _>("required_people"))
                    .map_err(invalid)?,
            })
        })
        .collect()
}

pub async fn for_occurrence(
    pool: &SqlitePool,
    transfer: &str,
    occurrence: &str,
) -> Result<Vec<Value>, StoreError> {
    let rows = sqlx::query("SELECT c.*,t.revision AS current_revision, (json_array_length(c.required_people) > 0 AND NOT EXISTS(SELECT 1 FROM json_each(c.required_people) person WHERE NOT EXISTS(SELECT 1 FROM transfer_party p JOIN transfer_agreement a ON a.party_uid = p.uid WHERE p.transfer_uid = c.transfer_uid AND p.actor_uid = person.value AND a.revision = c.revision AND a.level = 2))) AS all_agreed FROM transfer_cancellation c JOIN transfer t ON t.record_uid = c.transfer_uid WHERE c.transfer_uid = ? AND ? IN (SELECT value FROM json_each(c.occurrences)) ORDER BY c.revision,c.uid").bind(transfer).bind(occurrence).fetch_all(pool).await?;
    rows.into_iter().map(|row| {
        let current = row.get::<i64,_>("revision") == row.get::<i64,_>("current_revision");
        let applied: Option<String> = row.get("applied_fact_uid");
        Ok(json!({"uid":row.get::<String,_>("uid"),"revision":row.get::<i64,_>("revision"),"quantity":exact::read_decimal(&row,"quantity")?,"proposal_fact":row.get::<Option<String>,_>("proposal_fact_uid"),"applied_fact":applied,"all_agreed":row.get::<bool,_>("all_agreed"),"status":if applied.is_some() {"applied"} else if current {"pending"} else {"stale"}}))
    }).collect()
}
