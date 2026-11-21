//! Load balance inputs once and select each Record's current Promise and Person pairs.

use std::collections::{BTreeMap, HashMap};

use nucleus::execution::now;
use serde_json::to_string;
use sqlx::SqliteConnection;

use super::{
    invalid,
    occurrence::Occurrences,
    promise::{self, Promise},
};
use crate::{
    StoreError,
    transfer_effects::assumptions_on,
    transfer_loans::{self, Adjustment},
};

// Transfer, exchange or source Promise, Person, Record.
type Target = (String, String, String, String);
type Targets<'a> = HashMap<(&'a str, &'a str), Vec<(&'a str, &'a str)>>;
type Key<'a> = (&'a str, &'a str, &'a str);

pub(super) struct Inputs {
    promises: Vec<Promise>,
    mappings: Vec<Target>,
    handoffs: Vec<Target>,
    loans: Vec<Adjustment>,
}

#[derive(Default)]
pub(super) struct Selected<'a> {
    pub(super) promises: BTreeMap<Key<'a>, &'a Promise>,
    pub(super) incomplete: Vec<String>,
}

pub(super) struct Scope<'a> {
    pub(super) records: HashMap<&'a str, Selected<'a>>,
    pub(super) occurrences: Occurrences,
    pub(super) loans: HashMap<&'a str, &'a Adjustment>,
    pub(super) scenario_table: bool,
}

pub(super) fn record_ids(records: &[&str]) -> Result<String, StoreError> {
    to_string(records).map_err(invalid)
}

impl Inputs {
    pub(super) async fn load(
        connection: &mut SqliteConnection,
        ids: &str,
    ) -> Result<Self, StoreError> {
        let mappings = sqlx::query_as(
            "SELECT e.transfer_uid, e.exchange_uid, e.person_uid,
                    COALESCE(json_extract(effect.value, '$.record'), e.record_uid)
             FROM transfer_private_policy p JOIN transfer_private_policy_event e ON e.uid = p.event_uid
             LEFT JOIN json_each(e.effects_json) effect",
        )
        .fetch_all(&mut *connection)
        .await?;
        let handoffs = sqlx::query_as(
            "SELECT DISTINCT h.transfer_uid, h.source_promise_uid, h.participant_person_uid, a.local_record_uid
             FROM transfer_local_application a
             JOIN transfer_application_effect_handoff h ON h.uid = a.handoff_uid
             WHERE a.local_record_uid IN (SELECT value FROM json_each(?))",
        )
        .bind(ids)
        .fetch_all(&mut *connection)
        .await?;
        let promises = promise::load(connection).await?;
        let loans = transfer_loans::adjustments_on(connection, now().timestamp_millis()).await?;
        Ok(Self {
            promises,
            mappings,
            handoffs,
            loans,
        })
    }

    pub(super) async fn scope<'a>(
        &'a self,
        connection: &mut SqliteConnection,
        records: &[&'a str],
    ) -> Result<Scope<'a>, StoreError> {
        let records = self.select(records);
        let exchanges = records
            .values()
            .flat_map(|record| record.promises.values())
            .filter(|p| p.remote_occurrences.is_none() && p.has_commitment())
            .map(|p| (p.transfer.as_str(), p.exchange.as_str()))
            .collect();
        let occurrences = Occurrences::load(connection, exchanges).await?;
        let scenario_table = assumptions_on(connection).await?;
        let loans = self
            .loans
            .iter()
            .map(|loan| (loan.record.as_str(), loan))
            .collect();
        Ok(Scope {
            records,
            occurrences,
            loans,
            scenario_table,
        })
    }

    fn select<'a>(&'a self, records: &[&'a str]) -> HashMap<&'a str, Selected<'a>> {
        let mut records: HashMap<_, _> = records
            .iter()
            .map(|&record| (record, Selected::default()))
            .collect();
        let mut policies = HashMap::<(&str, &str), HashMap<&str, Vec<&str>>>::new();
        for (transfer, exchange, person, target) in &self.mappings {
            policies
                .entry((transfer, exchange))
                .or_default()
                .entry(person)
                .or_default()
                .push(target);
        }
        let mut applied = Targets::new();
        for (transfer, promise, person, target) in &self.handoffs {
            applied
                .entry((transfer, promise))
                .or_default()
                .push((person, target));
        }
        for promise in &self.promises {
            let transfer = promise.transfer.as_str();
            let exchange = promise.exchange.as_str();
            let policy = policies.get(&(transfer, exchange));
            let default = promise
                .record
                .as_deref()
                .map(|target| (promise.owner.as_str(), target));
            let mapped = policy
                .into_iter()
                .flatten()
                .flat_map(|(&person, targets)| targets.iter().map(move |&target| (person, target)));
            let prior = applied
                .get(&(transfer, promise.uid.as_str()))
                .into_iter()
                .flatten()
                .copied();
            for (person, target) in default.into_iter().chain(mapped).chain(prior) {
                let Some(record) = records.get_mut(target) else {
                    continue;
                };
                if !promise.includes(person)
                    || policy
                        .and_then(|policy| policy.get(person))
                        .is_some_and(|targets| !targets.contains(&target))
                {
                    continue;
                }
                let key = (transfer, exchange, person);
                let selected = record.promises.entry(key).or_insert(promise);
                if promise.revision > selected.revision
                    || (selected.owner != person && promise.owner == person)
                {
                    *selected = promise;
                }
            }
        }
        for (transfer, exchange, person, target) in &self.mappings {
            if let Some(record) = records.get_mut(target.as_str())
                && !record.promises.contains_key(&(
                    transfer.as_str(),
                    exchange.as_str(),
                    person.as_str(),
                ))
            {
                record.incomplete.push(format!(
                    "{transfer}: a private commitment has no available current terms"
                ));
            }
        }
        records
    }
}
