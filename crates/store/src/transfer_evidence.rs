use crate::StoreError;
use chrono::{DateTime, Utc};
use nucleus::transfer::outcome::{
    Content, Evidence, Measure, NeedSelection, NeedTerms, Request, Role,
};
use nucleus::{Cause, Fact, NewFact, RecordKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Row, SqliteConnection, SqlitePool};
use std::collections::{BTreeMap, HashSet};

fn invalid(message: impl ToString) -> StoreError {
    StoreError::Protocol(message.to_string())
}

fn encoded<T: Serialize>(value: &T) -> Result<String, StoreError> {
    let value = serde_json::to_string(value).map_err(invalid)?;
    if value.len() > nucleus::transfer::outcome::MAX_BYTES {
        return Err(invalid("Outcome evidence exceeds 32768 bytes"));
    }
    Ok(value)
}

pub async fn freeze_need_on(
    connection: &mut SqliteConnection,
    person: &str,
    selection: &NeedSelection,
) -> Result<NeedTerms, StoreError> {
    crate::record_revisions::check_expected_on(
        connection,
        &selection.need,
        selection.expected_need_revision,
    )
    .await?;
    let record = sqlx::query("SELECT r.quantity_mantissa,r.quantity_scale,r.unit_uid FROM record r WHERE r.uid=? AND r.deleted_at IS NULL AND (SELECT actor_uid FROM fact WHERE record_uid=r.uid ORDER BY rowid LIMIT 1)=?")
        .bind(&selection.need).bind(person).fetch_optional(&mut *connection).await?
        .ok_or_else(|| invalid("Choose a Need authored by the receiving Person"))?;
    let need_quantity = crate::exact::read_decimal(&record, "quantity")?;
    if !need_quantity.is_negative() {
        return Err(invalid(
            "The receiver's Need must have a negative quantity when bound",
        ));
    }
    let assertion = sqlx::query(
        "SELECT * FROM record_assertion WHERE uid=? AND subject_uid=? AND retracted_at IS NULL",
    )
    .bind(&selection.assertion)
    .bind(&selection.need)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or_else(|| invalid("Choose a current assertion of this Need"))?;
    let quantity = assertion
        .get::<Option<String>, _>("quantity_mantissa")
        .map(|mantissa| crate::exact::parse_decimal(&mantissa, assertion.get("quantity_scale")))
        .transpose()?;
    let requested = quantity.unwrap_or(
        need_quantity
            .checked_neg()
            .ok_or_else(|| invalid("Need amount overflows"))?,
    );
    if !requested.is_positive() {
        return Err(invalid("The requested assertion amount must be positive"));
    }
    let unit: Option<String> = if quantity.is_some() {
        assertion.get("unit_uid")
    } else {
        record.get("unit_uid")
    };
    let unit = if let Some(unit) = unit {
        let revision = if quantity.is_some() {
            assertion
                .get::<Option<i64>, _>("unit_revision")
                .ok_or_else(|| invalid("Need assertion has no unit version"))?
        } else {
            sqlx::query_scalar(
                "SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=?",
            )
            .bind(&unit)
            .fetch_one(&mut *connection)
            .await?
        };
        Some(
            crate::vocabulary::meaning_reference_on(
                connection,
                selection
                    .unit_vocabulary
                    .as_deref()
                    .ok_or_else(|| invalid("Choose the unit's Vocabulary"))?,
                &unit,
                revision,
            )
            .await?,
        )
    } else {
        if selection.unit_vocabulary.is_some() {
            return Err(invalid("This requested amount has no unit"));
        }
        None
    };
    let predicate: String = assertion.get("predicate_uid");
    let measure = Measure {
        meaning: crate::vocabulary::meaning_reference_on(
            connection,
            &selection.vocabulary,
            &predicate,
            assertion.get("predicate_revision"),
        )
        .await?,
        unit,
        object: assertion.get("object_uid"),
    };
    measure.validate().map_err(invalid)?;
    Ok(NeedTerms {
        selection: selection.clone(),
        measure,
        assertion_role: assertion.get("role"),
        need_quantity,
        requested,
    })
}

