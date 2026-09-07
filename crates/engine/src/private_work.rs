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
}

impl fmt::Display for WorkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotObject => "work metadata is not an object",
            Self::TooLarge => "work metadata exceeds its byte limit",
            Self::UnknownField => "work metadata contains an unknown field",
            Self::InvalidDate => "work metadata contains an invalid calendar date",
            Self::InvalidEstimate => "work metadata contains an invalid estimate",
            Self::InvalidLogs => "work metadata logs are not an array",
            Self::TooManyLogs => "work metadata exceeds its log limit",
            Self::InvalidLog => "work metadata contains an invalid log entry",
            Self::UnknownLogField => "work metadata log contains an unknown field",
            Self::TimestampTooLong => "work metadata timestamp exceeds its byte limit",
            Self::InvalidTimestamp => "work metadata contains an invalid timestamp",
            Self::EndBeforeStart => "work metadata log ends before it starts",
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
    start: Option<NaiveDate>,
    due: Option<NaiveDate>,
    estimate_minutes: Option<f64>,
    logs: Vec<WorkLog>,
}

impl WorkMetadata {
    pub fn parse(value: &Value) -> Result<Self, WorkError> {
        let object = value.as_object().ok_or(WorkError::NotObject)?;
        validate_shape(object)?;
        encoded_size(value)?;
        let start = optional_date(object.get("start"))?;
        let due = optional_date(object.get("due"))?;
        let estimate_minutes = optional_estimate(object.get("estimate_min"))?;
        let logs = match object.get("logs") {
            None => Vec::new(),
            Some(Value::Array(entries)) => {
                entries.iter().map(parse_log).collect::<Result<_, _>>()?
            }
            Some(_) => return Err(WorkError::InvalidLogs),
        };
        Ok(Self {
            start,
            due,
            estimate_minutes,
            logs,
        })
    }

    pub fn start(&self) -> Option<&NaiveDate> {
        self.start.as_ref()
    }

    pub fn due(&self) -> Option<&NaiveDate> {
        self.due.as_ref()
    }

    pub fn estimate_minutes(&self) -> Option<f64> {
        self.estimate_minutes
    }

    pub fn logs(&self) -> &[WorkLog] {
        &self.logs
    }
}

impl TryFrom<&Value> for WorkMetadata {
    type Error = WorkError;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

fn validate_shape(object: &Map<String, Value>) -> Result<(), WorkError> {
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "start" | "due" | "estimate_min" | "logs"))
    {
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
        let object = log.as_object().ok_or(WorkError::InvalidLog)?;
        if object
            .keys()
            .any(|key| !matches!(key.as_str(), "start" | "end"))
        {
            return Err(WorkError::UnknownLogField);
        }
        if !matches!(object.get("start"), Some(Value::String(_))) {
            return Err(WorkError::InvalidLog);
        }
        if !matches!(
            object.get("end"),
            None | Some(Value::Null | Value::String(_))
        ) {
            return Err(WorkError::InvalidLog);
        }
    }
    Ok(())
}

fn optional_date(value: Option<&Value>) -> Result<Option<NaiveDate>, WorkError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Some(value) = value.as_str() else {
        if value.is_null() {
            return Ok(None);
        }
        return Err(WorkError::InvalidDate);
    };
    if value.len() != 10
        || value.as_bytes()[4] != b'-'
        || value.as_bytes()[7] != b'-'
        || value
            .bytes()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return Err(WorkError::InvalidDate);
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(Some)
        .map_err(|_| WorkError::InvalidDate)
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
