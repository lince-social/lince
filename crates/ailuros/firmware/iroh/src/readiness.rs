use std::cell::Cell;

#[derive(Default)]
pub struct Readiness {
    synchronized: Cell<bool>,
}

impl Readiness {
    pub fn ready(
        &self,
        connected: bool,
        synchronized: impl FnOnce() -> bool,
        valid_time: bool,
    ) -> bool {
        if !connected {
            return false;
        }
        if !self.synchronized.get() && synchronized() {
            self.synchronized.set(true);
        }
        self.synchronized.get() && valid_time
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_sync_survives_consuming_sdk_status_and_network_reconnect() {
        let readiness = Readiness::default();
        let completion = Cell::new(true);
        assert!(readiness.ready(true, || completion.replace(false), true));
        assert!(!completion.get());
        assert!(readiness.ready(true, || panic!("Do not consume status again"), true));
        assert!(!readiness.ready(false, || panic!("Network is disconnected"), true));
        assert!(readiness.ready(true, || false, true));
    }

    #[test]
    fn network_and_synchronization_and_clock_floor_are_required() {
        let readiness = Readiness::default();
        assert!(!readiness.ready(false, || panic!("Network is disconnected"), true));
        assert!(!readiness.ready(true, || false, true));
        assert!(!readiness.ready(true, || true, false));
        assert!(readiness.ready(true, || false, true));
        assert!(!readiness.ready(true, || false, false));
    }
}
