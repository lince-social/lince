// non-lean-yet
// Scope: fn default_timezone_revision, enum DateKind, struct DateDraft, struct Draft.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 18, 20, 97, 125.
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 62, 72, 78,
//   92.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites from_value:
//   String (200), input: String (58), inputs: String (165).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   144, 148, 171, 180, 215, 216, 230, 233.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (16).

use nucleus::karma::rule_field::RuleConsequence;
use nucleus::karma::scheduled_change::{BoundaryInput, DateInput, Purpose};
use nucleus::karma::{
    CivilDateTime, FoldPolicy, GapPolicy, TimeZoneId, TimeZoneProvider, TzdbRevision,
};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

fn default_timezone_revision() -> TzdbRevision {
    static REVISION: OnceLock<TzdbRevision> = OnceLock::new();
    REVISION
        .get_or_init(|| {
            let _prepare = nucleus::performance::Span::new("karma-schedule-default-timezone");
            nucleus::performance::add("karma-schedule-default-timezones-prepared", 1);
            nucleus::karma::simple_frequency::utc_provider()
                .unwrap()
                .revision()
                .clone()
        })
        .clone()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DateKind {
    Instant,
    After,
    Local,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateDraft {
    pub kind: DateKind,
    pub value: String,
    pub timezone: String,
    pub tzdb: TzdbRevision,
    pub gap: GapPolicy,
    pub fold: FoldPolicy,
}

impl Default for DateDraft {
    fn default() -> Self {
        Self::with_value(DateKind::After, "1d".into())
    }
}

impl DateDraft {
    fn with_value(kind: DateKind, value: String) -> Self {
        Self {
            kind,
            value,
            timezone: "UTC".into(),
            tzdb: default_timezone_revision(),
            gap: GapPolicy::Pause,
            fold: FoldPolicy::First,
        }
    }

    pub fn input(&self) -> Result<DateInput, String> {
        match self.kind {
            DateKind::Instant => Ok(DateInput::Instant {
                at_ms: chrono::DateTime::parse_from_rfc3339(self.value.trim())
                    .map_err(|_| {
                        "Use a date with its UTC offset, such as 2030-01-01T09:00:00-03:00"
                            .to_string()
                    })?
                    .timestamp_millis(),
            }),
            DateKind::After => {
                let milliseconds = if let Some(value) = self.value.trim().strip_suffix("ms") {
                    value
                        .parse()
                        .map_err(|_| "Use a duration such as 3d, 2h or 500ms")?
                } else {
                    u64::try_from(
                        nucleus::parse_duration(self.value.trim())
                            .ok_or("Use a duration such as 3d, 2h or 500ms")?,
                    )
                    .map_err(|_| "Use a positive duration")?
                    .checked_mul(1000)
                    .ok_or("Duration is too large")?
                };
                Ok(DateInput::After { milliseconds })
            }
            DateKind::Local => {
                let value = chrono::NaiveDateTime::parse_from_str(
                    self.value.trim(),
                    "%Y-%m-%dT%H:%M:%S%.f",
                )
                .or_else(|_| {
                    chrono::NaiveDateTime::parse_from_str(self.value.trim(), "%Y-%m-%dT%H:%M")
                })
                .map_err(|_| "Use a local date and time such as 2030-01-01T09:00")?;
                Ok(DateInput::Local {
                    date: CivilDateTime::from_naive(value).map_err(|error| error.to_string())?,
                    timezone: TimeZoneId::new(self.timezone.trim())
                        .map_err(|error| error.to_string())?,
                    tzdb: self.tzdb.clone(),
                    gap: self.gap,
                    fold: self.fold,
                })
            }
        }
    }

    pub fn from_input(input: &DateInput) -> Self {
        match input {
            DateInput::Instant { at_ms } => Self::with_value(
                DateKind::Instant,
                chrono::DateTime::from_timestamp_millis(*at_ms)
                    .map_or_else(String::new, |date| date.to_rfc3339()),
            ),
            DateInput::After { milliseconds } => {
                Self::with_value(DateKind::After, format!("{milliseconds}ms"))
            }
            DateInput::Local {
                date,
                timezone,
                tzdb,
                gap,
                fold,
            } => Self {
                kind: DateKind::Local,
                value: date.to_string(),
                timezone: timezone.as_str().into(),
                tzdb: tzdb.clone(),
                gap: *gap,
                fold: *fold,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub uid: Option<String>,
    pub revision: Option<i64>,
    pub name: String,
    pub range: bool,
    pub dates: [DateDraft; 2],
    pub consequences: [String; 2],
}

fn default_date(index: usize) -> DateDraft {
    DateDraft::with_value(DateKind::After, ["1d", "2d"][index].into())
}

fn default_consequence(index: usize) -> String {
    ["@room = -1", "@room = 0"][index].into()
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            uid: None,
            revision: None,
            name: "Scheduled change".into(),
            range: false,
            dates: std::array::from_fn(default_date),
            consequences: std::array::from_fn(default_consequence),
        }
    }
}

impl Draft {
    pub fn inputs(&self) -> Result<Vec<BoundaryInput>, String> {
        if !self.valid() || self.name.trim().is_empty() {
            return Err("Enter a schedule name and valid fields".into());
        }
        (0..if self.range { 2 } else { 1 })
            .map(|index| {
                let consequence = RuleConsequence::parse(&self.consequences[index])?;
                Ok(BoundaryInput {
                    purpose: if !self.range {
                        Purpose::Once
                    } else if index == 0 {
                        Purpose::Start
                    } else {
                        Purpose::End
                    },
                    date: self.dates[index].input()?,
                    target: consequence.target,
                    consequences: consequence.consequences,
                })
            })
            .collect()
    }

    pub fn valid(&self) -> bool {
        self.name.len() <= 256
            && self
                .consequences
                .iter()
                .all(|source| source.len() <= 16_384)
            && self
                .dates
                .iter()
                .all(|date| date.value.len() <= 256 && date.timezone.len() <= 255)
    }

    pub fn from_value(value: &serde_json::Value) -> Result<Self, String> {
        let mut range = false;
        let mut dates = [None, None];
        let mut consequences = [None, None];
        let boundaries = value["boundaries"]
            .as_array()
            .ok_or("Schedule boundaries are unavailable")?;
        for boundary in boundaries
            .iter()
            .filter(|boundary| boundary["current"] == true)
        {
            let input = BoundaryInput::deserialize(&boundary["input"])
                .map_err(|error| error.to_string())?;
            let index = usize::from(input.purpose == Purpose::End);
            range |= input.purpose != Purpose::Once;
            dates[index] = Some(DateDraft::from_input(&input.date));
            consequences[index] = Some(
                RuleConsequence {
                    target: input.target,
                    consequences: input.consequences,
                }
                .as_text(),
            );
        }
        Ok(Self {
            uid: value["uid"].as_str().map(str::to_owned),
            revision: value["revision"].as_i64(),
            name: value["name"].as_str().unwrap_or_default().into(),
            range,
            dates: std::array::from_fn(|index| {
                dates[index].take().unwrap_or_else(|| default_date(index))
            }),
            consequences: std::array::from_fn(|index| {
                consequences[index]
                    .take()
                    .unwrap_or_else(|| default_consequence(index))
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_drafts_keep_explicit_zero_and_preserve_the_authored_date() {
        let draft = Draft {
            range: true,
            ..Default::default()
        };
        let inputs = draft.inputs().unwrap();
        assert_eq!(inputs[1].purpose, Purpose::End);
        assert_eq!(
            inputs[1].consequences,
            vec![nucleus::karma::Consequence::SetQuantity {
                value: Some(nucleus::DecimalValue::parse_inferred("0").unwrap())
            }]
        );
        for input in inputs {
            assert_eq!(
                DateDraft::from_input(&input.date).input().unwrap(),
                input.date
            );
        }
    }
}