pub(crate) async fn check_measure_on(
    connection: &mut SqliteConnection,
    measure: &Measure,
) -> Result<(), StoreError> {
    measure.validate().map_err(invalid)?;
    for reference in std::iter::once(&measure.meaning).chain(measure.unit.iter()) {
        let saved = crate::vocabulary::meaning_reference_on(
            connection,
            &reference.vocabulary_uid,
            &reference.concept_uid,
            reference.revision,
        )
        .await?;
        if saved != *reference {
            return Err(invalid(
                "Outcome meaning does not match its immutable definition",
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedOutcome {
    pub evidence: Evidence,
    pub fact: Fact,
}

async fn saved_on(
    connection: &mut SqliteConnection,
    uid: &str,
) -> Result<SavedOutcome, StoreError> {
    let row: (String, String) = sqlx::query_as(
        "SELECT payload_json,fact_uid FROM transfer_outcome_record WHERE record_uid=?",
    )
    .bind(uid)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or_else(|| invalid("Outcome version is unavailable"))?;
    if row.0.len() > nucleus::transfer::outcome::MAX_BYTES {
        return Err(invalid("Outcome version is too large"));
    }
    let fact = crate::facts::get_on(connection, &row.1)
        .await?
        .ok_or_else(|| invalid("Outcome Fact is unavailable"))?;
    let evidence: Evidence = serde_json::from_str(&row.0).map_err(invalid)?;
    if fact.record_uid != evidence.record
        || fact.actor_uid.as_deref() != Some(&evidence.author)
        || fact.payload.as_deref() != Some(&row.0)
        || !nucleus::fact::verify_chain_step(&fact)
        || evidence.domain != "lince.transfer-outcome.v1"
    {
        return Err(invalid("Outcome evidence does not match its original Fact"));
    }
    Ok(SavedOutcome { evidence, fact })
}

pub async fn get(pool: &SqlitePool, uid: &str) -> Result<SavedOutcome, StoreError> {
    saved_on(&mut *pool.acquire().await?, uid).await
}

pub async fn for_request(pool: &SqlitePool, request: &str) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar("SELECT record_uid FROM transfer_outcome_record WHERE request_id=?")
        .bind(request)
        .fetch_optional(pool)
        .await
}

pub struct OutcomeCommit {
    pub saved: SavedOutcome,
    pub created: bool,
}

pub struct OutcomeInput {
    pub occurrence: String,
    pub expected_revision: u64,
    pub person: String,
    pub request_id: String,
    pub request: Request,
    pub authorization_intent: Option<String>,
}

pub async fn record<F>(
    pool: &SqlitePool,
    input: OutcomeInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<OutcomeCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    if input.request_id.is_empty()
        || input.request_id.len() > 256
        || input.request_id.chars().any(char::is_control)
    {
        return Err(invalid("Choose an outcome request ID of 1–256 characters"));
    }
    let request_json = encoded(&(
        &input.occurrence,
        input.expected_revision,
        &input.person,
        &input.request,
    ))?;
    let mut tx = crate::write_tx(pool).await?;
    if !crate::people::is_active_on(&mut tx, &input.person).await? {
        return Err(invalid("Outcome author is not an active Person"));
    }
    if let Some((uid, previous)) = sqlx::query_as::<_, (String, String)>(
        "SELECT record_uid,request_json FROM transfer_outcome_record WHERE request_id=?",
    )
    .bind(&input.request_id)
    .fetch_optional(&mut *tx)
    .await?
    {
        if previous != request_json {
            return Err(invalid(
                "Outcome request ID was used with different contents",
            ));
        }
        return Ok(OutcomeCommit {
            saved: saved_on(&mut tx, &uid).await?,
            created: false,
        });
    }
    let occurrence = sqlx::query("SELECT o.transfer_uid,o.revision,o.giver_person_uid,o.receiver_person_uid FROM transfer_occurrence o WHERE o.uid=? AND o.quantity>0 AND o.giver_person_uid<>o.receiver_person_uid")
        .bind(&input.occurrence).fetch_optional(&mut *tx).await?.ok_or_else(|| invalid("Outcome needs a resource occurrence"))?;
    let transfer: String = occurrence.get("transfer_uid");
    let revision: i64 = occurrence.get("revision");
    let current_revision: i64 =
        sqlx::query_scalar("SELECT revision FROM transfer WHERE record_uid=?")
            .bind(&transfer)
            .fetch_one(&mut *tx)
            .await?;
    if u64::try_from(current_revision).ok() != Some(input.expected_revision) {
        return Err(invalid(
            "Transfer revision changed; review the current Transfer",
        ));
    }
    let receiver: String = occurrence.get("receiver_person_uid");
    let content = match input.request {
        Request::BindNeed {
            selection,
            expected_measure,
            supersedes,
        } => {
            if input.person != receiver {
                return Err(invalid("Only the receiver can bind their Need"));
            }
            let terms = freeze_need_on(&mut tx, &input.person, &selection).await?;
            if terms.measure != expected_measure {
                return Err(invalid(
                    "The requested meaning or unit changed; review the Need again",
                ));
            }
            Content::NeedBinding { terms, supersedes }
        }
        Request::State {
            role,
            measure,
            amount,
            withdrawn,
            need_binding,
            supersedes,
        } => {
            let expected: String = if role == Role::Giver {
                occurrence.get("giver_person_uid")
            } else {
                receiver.clone()
            };
            if expected != input.person {
                return Err(invalid(
                    "Outcome statement belongs to the occurrence's corresponding party",
                ));
            }
            check_measure_on(&mut tx, &measure).await?;
            if let Some(binding) = &need_binding {
                let binding = saved_on(&mut tx, binding).await?;
                if binding.evidence.occurrence != input.occurrence
                    || binding.evidence.author != receiver
                {
                    return Err(invalid(
                        "The Need binding must belong to this occurrence's receiver",
                    ));
                }
                match &binding.evidence.content {
                    Content::NeedBinding { terms, .. } if terms.measure == measure => {}
                    _ => {
                        return Err(invalid(
                            "Receiver confirmation must use the Need's exact meaning and unit versions",
                        ));
                    }
                }
            }
            Content::Statement {
                role,
                measure,
                quantity: Request::amount(&amount, withdrawn).map_err(invalid)?,
                withdrawn,
                need_binding,
                supersedes,
            }
        }
    };
    let measurement_key = content.measure().key().map_err(invalid)?;
    if let Some(previous) = content.supersedes() {
        let previous = saved_on(&mut tx, previous).await?;
        if previous.evidence.occurrence != input.occurrence
            || previous.evidence.author != input.person
            || previous.evidence.content.kind() != content.kind()
            || previous.evidence.content.measure() != content.measure()
        {
            return Err(invalid(
                "A correction must retain its original occurrence, author, role and measurement",
            ));
        }
    } else {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer_outcome_record WHERE occurrence_uid=? AND author_person_uid=? AND kind=? AND measurement_key=?)")
            .bind(&input.occurrence).bind(&input.person).bind(content.kind()).bind(&measurement_key).fetch_one(&mut *tx).await?;
        if exists {
            return Err(invalid(
                "Correct an existing outcome version instead of adding the same outcome again",
            ));
        }
    }
    let uid = nucleus::new_uid("r");
    let evidence = Evidence {
        domain: "lince.transfer-outcome.v1".into(),
        record: uid.clone(),
        transfer,
        occurrence: input.occurrence,
        agreement_revision: revision as u64,
        author: input.person.clone(),
        request_id: input.request_id.clone(),
        content,
    };
    let payload = encoded(&evidence)?;
    let organ: String = sqlx::query_scalar(
        "SELECT uid FROM record WHERE kind='organ' AND slug=? AND deleted_at IS NULL",
    )
    .bind(crate::organs::LOCAL_ORGAN_SLUG)
    .fetch_one(&mut *tx)
    .await?;
    crate::records::create_with_uid_on(
        &mut tx,
        &uid,
        crate::records::NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: if evidence.content.kind() == "need_binding" {
                "Requested outcome"
            } else {
                "Transfer outcome statement"
            },
            body: "Signed Transfer outcome. Read its original evidence and correction history.",
            quantity: crate::exact::zero(),
        },
        &organ,
        None,
    )
    .await?;
    crate::records::set_extension_on(
        &mut tx,
        &uid,
        nucleus::transfer::outcome::NAMESPACE,
        &json!(evidence),
    )
    .await?;
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
            "Outcome evidence needs its author's signing key or verified Action intent",
        ));
    }
    crate::facts::insert(&mut tx, &fact).await?;
    if let Some(intent) = &input.authorization_intent {
        crate::action_intents::link_pending_fact(&mut tx, intent, &fact).await?;
    }
    let binding = match &evidence.content {
        Content::Statement { need_binding, .. } => need_binding.as_deref(),
        _ => None,
    };
    sqlx::query("INSERT INTO transfer_outcome_record(record_uid,occurrence_uid,author_person_uid,kind,measurement_key,supersedes_uid,need_binding_uid,fact_uid,request_id,request_json,payload_json) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&uid).bind(&evidence.occurrence).bind(&evidence.author).bind(evidence.content.kind()).bind(measurement_key)
        .bind(evidence.content.supersedes()).bind(binding).bind(&fact.uid).bind(input.request_id).bind(request_json).bind(payload)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(OutcomeCommit {
        saved: SavedOutcome { evidence, fact },
        created: true,
    })
}

