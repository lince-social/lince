// non-lean-yet
// Scope: const MAX_WORK_BYTES, const MAX_WORK_LOGS, const MAX_WORK_TIMESTAMP_BYTES, const
//   MAX_ESTIMATE_MINUTES.
// BeautifulRust (baseline lines):
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 126, 130,
//   141, 265, 314.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites time_range:
//   String (96).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   325.
// - [Clippy probe: clippy::as_conversions] Use From or TryFrom when it preserves the conversion
//   contract. Sites 146.
// - [Clippy probe: clippy::exhaustive_enums] Review whether public API evolution requires
//   non_exhaustive. Sites 12.
// - [Clippy probe: clippy::indexing_slicing] Verify bounds or use checked access on fallible
//   inputs. Sites 325, 326, 327, 328, 329, 330, 331.
// - [Clippy probe: clippy::map_err_ignore] Preserve relevant causes and context at the boundary.
//   Sites 126, 130, 141, 265, 314.
// - [Clippy probe: clippy::match_same_arms] these match arms have identical bodies. Sites 339, 340.
// - [Clippy probe: clippy::missing_errors_doc] State the observable failure cases for exported
//   functions. Sites 78, 83, 87, 96.
// - [rustc probe: missing_docs] Document the externally visible contracts where applicable. Sites
//   6, 7, 8, 9, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 53, 59, 63, 69, 78, 83, 87,
//   96, 165, 171, 177, 183, 189, 193, 197.

use std::fmt;

use chrono::{DateTime, FixedOffset, NaiveDate};
use serde_json::{Map, Value};

pub const MAX_WORK_BYTES: usize = 1024 * 1024;
pub const MAX_WORK_LOGS: usize = 4096;
pub const MAX_WORK_TIMESTAMP_BYTES: usize = 64;
pub const MAX_ESTIMATE_MINUTES: f64 = 1_000_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkError {
    NotObject,
    TooLarge,
    UnknownField,
    InvalidDate,
    InvalidEstimate,
    InvalidLogs,
    TooManyLogs,
    InvalidLog,
    UnknownLogField,
    TimestampTooLong,
    InvalidTimestamp,
    EndBeforeStart,
    InvalidOccurrenceLink,
}

impl fmt::Display for WorkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotObject => "work metadata is not an object",
            Self::TooLarge => "work metadata exceeds its byte limit",
            Self::UnknownField => "work metadata contains an unknown field",
            Self::InvalidDate => "work metadata contains an invalid date or scheduled timestamp",
            Self::InvalidEstimate => "work metadata contains an invalid estimate",
            Self::InvalidLogs => "work metadata logs are not an array",
            Self::TooManyLogs => "work metadata exceeds its log limit",
            Self::InvalidLog => "work metadata contains an invalid log entry",
            Self::UnknownLogField => "work metadata log contains an unknown field",
            Self::TimestampTooLong => "work metadata timestamp exceeds its byte limit",
            Self::InvalidTimestamp => "work metadata contains an invalid timestamp",
            Self::EndBeforeStart => "work metadata range ends before it starts",
            Self::InvalidOccurrenceLink => {
                "work metadata contains an invalid projection occurrence link"
            }
        })
    }
}

impl std::error::Error for WorkError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkLog {
    start: DateTime<FixedOffset>,
    end: Option<DateTime<FixedOffset>>,
}

impl WorkLog {
    pub fn start(&self) -> &DateTime<FixedOffset> {
        &self.start
    }

