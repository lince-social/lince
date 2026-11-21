//! Retry deadlines for provider responses.

use std::time::{Duration, Instant, SystemTime};

use reqwest::header::{HeaderMap, RETRY_AFTER};

/// Maximum delay accepted from a provider's `Retry-After` header.
pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(60);

/// Reads a delay in seconds or an HTTP date. Past dates are due immediately.
pub fn delay_at(headers: &HeaderMap, now: SystemTime) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    if value.is_empty() {
        return None;
    }
    // Every previous step is capped at 60 seconds, so the arithmetic fits in u64.
    if let Some(seconds) = value.bytes().try_fold(0, |seconds, byte| match byte {
        b'0'..=b'9' => Some((seconds * 10 + u64::from(byte - b'0')).min(MAX_RETRY_AFTER.as_secs())),
        _ => None,
    }) {
        return Some(Duration::from_secs(seconds));
    }
    Some(
        httpdate::parse_http_date(value)
            .ok()?
            .duration_since(now)
            .unwrap_or_default()
            .min(MAX_RETRY_AFTER),
    )
}

/// Uses the provider's delay or falls back to bounded exponential backoff.
pub fn deadline(headers: &HeaderMap, attempt: u32) -> Instant {
    Instant::now()
        + delay_at(headers, SystemTime::now())
            .unwrap_or_else(|| Duration::from_millis(250 * (1 << attempt.min(4))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

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
            headers.insert(RETRY_AFTER, value.parse().expect("ASCII header"));
            assert_eq!(delay_at(&headers, UNIX_EPOCH), expected);
        }
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        for (seconds, expected) in [(12, 12), (3_600, 60)] {
            let mut headers = HeaderMap::new();
            headers.insert(
                RETRY_AFTER,
                httpdate::fmt_http_date(now + Duration::from_secs(seconds))
                    .parse()
                    .expect("formatted HTTP date"),
            );
            assert_eq!(delay_at(&headers, now), Some(Duration::from_secs(expected)));
            assert_eq!(
                delay_at(&headers, now + Duration::from_secs(4_000)),
                Some(Duration::ZERO)
            );
        }
    }
}
