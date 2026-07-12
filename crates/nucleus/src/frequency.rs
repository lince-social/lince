//! Frequency: the proto-Instinct of time (blueprint VI.1). Pure math over a
//! passed-in `now`, so the timer wheel and DST share one implementation.

use chrono::{DateTime, Datelike, Days, Months, TimeDelta, Utc};

/// Parse a duration literal (`90s`, `5m`, `2h`, `30d`) into seconds — the same
/// grammar the expression lexer uses for `sum(@x, 30d)` and signal schedules.
pub fn parse_duration(s: &str) -> Option<i64> {
    let s = s.trim();
    let (num, unit) = s.split_at(s.len().checked_sub(1)?);
    let n: f64 = num.parse().ok()?;
    let mult = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86400,
        _ => return None,
    };
    Some((n * mult as f64) as i64)
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrequencySpec {
    pub seconds: i64,
    pub days: i64,
    pub months: i64,
    /// 0 = Monday .. 6 = Sunday; when set, periods only count on that weekday.
    pub day_of_week: Option<u8>,
    pub next_at: DateTime<Utc>,
    pub finish_at: Option<DateTime<Utc>>,
    /// true: return all elapsed periods (catch-up); false: return at most 1.
    pub catch_up: bool,
}

impl FrequencySpec {
    fn period_is_zero(&self) -> bool {
        self.seconds <= 0 && self.days <= 0 && self.months <= 0
    }

    fn advance(&self, t: DateTime<Utc>) -> DateTime<Utc> {
        let mut t = t;
        if self.months > 0 {
            t = t + Months::new(self.months as u32);
        }
        if self.days > 0 {
            t = t + Days::new(self.days as u64);
        }
        if self.seconds > 0 {
            t += TimeDelta::seconds(self.seconds);
        }
        t
    }

    /// Every fire boundary in `(from, to]` — Imagination's virtual timer wheel
    /// (blueprint XII: same math as the real tick, on a virtual clock).
    pub fn boundaries(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<DateTime<Utc>> {
        let mut out = Vec::new();
        if self.period_is_zero() {
            return out;
        }
        let mut next = self.next_at;
        let mut guard = 0;
        while next <= to && guard < 100_000 {
            if let Some(finish) = self.finish_at {
                if next > finish {
                    break;
                }
            }
            if next > from
                && self
                    .day_of_week
                    .is_none_or(|dow| next.weekday().num_days_from_monday() as u8 == dow)
            {
                out.push(next);
            }
            next = self.advance(next);
            guard += 1;
        }
        out
    }

    /// Fire if due. Returns `(periods, new_next_at)`; None when not due, when
    /// past `finish_at`, or when the period is degenerate (would never advance).
    pub fn fire(&self, now: DateTime<Utc>) -> Option<(f64, DateTime<Utc>)> {
        if self.period_is_zero() || self.next_at > now {
            return None;
        }
        if let Some(finish) = self.finish_at {
            if self.next_at > finish {
                return None;
            }
        }
        let mut next = self.next_at;
        let mut elapsed: i64 = 0;
        while next <= now && elapsed < 100_000 {
            if self
                .day_of_week
                .is_none_or(|dow| next.weekday().num_days_from_monday() as u8 == dow)
            {
                elapsed += 1;
            }
            next = self.advance(next);
        }
        if elapsed == 0 {
            // due datetimes existed but none matched day_of_week: just advance
            return Some((0.0, next));
        }
        let periods = if self.catch_up { elapsed } else { 1 };
        Some((periods as f64, next))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn durations_parse() {
        assert_eq!(parse_duration("90s"), Some(90));
        assert_eq!(parse_duration("5m"), Some(300));
        assert_eq!(parse_duration("2h"), Some(7200));
        assert_eq!(parse_duration("30d"), Some(30 * 86400));
        assert_eq!(parse_duration("nope"), None);
    }

    fn daily(next: &str, catch_up: bool) -> FrequencySpec {
        FrequencySpec {
            seconds: 0,
            days: 1,
            months: 0,
            day_of_week: None,
            next_at: at(next),
            finish_at: None,
            catch_up,
        }
    }

    #[test]
    fn not_due_returns_none() {
        assert!(
            daily("2026-07-06T07:00:00Z", false)
                .fire(at("2026-07-05T10:00:00Z"))
                .is_none()
        );
    }

    #[test]
    fn due_once_advances_past_now() {
        let (p, next) = daily("2026-07-05T07:00:00Z", false)
            .fire(at("2026-07-05T10:00:00Z"))
            .unwrap();
        assert_eq!(p, 1.0);
        assert_eq!(next, at("2026-07-06T07:00:00Z"));
    }

    #[test]
    fn catch_up_counts_missed_periods() {
        // 4 days behind: catch_up=true returns 4, false returns 1 — both land in the future.
        let now = at("2026-07-05T10:00:00Z");
        let (p, next) = daily("2026-07-01T07:00:00Z", true).fire(now).unwrap();
        assert_eq!(p, 5.0); // 1st..5th inclusive
        assert!(next > now);
        let (p, _) = daily("2026-07-01T07:00:00Z", false).fire(now).unwrap();
        assert_eq!(p, 1.0);
    }

    #[test]
    fn month_plus_day_period() {
        // the LINCE.md example: one month plus one day
        let spec = FrequencySpec {
            seconds: 0,
            days: 1,
            months: 1,
            day_of_week: None,
            next_at: at("2026-01-01T10:00:00Z"),
            finish_at: None,
            catch_up: false,
        };
        let (_, next) = spec.fire(at("2026-01-01T10:00:00Z")).unwrap();
        assert_eq!(next, at("2026-02-02T10:00:00Z"));
    }

    #[test]
    fn day_of_week_filter() {
        // 2026-07-05 is a Sunday (dow 6); a Monday-only weekly check due Sunday counts 0
        let spec = FrequencySpec {
            seconds: 0,
            days: 1,
            months: 0,
            day_of_week: Some(0), // Monday
            next_at: at("2026-07-05T07:00:00Z"),
            finish_at: None,
            catch_up: true,
        };
        let (p, next) = spec.fire(at("2026-07-05T10:00:00Z")).unwrap();
        assert_eq!(p, 0.0);
        assert!(next > at("2026-07-05T10:00:00Z"));
    }
}