pub async fn history(
    pool: &SqlitePool,
    occurrence: &str,
    visible: Option<&HashSet<String>>,
) -> Result<Value, StoreError> {
    let visibility = visible
        .map(|set| serde_json::to_string(&set.iter().collect::<Vec<_>>()))
        .transpose()
        .map_err(invalid)?;
    let rows: Vec<String> = sqlx::query_scalar("SELECT e.record_uid FROM transfer_outcome_record e JOIN record r ON r.uid=e.record_uid WHERE e.occurrence_uid=? AND r.deleted_at IS NULL AND (? IS NULL OR e.record_uid IN (SELECT value FROM json_each(?))) ORDER BY e.rowid DESC LIMIT 257")
        .bind(occurrence).bind(&visibility).bind(&visibility).fetch_all(pool).await?;
    let more = rows.len() > nucleus::transfer::outcome::MAX_HISTORY;
    let mut entries = Vec::new();
    let mut superseded = HashSet::new();
    let mut heads = BTreeMap::<(String, String, String), usize>::new();
    for uid in rows.iter().take(nucleus::transfer::outcome::MAX_HISTORY) {
        let saved = get(pool, uid).await?;
        if let Some(visible) = visible {
            if saved
                .evidence
                .content
                .measure()
                .object
                .as_ref()
                .is_some_and(|uid| !visible.contains(uid))
                || saved
                    .evidence
                    .content
                    .supersedes()
                    .is_some_and(|uid| !visible.contains(uid))
            {
                continue;
            }
            match &saved.evidence.content {
                Content::NeedBinding { terms, .. } if !visible.contains(&terms.selection.need) => {
                    continue;
                }
                Content::Statement {
                    need_binding: Some(uid),
                    ..
                } => {
                    if !visible.contains(uid) {
                        continue;
                    }
                    let binding = get(pool, uid).await?;
                    if let Content::NeedBinding { terms, .. } = &binding.evidence.content
                        && !visible.contains(&terms.selection.need)
                    {
                        continue;
                    }
                }
                _ => {}
            }
        }
        if let Some(previous) = saved.evidence.content.supersedes() {
            superseded.insert(previous.to_owned());
        }
        entries.push(saved);
    }
    for entry in &entries {
        if !superseded.contains(&entry.evidence.record) {
            *heads
                .entry((
                    entry.evidence.author.clone(),
                    entry.evidence.content.kind().into(),
                    entry.evidence.content.measure().key().map_err(invalid)?,
                ))
                .or_default() += 1;
        }
    }
    let entries = entries.into_iter().map(|entry| {
        let key = (entry.evidence.author.clone(), entry.evidence.content.kind().into(), entry.evidence.content.measure().key().unwrap_or_default());
        json!({"superseded":superseded.contains(&entry.evidence.record),"conflicting":heads.get(&key).is_some_and(|count| *count>1),
            "evidence":entry.evidence,"fact":entry.fact})
    }).collect::<Vec<_>>();
    Ok(json!({"entries":entries,"more":more}))
}

