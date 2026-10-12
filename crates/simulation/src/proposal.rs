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
        let row = store::records::resolve(&store.pool, reference)
            .await?
            .ok_or("Proposal Record is missing or deleted")?;
        let revision = store::record_revisions::get_on(&mut *store.pool.acquire().await?, &row.uid)
            .await?
            .revision;
        let mut assertions: Vec<_> =
            store::assertions::for_subjects(&store.pool, &[row.uid.clone()])
                .await?
                .into_iter()
                .map(|assertion| Assertion {
                    uid: assertion.uid,
                    predicate: assertion.predicate_uid,
                    object: assertion.object_uid,
                    quantity: assertion.quantity,
                    unit: assertion.unit_uid,
                    role: assertion.role,
                })
                .collect();
        assertions.sort_by(|left, right| left.uid.cmp(&right.uid));
        Ok(Self {
            work: store::records::get_extension(&store.pool, &row.uid, "work")
                .await?
                .unwrap_or_else(|| serde_json::json!({})),
            uid: row.uid,
            revision,
            kind: row.kind,
            head: row.head,
            body: row.body,
            quantity: row.quantity,
            unit: row.unit_uid,
            slug: row.slug,
            assertions,
        })
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Occurrence {
    pub rule: String,
    pub event: String,
    pub time: nucleus::schedule::TimeRange,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub cell: String,
    pub record: String,
    pub state: Option<Record>,
    pub occurrences: Vec<Occurrence>,
}

impl Outcome {
    pub async fn read(
        store: &store::Store,
        cell: &str,
        uid: &str,
        from_ms: i64,
        until_ms: i64,
    ) -> Result<Self> {
        use store::sqlx::Row;
        let state = if store::records::resolve(&store.pool, uid).await?.is_some() {
            Some(Record::read(store, uid).await?)
        } else {
            None
        };
        let work = state
            .as_ref()
            .map(|record| record.work.clone())
            .unwrap_or(Value::Null);
        let rows = store::sqlx::query("SELECT a.rule_uid, a.event_id, a.intended_at FROM karma_rule_application a JOIN recurrence r ON r.uid = a.rule_uid WHERE r.record_uid = ? AND a.status = 'applied' ORDER BY a.intended_at, a.rule_uid, a.event_id")
            .bind(uid).fetch_all(&store.pool).await?;
        let mut occurrences = Vec::new();
        for row in rows {
            let time = chrono::DateTime::parse_from_rfc3339(row.get::<&str, _>("intended_at"))?
                .timestamp_millis();
            if time < from_ms || time > until_ms {
                continue;
            }
            if let Some(time) = nucleus::schedule::range(
                work["start"].as_str(),
                work["due"].as_str(),
                work["estimate_min"].as_f64(),
                Some(time),
            )? {
                occurrences.push(Occurrence {
                    rule: row.get("rule_uid"),
                    event: row.get("event_id"),
                    time,
                });
            }
        }
        Ok(Self {
            cell: cell.into(),
            record: uid.into(),
            state,
            occurrences,
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
        for reference in references {
            let record = Record::read(source, reference).await?;
            live.refuse_unreadable(actor, &record.dependencies())
                .await?;
            if !records
                .iter()
                .any(|existing: &Record| existing.uid == record.uid)
            {
                records.push(record);
            }
        }
        if records.len() > 32 {
            return Err("Choose at most 32 distinct Records for the proposal".into());
        }
        Ok(Self {
            id: nucleus::new_uid("p"),
            cell: cell.into(),
            source: source.state_hash().await?,
            base_ms,
            actor: actor.map(str::to_owned),
            records,
            changes: Vec::new(),
        })
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
            || serde_json::to_vec(self)?.len() > 1024 * 1024
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
