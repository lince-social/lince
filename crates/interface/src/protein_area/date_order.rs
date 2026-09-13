use super::*;
use chrono::{Local, NaiveDate};

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
        return;
    }
    let day = Local::now().date_naive();
    if state.ordered_day == Some(day) && !state.dirty {
        return;
    }
    state.data.sort_by(|a, b| key(a, day).cmp(&key(b, day)));
    let order: Vec<_> = state
        .data
        .iter()
        .filter_map(|row| row["uid"].as_str().map(str::to_string))
        .collect();
    if state.order.as_ref() != &order {
        state.order = std::sync::Arc::new(order);
        state.dirty = true;
        state.revision = state.revision.wrapping_add(1);
    }
    state.ordered_day = Some(day);
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
    let Some(signal) = world.get_resource::<crate::wake::WakeSignal>().cloned() else {
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
