use crate::{sand::Square, wake::WakeSignal};
use bevy::prelude::*;
use std::{
    collections::HashMap,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[derive(Component, Clone, Copy)]
#[require(Square)]
pub struct TimeLimit(pub Duration);

impl Default for TimeLimit {
    fn default() -> Self {
        Self(Duration::from_secs(10))
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TimeLimitSystems;

struct Alarm {
    sender: Option<mpsc::Sender<Option<Instant>>>,
    task: Option<thread::JoinHandle<()>>,
}

impl Alarm {
    fn new(wake: WakeSignal) -> Self {
        let (sender, receiver) = mpsc::channel::<Option<Instant>>();
        let task = thread::Builder::new()
            .name("visibility-time-limit".into())
            .spawn(move || {
                let mut deadline: Option<Instant> = None;
                loop {
                    let message = match deadline {
                        Some(at) => {
                            receiver.recv_timeout(at.saturating_duration_since(Instant::now()))
                        }
                        None => receiver
                            .recv()
                            .map_err(|_| mpsc::RecvTimeoutError::Disconnected),
                    };
                    match message {
                        Ok(next) => deadline = next,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            deadline = None;
                            wake.ring();
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .expect("start visibility timer");
        Self {
            sender: Some(sender),
            task: Some(task),
        }
    }
}

impl Drop for Alarm {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}

#[derive(Resource, Default)]
struct Deadlines {
    active: HashMap<Entity, Instant>,
    alarm: Option<Alarm>,
    next: Option<Instant>,
}

pub struct TimeLimitPlugin;

impl Plugin for TimeLimitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Deadlines>().add_systems(
            PostUpdate,
            update
                .in_set(TimeLimitSystems)
                .run_if(crate::laboratory::normal),
        );
    }
}

fn update(
    mut deadlines: ResMut<Deadlines>,
    mut queries: ParamSet<(
        Query<(Entity, &TimeLimit, &Visibility), Or<(Changed<TimeLimit>, Changed<Visibility>)>>,
        Query<&mut Visibility, With<TimeLimit>>,
    )>,
    wake: Option<Res<WakeSignal>>,
    mut focus: Option<ResMut<bevy::input_focus::InputFocus>>,
    parents: Query<&ChildOf>,
) {
    let now = Instant::now();
    for (entity, limit, visibility) in &queries.p0() {
        if *visibility == Visibility::Hidden {
            deadlines.active.remove(&entity);
        } else if let Some(at) = now.checked_add(limit.0) {
            deadlines.active.insert(entity, at);
        } else {
            deadlines.active.remove(&entity);
        }
    }
    let mut squares = queries.p1();
    deadlines.active.retain(|entity, at| {
        let Ok(mut visibility) = squares.get_mut(*entity) else {
            return false;
        };
        if now < *at {
            return true;
        }
        visibility.set_if_neq(Visibility::Hidden);
        if let Some(focus) = &mut focus {
            let mut focused = focus.get();
            while let Some(current) = focused {
                if current == *entity {
                    focus.clear();
                    break;
                }
                focused = parents.get(current).ok().map(ChildOf::parent);
            }
        }
        if let Some(wake) = &wake {
            wake.ring();
        }
        false
    });
    let next = deadlines.active.values().copied().min();
    if deadlines.alarm.is_none()
        && next.is_some()
        && let Some(wake) = wake
    {
        deadlines.alarm = Some(Alarm::new(wake.clone()));
        deadlines.next = None;
    }
    if deadlines.next != next {
        if let Some(alarm) = &deadlines.alarm {
            let _ = alarm.sender.as_ref().unwrap().send(next);
        }
        deadlines.next = next;
    }
}

pub(crate) fn pause(world: &mut World) {
    if let Some(mut deadlines) = world.get_resource_mut::<Deadlines>() {
        if let Some(alarm) = &deadlines.alarm {
            let _ = alarm.sender.as_ref().unwrap().send(None);
        }
        deadlines.next = None;
    }
}

pub(crate) fn resume(world: &mut World, duration: Duration) {
    if let Some(mut deadlines) = world.get_resource_mut::<Deadlines>() {
        for at in deadlines.active.values_mut() {
            if let Some(next) = at.checked_add(duration) {
                *at = next;
            }
        }
        deadlines.next = None;
    }
}

pub(crate) mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    fn idle_window_wakes_once_and_hides_the_square_at_its_deadline() {
        let (sender, receiver) = mpsc::channel();
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins(TimeLimitPlugin)
            .insert_resource(WakeSignal::new(move || {
                let _ = sender.send(());
            }));
        let square = app
            .world_mut()
            .spawn((TimeLimit(Duration::from_millis(30)), Visibility::Inherited))
            .id();
        app.update();
        receiver.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(
            *app.world().get::<Visibility>(square).unwrap(),
            Visibility::Inherited
        );
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(square).unwrap(),
            Visibility::Hidden
        );
        assert!(app.world().resource::<Deadlines>().active.is_empty());
        while receiver.try_recv().is_ok() {}
        assert!(receiver.recv_timeout(Duration::from_millis(50)).is_err());
    }

    #[cfg_attr(test, test)]
    fn quiet_frames_do_not_extend_deadlines_and_expired_squares_stay_hidden() {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins(TimeLimitPlugin);
        let square = app.world_mut().spawn(TimeLimit::default()).id();
        app.update();
        let at = app.world().resource::<Deadlines>().active[&square];
        for _ in 0..20 {
            app.update();
        }
        assert_eq!(app.world().resource::<Deadlines>().active[&square], at);
        app.world_mut()
            .resource_mut::<Deadlines>()
            .active
            .insert(square, Instant::now());
        app.update();
        for _ in 0..20 {
            app.update();
        }
        assert_eq!(
            *app.world().get::<Visibility>(square).unwrap(),
            Visibility::Hidden
        );
        assert!(app.world().resource::<Deadlines>().active.is_empty());
    }

    #[cfg_attr(test, test)]
    fn hiding_removing_and_reshowing_cancel_or_replace_the_old_deadline() {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins(TimeLimitPlugin);
        let square = app.world_mut().spawn(TimeLimit::default()).id();
        app.update();
        app.world_mut()
            .entity_mut(square)
            .insert(Visibility::Hidden);
        app.update();
        assert!(app.world().resource::<Deadlines>().active.is_empty());
        app.world_mut()
            .entity_mut(square)
            .insert(Visibility::Inherited);
        app.update();
        assert!(
            app.world()
                .resource::<Deadlines>()
                .active
                .contains_key(&square)
        );
        app.world_mut().entity_mut(square).remove::<TimeLimit>();
        app.update();
        assert!(app.world().resource::<Deadlines>().active.is_empty());
        app.world_mut()
            .entity_mut(square)
            .insert(TimeLimit::default());
        app.update();
        app.world_mut().despawn(square);
        app.update();
        assert!(app.world().resource::<Deadlines>().active.is_empty());
    }

    crate::laboratory_cases! {
        idle_window_wakes_once_and_hides_the_square_at_its_deadline,
        quiet_frames_do_not_extend_deadlines_and_expired_squares_stay_hidden,
        hiding_removing_and_reshowing_cancel_or_replace_the_old_deadline,
    }
}
