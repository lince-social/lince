use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::karma::TypedUid;
use crate::simulation::{FactId, Quantity, RuleOccurrence};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CycleKind {
    Feedback,
    SettledFeedback,
    TimedRecurrence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuleChange {
    pub record: TypedUid,
    pub fact: FactId,
    pub before: Quantity,
    pub after: Quantity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransferChange {
    pub cell: String,
    pub at_ms: i64,
    pub virtual_ms: i64,
    pub before: crate::transfer::karma::Snapshot,
    pub after: crate::transfer::karma::Snapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuleStep {
    pub cell: String,
    pub occurrence: RuleOccurrence,
    pub changes: Vec<RuleChange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transfer_changes: Vec<TransferChange>,
    pub consequences: Vec<String>,
    pub at_ms: i64,
    pub virtual_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuleCycle {
    pub id: String,
    pub kind: CycleKind,
    pub cell: String,
    pub rules: Vec<String>,
    pub steps: Vec<RuleStep>,
    pub truncated: bool,
    pub repetitions: u64,
}

type Application = (String, String, String, u64);

#[derive(Debug)]
struct Link {
    step: RuleStep,
    parent: Option<usize>,
    depth: usize,
    jumps: Vec<usize>,
}

#[derive(Debug, Default)]
pub struct Trace {
    links: Vec<Link>,
    facts: BTreeMap<String, usize>,
    aliases: BTreeMap<(String, String), usize>,
    applications: BTreeMap<Application, usize>,
    rules: BTreeMap<(String, String), Vec<usize>>,
    frequencies: BTreeMap<(String, String, String), usize>,
    cycles: Vec<RuleCycle>,
    seen: BTreeMap<(String, bool, Vec<String>), usize>,
    changed: BTreeSet<usize>,
    bytes: u64,
}

impl Trace {
    fn transfer_change(
        &mut self,
        index: usize,
        change: TransferChange,
        limit: u64,
    ) -> Result<(), ()> {
        if change.before == change.after {
            return Ok(());
        }
        let step = &mut self.links[index].step;
        let size = serde_json::to_vec(&change).map_err(|_| ())?.len() as u64;
        if self.bytes.saturating_add(size) > limit {
            return Err(());
        }
        self.bytes += size;
        step.transfer_changes.push(change.clone());
        for (index, cycle) in self.cycles.iter_mut().enumerate() {
            for selected in &mut cycle.steps {
                if selected.cell == step.cell && selected.occurrence == step.occurrence {
                    selected.transfer_changes.push(change.clone());
                    self.changed.insert(index);
                }
            }
        }
        Ok(())
    }

    pub fn record_transfer_change(
        &mut self,
        cell: &str,
        occurrence: &RuleOccurrence,
        change: TransferChange,
        limit: u64,
    ) -> Result<(), ()> {
        let key = (
            cell.to_owned(),
            occurrence.rule_uid.clone(),
            occurrence.event_id.clone(),
            occurrence.revision,
        );
        let Some(index) = self.applications.get(&key).copied() else {
            return Ok(());
        };
        self.transfer_change(index, change, limit)
    }

    pub fn record_received_transfer_change(
        &mut self,
        command: &str,
        change: TransferChange,
        limit: u64,
    ) -> Result<(), ()> {
        let Some(index) = self.facts.get(command).copied() else {
            return Ok(());
        };
        self.transfer_change(index, change, limit)
    }
    fn ancestor(&self, mut descendant: usize, candidate: usize) -> bool {
        let Some(mut distance) = self.links[descendant]
            .depth
            .checked_sub(self.links[candidate].depth)
        else {
            return false;
        };
        let mut bit = 0;
        while distance != 0 {
            if distance & 1 == 1 {
                let Some(parent) = self.links[descendant].jumps.get(bit) else {
                    return false;
                };
                descendant = *parent;
            }
            bit += 1;
            distance >>= 1;
        }
        descendant == candidate
    }

    pub fn link_effect(&mut self, cell: &str, fact: &str, occurrence: &RuleOccurrence) {
        let key = (
            cell.to_owned(),
            occurrence.rule_uid.clone(),
            occurrence.event_id.clone(),
            occurrence.revision,
        );
        if let Some(index) = self.applications.get(&key) {
            self.aliases
                .insert((cell.to_owned(), fact.to_owned()), *index);
            self.facts.entry(fact.to_owned()).or_insert(*index);
        }
    }

    pub fn link_received(&mut self, cell: &str, fact: &str, original: &str) {
        if let Some(index) = self.facts.get(original).copied() {
            self.aliases
                .insert((cell.to_owned(), fact.to_owned()), index);
            self.facts.entry(fact.to_owned()).or_insert(index);
        }
    }

    pub fn append(&mut self, step: RuleStep, byte_limit: u64) -> Result<Option<RuleCycle>, ()> {
        let application = (
            step.cell.clone(),
            step.occurrence.rule_uid.clone(),
            step.occurrence.event_id.clone(),
            step.occurrence.revision,
        );
        if self.applications.contains_key(&application) {
            return Ok(None);
        }
        self.bytes = self
            .bytes
            .saturating_add(serde_json::to_vec(&step).map_err(|_| ())?.len() as u64);
        if self.bytes > byte_limit {
            return Err(());
        }
        let parent = self
            .aliases
            .get(&(step.cell.clone(), step.occurrence.event_id.clone()))
            .or_else(|| self.facts.get(&step.occurrence.event_id))
            .copied();
        let key = (step.cell.clone(), step.occurrence.rule_uid.clone());
        let previous = parent.and_then(|parent| {
            self.rules
                .get(&key)
                .into_iter()
                .flatten()
                .rev()
                .copied()
                .find(|candidate| self.ancestor(parent, *candidate))
        });
        let timed = step.occurrence.frequency.as_ref().and_then(|frequency| {
            let key = (
                step.cell.clone(),
                frequency.as_str().to_owned(),
                step.occurrence.rule_uid.clone(),
            );
            let previous = self.frequencies.insert(key, self.links.len());
            previous.filter(|index| {
                let previous = &self.links[*index].step;
                self.links[*index].step.occurrence.event_id != step.occurrence.event_id
                    && previous.occurrence.intended_at_ms.unwrap_or(previous.at_ms)
                        < step.occurrence.intended_at_ms.unwrap_or(step.at_ms)
            })
        });
        let depth = parent.map_or(0, |index| self.links[index].depth + 1);
        let mut jumps = Vec::new();
        if let Some(parent) = parent {
            jumps.push(parent);
            let mut bit = 0;
            while let Some(ancestor) = self.links[*jumps.last().unwrap()].jumps.get(bit).copied() {
                jumps.push(ancestor);
                bit += 1;
            }
        }
        let index = self.links.len();
        for change in &step.changes {
            self.facts.insert(String::from(change.fact.clone()), index);
        }
        self.links.push(Link {
            step,
            parent,
            depth,
            jumps,
        });
        self.applications.insert(application, index);
        self.rules.entry(key).or_default().push(index);
        let (previous, kind) = if let Some(previous) = previous {
            (previous, CycleKind::Feedback)
        } else if let Some(previous) = timed {
            (previous, CycleKind::TimedRecurrence)
        } else {
            return Ok(None);
        };
        let mut steps = Vec::new();
        let mut rules = BTreeSet::new();
        let mut cursor = Some(index);
        let mut length = 0;
        while let Some(cursor_index) = cursor {
            let link = &self.links[cursor_index];
            rules.insert(link.step.occurrence.rule_uid.clone());
            if steps.len() < 64 || cursor_index == previous {
                steps.push(link.step.clone());
            }
            length += 1;
            if cursor_index == previous {
                break;
            }
            cursor = if kind == CycleKind::TimedRecurrence {
                Some(previous)
            } else {
                link.parent
            };
        }
        steps.reverse();
        let rules: Vec<_> = rules.into_iter().collect();
        let key = (
            self.links[index].step.cell.clone(),
            kind == CycleKind::TimedRecurrence,
            rules.clone(),
        );
        if let Some(index) = self.seen.get(&key).copied() {
            let cycle = &mut self.cycles[index];
            cycle.repetitions = cycle.repetitions.saturating_add(1);
            self.changed.insert(index);
            if cycle.kind != kind {
                cycle.kind = kind;
                self.changed.insert(index);
            }
            return Ok(Some(cycle.clone()));
        }
        let cycle = RuleCycle {
            id: format!("cycle:{}", self.cycles.len() + 1),
            kind,
            cell: self.links[index].step.cell.clone(),
            rules,
            truncated: length > steps.len(),
            steps,
            repetitions: 1,
        };
        self.seen.insert(key, self.cycles.len());
        self.changed.insert(self.cycles.len());
        self.cycles.push(cycle.clone());
        Ok(Some(cycle))
    }

    pub fn settle(&mut self, cell: &str) {
        for (index, cycle) in self.cycles.iter_mut().enumerate() {
            if cycle.cell == cell && cycle.kind == CycleKind::Feedback {
                cycle.kind = CycleKind::SettledFeedback;
                self.changed.insert(index);
            }
        }
    }

    pub fn cycles(&self) -> Vec<RuleCycle> {
        self.cycles.clone()
    }

    pub fn drain(&mut self, cell: &str) -> Vec<RuleCycle> {
        let selected: Vec<_> = self
            .changed
            .iter()
            .copied()
            .filter(|index| self.cycles[*index].cell == cell)
            .collect();
        selected
            .into_iter()
            .map(|index| {
                self.changed.remove(&index);
                self.cycles[index].clone()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::karma::{DecimalValue, ReferenceKind};

    fn step(rule: &str, event: &str, value: &str) -> RuleStep {
        RuleStep {
            cell: "a".into(),
            occurrence: RuleOccurrence {
                rule_uid: rule.into(),
                revision: 1,
                event_id: event.into(),
                frequency: None,
                intended_at_ms: Some(1000),
            },
            changes: vec![RuleChange {
                record: TypedUid::new(ReferenceKind::Record, crate::new_uid("r")).unwrap(),
                fact: crate::new_uid("f").try_into().unwrap(),
                before: Quantity {
                    value: DecimalValue::parse_inferred("0").unwrap(),
                    unit: None,
                },
                after: Quantity {
                    value: DecimalValue::parse_inferred(value).unwrap(),
                    unit: None,
                },
            }],
            transfer_changes: Vec::new(),
            consequences: vec!["set-quantity".into()],
            at_ms: 1000,
            virtual_ms: 1000,
        }
    }

    #[test]
    fn rising_values_expose_the_actual_feedback_and_a_finished_chain_settles() {
        let mut trace = Trace::default();
        let first = step("A", "input", "1");
        let fact = String::from(first.changes[0].fact.clone());
        trace.append(first, 65536).unwrap();
        let second = step("B", &fact, "2");
        let fact = String::from(second.changes[0].fact.clone());
        trace.append(second, 65536).unwrap();
        let cycle = trace.append(step("A", &fact, "3"), 65536).unwrap().unwrap();
        assert_eq!(cycle.kind, CycleKind::Feedback);
        assert_eq!(cycle.rules, ["A", "B"]);
        assert_eq!(cycle.steps.len(), 3);
        assert_eq!(cycle.steps[2].changes[0].after.value.to_string(), "3");
        trace.settle("a");
        assert_eq!(trace.drain("a")[0].kind, CycleKind::SettledFeedback);
    }

    #[test]
    fn sibling_branches_do_not_claim_a_cycle_between_each_other() {
        let mut trace = Trace::default();
        let mut first = step("A", "input", "1");
        let fact = String::from(first.changes[0].fact.clone());
        let mut sibling = first.changes[0].clone();
        sibling.fact = crate::new_uid("f").try_into().unwrap();
        let sibling_fact = String::from(sibling.fact.clone());
        first.changes.push(sibling);
        trace.append(first, 65536).unwrap();
        trace.append(step("B", &fact, "2"), 65536).unwrap();
        assert!(
            trace
                .append(step("B", &sibling_fact, "3"), 65536)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn scheduled_occurrences_have_a_separate_recurrence_label() {
        let mut trace = Trace::default();
        let frequency = TypedUid::new(ReferenceKind::Frequency, crate::new_uid("r")).unwrap();
        let mut first = step("A", "day1", "-1");
        first.occurrence.frequency = Some(frequency.clone());
        trace.append(first, 65536).unwrap();
        let mut second = step("A", "day2", "-1");
        second.occurrence.frequency = Some(frequency);
        second.at_ms = 2000;
        second.occurrence.intended_at_ms = Some(2000);
        second.virtual_ms = 2000;
        assert_eq!(
            trace.append(second, 65536).unwrap().unwrap().kind,
            CycleKind::TimedRecurrence
        );
    }

    #[test]
    fn queued_actions_and_received_facts_keep_their_applied_rule_ancestry() {
        let mut trace = Trace::default();
        let mut first = step("A", "input", "1");
        first.changes.clear();
        first.consequences = vec!["run-action".into()];
        let occurrence = first.occurrence.clone();
        trace.append(first, 65536).unwrap();
        trace.link_effect("a", "effect-fact", &occurrence);
        trace.link_received("b", "received-in-b", "effect-fact");
        let mut second = step("B", "received-in-b", "2");
        second.cell = "b".into();
        let fact = String::from(second.changes[0].fact.clone());
        trace.append(second, 65536).unwrap();
        trace.link_received("a", "received-in-a", &fact);
        let cycle = trace
            .append(step("A", "received-in-a", "3"), 65536)
            .unwrap()
            .unwrap();
        assert_eq!(cycle.rules, ["A", "B"]);
        assert_eq!(
            cycle
                .steps
                .iter()
                .map(|step| step.cell.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "a"]
        );
    }

    #[test]
    fn missed_occurrences_are_recurrence_even_when_observed_together() {
        let mut trace = Trace::default();
        let frequency = TypedUid::new(ReferenceKind::Frequency, crate::new_uid("r")).unwrap();
        let mut first = step("A", "day1", "-1");
        first.occurrence.frequency = Some(frequency.clone());
        first.at_ms = 3000;
        first.virtual_ms = 3000;
        trace.append(first, 65536).unwrap();
        let mut second = step("A", "day2", "-1");
        second.occurrence.frequency = Some(frequency);
        second.occurrence.intended_at_ms = Some(2000);
        second.at_ms = 3000;
        second.virtual_ms = 3000;
        assert_eq!(
            trace.append(second, 65536).unwrap().unwrap().kind,
            CycleKind::TimedRecurrence
        );
    }

    #[test]
    fn committed_transfer_changes_update_existing_cycle_samples_and_enforce_the_budget() {
        let mut trace = Trace::default();
        let first = step("A", "input", "1");
        let occurrence = first.occurrence.clone();
        let fact = String::from(first.changes[0].fact.clone());
        trace.append(first, 65536).unwrap();
        trace.link_effect("a", "remote-command", &occurrence);
        let second = step("A", &fact, "2");
        trace.append(second, 65536).unwrap().unwrap();
        trace.drain("a");
        let before = crate::transfer::karma::Snapshot {
            transfer: crate::new_uid("r"),
            revision: 1,
            active: false,
            published: false,
            ready: false,
            participants: BTreeMap::new(),
        };
        let mut after = before.clone();
        after.published = true;
        let change = TransferChange {
            cell: "b".into(),
            at_ms: 1010,
            virtual_ms: 1010,
            before,
            after,
        };
        let bytes = trace.bytes;
        assert!(
            trace
                .record_received_transfer_change("remote-command", change.clone(), bytes)
                .is_err()
        );
        assert!(trace.drain("a").is_empty());
        trace
            .record_received_transfer_change("remote-command", change.clone(), 65536)
            .unwrap();
        let cycles = trace.drain("a");
        assert_eq!(cycles[0].steps[0].transfer_changes, [change.clone()]);
        let mut unchanged = change;
        unchanged.after = unchanged.before.clone();
        trace
            .record_transfer_change("a", &occurrence, unchanged, 0)
            .unwrap();
        assert!(trace.drain("a").is_empty());
    }

    #[test]
    fn a_received_change_keeps_its_origin_through_a_second_delivery() {
        let mut trace = Trace::default();
        let first = step("A", "input", "1");
        let occurrence = first.occurrence.clone();
        trace.append(first, 65536).unwrap();
        trace.link_effect("a", "command", &occurrence);
        trace.link_received("b", "signed-change", "command");
        trace.link_received("a", "snapshot", "signed-change");
        let cycle = trace
            .append(step("A", "snapshot", "2"), 65536)
            .unwrap()
            .unwrap();
        assert_eq!(cycle.kind, CycleKind::Feedback);
        assert_eq!(cycle.rules, ["A"]);
    }
}
