use std::collections::{BTreeMap, HashMap};

use serde_json::{Value, json};
use store::transfers::{OccurrenceSettlementProgress, TransferOccurrenceRow};

#[derive(Default)]
pub(super) struct Totals<'a> {
    resources: BTreeMap<(Option<&'a str>, Option<&'a str>), Resource>,
    pub statuses: BTreeMap<&'static str, usize>,
}

#[derive(Default)]
struct Resource {
    canonical: f64,
    settled: f64,
    remaining: f64,
    cancelled: f64,
}

impl<'a> Totals<'a> {
    pub(super) fn add(
        &mut self,
        occurrence: &'a TransferOccurrenceRow,
        progress: &OccurrenceSettlementProgress,
    ) -> &'static str {
        let resource = self
            .resources
            .entry((
                occurrence.concept_uid.as_deref(),
                occurrence.unit_uid.as_deref(),
            ))
            .or_default();
        resource.canonical += progress.canonical_quantity;
        resource.settled += progress.settled_quantity;
        resource.remaining += progress.remaining_quantity;
        resource.cancelled += progress.cancelled_quantity;
        let status = status(occurrence, progress);
        *self.statuses.entry(status).or_default() += 1;
        status
    }

    pub(super) fn by_resource(&self, names: &HashMap<String, String>) -> Vec<Value> {
        self.resources
            .iter()
            .map(|(&(concept, unit), resource)| {
                json!({
                    "concept": concept,
                    "concept_name": concept.and_then(|uid| names.get(uid)),
                    "unit": unit,
                    "unit_name": unit.and_then(|uid| names.get(uid)),
                    "canonical_quantity": resource.canonical,
                    "settled_quantity": resource.settled,
                    "remaining_quantity": resource.remaining,
                    "cancelled_quantity": resource.cancelled,
                })
            })
            .collect()
    }
}

fn status(
    occurrence: &TransferOccurrenceRow,
    progress: &OccurrenceSettlementProgress,
) -> &'static str {
    if occurrence.system_disputed {
        "system_disputed"
    } else if occurrence.disputed {
        "disputed"
    } else if progress.cancelled_exact.is_positive() && progress.remaining_exact.is_zero() {
        "cancelled"
    } else if progress.settled {
        "settled"
    } else if progress.partially_settled {
        "partially_settled"
    } else if occurrence.delivery_claimed && occurrence.receipt_claimed {
        "confirmed"
    } else {
        "active"
    }
}
