use std::collections::BTreeMap;

use nucleus::DecimalValue;
use serde::{Deserialize, Serialize};

pub use nucleus::simulation::sharing::TransferSource;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Incoming,
    Outgoing,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransferAssumption {
    pub key: String,
    pub title: String,
    pub person: String,
    pub record: String,
    pub quantity: DecimalValue,
    pub unit: Option<String>,
    pub direction: Direction,
    #[serde(default)]
    pub source: Option<TransferSource>,
}

#[derive(Clone)]
pub(crate) struct Progress {
    group: String,
    scope: Option<String>,
    canonical: DecimalValue,
    local: DecimalValue,
}

impl Default for Progress {
    fn default() -> Self {
        Self {
            group: String::new(),
            scope: None,
            canonical: store::exact::zero(),
            local: store::exact::zero(),
        }
    }
}

impl Serialize for Progress {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (
            &self.group,
            &self.scope,
            self.canonical.to_string(),
            self.local.to_string(),
        )
            .serialize(serializer)
    }
}

struct Quote {
    key: String,
    scope: Option<String>,
    remaining: Option<DecimalValue>,
    before: DecimalValue,
    local_before: DecimalValue,
    formula: String,
}

async fn quote(store: &store::Store, assumption: &TransferAssumption) -> crate::Result<Quote> {
    let binding = store::transfer_accounting::Binding {
        transfer: assumption
            .source
            .as_ref()
            .map(|value| value.transfer.as_str())
            .unwrap_or_default(),
        exchange: assumption
            .source
            .as_ref()
            .map(|value| value.exchange.as_str())
            .unwrap_or_default(),
        occurrence: assumption
            .source
            .as_ref()
            .and_then(|value| value.occurrence.as_deref()),
        person: &assumption.person,
        record: Some(&assumption.record),
        unit: assumption.unit.as_deref(),
        outgoing: assumption.direction == Direction::Outgoing,
    };
    let formula = store::transfer_accounting::effective(&store.pool, &binding)
        .await?
        .formula;
    let Some(source) = &assumption.source else {
        return Ok(Quote {
            key: format!("draft:{}:{}", assumption.person, assumption.key),
            scope: None,
            remaining: None,
            before: store::exact::zero(),
            local_before: store::exact::zero(),
            formula,
        });
    };
    let rows = protein::execute_for_with_signer(
        store,
        &protein::Protein {
            source: protein::Source::Transfer,
            filter: vec![protein::Predicate::UidEq(source.transfer.clone())],
            fields: None,
            include: Default::default(),
            aggregate: None,
            order: Vec::new(),
            limit: None,
        },
        None,
        Some(&assumption.person),
    )
    .await?;
    let transfer = rows
        .iter()
        .find(|row| row["uid"] == source.transfer)
        .ok_or("Transfer assumptions require a visible proposal on this Organ")?;
    if transfer["revision"].as_u64() != Some(source.revision) {
        return Err("Transfer terms changed after this scenario was prepared".into());
    }
    let promise = transfer["promises"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["uid"] == source.promise))
        .ok_or("Transfer promise is unavailable")?;
    if promise["exchange"] != source.exchange {
        return Err("Transfer exchange changed after this scenario was prepared".into());
    }
    let endpoint = match assumption.direction {
        Direction::Incoming => "receiver",
        Direction::Outgoing => "giver",
    };
    if promise[endpoint] != assumption.person {
        return Err("Transfer assumption does not belong to this Person and direction".into());
    }
    if promise["unit"].as_str() != assumption.unit.as_deref() {
        return Err("Transfer assumption uses a different unit from the proposal".into());
    }
    if source.occurrence.is_none() {
        let paths: std::collections::BTreeSet<_> = transfer["occurrences"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|row| row["promise"] == source.promise)
            .filter_map(|row| row["exchange_path"].as_str().or(row["uid"].as_str()))
            .collect();
        if paths.len() > 1 {
            return Err(
                "Choose individual occurrences when an exchange has several delivery dates".into(),
            );
        }
    }
    let occurrence = if let Some(occurrence) = &source.occurrence {
        Some(
            transfer["occurrences"]
                .as_array()
                .and_then(|rows| {
                    rows.iter()
                        .find(|row| row["uid"] == *occurrence && row["promise"] == source.promise)
                })
                .ok_or("Transfer occurrence is unavailable")?,
        )
    } else {
        None
    };
    let scope = occurrence.map(|row| {
        row["exchange_path"]
            .as_str()
            .or(row["uid"].as_str())
            .unwrap_or_default()
            .to_string()
    });
    let total = if let Some(occurrence) = occurrence {
        occurrence["quantity"]
            .as_f64()
            .ok_or("Transfer quantity is unavailable")?
    } else {
        promise["delta"]
            .as_f64()
            .ok_or("Transfer quantity is unavailable")?
            .abs()
    };
    let cancelled = occurrence
        .or_else(|| {
            transfer["occurrences"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|row| row["promise"] == source.promise)
        })
        .and_then(|row| row["settlement_progress"]["cancelled_quantity_exact"].as_str())
        .map(nucleus::DecimalValue::parse_inferred)
        .transpose()?
        .unwrap_or_else(store::exact::zero);
    let total =
        store::exact::difference(nucleus::transfer::application::amount(total)?, cancelled)?;
    let promises: Vec<_> = transfer["promises"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["exchange"] == source.exchange && row[endpoint] == assumption.person)
        .filter_map(|row| row["uid"].as_str())
        .collect();
    let promises = serde_json::to_string(&promises)?;
    let occurrences: Vec<_> = transfer["occurrences"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| {
            scope.as_deref().is_some_and(|scope| {
                row["exchange_path"].as_str().or(row["uid"].as_str()) == Some(scope)
            })
        })
        .filter_map(|row| row["uid"].as_str())
        .collect();
    let occurrences = serde_json::to_string(&occurrences)?;
    if matches!(
        promise["state"].as_str(),
        Some("withdrawn" | "broken" | "expired")
    ) {
        return Err("Transfer obligation is no longer available in this snapshot".into());
    }
    let rows = store::sqlx::query(
        "SELECT canonical, delta_mantissa, delta_scale FROM (
            SELECT s.canonical_quantity AS canonical, f.delta_mantissa, f.delta_scale
            FROM transfer_occurrence_settlement_slice s JOIN fact f ON f.uid = s.application_fact_uid
            WHERE s.transfer_uid = ? AND s.promise_uid IN (SELECT value FROM json_each(?)) AND s.owner_person_uid = ?
              AND (? IS NULL OR s.occurrence_uid IN (SELECT value FROM json_each(?)))
            UNION ALL
            SELECT h.canonical_quantity, f.delta_mantissa, f.delta_scale FROM transfer_local_application a
            JOIN transfer_application_effect_handoff h ON h.uid = a.handoff_uid JOIN fact f ON f.uid = a.application_fact_uid
            WHERE h.transfer_uid = ? AND h.source_promise_uid IN (SELECT value FROM json_each(?)) AND h.participant_person_uid = ?
              AND (? IS NULL OR h.occurrence_uid IN (SELECT value FROM json_each(?))))",
    )
    .bind(&source.transfer)
    .bind(&promises)
    .bind(&assumption.person)
    .bind(&scope)
    .bind(&occurrences)
    .bind(&source.transfer)
    .bind(&promises)
    .bind(&assumption.person)
    .bind(&scope)
    .bind(&occurrences)
    .fetch_all(&store.pool)
    .await?;
    use store::sqlx::Row;
    let before = store::exact::sum_exact(
        rows.iter()
            .map(|row| nucleus::transfer::application::amount(row.get("canonical")))
            .collect::<Result<Vec<_>, _>>()?,
    )?;
    let local_before = store::exact::sum_exact(
        rows.iter()
            .map(|row| store::exact::read_decimal(row, "delta"))
            .collect::<Result<Vec<_>, _>>()?,
    )?;
    Ok(Quote {
        key: format!(
            "{}:{}:{}",
            source.transfer, source.exchange, assumption.person
        ),
        scope,
        remaining: Some(store::exact::difference(total, before)?),
        before,
        local_before,
        formula,
    })
}

