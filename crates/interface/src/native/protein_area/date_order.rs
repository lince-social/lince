// non-lean-yet

//! Native integration for closest-date ordering and local midnight wakeups.

pub(super) use crate::record_date_order::Prepared;

use super::{Runtime, State};
use crate::{
    host::performance::{Span, Stage},
    native::wake::WakeSignal,
};
use bevy::prelude::{Resource, World};
use chrono::{Local, NaiveDate};
use std::time::Duration;
use tokio::{runtime::Handle, task::JoinHandle, time::sleep};

#[derive(Resource, Default)]
struct Alarm {
    day: Option<NaiveDate>,
    task: Option<JoinHandle<()>>,
}

impl Drop for Alarm {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
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
    let _ordering = Span::new(Stage::RecordDateOrder);
    let day = Local::now().date_naive();
    if state
        .ordering
        .refresh(day, &mut state.data, &mut state.order)
    {
        state.dirty = true;
        state.revision = state.revision.wrapping_add(1);
    }
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
    let Some(signal) = world.get_resource::<WakeSignal>().cloned() else {
        return;
    };
    let Ok(runtime) = Handle::try_current() else {
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
        .unwrap_or(Duration::from_secs(1));
    world.insert_resource(Alarm {
        day: Some(now.date_naive()),
        task: Some(runtime.spawn(async move {
            sleep(delay).await;
            signal.ring();
        })),
    });
}