pub async fn need_options(
    pool: &SqlitePool,
    person: &str,
    visible: Option<&HashSet<String>>,
) -> Result<Value, StoreError> {
    let visibility = visible
        .map(serde_json::to_string)
        .transpose()
        .map_err(invalid)?;
    let rows = sqlx::query("SELECT r.uid AS need,rr.revision,a.uid AS assertion,lc.lingua_uid AS vocabulary,uc.lingua_uid AS unit_vocabulary,r.head,c.canonical_name,l.name AS vocabulary_name FROM record r JOIN record_revision rr ON rr.record_uid=r.uid JOIN record_assertion a ON a.subject_uid=r.uid AND a.retracted_at IS NULL JOIN concept c ON c.uid=a.predicate_uid JOIN lingua_concept lc ON lc.concept_uid=c.uid JOIN lingua l ON l.uid=lc.lingua_uid LEFT JOIN lingua_concept uc ON uc.concept_uid=CASE WHEN a.quantity_mantissa IS NULL THEN r.unit_uid ELSE a.unit_uid END WHERE r.deleted_at IS NULL AND r.quantity_mantissa LIKE '-%' AND (SELECT actor_uid FROM fact WHERE record_uid=r.uid ORDER BY rowid LIMIT 1)=? AND lc.lingua_uid<>'g_local' AND (uc.lingua_uid IS NULL OR uc.lingua_uid<>'g_local') AND (? IS NULL OR r.uid IN (SELECT value FROM json_each(?))) ORDER BY r.uid,a.uid,lc.lingua_uid,uc.lingua_uid LIMIT 65")
        .bind(person).bind(&visibility).bind(&visibility).fetch_all(pool).await?;
    let more = rows.len() > 64;
    let mut options = Vec::new();
    let mut connection = pool.acquire().await?;
    for row in rows.into_iter().take(64) {
        let selection = NeedSelection {
            need: row.get("need"),
            expected_need_revision: row.get("revision"),
            assertion: row.get("assertion"),
            vocabulary: row.get("vocabulary"),
            unit_vocabulary: row.get("unit_vocabulary"),
        };
        let terms = match freeze_need_on(&mut connection, person, &selection).await {
            Ok(terms) => terms,
            Err(sqlx::Error::Protocol(_)) => continue,
            Err(error) => return Err(error),
        };
        if terms
            .measure
            .object
            .as_ref()
            .is_some_and(|uid| visible.is_some_and(|visible| !visible.contains(uid)))
        {
            continue;
        }
        let label = format!(
            "{} · #{} · {}",
            row.get::<String, _>("head"),
            row.get::<String, _>("canonical_name"),
            row.get::<String, _>("vocabulary_name")
        );
        options.push(json!({"label":label,"terms":terms}));
    }
    Ok(json!({"options":options,"more":more}))
}