impl crate::world::World {
    pub(crate) async fn assume_transfer(
        &mut self,
        cell: &str,
        id: &str,
        mut assumption: TransferAssumption,
        cause: nucleus::simulation::Cause,
    ) -> crate::Result<()> {
        assumption.person = self.resolve_reference(&assumption.person);
        assumption.record = self.resolve_reference(&assumption.record);
        assumption.unit = assumption
            .unit
            .as_ref()
            .map(|unit| self.resolve_reference(unit));
        if let Some(source) = &mut assumption.source {
            source.transfer = self.resolve_reference(&source.transfer);
            source.promise = self.resolve_reference(&source.promise);
            source.occurrence = source
                .occurrence
                .as_ref()
                .map(|value| self.resolve_reference(value));
        }
        let node = &self.nodes[cell];
        node.set_time(self.now_ms)?;
        let store = &node.engine().store;
        let organ = store::organs::local(&store.pool)
            .await?
            .ok_or("missing Organ")?;
        let person = store::records::resolve(&store.pool, &assumption.person)
            .await?
            .ok_or("missing Person")?;
        let record = store::records::resolve(&store.pool, &assumption.record)
            .await?
            .ok_or("missing private Record binding")?;
        if person.kind != "person"
            || person.organ_uid.as_deref() != Some(&organ.uid)
            || record.organ_uid.as_deref() != Some(&organ.uid)
        {
            return Err("Transfer assumptions may change only the owner's local Records".into());
        }
        assumption.person = person.uid;
        assumption.record = record.uid.clone();
        let quote = node.execution.scope(quote(store, &assumption)).await?;
        let group = format!("{cell}:{}", quote.key);
        if self.assumed_transfers.values().any(|previous| {
            previous.group == group && previous.scope.is_some() != quote.scope.is_some()
        }) {
            return Err(
                "Choose either individual occurrences or the whole exchange for its assumptions"
                    .into(),
            );
        }
        let key = format!("{group}:{}", quote.scope.as_deref().unwrap_or("all"));
        let previous = self
            .assumed_transfers
            .get(&key)
            .cloned()
            .unwrap_or_default();
        let canonical = assumption.quantity;
        let assumed = store::exact::sum_exact([previous.canonical, canonical])?;
        if canonical.mantissa() <= 0
            || quote.remaining.is_some_and(|remaining| {
                assumed.exact_numeric_cmp(remaining) == std::cmp::Ordering::Greater
            })
        {
            return Err("Transfer assumption exceeds the remaining amount in this snapshot".into());
        }
        let after = store::exact::sum_exact([quote.before, assumed])?;
        let prior = store::exact::sum_exact([quote.local_before, previous.local])?;
        let (delta, _) = store::transfer_accounting::calculate(&quote.formula, after, prior)?;
        let effects = node
            .execution
            .scope(store::transfer_effects::quote(
                &store.pool,
                &store::transfer_accounting::Binding {
                    transfer: assumption
                        .source
                        .as_ref()
                        .map(|source| source.transfer.as_str())
                        .unwrap_or_default(),
                    exchange: assumption
                        .source
                        .as_ref()
                        .map(|source| source.exchange.as_str())
                        .unwrap_or_default(),
                    occurrence: assumption
                        .source
                        .as_ref()
                        .and_then(|source| source.occurrence.as_deref()),
                    person: &assumption.person,
                    record: Some(&assumption.record),
                    unit: assumption.unit.as_deref(),
                    outgoing: assumption.direction == Direction::Outgoing,
                },
                after,
            ))
            .await?;
        let changes: BTreeMap<String, DecimalValue> = effects
            .as_ref()
            .map(|group| {
                group
                    .effects
                    .iter()
                    .map(|effect| (effect.record.clone(), effect.delta))
                    .collect()
            })
            .unwrap_or_else(|| BTreeMap::from([(record.uid, delta)]));
        let delta = changes[&assumption.record];
        let pool = store.pool.clone();
        if let Some(source) = &assumption.source {
            store::sqlx::query("CREATE TABLE IF NOT EXISTS simulation_assumed_transfer_effect (uid TEXT PRIMARY KEY, input_uid TEXT NOT NULL, transfer_uid TEXT NOT NULL, exchange_uid TEXT NOT NULL, occurrence_uid TEXT, person_uid TEXT NOT NULL, record_uid TEXT NOT NULL, unit_uid TEXT, mode TEXT NOT NULL, cumulative_mantissa TEXT NOT NULL, cumulative_scale INTEGER NOT NULL, canonical_mantissa TEXT NOT NULL, canonical_scale INTEGER NOT NULL, local_mantissa TEXT NOT NULL, local_scale INTEGER NOT NULL) STRICT").execute(&pool).await?;
            let mut tx = pool.begin().await?;
            for (record, delta) in &changes {
                let (cumulative_mantissa, cumulative_scale) = store::exact::decimal_columns(after);
                let (canonical_mantissa, canonical_scale) =
                    store::exact::decimal_columns(canonical);
                let (local_mantissa, local_scale) = store::exact::decimal_columns(*delta);
                store::sqlx::query("INSERT INTO simulation_assumed_transfer_effect (uid, input_uid, transfer_uid, exchange_uid, occurrence_uid, person_uid, record_uid, unit_uid, mode, cumulative_mantissa, cumulative_scale, canonical_mantissa, canonical_scale, local_mantissa, local_scale) VALUES (?, ?, ?, ?, ?, ?, ?, (SELECT unit_uid FROM record WHERE uid = ?), ?, ?, ?, ?, ?, ?, ?)")
                .bind(format!("{id}:{record}")).bind(id).bind(&source.transfer).bind(&source.exchange).bind(&source.occurrence).bind(&assumption.person).bind(record).bind(record).bind(if effects.as_ref().is_some_and(|group|group.effects.iter().any(|effect|effect.record == *record && effect.mode == nucleus::transfer::application::EffectMode::Fulfilment)) {"fulfilment"} else {"quantity"})
                .bind(cumulative_mantissa).bind(cumulative_scale).bind(canonical_mantissa).bind(canonical_scale).bind(local_mantissa).bind(local_scale).execute(&mut *tx).await?;
            }
            tx.commit().await?;
        }
        let before_trace = self.trace.len();
        let invoked = self
            .invoke(
                cell,
                &crate::scenario::Invocation {
                    id: id.into(),
                    actor: None,
                    action: engine::actions::Action::AddQuantityGroupExact { changes },
                },
                cause,
            )
            .await;
        if invoked.is_err()
            || !self.trace[before_trace..].iter().any(|event| {
                matches!(&event.observation,
            nucleus::simulation::Observation::ActionAccepted { input, .. } if input == id)
            })
        {
            if assumption.source.is_some() {
                store::sqlx::query(
                    "DELETE FROM simulation_assumed_transfer_effect WHERE input_uid = ?",
                )
                .bind(id)
                .execute(&pool)
                .await?;
            }
            invoked?;
            return Err("Transfer assumption was refused by the ordinary Record action".into());
        }
        self.assumed_transfers.insert(
            key,
            Progress {
                group,
                scope: quote.scope,
                canonical: assumed,
                local: store::exact::sum_exact([previous.local, delta])?,
            },
        );
        Ok(())
    }
}

pub(crate) type Transfers = BTreeMap<String, Progress>;
