use super::*;
use smithay::{
    reexports::{
        wayland_protocols::ext::idle_notify::v1::server::{
            ext_idle_notification_v1::ExtIdleNotificationV1,
            ext_idle_notifier_v1::{self, ExtIdleNotifierV1},
        },
        wayland_server::{DataInit, Dispatch, GlobalDispatch, New},
    },
    wayland::idle_inhibit::{IdleInhibitHandler, IdleInhibitManagerState},
};
use std::time::Duration;

#[derive(Clone, Copy)]
struct Deadline {
    timeout: Duration,
    next: Option<Instant>,
    ignores_inhibitors: bool,
    inhibited: bool,
}

impl Deadline {
    fn new(timeout: Duration, ignores_inhibitors: bool, now: Instant, inhibited: bool) -> Self {
        let mut value = Self { timeout, next: None, ignores_inhibitors, inhibited: false };
        value.activity(now);
        value.inhibit(inhibited, now);
        value
    }

    fn activity(&mut self, now: Instant) -> bool {
        let resumed = self.next.is_none() && !self.inhibited;
        if !self.inhibited { self.next = now.checked_add(self.timeout); }
        resumed
    }

    fn inhibit(&mut self, inhibited: bool, now: Instant) -> bool {
        let inhibited = inhibited && !self.ignores_inhibitors;
        if self.inhibited == inhibited { return false; }
        let resumed = inhibited && self.next.is_none();
        self.inhibited = inhibited;
        self.next = if inhibited { None } else { now.checked_add(self.timeout) };
        resumed
    }

    fn expire(&mut self, now: Instant) -> bool {
        if self.next.is_some_and(|deadline| deadline <= now) {
            self.next = None;
            true
        } else { false }
    }
}

pub(super) struct Idle {
    _inhibit: IdleInhibitManagerState,
    inhibitors: HashMap<WlSurface, usize>,
    notifications: Vec<(ExtIdleNotificationV1, WlSeat, Deadline)>,
    inhibited: bool,
}

impl Idle {
    pub(super) fn new(handle: &DisplayHandle) -> Self {
        handle.create_global::<State, ExtIdleNotifierV1, _>(2, ());
        Self {
            _inhibit: IdleInhibitManagerState::new::<State>(handle),
            inhibitors: HashMap::new(),
            notifications: Vec::new(),
            inhibited: false,
        }
    }

    fn prune(&mut self) {
        self.notifications.retain(|(notification, seat, _)| notification.is_alive() && seat.is_alive());
        self.inhibitors.retain(|surface, _| surface.is_alive());
    }

    fn set_inhibited(&mut self, inhibited: bool, now: Instant) {
        self.inhibited = inhibited;
        for (notification, _, deadline) in &mut self.notifications {
            if deadline.inhibit(inhibited, now) { notification.resumed(); }
        }
    }

    #[cfg(any(feature = "nested", all(test, feature = "server-tests")))]
    fn activity(&mut self, now: Instant) {
        self.prune();
        for (notification, _, deadline) in &mut self.notifications {
            if deadline.activity(now) { notification.resumed(); }
        }
    }

    fn tick(&mut self, now: Instant) {
        self.prune();
        for (notification, _, deadline) in &mut self.notifications {
            if deadline.expire(now) { notification.idled(); }
        }
    }

    #[cfg(any(feature = "nested", all(test, feature = "server-tests")))]
    fn timeout(&self, now: Instant) -> Option<Duration> {
        self.notifications.iter().filter_map(|(_, _, deadline)| deadline.next)
            .min().map(|deadline| deadline.saturating_duration_since(now))
    }
}

impl State {
    pub(super) fn refresh_idle(&mut self) {
        let inhibited = self.lock.owner.is_none() && self.idle.inhibitors.keys().any(|surface| {
            let Some(root) = self.input_root(surface) else { return false };
            if let Some(id) = self.id(&root) {
                return self.windows.get(&id).is_some_and(|window| window.mapped)
                    && self.presented_geometry.as_ref().is_some_and(|geometry| geometry.items.iter()
                        .any(|item| item.id == lince_interface::host::ItemId::Application(id)
                            && self.outputs.contains_key(&item.output)
                            && item.clip.as_ref().is_none_or(|clip| !clip.is_empty())));
            }
            self.layers.surfaces.get(&root).is_some_and(|layer| {
                smithay::desktop::layer_map_for_output(self.layer_output(&root))
                    .layer_geometry(layer).is_some()
            })
        });
        let now = Instant::now();
        self.idle.set_inhibited(inhibited, now);
        self.idle.tick(now);
    }
}

