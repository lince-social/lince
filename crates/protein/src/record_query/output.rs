use std::{cmp::Ordering, collections::BTreeMap};

use crate::GroupBy;

use super::{
    CountGroup, CountRow, QueryError, RecordScalarRow,
    budget::{Budget, within},
    snapshot::Prepared,
};

#[derive(Clone, Copy)]
pub(super) struct SortKey {
    pub(super) field: SortField,
    pub(super) descending: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum SortField {
    Head,
    Slug,
    Kind,
    Quantity,
    StartDate,
    DueDate,
    CreatedAt,
    UpdatedAt,
}

pub(super) fn counts(
    selected: &[&RecordScalarRow],
    by: GroupBy,
    prepared: &Prepared<'_>,
    limit: Option<usize>,
    budget: &mut Budget<'_>,
) -> Result<Vec<CountRow>, QueryError> {
    let mut out = match by {
        GroupBy::Total => vec![CountRow {
            group: CountGroup::Total,
            count: selected.len(),
        }],
        GroupBy::Kind => {
            let mut groups = BTreeMap::new();
            for &&RecordScalarRow { kind, .. } in selected {
                budget.step()?;
                let count = groups.entry(kind.as_str()).or_insert(CountRow {
                    group: CountGroup::Kind(kind),
                    count: 0,
                });
                increment(&mut count.count)?;
            }
            groups.into_values().collect()
        }
        GroupBy::Concept => {
            let mut groups = BTreeMap::new();
            for row in selected {
                budget.step()?;
                let concept = prepared.identity_concepts.get(row.uid.as_str()).copied();
                // Named concepts sort by UID, with the ungrouped count last.
                increment(groups.entry((concept.is_none(), concept)).or_default())?;
            }
            groups
                .into_iter()
                .map(|((_, concept), count)| CountRow {
                    group: CountGroup::Concept(concept.map(str::to_owned)),
                    count,
                })
                .collect()
        }
        _ => return Err(QueryError::Unsupported),
    };
    limit_results(&mut out, limit, budget.limits.results)?;
    Ok(out)
}

fn increment(count: &mut usize) -> Result<(), QueryError> {
    *count = count.checked_add(1).ok_or(QueryError::LimitExceeded)?;
    Ok(())
}

pub(super) fn limit_results<T>(
    rows: &mut Vec<T>,
    limit: Option<usize>,
    maximum: usize,
) -> Result<(), QueryError> {
    if let Some(limit) = limit {
        rows.truncate(limit);
    } else {
        within(rows.len(), maximum)?;
    }
    Ok(())
}

pub(super) fn order_records(
    rows: &mut [&RecordScalarRow],
    order: &[SortKey],
    budget: &mut Budget<'_>,
) -> Result<(), QueryError> {
    let levels = (usize::BITS - rows.len().saturating_sub(1).leading_zeros() + 1) as usize;
    let comparisons = rows
        .len()
        .checked_mul(levels)
        .and_then(|value| value.checked_mul(order.len().max(1)))
        .ok_or(QueryError::LimitExceeded)?;
    budget.steps(comparisons)?;
    // The caller selects rows from a UID-keyed BTreeMap.
    if !order.is_empty() {
        rows.sort_unstable_by(|left, right| {
            for &key in order {
                let compared = compare_field(left, right, key);
                if compared != Ordering::Equal {
                    return compared;
                }
            }
            left.uid.cmp(&right.uid)
        });
    }
    Ok(())
}

fn compare_field(left: &RecordScalarRow, right: &RecordScalarRow, key: SortKey) -> Ordering {
    match key.field {
        SortField::Head => direction(left.head.cmp(&right.head), key.descending),
        SortField::Slug => optional(left.slug.as_ref(), right.slug.as_ref(), key.descending),
        SortField::Kind => direction(left.kind.as_str().cmp(right.kind.as_str()), key.descending),
        SortField::Quantity => direction(
            left.quantity.exact_numeric_cmp(right.quantity),
            key.descending,
        ),
        SortField::StartDate => optional(
            left.start_date.as_ref(),
            right.start_date.as_ref(),
            key.descending,
        ),
        SortField::DueDate => optional(
            left.due_date.as_ref(),
            right.due_date.as_ref(),
            key.descending,
        ),
        SortField::CreatedAt => direction(left.created_at.cmp(&right.created_at), key.descending),
        SortField::UpdatedAt => direction(left.updated_at.cmp(&right.updated_at), key.descending),
    }
}

fn optional<T: Ord>(left: Option<&T>, right: Option<&T>, descending: bool) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => direction(left.cmp(right), descending),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn direction(order: Ordering, descending: bool) -> Ordering {
    if descending { order.reverse() } else { order }
}