    pub fn end(&self) -> Option<&DateTime<FixedOffset>> {
        self.end.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkMetadata {
    start: Option<nucleus::schedule::TimeValue>,
    due: Option<nucleus::schedule::TimeValue>,
    estimate_minutes: Option<f64>,
    logs: Vec<WorkLog>,
    occurrence: Option<nucleus::projection::OccurrenceLink>,
}

impl WorkMetadata {
    pub fn parse(value: &Value) -> Result<Self, WorkError> {
        nucleus::performance::add("work-metadata-parse-requests", 1);
        Self::read(value, |entries| entries.iter().map(parse_log).collect())
    }

    pub fn validate(value: &Value) -> Result<(), WorkError> {
        Self::read_without_logs(value).map(|_| ())
    }

    pub fn validate_log(value: &Value) -> Result<(), WorkError> {
        let _validation = nucleus::performance::Span::new("work-log-validation");
        nucleus::performance::add("work-log-validation-requests", 1);
        validate_log_shape(value)?;
        let mut bytes = encoded_size(value)?;
        add_size(&mut bytes, b"{\"logs\":[]}".len())?;
        parse_log(value).map(|_| ())
    }

    pub fn time_range(
        value: &Value,
        occurrence_ms: Option<i64>,
    ) -> Result<Option<nucleus::schedule::TimeRange>, String> {
        let work = Self::read_without_logs(value).map_err(|error| error.to_string())?;
        nucleus::schedule::Endpoints::new(work.start.as_ref(), work.due.as_ref())?
            .range(work.estimate_minutes, occurrence_ms)
    }

    fn read_without_logs(value: &Value) -> Result<Self, WorkError> {
        nucleus::performance::add("work-metadata-validation-requests", 1);
        Self::read(value, |entries| {
            for entry in entries {
                parse_log(entry)?;
            }
            Ok(Vec::new())
        })
    }

    fn read(
        value: &Value,
        logs: impl FnOnce(&[Value]) -> Result<Vec<WorkLog>, WorkError>,
    ) -> Result<Self, WorkError> {
        let _preparation = nucleus::performance::Span::new("work-metadata-preparation");
        let object = value.as_object().ok_or(WorkError::NotObject)?;
        validate_shape(object)?;
        encoded_size(value)?;
        let start = optional_date(object.get("start"))?;
        let due = optional_date(object.get("due"))?;
        let endpoints = nucleus::schedule::Endpoints::new(start.as_ref(), due.as_ref())
            .map_err(|_| WorkError::EndBeforeStart)?;
        let estimate_minutes = optional_estimate(object.get("estimate_min"))?;
        endpoints
            .range(estimate_minutes, None)
            .map_err(|_| WorkError::InvalidTimestamp)?;
        let logs = match object.get("logs") {
            None => Vec::new(),
            Some(Value::Array(entries)) => logs(entries)?,
            Some(_) => return Err(WorkError::InvalidLogs),
        };
        let occurrence = match object.get("projection_occurrence") {
            None | Some(Value::Null) => None,
            Some(value) => {
                let link =
                    <nucleus::projection::OccurrenceLink as serde::Deserialize>::deserialize(value)
                        .map_err(|_| WorkError::InvalidOccurrenceLink)?;
                if link.record.kind() != nucleus::karma::ReferenceKind::Record
                    || !nucleus::valid_uid(&link.occurrence.rule_uid, "rec")
                    || link.occurrence.event_id.is_empty()
                    || link.occurrence.event_id.len() > 1024
                    || link.occurrence.revision > i64::MAX as u64
                    || link.occurrence.intended_at_ms.is_some_and(|time| {
                        !(0..=nucleus::schedule::MAX_INSTANT_MS).contains(&time)
                    })
                {
                    return Err(WorkError::InvalidOccurrenceLink);
                }
                Some(link)
            }
        };
        Ok(Self {
            start,
            due,
            estimate_minutes,
            logs,
            occurrence,
        })
    }

    pub fn start(&self) -> Option<NaiveDate> {
        self.start
            .as_ref()
            .map(nucleus::schedule::TimeValue::stored_date)
    }

    pub fn due(&self) -> Option<NaiveDate> {
        self.due
            .as_ref()
            .map(nucleus::schedule::TimeValue::stored_date)
    }

    pub fn start_ms(&self) -> Option<i64> {
        self.start
            .as_ref()
            .and_then(nucleus::schedule::TimeValue::instant_ms)
    }

    pub fn due_ms(&self) -> Option<i64> {
        self.due
            .as_ref()
            .and_then(nucleus::schedule::TimeValue::instant_ms)
    }

    pub fn estimate_minutes(&self) -> Option<f64> {
        self.estimate_minutes
    }

    pub fn logs(&self) -> &[WorkLog] {
        &self.logs
    }

    pub fn occurrence(&self) -> Option<&nucleus::projection::OccurrenceLink> {
        self.occurrence.as_ref()
    }
}

impl TryFrom<&Value> for WorkMetadata {
    type Error = WorkError;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

fn validate_shape(object: &Map<String, Value>) -> Result<(), WorkError> {
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "start" | "due" | "estimate_min" | "logs" | "projection_occurrence"
        )
    }) {
        return Err(WorkError::UnknownField);
    }
    let Some(logs) = object.get("logs") else {
        return Ok(());
    };
    let Value::Array(logs) = logs else {
        return Err(WorkError::InvalidLogs);
    };
    if logs.len() > MAX_WORK_LOGS {
        return Err(WorkError::TooManyLogs);
    }
    for log in logs {
        validate_log_shape(log)?;
    }
    Ok(())
}

fn validate_log_shape(value: &Value) -> Result<(), WorkError> {
    let object = value.as_object().ok_or(WorkError::InvalidLog)?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "start" | "end"))
    {
        return Err(WorkError::UnknownLogField);
    }
    if !matches!(object.get("start"), Some(Value::String(_)))
        || !matches!(
            object.get("end"),
            None | Some(Value::Null | Value::String(_))
        )
    {
        return Err(WorkError::InvalidLog);
    }
    Ok(())
}

