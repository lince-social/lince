use super::*;

pub(super) struct Rate { credits: u128, last: Instant }

impl Default for Rate {
    fn default() -> Self { Self { credits: u128::from(contract::REQUEST_BURST) * 1_000_000_000, last: Instant::now() } }
}

impl Rate {
    pub(super) fn take(&mut self, now: Instant) -> bool {
        let added = now.saturating_duration_since(self.last).as_nanos().saturating_mul(u128::from(contract::REQUESTS_PER_SECOND));
        self.credits = self.credits.saturating_add(added).min(u128::from(contract::REQUEST_BURST) * 1_000_000_000);
        self.last = now;
        if self.credits < 1_000_000_000 { return false; }
        self.credits -= 1_000_000_000;
        true
    }
}

#[derive(Default)]
pub(super) struct Flow {
    serial: u64,
    acknowledged: u64,
    pending: VecDeque<(u64, String)>,
}

impl Flow {
    pub(super) fn acknowledged(&self) -> u64 { self.acknowledged }
    pub(super) fn room(&self, reserved: usize) -> bool { self.pending.len().saturating_add(reserved) < contract::EVENT_LIMIT }

    pub(super) fn issue(&mut self, reserved: usize) -> Result<(u64, String), String> {
        if !self.room(reserved) { return Err("Acknowledge delivered management events before sending more requests".into()); }
        let mut bytes = [0_u8; 16];
        let count = rustix::rand::getrandom(&mut bytes[..], rustix::rand::GetRandomFlags::NONBLOCK).map_err(|error| error.to_string())?;
        if count != bytes.len() { return Err("The management flow token could not be initialized".into()); }
        let token = bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        self.serial = self.serial.checked_add(1).ok_or("Management event identifiers are exhausted")?;
        self.pending.push_back((self.serial, token.clone()));
        Ok((self.serial, token))
    }

    pub(super) fn acknowledge(&mut self, serial: u64, token: &str) -> bool {
        if serial <= self.acknowledged { return true; }
        if token.len() != 32 || !self.pending.iter().any(|(sent, expected)| *sent == serial && expected == token) { return false; }
        self.acknowledged = serial;
        while self.pending.front().is_some_and(|(sent, _)| *sent <= serial) { self.pending.pop_front(); }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rate_bucket_limits_bursts_and_refills_from_elapsed_time() {
        let mut rate = Rate::default();
        let began = rate.last;
        for _ in 0..contract::REQUEST_BURST { assert!(rate.take(began)); }
        assert!(!rate.take(began));
        assert!(!rate.take(began + Duration::from_millis(15)));
        assert!(rate.take(began + Duration::from_millis(16)));
        assert!(!rate.take(began + Duration::from_millis(16)));
        for _ in 0..contract::REQUEST_BURST { assert!(rate.take(began + Duration::from_secs(1000))); }
        assert!(!rate.take(began + Duration::from_secs(1000)));
    }

    #[test]
    fn flow_cannot_consume_reserved_replies_or_accept_a_guessed_acknowledgement() {
        let mut flow = Flow::default();
        for _ in 0..contract::EVENT_LIMIT - contract::IN_FLIGHT_LIMIT { flow.issue(contract::IN_FLIGHT_LIMIT).unwrap(); }
        assert!(!flow.room(contract::IN_FLIGHT_LIMIT));
        assert!(flow.issue(contract::IN_FLIGHT_LIMIT).is_err());
        let mut invalid = flow.pending.back().unwrap().1.clone();
        invalid.replace_range(..1, if invalid.starts_with('0') { "1" } else { "0" });
        assert!(!flow.acknowledge(flow.serial, &invalid));
        for reserved in (0..contract::IN_FLIGHT_LIMIT).rev() { flow.issue(reserved).unwrap(); }
        assert_eq!(flow.pending.len(), contract::EVENT_LIMIT);
        let (serial, token) = flow.pending.back().unwrap().clone();
        assert!(flow.acknowledge(serial, &token));
        assert!(flow.pending.is_empty());
        assert!(flow.room(contract::IN_FLIGHT_LIMIT));
        assert!(flow.acknowledge(serial, &token));
        assert!(flow.pending.is_empty());
    }
}
