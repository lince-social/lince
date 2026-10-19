pub trait Power {
    type Mode: Copy;
    const LOW_LATENCY: Self::Mode;
    fn current(&mut self) -> anyhow::Result<Self::Mode>;
    fn set(&mut self, mode: Self::Mode) -> anyhow::Result<()>;
}

pub struct Latency<P: Power> {
    power: P,
    previous: Option<P::Mode>,
    enabled: bool,
}

impl<P: Power> Latency<P> {
    pub fn new(power: P) -> Self {
        Self {
            power,
            previous: None,
            enabled: false,
        }
    }

    pub fn enable(&mut self) -> anyhow::Result<()> {
        if self.enabled {
            return Ok(());
        }
        if self.previous.is_none() {
            self.previous = Some(self.power.current()?);
        }
        if let Err(error) = self.power.set(P::LOW_LATENCY) {
            let _ = self.disable();
            return Err(error);
        }
        self.enabled = true;
        Ok(())
    }

    pub fn disable(&mut self) -> anyhow::Result<()> {
        if let Some(previous) = self.previous {
            self.enabled = false;
            self.power.set(previous)?;
            self.previous = None;
        }
        Ok(())
    }
}

impl<P: Power> Drop for Latency<P> {
    fn drop(&mut self) {
        let _ = self.disable();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, collections::VecDeque, rc::Rc};

    #[derive(Default)]
    struct State {
        mode: u8,
        read_failure: bool,
        set_failures: VecDeque<bool>,
        writes: Vec<u8>,
    }

    struct Device(Rc<RefCell<State>>);

    impl Power for Device {
        type Mode = u8;
        const LOW_LATENCY: u8 = 0;

        fn current(&mut self) -> anyhow::Result<u8> {
            let state = self.0.borrow();
            anyhow::ensure!(!state.read_failure, "Read failed");
            Ok(state.mode)
        }

        fn set(&mut self, mode: u8) -> anyhow::Result<()> {
            let mut state = self.0.borrow_mut();
            state.writes.push(mode);
            anyhow::ensure!(
                !state.set_failures.pop_front().unwrap_or(false),
                "Write failed"
            );
            state.mode = mode;
            Ok(())
        }
    }

    fn fixture(mode: u8) -> (Latency<Device>, Rc<RefCell<State>>) {
        let state = Rc::new(RefCell::new(State {
            mode,
            ..Default::default()
        }));
        (Latency::new(Device(state.clone())), state)
    }

    #[test]
    fn repeated_capture_cycles_and_drop_restore_the_actual_prior_mode() {
        for previous in 0..=2 {
            let (mut latency, state) = fixture(previous);
            latency.disable().unwrap();
            latency.enable().unwrap();
            latency.enable().unwrap();
            assert_eq!(state.borrow().writes, [0]);
            assert_eq!(state.borrow().mode, 0);
            latency.disable().unwrap();
            latency.disable().unwrap();
            assert_eq!(state.borrow().mode, previous);
            latency.enable().unwrap();
            drop(latency);
            assert_eq!(state.borrow().mode, previous);
            assert_eq!(state.borrow().writes, [0, previous, 0, previous]);
        }
    }

    #[test]
    fn failed_mode_query_does_not_change_power_and_can_be_retried() {
        let (mut latency, state) = fixture(2);
        state.borrow_mut().read_failure = true;
        assert!(latency.enable().is_err());
        assert!(state.borrow().writes.is_empty());
        state.borrow_mut().read_failure = false;
        latency.enable().unwrap();
        drop(latency);
        assert_eq!(state.borrow().mode, 2);
    }

    #[test]
    fn failed_enable_restores_the_prior_mode_before_returning() {
        let (mut latency, state) = fixture(1);
        state.borrow_mut().set_failures.extend([true, false]);
        assert!(latency.enable().is_err());
        assert_eq!(state.borrow().writes, [0, 1]);
        assert_eq!(state.borrow().mode, 1);
        latency.disable().unwrap();
        assert_eq!(state.borrow().writes, [0, 1]);
        latency.enable().unwrap();
        drop(latency);
        assert_eq!(state.borrow().mode, 1);
    }

    #[test]
    fn failed_restore_keeps_the_original_mode_for_retry() {
        let (mut latency, state) = fixture(2);
        latency.enable().unwrap();
        state.borrow_mut().set_failures.push_back(true);
        assert!(latency.disable().is_err());
        assert_eq!(state.borrow().mode, 0);
        latency.enable().unwrap();
        assert_eq!(state.borrow().writes, [0, 2, 0]);
        latency.disable().unwrap();
        assert_eq!(state.borrow().mode, 2);
        assert_eq!(state.borrow().writes, [0, 2, 0, 2]);
    }

    #[test]
    fn failed_enable_and_rollback_retain_the_prior_mode_for_recovery() {
        let (mut latency, state) = fixture(2);
        state.borrow_mut().set_failures.extend([true, true]);
        assert!(latency.enable().is_err());
        assert_eq!(state.borrow().writes, [0, 2]);
        latency.enable().unwrap();
        assert_eq!(state.borrow().mode, 0);
        latency.disable().unwrap();
        assert_eq!(state.borrow().writes, [0, 2, 0, 2]);
        assert_eq!(state.borrow().mode, 2);
    }
}
