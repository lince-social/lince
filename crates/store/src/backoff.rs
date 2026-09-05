use chrono::{DateTime, Duration, Utc};

pub const DEFAULT_BASE_SECONDS: u32 = 30;
pub const DEFAULT_MAX_SECONDS: u32 = 3600;

pub fn delay_seconds(attempts: i64, base_seconds: u32, max_seconds: u32) -> u64 {
    let base = u64::from(base_seconds.max(1));
    let ceiling = u64::from(max_seconds.max(base_seconds.max(1)));
    let exponent = u32::try_from(attempts.max(0)).unwrap_or(u32::MAX).min(20);
    base.saturating_mul(1_u64 << exponent).min(ceiling)
}

pub fn next_attempt_at(
    now: DateTime<Utc>,
    attempts: i64,
    base_seconds: u32,
    max_seconds: u32,
) -> DateTime<Utc> {
    let delay = delay_seconds(attempts, base_seconds, max_seconds);
    now + Duration::seconds(i64::try_from(delay).unwrap_or(i64::MAX))
}

pub fn due(now: DateTime<Utc>, next_attempt_at: Option<&str>) -> bool {
    let Some(next) = next_attempt_at else {
        return true;
    };
    DateTime::parse_from_rfc3339(next)
        .map(|when| when.with_timezone(&Utc) <= now)
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_failure_waits_the_base_delay() {
        assert_eq!(delay_seconds(0, 30, 3600), 30);
    }

    #[test]
    fn each_failure_doubles_the_wait() {
        assert_eq!(delay_seconds(1, 30, 3600), 60);
        assert_eq!(delay_seconds(2, 30, 3600), 120);
    }

    #[test]
    fn the_ceiling_holds_however_long_a_peer_is_down() {
        assert_eq!(delay_seconds(40, 30, 3600), 3600);
        assert_eq!(delay_seconds(i64::MAX, 30, 3600), 3600);
    }

    #[test]
    fn a_row_that_has_never_failed_is_due_now() {
        assert!(due(Utc::now(), None));
    }

    #[test]
    fn an_unreadable_stamp_is_due_rather_than_stranded() {
        assert!(due(Utc::now(), Some("not a timestamp")));
    }
}
