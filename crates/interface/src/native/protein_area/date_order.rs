// non-lean-yet
// Scope: struct Prepared, struct Alarm, fn key, fn sort.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 97.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   85, 88, 90, 92.

use super::*;
use crate::host::performance::{self, Count, Stage};
use chrono::{Local, NaiveDate};
use std::sync::{Arc, Weak};

#[derive(Default)]
pub(super) struct Prepared {
    day: Option<NaiveDate>,
    data: Weak<Vec<Value>>,
    order: Weak<Vec<String>>,
}

impl Prepared {
    fn rows_current(&self, day: NaiveDate, data: &Arc<Vec<Value>>) -> bool {
        self.day == Some(day) && std::ptr::eq(self.data.as_ptr(), Arc::as_ptr(data))
    }

    fn order_current(&self, order: &Arc<Vec<String>>) -> bool {
        std::ptr::eq(self.order.as_ptr(), Arc::as_ptr(order))
    }

    fn clear(&mut self) {
        if self.day.take().is_some() {
            self.data = Weak::new();
            self.order = Weak::new();
        }
    }

    fn remember(&mut self, day: NaiveDate, data: &Arc<Vec<Value>>, order: &Arc<Vec<String>>) {
        self.day = Some(day);
        self.data = Arc::downgrade(data);
        self.order = Arc::downgrade(order);
    }
}

#[derive(Resource, Default)]
struct Alarm {
    day: Option<NaiveDate>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for Alarm {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

fn key(row: &Value, today: NaiveDate) -> (u64, Option<NaiveDate>, &str) {
    let date = row["due_date"]
        .as_str()
        .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok());
    (
        date.map_or(u64::MAX, |date| {
            date.signed_duration_since(today).num_days().unsigned_abs()
        }),
        date,
        row["uid"].as_str().unwrap_or_default(),
    )
}

pub(super) fn sort(state: &mut State) {
    if !state
        .applied
        .as_ref()
        .is_some_and(|config| config.closest_end_date)
    {
        state.ordering.clear();
        return;
    }
    let _ordering = performance::Span::new(Stage::RecordDateOrder);
    let day = Local::now().date_naive();
    let rows_current = state.ordering.rows_current(day, &state.data);
    if rows_current && state.ordering.order_current(&state.order) {
        performance::count(Count::RecordDateOrderReuses);
        return;
    }
    state.ordering.clear();
    if !rows_current && let Some(mut sources) = permutation(&state.data, day) {
        performance::count(Count::RecordDateDataReorders);
        let rows = Arc::make_mut(&mut state.data);
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
    let order = {
        let ids = state.data.iter().filter_map(|row| row["uid"].as_str());
        (!ids.clone().eq(state.order.iter().map(String::as_str)))
            .then(|| ids.map(str::to_owned).collect::<Vec<_>>())
    };
    if let Some(order) = order {
        performance::add(Count::RecordDateOrderIdsCopied, order.len() as u64);
        state.order = Arc::new(order);
        state.dirty = true;
        state.revision = state.revision.wrapping_add(1);
    }
    state.ordering.remember(day, &state.data, &state.order);
}

fn permutation(rows: &[Value], day: NaiveDate) -> Option<Vec<usize>> {
    performance::add(Count::RecordDateKeysPrepared, rows.len() as u64);
    let mut ordered: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(source, row)| (key(row, day), source))
        .collect();
    if ordered.windows(2).all(|pair| pair[0].0 <= pair[1].0) {
        return None;
    }
    ordered.sort_unstable();
    Some(ordered.into_iter().map(|(_, source)| source).collect())
}

pub(super) fn wake(world: &mut World) {
    let enabled = world.resource::<Runtime>().areas.values().any(|state| {
        state
            .applied
            .as_ref()
            .is_some_and(|config| config.enabled && config.closest_end_date)
    });
    if !enabled {
        world.remove_resource::<Alarm>();
        return;
    }
    let now = Local::now();
    if world
        .get_resource::<Alarm>()
        .is_some_and(|alarm| alarm.day == Some(now.date_naive()))
    {
        return;
    }
    let Some(signal) = world
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned()
    else {
        return;
    };
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let Some(tomorrow) = now
        .date_naive()
        .succ_opt()
        .and_then(|day| day.and_hms_opt(0, 0, 0))
    else {
        return;
    };
    let Some(tomorrow) = tomorrow.and_local_timezone(Local).earliest() else {
        return;
    };
    let delay = tomorrow
        .signed_duration_since(now)
        .to_std()
        .unwrap_or(std::time::Duration::from_secs(1));
    world.insert_resource(Alarm {
        day: Some(now.date_naive()),
        task: Some(runtime.spawn(async move {
            tokio::time::sleep(delay).await;
            signal.ring();
        })),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_dates_include_past_dates_and_put_missing_dates_last() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 13).unwrap();
        let mut rows = vec![
            serde_json::json!({"uid":"far-past", "due_date":"2025-01-01"}),
            serde_json::json!({"uid":"missing"}),
            serde_json::json!({"uid":"tomorrow", "due_date":"2026-09-14"}),
            serde_json::json!({"uid":"yesterday", "due_date":"2026-09-12"}),
            serde_json::json!({"uid":"today", "due_date":"2026-09-13"}),
        ];
        rows.sort_by(|a, b| key(a, today).cmp(&key(b, today)));
        assert_eq!(
            rows.iter()
                .map(|r| r["uid"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["today", "yesterday", "tomorrow", "far-past", "missing"]
        );
    }
}
