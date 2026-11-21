use std::borrow::Cow;

use super::{MAX_INSTANT_MS, TimeRange, TimeValue};
use crate::performance::{self, Span};

const MAX_ESTIMATE_MINUTES: f64 = 1_000_000_000.0;
const MILLIS_PER_MINUTE: f64 = 60_000.0;

/// Validated start and due values that borrow prepared input or own decoded input.
#[derive(Debug)]
pub struct Endpoints<'a> {
    start: Option<Cow<'a, TimeValue>>,
    due: Option<Cow<'a, TimeValue>>,
}

impl<'a> Endpoints<'a> {
    /// Checks the order of two borrowed endpoints.
    ///
    /// # Errors
    ///
    /// Returns an error if due precedes start or a local date overflows.
    pub fn new(start: Option<&'a TimeValue>, due: Option<&'a TimeValue>) -> Result<Self, String> {
        Self::checked(start.map(Cow::Borrowed), due.map(Cow::Borrowed))
    }

    fn checked(
        start: Option<Cow<'a, TimeValue>>,
        due: Option<Cow<'a, TimeValue>>,
    ) -> Result<Self, String> {
        if let (Some(start), Some(due)) = (start.as_deref(), due.as_deref()) {
            let reversed = match (start.instant_ms(), due.instant_ms()) {
                (Some(start), Some(due)) => start > due,
                _ => {
                    let date = |value: &TimeValue| {
                        value
                            .checked_stored_date()
                            .ok_or("Scheduling timestamp is outside the supported range")
                    };
                    date(start)? > date(due)?
                }
            };
            if reversed {
                return Err("Due time must not be before start time".into());
            }
        }
        Ok(Self { start, due })
    }

    /// Borrows the validated start value, if present.
    pub fn start(&self) -> Option<&TimeValue> {
        self.start.as_deref()
    }

    /// Borrows the validated due value, if present.
    pub fn due(&self) -> Option<&TimeValue> {
        self.due.as_deref()
    }

    /// Calculates a window, using a supplied occurrence as the start when available.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid estimate or bounds outside the supported range.
    pub fn range(
        &self,
        estimate_minutes: Option<f64>,
        occurrence_ms: Option<i64>,
    ) -> Result<Option<TimeRange>, String> {
        let start = self.start().and_then(TimeValue::instant_ms);
        let due = self.due().and_then(TimeValue::instant_ms);
        let estimate = estimate_minutes.unwrap_or(0.0);
        if !estimate.is_finite() || !(0.0..=MAX_ESTIMATE_MINUTES).contains(&estimate) {
            return Err("Estimate must be a finite, nonnegative number of minutes".into());
        }
        let duration = if let (Some(start), Some(due)) = (start, due) {
            due.checked_sub(start).ok_or("Time range overflow")?
        } else {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "the rounded nonnegative estimate is bounded at 60 trillion milliseconds"
            )]
            let estimated_ms = (estimate * MILLIS_PER_MINUTE).round() as i64;
            estimated_ms
        };
        let (from_ms, until_ms) = match (occurrence_ms, start, due) {
            (Some(from), _, _) | (None, Some(from), None) => (from, from.checked_add(duration)),
            (None, Some(from), Some(until)) => (from, Some(until)),
            (None, None, Some(until)) => (
                until.checked_sub(duration).ok_or("Time range overflow")?,
                Some(until),
            ),
            (None, None, None) => return Ok(None),
        };
        let until_ms = until_ms.ok_or("Time range overflow")?;
        if from_ms < 0 || until_ms > MAX_INSTANT_MS {
            return Err("Time range is outside the supported range".into());
        }
        Ok(Some(TimeRange {
            from_ms,
            until_ms: (until_ms > from_ms).then_some(until_ms),
        }))
    }
}

impl Endpoints<'static> {
    /// Parses optional stored text and checks endpoint order.
    ///
    /// # Errors
    ///
    /// Returns an error if either endpoint is invalid or the due value is earlier.
    pub fn parse(start: Option<&str>, due: Option<&str>) -> Result<Self, String> {
        let _prepare = Span::new("schedule-endpoint-preparation");
        performance::add("schedule-endpoint-parses", 1);
        let start = start.map(TimeValue::parse).transpose()?.map(Cow::Owned);
        let due = due.map(TimeValue::parse).transpose()?.map(Cow::Owned);
        Self::checked(start, due)
    }
}

/// Validates stored endpoint text and ordering.
///
/// # Errors
///
/// Returns the same errors as [Endpoints::parse].
pub fn validate_endpoints(start: Option<&str>, due: Option<&str>) -> Result<(), String> {
    Endpoints::parse(start, due).map(|_| ())
}

/// Parses stored bounds and computes the resulting timeline range.
///
/// # Errors
///
/// Returns endpoint errors, an invalid estimate, or bounds outside the supported range.
pub fn range(
    start: Option<&str>,
    due: Option<&str>,
    estimate_minutes: Option<f64>,
    occurrence_ms: Option<i64>,
) -> Result<Option<TimeRange>, String> {
    Endpoints::parse(start, due)?.range(estimate_minutes, occurrence_ms)
}
