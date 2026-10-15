use std::collections::BTreeSet;

use engine::{
    Engine,
    actions::Action,
    record_change::{Mutation, Request},
};
use nucleus::{DecimalValue, karma::CanonicalHash};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    Result,
    scenario::{Event, Input, Invocation, Scenario},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Assertion {
    pub uid: String,
    pub predicate: String,
    pub object: Option<String>,
    pub quantity: Option<DecimalValue>,
    pub unit: Option<String>,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub uid: String,
    pub revision: i64,
    pub kind: String,
    pub head: String,
    pub body: String,
    pub quantity: DecimalValue,
    pub unit: Option<String>,
    pub slug: Option<String>,
    pub work: Value,
    pub assertions: Vec<Assertion>,
}

impl Record {
    pub async fn read(store: &store::Store, reference: &str) -> Result<Self> {
        Self::read_bounded(store, reference, crate::limits::JSON_BYTES).await
    }

    pub async fn read_bounded(store: &store::Store, reference: &str, limit: u64) -> Result<Self> {
        use store::sqlx::Row;
        let exceeded = || -> crate::Error {
            Box::new(crate::limits::Exceeded {
                resource: "Record capture",
                limit,
            })
        };
        let mut tx = store.pool.begin().await?;
        let header = store::sqlx::query("SELECT substr(uid,1,128) AS uid, length(CAST(uid AS BLOB)) + length(CAST(head AS BLOB)) + length(CAST(body AS BLOB)) + length(CAST(kind AS BLOB)) + COALESCE(length(CAST(slug AS BLOB)),0) + COALESCE(length(CAST(unit_uid AS BLOB)),0) + length(CAST(quantity_mantissa AS BLOB)) AS bytes FROM record WHERE (uid = ? OR slug = ?) AND deleted_at IS NULL ORDER BY (uid = ?) DESC LIMIT 1")
            .bind(reference).bind(reference).bind(reference).fetch_optional(&mut *tx).await?.ok_or("Proposal Record is missing or deleted")?;
        let uid: String = header.try_get("uid")?;
        if !nucleus::valid_uid(&uid, "r") {
            return Err("Invalid captured Record uid".into());
        }
        let mut retained: u64 = header.try_get::<i64, _>("bytes")?.try_into()?;
        if retained > limit {
            return Err(exceeded());
        }
        let work_bytes:i64 = store::sqlx::query_scalar("SELECT COALESCE((SELECT length(CAST(fds AS BLOB)) FROM record_extension WHERE record_uid = ? AND namespace = 'work'),2)").bind(&uid).fetch_one(&mut *tx).await?;
        retained = retained.saturating_add(work_bytes.try_into()?);
        let counts = store::sqlx::query("SELECT count(*) AS count, COALESCE(sum(length(CAST(a.uid AS BLOB)) + length(CAST(a.predicate_uid AS BLOB)) + COALESCE(length(CAST(a.object_uid AS BLOB)),0) + length(CAST(a.role AS BLOB)) + COALESCE(length(CAST(a.quantity_mantissa AS BLOB)),0) + COALESCE(length(CAST(a.unit_uid AS BLOB)),0)),0) AS bytes FROM record_assertion a JOIN concept c ON c.uid = a.predicate_uid WHERE a.subject_uid = ? AND a.retracted_at IS NULL").bind(&uid).fetch_one(&mut *tx).await?;
        let count: u64 = counts.try_get::<i64, _>("count")?.try_into()?;
        retained = retained.saturating_add(counts.try_get::<i64, _>("bytes")?.try_into()?);
        if retained > limit || count > crate::limits::rows::<Assertion>(limit) {
            return Err(exceeded());
        }
        let row = store::sqlx::query("SELECT uid,kind,head,body,quantity_mantissa,quantity_scale,unit_uid,slug FROM record WHERE uid = ? AND deleted_at IS NULL").bind(&uid).fetch_one(&mut *tx).await?;
        let raw: Option<String> = store::sqlx::query_scalar(
            "SELECT fds FROM record_extension WHERE record_uid = ? AND namespace = 'work'",
        )
        .bind(&uid)
        .fetch_optional(&mut *tx)
        .await?;
        let mut assertions = Vec::new();
        let mut after = String::new();
        loop {
            let batch = store::sqlx::query("SELECT a.uid,a.predicate_uid,a.object_uid,a.role,a.quantity_mantissa,a.quantity_scale,a.unit_uid FROM record_assertion a JOIN concept c ON c.uid = a.predicate_uid WHERE a.subject_uid = ? AND a.retracted_at IS NULL AND a.uid > ? ORDER BY a.uid LIMIT ?")
                .bind(&uid).bind(&after).bind(crate::limits::BATCH_ROWS as i64).fetch_all(&mut *tx).await?;
            if batch.is_empty() {
                break;
            }
            for assertion in batch {
                after = assertion.try_get("uid")?;
                assertions.push(Assertion {
                    uid: after.clone(),
                    predicate: assertion.try_get("predicate_uid")?,
                    object: assertion.try_get("object_uid")?,
                    role: assertion.try_get("role")?,
                    unit: assertion.try_get("unit_uid")?,
                    quantity: if assertion
                        .try_get::<Option<String>, _>("quantity_mantissa")?
                        .is_some()
                    {
                        Some(store::exact::read_decimal(&assertion, "quantity")?)
                    } else {
                        None
                    },
                });
            }
        }
        let record = Self {
            uid: uid.clone(),
            revision: store::record_revisions::get_on(&mut *tx, &uid)
                .await?
                .revision,
            kind: row.try_get("kind")?,
            head: row.try_get("head")?,
            body: row.try_get("body")?,
            quantity: store::exact::read_decimal(&row, "quantity")?,
            unit: row.try_get("unit_uid")?,
            slug: row.try_get("slug")?,
            work: raw
                .map(|raw| serde_json::from_str(&raw))
                .transpose()?
                .unwrap_or_else(|| serde_json::json!({})),
            assertions,
        };
        crate::limits::size(&record, limit, "Record capture")?;
        tx.commit().await?;
        Ok(record)
    }

