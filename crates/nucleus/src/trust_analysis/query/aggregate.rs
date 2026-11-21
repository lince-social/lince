//! Accumulate exact quantities, weighted indices, and caps by measurement basis.

use std::collections::{BTreeMap, BTreeSet};

use super::{Analysis, Group, Query};
use crate::{DecimalValue, karma::Rounding};

const ZERO: DecimalValue = DecimalValue::from_integer(0);

fn multiply(left: DecimalValue, right: DecimalValue) -> Option<(DecimalValue, bool)> {
    let scale = (left.scale() + right.scale()).min(18);
    let product = left.mul_exact(right, scale, Rounding::HalfEven)?;
    Some((product.value, !product.exact))
}

fn capped(
    value: DecimalValue,
    used: DecimalValue,
    cap: Option<DecimalValue>,
) -> Option<DecimalValue> {
    Some(if let Some(cap) = cap {
        let remaining = cap.aligned_sub(used)?;
        if value.exact_numeric_cmp(remaining).is_gt() {
            remaining
        } else {
            value
        }
    } else {
        value
    })
}

pub(super) fn aggregate(analysis: &mut Analysis, query: &Query) -> Result<(), String> {
    let mut groups = BTreeMap::<_, Vec<_>>::new();
    for row in analysis.rows.iter_mut().filter(|row| row.selected) {
        groups
            .entry((row.measurement.id()?, row.level))
            .or_default()
            .push(row);
    }
    let mut issuers = BTreeMap::new();
    let mut contacts = BTreeMap::new();
    let mut basis = None;
    for ((measurement, level), rows) in groups {
        if basis.as_ref() != Some(&measurement) {
            issuers.clear();
            contacts.clear();
            basis = Some(measurement);
        }
        let Some(first) = rows.first() else { continue };
        let mut group = Group {
            measurement: first.measurement.clone(),
            level,
            raw_quantity: Some(ZERO),
            weighted_index: Some(ZERO),
            occurrences: 0,
            issuers: Vec::new(),
            counterparties: Vec::new(),
            index_rounded: false,
            capped: false,
            arithmetic_error: false,
        };
        let mut sources = BTreeSet::new();
        let mut issuer_set = BTreeSet::new();
        let mut counterparties = BTreeSet::new();
        for row in rows {
            group.raw_quantity = group
                .raw_quantity
                .and_then(|sum| sum.aligned_add(row.quantity));
            let mut weighted = multiply(row.quantity, query.weights.evidence(level));
            for _ in 1..row.distance.unwrap_or(1) {
                weighted = weighted.and_then(|(value, rounded)| {
                    multiply(value, query.weights.distance_decay)
                        .map(|(next, changed)| (next, rounded || changed))
                });
            }
            let computed = weighted.and_then(|(value, rounded)| {
                let issuer_used = *issuers.get(&row.issuer).unwrap_or(&ZERO);
                let retained = capped(value, issuer_used, query.issuer_cap)?;
                let first = row
                    .path
                    .as_ref()
                    .and_then(|path| path.identities.get(1))
                    .unwrap_or(&row.counterparty);
                let contact_used = *contacts.get(first).unwrap_or(&ZERO);
                let retained = capped(retained, contact_used, query.first_contact_cap)?;
                let next_issuer = if query.issuer_cap.is_some() {
                    Some(issuer_used.aligned_add(retained)?)
                } else {
                    None
                };
                let next_contact = if query.first_contact_cap.is_some() {
                    Some(contact_used.aligned_add(retained)?)
                } else {
                    None
                };
                if let Some(next) = next_issuer {
                    issuers.insert(&row.issuer, next);
                }
                if let Some(next) = next_contact {
                    contacts.insert(first, next);
                }
                Some((retained, rounded, retained.exact_numeric_cmp(value).is_lt()))
            });
            if let Some((value, rounded, capped)) = computed {
                row.weighted_index = Some(value);
                row.index_rounded = rounded;
                row.capped = capped;
                group.weighted_index = group.weighted_index.and_then(|sum| sum.aligned_add(value));
            } else {
                row.arithmetic_error = true;
                group.weighted_index = None;
            }
            sources.insert(&row.source);
            issuer_set.insert(&row.issuer);
            counterparties.insert(&row.counterparty);
            group.index_rounded |= row.index_rounded;
            group.capped |= row.capped;
            group.arithmetic_error |= row.arithmetic_error
                || group.raw_quantity.is_none()
                || group.weighted_index.is_none();
        }
        group.occurrences = sources.len();
        group.issuers = issuer_set.into_iter().cloned().collect();
        group.counterparties = counterparties.into_iter().cloned().collect();
        analysis.groups.push(group);
    }
    if query.issuer_cap.is_some() || query.first_contact_cap.is_some() {
        let uncertain = analysis
            .rows
            .iter()
            .filter(|row| row.arithmetic_error)
            .map(|row| row.measurement.id())
            .collect::<Result<BTreeSet<_>, _>>()?;
        for row in &mut analysis.rows {
            if uncertain.contains(&row.measurement.id()?) {
                row.weighted_index = None;
                row.arithmetic_error = true;
            }
        }
        for group in &mut analysis.groups {
            if uncertain.contains(&group.measurement.id()?) {
                group.weighted_index = None;
                group.arithmetic_error = true;
            }
        }
    }
    Ok(())
}
