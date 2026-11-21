// non-lean-yet
// Scope: impl Engine.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 47.
// - [Clippy probe: clippy::missing_errors_doc] State the observable failure cases for exported
//   functions. Sites 9.
// - [rustc probe: missing_docs] Document the externally visible contracts where applicable. Sites
//   9.

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
        let _resolution =
            nucleus::performance::Span::new("karma-condition-record-resolution-await");
        let mut pending = vec![(condition, 0)];
        let mut records = BTreeSet::new();
        let mut budget = 128usize;
        while let Some((condition, depth)) = pending.pop() {
            if depth >= 4 {
                return Err(crate::karma_preview::invalid("Reading nesting is too deep"));
            }
            for token in condition.reads_ref() {
                budget = budget
                    .checked_sub(1)
                    .ok_or_else(|| invalid("Too many readings"))?;
                if nucleus::transfer::karma::is_reading(&token.func) {
                    self.require_permission(actor, "transfer:read").await?;
                    let mut references = token.slug.split('|');
                    let state = self
                        .cached_transfer_snapshot(references.next().unwrap_or_default(), actor)
                        .await?;
                    if nucleus::transfer::karma::is_agreement_reading(&token.func) {
                        let reference = references
                            .next()
                            .ok_or_else(|| invalid("Agreement Person is unavailable"))?;
                        let person = store::records::resolve_kind(&self.store.pool, reference)
                            .await?
                            .filter(|record| record.kind == "person")
                            .ok_or_else(|| invalid("Agreement Person is unavailable"))?;
                        if !state.participants.contains_key(&person.uid) {
                            return Err(invalid(
                                "This Person's agreement is unavailable or hidden",
                            ));
                        }
                    }
                    records.insert(state.transfer.clone());
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
                if token.func == nucleus::expr::HAS_ASSERTION {
                    let (first, rest) = token
                        .slug
                        .split_once('|')
                        .ok_or_else(|| invalid("Condition concept is unavailable"))?;
                    let mut references = rest.split('|');
                    store::concepts::resolve(
                        &self.store.pool,
                        references.next().unwrap_or_default(),
                    )
                    .await?
                    .ok_or_else(|| invalid("Condition concept is unavailable"))?;
                    for reference in std::iter::once(first).chain(references) {
                        records.insert(self.resolve(reference).await?);
                    }
                    continue;
                }
                if token.func == "freq" {
                    self.require_permission(actor, "frequency:read").await?;
                    records.insert(self.resolve_frequency_uid(&token.slug).await?);
                    continue;
                }
                if token.func == "startup" {
                    let record = store::records::resolve_kind(&self.store.pool, &token.slug)
                        .await?
                        .filter(|record| record.kind == "device")
                        .ok_or_else(|| invalid("startup() needs a Cell Record"))?;
                    records.insert(record.uid);
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
                if token.func == "query_command" {
                    self.require_permission(actor, "organ:update").await?;
                }
                for name in token.slug.split('|') {
                    let record = store::records::resolve_uid(&self.store.pool, name)
                        .await?
                        .ok_or_else(|| invalid("Condition Record is unavailable"))?;
                    if token.func == "value" {
                        let condition = store::recurrence::for_record(&self.store.pool, &record)
                            .await?
                            .into_iter()
                            .filter(|rule| !rule.is_paused())
                            .find_map(|rule| rule.condition)
                            .ok_or_else(|| invalid("Record has no value Rule"))?;
                        pending.push((condition.parsed().map_err(invalid)?, depth + 1));
                    }
                    records.insert(record);
                }
            }
        }
        let records: Vec<_> = records.into_iter().collect();
        self.refuse_unreadable_karma_inputs(actor, &records).await?;
        Ok(records)
    }
}
