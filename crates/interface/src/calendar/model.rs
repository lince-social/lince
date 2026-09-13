use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

pub fn parse(value: &str) -> Option<NaiveDate> {
    if value.len() != 10 {
        return None;
    }
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
    ((1..=9999).contains(&date.year()) && date.format("%Y-%m-%d").to_string() == value)
        .then_some(date)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calendar {
    pub year: i32,
    pub month: u32,
    pub start: Option<String>,
    pub end: Option<String>,
    pub selecting_end: bool,
    pub area: Option<String>,
}

impl Default for Calendar {
    fn default() -> Self {
        let today = chrono::Local::now().date_naive();
        Self {
            year: today.year(),
            month: today.month(),
            start: None,
            end: None,
            selecting_end: false,
            area: None,
        }
    }
}

impl Calendar {
    pub fn valid(&self) -> bool {
        (1..=9999).contains(&self.year)
            && NaiveDate::from_ymd_opt(self.year, self.month, 1).is_some()
            && self.start.as_deref().is_none_or(|v| parse(v).is_some())
            && self.end.as_deref().is_none_or(|v| parse(v).is_some())
            && self
                .area
                .as_ref()
                .is_none_or(|id| !id.is_empty() && id.len() <= 128)
            && self
                .start
                .as_ref()
                .zip(self.end.as_ref())
                .is_none_or(|(a, b)| a <= b)
    }

    pub fn move_months(&mut self, delta: i32) {
        let index = ((i64::from(self.year) - 1) * 12 + i64::from(self.month) - 1
            + i64::from(delta))
        .clamp(0, 9999 * 12 - 1);
        self.year = (index / 12 + 1) as i32;
        self.month = (index % 12 + 1) as u32;
    }

    pub fn days(&self) -> Vec<Option<NaiveDate>> {
        let Some(first) = NaiveDate::from_ymd_opt(self.year, self.month, 1) else {
            return Vec::new();
        };
        let offset = first.weekday().num_days_from_monday() as usize;
        (0usize..42)
            .map(|index| {
                index
                    .checked_sub(offset)
                    .and_then(|day| NaiveDate::from_ymd_opt(self.year, self.month, day as u32 + 1))
            })
            .collect()
    }

    pub fn select(&mut self, date: &str) -> Result<(), &'static str> {
        if parse(date).is_none() {
            return Err("Choose a valid date");
        }
        if self.selecting_end {
            if self.start.as_deref().is_some_and(|start| start > date) {
                return Err("End is before start");
            }
            self.end = Some(date.into());
        } else {
            if self.end.as_deref().is_some_and(|end| end < date) {
                return Err("Start is after end");
            }
            self.start = Some(date.into());
        }
        Ok(())
    }
}

pub(super) fn span(row: &serde_json::Value) -> Option<(NaiveDate, NaiveDate)> {
    for property in ["start_date", "due_date"] {
        if !row[property].is_null() && row[property].as_str().and_then(parse).is_none() {
            return None;
        }
    }
    let start = row["start_date"].as_str().and_then(parse);
    let end = row["due_date"].as_str().and_then(parse);
    match (start, end) {
        (Some(a), Some(b)) if a <= b => Some((a, b)),
        (Some(date), None) | (None, Some(date)) => Some((date, date)),
        _ => None,
    }
}