impl Server {
    #[cfg(any(feature = "nested", all(test, feature = "server-tests")))]
    pub(crate) fn activity(&mut self) { self.state.idle.activity(Instant::now()); }

    #[cfg(any(feature = "nested", all(test, feature = "server-tests")))]
    pub(crate) fn idle_timeout(&self) -> Option<Duration> { self.state.idle.timeout(Instant::now()) }
}

impl IdleInhibitHandler for State {
    fn inhibit(&mut self, surface: WlSurface) {
        if self.idle.inhibitors.len() >= MAX_SURFACES && !self.idle.inhibitors.contains_key(&surface) {
            self.reject(&surface, "Park idle inhibitor limit reached");
            return;
        }
        let count = self.idle.inhibitors.entry(surface).or_default();
        *count = count.saturating_add(1);
        self.refresh_idle();
    }

    fn uninhibit(&mut self, surface: WlSurface) {
        if let Some(count) = self.idle.inhibitors.get_mut(&surface) {
            *count = count.saturating_sub(1);
            if *count == 0 { self.idle.inhibitors.remove(&surface); }
        }
        self.refresh_idle();
    }
}

impl GlobalDispatch<ExtIdleNotifierV1, ()> for State {
    fn bind(_: &mut Self, _: &DisplayHandle, _: &Client, resource: New<ExtIdleNotifierV1>, _: &(), data: &mut DataInit<'_, Self>) {
        data.init(resource, ());
    }
}

impl Dispatch<ExtIdleNotifierV1, ()> for State {
    fn request(state: &mut Self, _: &Client, resource: &ExtIdleNotifierV1, request: ext_idle_notifier_v1::Request, _: &(), _: &DisplayHandle, data: &mut DataInit<'_, Self>) {
        let (id, timeout, seat, ignores) = match request {
            ext_idle_notifier_v1::Request::GetIdleNotification { id, timeout, seat } => (id, timeout, seat, false),
            ext_idle_notifier_v1::Request::GetInputIdleNotification { id, timeout, seat } => (id, timeout, seat, true),
            _ => return,
        };
        let notification = data.init(id, ());
        state.idle.prune();
        if Seat::<State>::from_resource(&seat).as_ref() != Some(&state.seat)
            || state.idle.notifications.len() >= 256
        {
            resource.post_error(0u32, "Park idle notification seat or capacity is invalid");
            return;
        }
        let deadline = Deadline::new(Duration::from_millis(u64::from(timeout)), ignores, Instant::now(), state.idle.inhibited);
        state.idle.notifications.push((notification, seat, deadline));
    }
}

impl Dispatch<ExtIdleNotificationV1, ()> for State {
    fn request(_: &mut Self, _: &Client, _: &ExtIdleNotificationV1, _: <ExtIdleNotificationV1 as Resource>::Request, _: &(), _: &DisplayHandle, _: &mut DataInit<'_, Self>) {}

    fn destroyed(state: &mut Self, _: ClientId, resource: &ExtIdleNotificationV1, _: &()) {
        state.idle.notifications.retain(|(notification, _, _)| notification != resource);
    }
}

smithay::delegate_idle_inhibit!(State);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_notifications_resume_and_restart_after_inhibition() {
        let now = Instant::now();
        let timeout = Duration::from_secs(10);
        let mut deadline = Deadline::new(timeout, false, now, false);
        assert!(!deadline.expire(now + Duration::from_secs(9)));
        assert!(deadline.expire(now + timeout));
        assert!(!deadline.expire(now + timeout));
        assert!(deadline.inhibit(true, now + timeout));
        assert!(!deadline.activity(now + timeout));
        assert!(!deadline.expire(now + Duration::from_secs(100)));
        assert!(!deadline.inhibit(false, now + Duration::from_secs(100)));
        assert_eq!(deadline.next, Some(now + Duration::from_secs(110)));
        assert!(deadline.expire(now + Duration::from_secs(110)));
        assert!(deadline.activity(now + Duration::from_secs(111)));
        assert!(!deadline.activity(now + Duration::from_secs(112)));
    }

    #[test]
    fn input_idle_notifications_ignore_application_inhibitors() {
        let now = Instant::now();
        let mut deadline = Deadline::new(Duration::ZERO, true, now, true);
        assert!(deadline.expire(now));
        assert!(!deadline.inhibit(false, now));
        assert!(!deadline.inhibit(true, now));
        assert!(deadline.activity(now));
        assert!(deadline.expire(now));
    }
}