    pub fn dependencies(&self) -> Vec<String> {
        std::iter::once(self.uid.clone())
            .chain(
                self.assertions
                    .iter()
                    .filter_map(|assertion| assertion.object.clone()),
            )
            .collect()
    }
}

pub fn validate_record(record: &Record) -> Result<()> {
    if !nucleus::valid_uid(&record.uid, "r") || record.revision <= 0 {
        return Err("Invalid captured Record identity or revision".into());
    }
    engine::private_work::WorkMetadata::parse(&record.work)?;
    let mut assertions = BTreeSet::new();
    for assertion in &record.assertions {
        if !nucleus::valid_uid(&assertion.uid, "a")
            || !assertions.insert(&assertion.uid)
            || !nucleus::valid_uid(&assertion.predicate, "c")
            || assertion
                .object
                .as_ref()
                .is_some_and(|uid| !nucleus::valid_uid(uid, "r"))
            || assertion
                .unit
                .as_ref()
                .is_some_and(|uid| !nucleus::valid_uid(uid, "c"))
        {
            return Err("Invalid captured Assertion identity".into());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Occurrence {
    pub rule: String,
    pub revision: u64,
    pub event: String,
    pub intended_at_ms: i64,
    pub observed_at_ms: i64,
    pub time: Option<nucleus::schedule::TimeRange>,
    pub unavailable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureStatus {
    Captured,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OccurrenceGap {
    SourceHistory,
    Interrupted,
    UnavailableTemplate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub cell: String,
    pub record: String,
    pub state: Option<Record>,
    pub occurrences: Vec<Occurrence>,
    pub status: CaptureStatus,
    pub occurrence_gaps: Vec<OccurrenceGap>,
}

impl Outcome {
    pub async fn read(
        store: &store::Store,
        cell: &str,
        uid: &str,
        occurrences: Vec<Occurrence>,
        occurrence_gaps: Vec<OccurrenceGap>,
        limit: u64,
    ) -> Result<Self> {
        let exists: bool = store::sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM record WHERE uid = ? AND deleted_at IS NULL)",
        )
        .bind(uid)
        .fetch_one(&store.pool)
        .await?;
        let state = if exists {
            Some(Record::read_bounded(store, uid, limit).await?)
        } else {
            None
        };
        Ok(Self {
            cell: cell.into(),
            record: uid.into(),
            state,
            occurrences,
            status: CaptureStatus::Captured,
            occurrence_gaps,
        })
    }

    pub fn content(&self) -> Result<Value> {
        let mut value = serde_json::to_value(self)?;
        if let Some(state) = value["state"].as_object_mut() {
            state.remove("revision");
        }
        Ok(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Edit {
    Text {
        head: Option<String>,
        body: Option<String>,
    },
    Property {
        mutation: Mutation,
    },
    Work {
        value: Value,
    },
}

pub fn validate_mutation(mutation: &Mutation) -> Result<()> {
    if !matches!(
        mutation,
        Mutation::Text { .. }
            | Mutation::Quantity { .. }
            | Mutation::Slug { .. }
            | Mutation::Work { .. }
            | Mutation::Assertion { .. }
            | Mutation::NumberAssertion { .. }
            | Mutation::RetractAssertion { .. }
    ) {
        return Err("This Record property is unavailable in a scenario".into());
    }
    if serde_json::to_vec(mutation)?.len() > 262_144 {
        return Err("Proposed property change exceeds its size limit".into());
    }
    Ok(())
}

impl Edit {
    pub fn action(&self, record: &str, id: &str, at_ms: i64) -> Result<Action> {
        use sha2::{Digest, Sha256};
        if at_ms < 0 {
            return Err("Proposal time is out of range".into());
        }
        let hash = Sha256::digest(serde_json::to_vec(&(record, id, at_ms, self))?);
        let entropy = u128::from_be_bytes(hash[..16].try_into()?);
        let operation = format!("op_{}", nucleus::ulid_from(at_ms as u64, entropy));
        Ok(match self {
            Self::Text { head, body } => Action::EditRecordText {
                target: record.into(),
                head: head.clone(),
                body: body.clone(),
            },
            Self::Property { mutation } => Action::ChangeRecord {
                request: Request {
                    id: operation,
                    record_uid: record.into(),
                    mutation: mutation.clone(),
                },
            },
            Self::Work { value } => Action::SetExtension {
                target: record.into(),
                namespace: "work".into(),
                fds: value.clone(),
            },
        })
    }

    fn validate(&self) -> Result<()> {
        match self {
            Self::Text { head, body } => {
                if head.is_none() && body.is_none()
                    || head.as_ref().is_some_and(|value| value.len() > 65_536)
                    || body.as_ref().is_some_and(|value| value.len() > 262_144)
                {
                    return Err("Choose a bounded title or description change".into());
                }
                Ok(())
            }
            Self::Property { mutation } => validate_mutation(mutation),
            Self::Work { value } => {
                if !value.is_object() || serde_json::to_vec(value)?.len() > 262_144 {
                    return Err("Choose bounded Work metadata for the appointment".into());
                }
                nucleus::schedule::validate_endpoints(
                    value["start"].as_str(),
                    value["due"].as_str(),
                )?;
                nucleus::schedule::range(
                    value["start"].as_str(),
                    value["due"].as_str(),
                    value["estimate_min"].as_f64(),
                    None,
                )?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub id: String,
    pub record: String,
    pub after_ms: u64,
    pub edit: Edit,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub id: String,
    pub cell: String,
    pub source: CanonicalHash,
    pub base_ms: i64,
    pub actor: Option<String>,
    pub records: Vec<Record>,
    pub changes: Vec<Change>,
}

impl Proposal {
    pub async fn capture(
        live: &Engine,
        source: &store::Store,
        actor: Option<&str>,
        cell: &str,
        base_ms: i64,
        references: &[String],
    ) -> Result<Self> {
        if references.is_empty() || references.len() > 64 {
            return Err("Choose 1–32 Records for the proposal".into());
        }
        let mut records = Vec::new();
        let mut remaining = crate::limits::CAPTURE_BYTES.saturating_sub(4096);
        for reference in references {
            if records.iter().any(|record: &Record| {
                &record.uid == reference || record.slug.as_ref() == Some(reference)
            }) {
                continue;
            }
            if records.len() >= 32 {
                return Err("Choose at most 32 distinct Records for the proposal".into());
            }
            let record = Record::read_bounded(source, reference, remaining).await?;
            live.refuse_unreadable(actor, &record.dependencies())
                .await?;
            if !records
                .iter()
                .any(|existing: &Record| existing.uid == record.uid)
            {
                remaining = remaining.saturating_sub(
                    crate::limits::size(&record, remaining, "proposal capture")? + 1,
                );
                records.push(record);
            }
        }
        if records.len() > 32 || cell.len() > 64 || actor.is_some_and(|actor| actor.len() > 128) {
            return Err("Choose at most 32 distinct Records for the proposal".into());
        }
        let proposal = Self {
            id: nucleus::new_uid("p"),
            cell: cell.into(),
            source: source.state_hash().await?,
            base_ms,
            actor: actor.map(str::to_owned),
            records,
            changes: Vec::new(),
        };
        crate::limits::size(&proposal, crate::limits::CAPTURE_BYTES, "proposal capture")?;
        Ok(proposal)
    }

    pub fn validate(&self, scenario: &Scenario) -> Result<()> {
        let valid_id = |value: &str| {
            !value.is_empty()
                && value.len() <= 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        };
        if !valid_id(&self.id)
            || self.base_ms != scenario.start_ms
            || !scenario
                .cells
                .iter()
                .any(|cell| cell.name == self.cell && cell.database.is_some())
            || self.records.is_empty()
            || self.records.len() > 32
            || self.changes.len() > 128
            || crate::limits::size(self, crate::limits::CAPTURE_BYTES, "proposal").is_err()
        {
            return Err("Invalid or oversized Record proposal".into());
        }
        let mut records = BTreeSet::new();
        for record in &self.records {
            if !nucleus::valid_uid(&record.uid, "r")
                || record.revision <= 0
                || !records.insert(&record.uid)
            {
                return Err("Proposal Records must have unique identities and revisions".into());
            }
        }
        let mut inputs: BTreeSet<_> = scenario
            .inputs
            .iter()
            .map(|input| input.id.clone())
            .collect();
        for change in &self.changes {
            if !valid_id(&change.id)
                || self.input_id(change).len() > 64
                || !records.contains(&change.record)
                || !inputs.insert(self.input_id(change))
                || self
                    .base_ms
                    .checked_add(i64::try_from(change.after_ms)?)
                    .is_none_or(|at| at > scenario.end_ms)
            {
                return Err(
                    "Proposed changes need unique IDs, bound Records and times inside the window"
                        .into(),
                );
            }
            change.edit.validate()?;
        }
        Ok(())
    }

    pub fn input_id(&self, change: &Change) -> String {
        format!("{}-{}", self.id, change.id)
    }

    pub fn inputs(&self) -> Result<Vec<Input>> {
        self.changes
            .iter()
            .map(|change| {
                let id = self.input_id(change);
                let at_ms = self
                    .base_ms
                    .checked_add(i64::try_from(change.after_ms)?)
                    .ok_or("Proposal time overflow")?;
                Ok(Input {
                    id: id.clone(),
                    cell: self.cell.clone(),
                    at_ms,
                    event: Event::Action {
                        invocation: Invocation {
                            action: change.edit.action(&change.record, &id, at_ms)?,
                            id,
                            actor: self.actor.clone(),
                        },
                    },
                })
            })
            .collect()
    }

    pub async fn verify_source(&self, store: &store::Store) -> Result<()> {
        if store.state_hash().await? != self.source {
            return Err(
                "Proposal source changed; capture a fresh base and review the changes".into(),
            );
        }
        Ok(())
    }

    pub async fn bind(&self, engine: &Engine) -> Result<()> {
        for original in &self.records {
            engine
                .refuse_unreadable(self.actor.as_deref(), &original.dependencies())
                .await?;
            if Record::read(&engine.store, &original.uid).await? != *original {
                return Err(format!(
                    "Record {} changed since the proposal was captured",
                    original.uid
                )
                .into());
            }
        }
        for input in self.inputs()? {
            let Event::Action { invocation } = input.event else {
                unreachable!()
            };
            engine
                .authorize_action(&invocation.action, self.actor.as_deref())
                .await?;
            let body_target = match &invocation.action {
                Action::EditRecordText {
                    target,
                    body: Some(_),
                    ..
                } => Some(target),
                Action::ChangeRecord { request }
                    if matches!(&request.mutation, Mutation::Text { .. }) =>
                {
                    Some(&request.record_uid)
                }
                _ => None,
            };
            if let Some(target) = body_target {
                let original = self
                    .records
                    .iter()
                    .find(|record| &record.uid == target)
                    .ok_or("Missing proposal Record")?;
                if nucleus::component::Document::decode(&original.body).is_ok()
                    || nucleus::canvas::Document::decode(&original.body).is_ok()
                {
                    return Err(
                        "Use the composition editor to change an executable document".into(),
                    );
                }
            }
        }
        Ok(())
    }
}