pub async fn source_measures(
    pool: &SqlitePool,
    record: &str,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<Value>, StoreError> {
    if visible.is_some_and(|visible| !visible.contains(record)) {
        return Ok(Vec::new());
    }
    let rows = sqlx::query("SELECT a.predicate_uid,a.predicate_revision,a.object_uid,c.canonical_name,lc.lingua_uid AS vocabulary,CASE WHEN a.quantity_mantissa IS NULL THEN r.unit_uid ELSE a.unit_uid END AS unit_uid,CASE WHEN a.quantity_mantissa IS NULL THEN (SELECT MAX(revision) FROM concept_definition_version WHERE concept_uid=r.unit_uid) ELSE a.unit_revision END AS unit_revision,uc.lingua_uid AS unit_vocabulary FROM record_assertion a JOIN record r ON r.uid=a.subject_uid JOIN concept c ON c.uid=a.predicate_uid JOIN lingua_concept lc ON lc.concept_uid=c.uid LEFT JOIN lingua_concept uc ON uc.concept_uid=CASE WHEN a.quantity_mantissa IS NULL THEN r.unit_uid ELSE a.unit_uid END WHERE a.subject_uid=? AND a.retracted_at IS NULL AND r.deleted_at IS NULL AND lc.lingua_uid<>'g_local' AND (uc.lingua_uid IS NULL OR uc.lingua_uid<>'g_local') ORDER BY a.uid,lc.lingua_uid,uc.lingua_uid LIMIT 64")
        .bind(record).fetch_all(pool).await?;
    let mut options = Vec::new();
    let mut connection = pool.acquire().await?;
    for row in rows {
        let object: Option<String> = row.get("object_uid");
        if object
            .as_ref()
            .is_some_and(|uid| visible.is_some_and(|visible| !visible.contains(uid)))
        {
            continue;
        }
        let vocabulary: String = row.get("vocabulary");
        let concept: String = row.get("predicate_uid");
        let unit = match (
            row.get::<Option<String>, _>("unit_uid"),
            row.get::<Option<String>, _>("unit_vocabulary"),
        ) {
            (Some(unit), Some(vocabulary)) => Some(
                crate::vocabulary::meaning_reference_on(
                    &mut connection,
                    &vocabulary,
                    &unit,
                    row.get("unit_revision"),
                )
                .await?,
            ),
            (None, None) => None,
            _ => continue,
        };
        let measure = Measure {
            meaning: crate::vocabulary::meaning_reference_on(
                &mut connection,
                &vocabulary,
                &concept,
                row.get("predicate_revision"),
            )
            .await?,
            unit,
            object,
        };
        if !options
            .iter()
            .any(|option: &Value| option["measure"] == json!(measure))
        {
            options.push(json!({"label":row.get::<String,_>("canonical_name"),"measure":measure,"need_binding":null}));
        }
    }
    Ok(options)
}
