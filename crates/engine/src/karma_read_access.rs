use std::collections::BTreeSet;

use nucleus::karma::Condition;

use crate::karma_preview::invalid;
use crate::{Engine, EngineError};

impl Engine {
    pub async fn karma_condition_records(
        &self,
        condition: Condition,
        actor: Option<&str>,
    ) -> Result<Vec<String>, EngineError> {
        let mut pending = vec![(condition, 0)];
        let mut records = BTreeSet::new();
        let mut budget = 128usize;
        while let Some((condition, depth)) = pending.pop() {
            if depth >= 4 {
                return Err(crate::karma_preview::invalid("Reading nesting is too deep"));
            }
            for token in condition.reads() {
                budget = budget
                    .checked_sub(1)
                    .ok_or_else(|| invalid("Too many readings"))?;
                if nucleus::transfer::karma::is_reading(&token.func) {
                    self.require_permission(actor, "transfer:read").await?;
                    let references: Vec<_> = token.slug.split('|').collect();
                    let state = self.cached_transfer_snapshot(references[0], actor).await?;
                    if nucleus::transfer::karma::is_agreement_reading(&token.func) {
                        let person = store::records::resolve(&self.store.pool, references[1]).await?
                            .filter(|record| record.kind == "person")
                            .ok_or_else(|| invalid("Agreement Person is unavailable"))?;
                        if !state.participants.contains_key(&person.uid) {
                            return Err(invalid("This Person's agreement is unavailable or hidden"));
                        }
                    }
                    records.insert(state.transfer);
                    continue;
                }
                if token.func == nucleus::expr::ASSERTION {
                    let concept = store::concepts::resolve(&self.store.pool, &token.slug)
                        .await?
                        .ok_or_else(|| invalid("Condition concept is unavailable"))?;
                    records.extend(
                        store::ledger::records_with_concept(&self.store.pool, &concept).await?,
                    );
                    continue;
                }
                if token.func == "freq" {
                    self.require_permission(actor, "frequency:read").await?;
                    records.insert(self.resolve_frequency_uid(&token.slug).await?);
                    continue;
                }
                if actor.is_some()
                    && nucleus::expr::extension_parts(&token.func).is_none()
                    && !matches!(
                        token.func.as_str(),
                        "quantity"
                            | "signal"
                            | "query_command"
                            | "value"
                            | "sum"
                            | "sum_pos"
                            | "sum_neg"
                            | "hours_since_fact"
                            | "distance"
                            | "promise_state"
                    )
                {
                    return Err(invalid("This reading has no authorized private preview"));
                }
                if token.func == "query_command" { self.require_permission(actor, "organ:update").await?; }
                for name in token.slug.split('|') {
                    let record = store::records::resolve(&self.store.pool, name)
                        .await?
                        .ok_or_else(|| invalid("Condition Record is unavailable"))?;
                    records.insert(record.uid.clone());
                    if token.func == "value" {
                        let rule = store::recurrence::for_record(&self.store.pool, &record.uid)
                            .await?
                            .into_iter()
                            .find(|rule| rule.condition.is_some() && !rule.is_paused())
                            .ok_or_else(|| invalid("Record has no value Rule"))?;
                        pending.push((
                            rule.condition.unwrap().parsed().map_err(invalid)?,
                            depth + 1,
                        ));
                    }
                }
            }
        }
        let records: Vec<_> = records.into_iter().collect();
        self.refuse_unreadable_karma_inputs(actor, &records).await?;
        Ok(records)
    }
}
