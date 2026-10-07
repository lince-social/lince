use reqwest::header::{HeaderMap, RETRY_AFTER};
use std::time::{Duration, Instant, SystemTime};

pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(60);

pub fn delay_at(headers: &HeaderMap, now: SystemTime) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    if value.is_empty() {
        return None;
    }
    if value.bytes().all(|byte| byte.is_ascii_digit()) {
        let seconds = value.bytes().fold(0u64, |seconds, byte| {
            seconds
                .saturating_mul(10)
                .saturating_add(u64::from(byte - b'0'))
                .min(MAX_RETRY_AFTER.as_secs())
        });
        return Some(Duration::from_secs(seconds));
    }
    Some(
        httpdate::parse_http_date(value)
            .ok()?
            .duration_since(now)
            .unwrap_or(Duration::ZERO)
            .min(MAX_RETRY_AFTER),
    )
}

pub fn deadline(headers: &HeaderMap, attempt: u32) -> Instant {
    Instant::now()
        + delay_at(headers, SystemTime::now())
            .unwrap_or_else(|| Duration::from_millis(250 * (1 << attempt.min(4))))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retry_headers_handle_overflow_dates_and_malformed_values() {
        for (value, expected) in [
            ("7", Some(Duration::from_secs(7))),
            (
                "9999999999999999999999999999999999999999999",
                Some(MAX_RETRY_AFTER),
            ),
            ("invalid", None),
            ("", None),
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(RETRY_AFTER, value.parse().unwrap());
            assert_eq!(delay_at(&headers, SystemTime::UNIX_EPOCH), expected);
        }
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1000000);
        for (seconds, expected) in [(12, 12), (3600, 60)] {
            let mut headers = HeaderMap::new();
            headers.insert(
                RETRY_AFTER,
                httpdate::fmt_http_date(now + Duration::from_secs(seconds))
                    .parse()
                    .unwrap(),
            );
            assert_eq!(delay_at(&headers, now), Some(Duration::from_secs(expected)));
            assert_eq!(
                delay_at(&headers, now + Duration::from_secs(4000)),
                Some(Duration::ZERO)
            );
        }
    }
}
