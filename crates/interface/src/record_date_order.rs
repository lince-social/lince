//! Closest-date ordering for immutable Protein Area row snapshots.

use crate::host::performance::{self, Count};
use chrono::NaiveDate;
use serde_json::Value;
use std::{
    ptr,
    sync::{Arc, Weak},
};

#[derive(Default)]
pub(crate) struct Prepared {
    day: Option<NaiveDate>,
    rows: Weak<Vec<Value>>,
    order: Weak<Vec<String>>,
}

impl Prepared {
    pub(crate) fn clear(&mut self) {
        if self.day.take().is_some() {
            self.rows = Weak::new();
            self.order = Weak::new();
        }
    }

    /// Refreshes the ordered snapshots and reports whether the published IDs changed.
    pub(crate) fn refresh(
        &mut self,
        day: NaiveDate,
        rows: &mut Arc<Vec<Value>>,
        order: &mut Arc<Vec<String>>,
    ) -> bool {
        let rows_current = self.day == Some(day) && ptr::eq(self.rows.as_ptr(), Arc::as_ptr(rows));
        if rows_current && ptr::eq(self.order.as_ptr(), Arc::as_ptr(order)) {
            performance::count(Count::RecordDateOrderReuses);
            return false;
        }
        self.clear();
        if !rows_current && let Some(sources) = permutation(rows, day) {
            performance::count(Count::RecordDateDataReorders);
            apply_permutation(Arc::make_mut(rows).as_mut_slice(), sources);
        }
        let ids = rows.iter().filter_map(uid);
        let changed = !ids.clone().eq(order.iter().map(String::as_str));
        if changed {
            let ids: Vec<_> = ids.map(str::to_owned).collect();
            performance::add(Count::RecordDateOrderIdsCopied, ids.len() as u64);
            *order = Arc::new(ids);
        }
        self.day = Some(day);
        self.rows = Arc::downgrade(rows);
        self.order = Arc::downgrade(order);
        changed
    }
}

fn uid(row: &Value) -> Option<&str> {
    row.get("uid").and_then(Value::as_str)
}

fn key(row: &Value, today: NaiveDate) -> (u64, Option<NaiveDate>, &str) {
    let date = row
        .get("due_date")
        .and_then(Value::as_str)
        .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok());
    let distance = date.map_or(u64::MAX, |date| {
        date.signed_duration_since(today).num_days().unsigned_abs()
    });
    (distance, date, uid(row).unwrap_or_default())
}

fn permutation(rows: &[Value], day: NaiveDate) -> Option<Vec<usize>> {
    performance::add(Count::RecordDateKeysPrepared, rows.len() as u64);
    let mut ordered: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(source, row)| (key(row, day), source))
        .collect();
    if ordered
        .array_windows()
        .all(|[left, right]| left.0 <= right.0)
    {
        return None;
    }
    // Source indices break ties without allocating a stable-sort buffer.
    ordered.sort_unstable();
    Some(ordered.into_iter().map(|(_, source)| source).collect())
}

fn apply_permutation(rows: &mut [Value], mut sources: Vec<usize>) {
    // The sort yields each original index exactly once, so each cycle closes
    // at its starting index and every index is valid for both slices.
    for start in 0..sources.len() {
        let mut current = start;
        let mut next = sources[current];
        while next != start {
            rows.swap(current, next);
            sources[current] = current;
            current = next;
            next = sources[current];
        }
        sources[current] = current;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nearest_dates_move_rows_and_ids_together_without_mutating_shared_rows() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 13).unwrap();
        let mut rows = Arc::new(vec![
            json!({"uid":"far-past", "due_date":"2025-01-01"}),
            json!({"uid":"missing"}),
            json!({"uid":"tomorrow", "due_date":"2026-09-14"}),
            json!({"uid":"yesterday", "due_date":"2026-09-12", "ordinal":0}),
            json!({"uid":"today", "due_date":"2026-09-13"}),
            json!({"uid":"yesterday", "due_date":"2026-09-12", "ordinal":1}),
        ]);
        let original = Arc::clone(&rows);
        let mut order = Arc::default();
        let mut prepared = Prepared::default();
        assert!(prepared.refresh(today, &mut rows, &mut order));
        let expected = [
            "today",
            "yesterday",
            "yesterday",
            "tomorrow",
            "far-past",
            "missing",
        ];
        assert_eq!(rows.iter().filter_map(uid).collect::<Vec<_>>(), expected);
        assert_eq!(order.as_slice(), expected);
        assert_eq!(uid(&original[0]), Some("far-past"));
        assert_eq!(rows[1]["ordinal"], 0);
        assert_eq!(rows[2]["ordinal"], 1);

        let ordered = Arc::clone(&rows);
        let published = Arc::clone(&order);
        assert!(!prepared.refresh(today, &mut rows, &mut order));
        assert!(Arc::ptr_eq(&rows, &ordered));
        assert!(Arc::ptr_eq(&order, &published));
    }

    #[test]
    fn changing_day_reorders_rows_and_publishes_only_changed_ids() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 13).unwrap();
        let mut rows = Arc::new(vec![
            json!({"uid":"today", "due_date":"2026-09-13"}),
            json!({"uid":"tomorrow", "due_date":"2026-09-14"}),
        ]);
        let mut order = Arc::new(vec!["today".into(), "tomorrow".into()]);
        let before = Arc::clone(&order);
        let mut prepared = Prepared::default();
        assert!(!prepared.refresh(today, &mut rows, &mut order));
        assert!(Arc::ptr_eq(&order, &before));

        let tomorrow = today.succ_opt().unwrap();
        assert!(prepared.refresh(tomorrow, &mut rows, &mut order));
        assert_eq!(order.as_slice(), ["tomorrow", "today"]);
        assert_eq!(
            rows.iter().filter_map(uid).collect::<Vec<_>>(),
            order.as_slice()
        );
        assert_eq!(before.as_slice(), ["today", "tomorrow"]);
    }
}