fn optional_date(value: Option<&Value>) -> Result<Option<nucleus::schedule::TimeValue>, WorkError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Some(value) = value.as_str() else {
        if value.is_null() {
            return Ok(None);
        }
        return Err(WorkError::InvalidDate);
    };
    nucleus::schedule::TimeValue::parse(value)
        .map(Some)
        .map_err(|_| {
            if value.contains('T') {
                WorkError::InvalidTimestamp
            } else {
                WorkError::InvalidDate
            }
        })
}

fn optional_estimate(value: Option<&Value>) -> Result<Option<f64>, WorkError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let estimate = value.as_f64().ok_or(WorkError::InvalidEstimate)?;
    if !estimate.is_finite() || !(0.0..=MAX_ESTIMATE_MINUTES).contains(&estimate) {
        return Err(WorkError::InvalidEstimate);
    }
    Ok(Some(estimate))
}

fn parse_log(value: &Value) -> Result<WorkLog, WorkError> {
    let object = value.as_object().ok_or(WorkError::InvalidLog)?;
    let start = parse_timestamp(
        object
            .get("start")
            .and_then(Value::as_str)
            .ok_or(WorkError::InvalidLog)?,
    )?;
    let end = match object.get("end") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) => Some(parse_timestamp(value)?),
        Some(_) => return Err(WorkError::InvalidLog),
    };
    if end.as_ref().is_some_and(|end| end < &start) {
        return Err(WorkError::EndBeforeStart);
    }
    Ok(WorkLog { start, end })
}

fn parse_timestamp(value: &str) -> Result<DateTime<FixedOffset>, WorkError> {
    if value.len() > MAX_WORK_TIMESTAMP_BYTES {
        return Err(WorkError::TimestampTooLong);
    }
    if !has_explicit_offset(value) {
        return Err(WorkError::InvalidTimestamp);
    }
    DateTime::parse_from_rfc3339(value).map_err(|_| WorkError::InvalidTimestamp)
}

fn has_explicit_offset(value: &str) -> bool {
    if value.ends_with('Z') || value.ends_with('z') {
        return true;
    }
    let bytes = value.as_bytes();
    if bytes.len() < 6 {
        return false;
    }
    let offset = &bytes[bytes.len() - 6..];
    matches!(offset[0], b'+' | b'-')
        && offset[1].is_ascii_digit()
        && offset[2].is_ascii_digit()
        && offset[3] == b':'
        && offset[4].is_ascii_digit()
        && offset[5].is_ascii_digit()
}

fn encoded_size(value: &Value) -> Result<usize, WorkError> {
    let mut size = 0usize;
    let mut values = vec![value];
    while let Some(value) = values.pop() {
        match value {
            Value::Null => add_size(&mut size, 4)?,
            Value::Bool(true) => add_size(&mut size, 4)?,
            Value::Bool(false) => add_size(&mut size, 5)?,
            Value::Number(number) => add_size(&mut size, number.to_string().len())?,
            Value::String(value) => add_size(&mut size, encoded_string_size(value)?)?,
            Value::Array(array) => {
                add_size(&mut size, 2)?;
                if !array.is_empty() {
                    add_size(&mut size, array.len() - 1)?;
                    values.extend(array.iter());
                }
            }
            Value::Object(object) => {
                add_size(&mut size, 2)?;
                if !object.is_empty() {
                    let punctuation = object
                        .len()
                        .checked_mul(2)
                        .and_then(|value| value.checked_sub(1))
                        .ok_or(WorkError::TooLarge)?;
                    add_size(&mut size, punctuation)?;
                    for (key, value) in object {
                        add_size(&mut size, encoded_string_size(key)?)?;
                        values.push(value);
                    }
                }
            }
        }
    }
    Ok(size)
}

fn encoded_string_size(value: &str) -> Result<usize, WorkError> {
    let mut size = 2usize;
    for character in value.chars() {
        let bytes = match character {
            '"' | '\\' | '\u{0008}' | '\t' | '\n' | '\u{000c}' | '\r' => 2,
            '\u{0000}'..='\u{001f}' => 6,
            _ => character.len_utf8(),
        };
        add_size(&mut size, bytes)?;
    }
    Ok(size)
}

fn add_size(size: &mut usize, amount: usize) -> Result<(), WorkError> {
    *size = size.checked_add(amount).ok_or(WorkError::TooLarge)?;
    if *size > MAX_WORK_BYTES {
        return Err(WorkError::TooLarge);
    }
    Ok(())
}
