use nucleus::karma::{
    Cadence, CanonicalHash, DurationBinding, FrequencyAst, FrequencyCadenceAst, Slug, TimestampMs,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Frequency {
    pub uid: String,
    pub slug: String,
    pub status: String,
    pub quantity: nucleus::DecimalValue,
    pub handle_revision: u64,
    pub head_revision_hash: CanonicalHash,
    pub active_revision_hash: Option<CanonicalHash>,
    pub last_run_revision_hash: Option<CanonicalHash>,
    pub last_run_parameters: Option<
        std::collections::BTreeMap<
            nucleus::karma::LocalId,
            nucleus::karma::FrequencyParameterValue,
        >,
    >,
    pub definition: FrequencyAst,
    pub next_at_ms: Option<i64>,
    pub next_local: Option<String>,
}

impl Frequency {
    pub fn same_layout(&self, other: &Self) -> bool {
        self.uid == other.uid
            && self.slug == other.slug
            && self.status == other.status
            && self.quantity == other.quantity
            && self.handle_revision == other.handle_revision
            && self.head_revision_hash == other.head_revision_hash
            && self.active_revision_hash == other.active_revision_hash
            && self.last_run_revision_hash == other.last_run_revision_hash
            && self.last_run_parameters == other.last_run_parameters
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unit {
    Milliseconds,
    Seconds,
    #[default]
    Minutes,
    Hours,
    Days,
    Weeks,
    Months,
    Years,
}

impl Unit {
    pub fn label(self) -> &'static str {
        match self {
            Self::Milliseconds => "ms",
            Self::Seconds => "seconds",
            Self::Minutes => "minutes",
            Self::Hours => "hours",
            Self::Days => "days",
            Self::Weeks => "weeks",
            Self::Months => "months",
            Self::Years => "years",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub uid: Option<String>,
    pub revision: Option<u64>,
    pub original: Option<FrequencyAst>,
    pub fields: [String; 4],
    pub original_fields: Option<[String; 4]>,
    pub cadence_editable: bool,
    pub weekdays: Vec<nucleus::karma::CivilWeekday>,
    pub original_weekdays: Vec<nucleus::karma::CivilWeekday>,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            uid: None,
            revision: None,
            original: None,
            fields: [
                String::new(),
                String::new(),
                "1 minute".into(),
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            ],
            original_fields: None,
            cadence_editable: true,
            weekdays: Vec::new(),
            original_weekdays: Vec::new(),
        }
    }
}

impl Draft {
    pub fn edit(row: &Frequency) -> Self {
        let mut draft = Self {
            uid: Some(row.uid.clone()),
            revision: Some(row.handle_revision),
            original: Some(row.definition.clone()),
            cadence_editable: false,
            ..Default::default()
        };
        draft.fields[0] = row.slug.clone();
        draft.fields[1] = row.definition.purpose.clone();
        if let FrequencyCadenceAst::Elapsed {
            interval: DurationBinding::Literal { value },
            anchor,
        } = &row.definition.cadence
        {
            draft.fields[2] = super::interval::elapsed(value.get());
            draft.fields[3] = chrono::DateTime::from_timestamp_millis(
                row.next_at_ms.unwrap_or(anchor.as_millis()),
            )
            .unwrap()
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            draft.cadence_editable = super::interval::parse(&draft.fields[2]).is_ok();
        }
        if let Ok(compiled) = row.definition.compile(&Default::default())
            && let nucleus::karma::CompiledSchedule::Calendar { schedule } = compiled.schedule
        {
            let step = schedule.cadence.every;
            draft.fields[2] = super::interval::describe(step);
            draft.fields[3] = row
                .next_local
                .clone()
                .unwrap_or_else(|| schedule.anchor.to_string());
            draft.weekdays = schedule
                .cadence
                .land_on
                .as_ref()
                .map(|days| days.iter().collect())
                .unwrap_or_default();
            draft.cadence_editable = row.definition.parameters.is_empty();
        }
        draft.original_fields = Some(draft.fields.clone());
        draft.original_weekdays = draft.weekdays.clone();
        draft
    }

    pub fn valid(&self) -> bool {
        self.weekdays.len() <= 7
            && self.fields.iter().all(|field| field.len() <= 32_768)
            && self.uid.as_ref().is_none_or(|uid| uid.len() <= 256)
            && self.uid.is_some() == self.revision.is_some()
    }

    pub fn definition(&self) -> Result<FrequencyAst, String> {
        if !self.valid() {
            return Err("Frequency draft is too large or incomplete".into());
        }
        let frequency = {
            let slug = Slug::new(self.fields[0].trim().trim_start_matches('@'))
                .map_err(|error| error.to_string())?;
            if let Some(original) = &self.original
                && self
                    .original_fields
                    .as_ref()
                    .is_some_and(|fields| fields[2..] == self.fields[2..])
                && self.original_weekdays == self.weekdays
            {
                let mut edited = original.clone();
                if slug != original.slug {
                    return Err(
                        "Keep the existing slug so linked Karma rules continue to find it".into(),
                    );
                }
                edited.purpose = self.fields[1].trim().into();
                return Ok(edited);
            }
            let step = super::interval::parse(&self.fields[2])?;
            let land_on = if self.weekdays.is_empty() {
                None
            } else {
                Some(
                    nucleus::karma::WeekdaySet::new(self.weekdays.iter().copied())
                        .map_err(|error| error.to_string())?,
                )
            };
            if let Some(original) = &self.original
                && let FrequencyCadenceAst::Calendar { cadence, .. } = &original.cadence
            {
                let mut edited = original.clone();
                if slug != original.slug {
                    return Err(
                        "Keep the existing slug so linked Karma rules continue to find it".into(),
                    );
                }
                edited.purpose = self.fields[1].trim().into();
                let mut cadence = cadence.clone();
                cadence.land_on = land_on.clone();
                let binding = |count| {
                    std::num::NonZeroU32::new(count)
                        .map(|value| nucleus::karma::PositiveIntegerBinding::Literal { value })
                };
                cadence.every = nucleus::karma::CadenceStepAst {
                    years: binding(step.years),
                    months: binding(step.months),
                    weeks: binding(step.weeks),
                    days: binding(step.days),
                    hours: binding(step.hours),
                    minutes: binding(step.minutes),
                    seconds: binding(step.seconds),
                    milliseconds: binding(step.milliseconds),
                };
                let anchor = nucleus::karma::CivilDateTime::parse_canonical(self.fields[3].trim())
                    .map_err(|error| error.to_string())?;
                if let FrequencyCadenceAst::Calendar {
                    cadence: target,
                    anchor: start,
                    ..
                } = &mut edited.cadence
                {
                    *target = cadence;
                    *start = anchor;
                }
                edited
                    .compile(&Default::default())
                    .map_err(|error| error.to_string())?;
                return Ok(edited);
            }
            let anchor = chrono::DateTime::parse_from_rfc3339(self.fields[3].trim()).map_err(|_| "Next date must include a date, time, and offset, such as 2026-09-20T09:00:00-03:00")?;
            let anchor = TimestampMs::from_millis(anchor.timestamp_millis())
                .map_err(|error| error.to_string())?;
            let fresh = nucleus::karma::simple_frequency::frequency_from_cadence(
                slug.clone(),
                self.fields[1].trim().to_string(),
                &Cadence {
                    land_on,
                    ..Cadence::every(step)
                },
                anchor,
            )
            .map_err(|error| error.to_string())?;
            if let Some(original) = &self.original {
                let mut edited = original.clone();
                edited.slug = slug;
                edited.purpose = self.fields[1].trim().into();
                edited.cadence = fresh.cadence;
                edited
            } else {
                fresh
            }
        };
        if self
            .original
            .as_ref()
            .is_some_and(|original| original.slug != frequency.slug)
        {
            return Err("Keep the existing slug so linked Karma rules continue to find it".into());
        }
        frequency
            .compile(&Default::default())
            .map_err(|error| error.to_string())?;
        Ok(frequency)
    }
}

pub fn schedule(frequency: &Frequency) -> String {
    match &frequency.definition.cadence {
        FrequencyCadenceAst::Elapsed { interval, .. } => match interval {
            DurationBinding::Literal { value } => {
                format!("Every {}", super::interval::elapsed(value.get()))
            }
            DurationBinding::Parameter { parameter } => format!("Every ${}", parameter.as_str()),
        },
        FrequencyCadenceAst::Calendar { timezone, .. } => {
            if let Ok(compiled) = frequency.definition.compile(&Default::default())
                && let nucleus::karma::CompiledSchedule::Calendar { schedule } = compiled.schedule
            {
                let mut label = format!(
                    "Every {}",
                    super::interval::describe(schedule.cadence.every)
                );
                if let Some(days) = schedule.cadence.land_on {
                    let days: Vec<_> = days.iter().map(super::interval::weekday_label).collect();
                    label.push_str(&format!(" · then {}", days.join(", ")));
                }
                format!("{label} · {}", timezone.as_str())
            } else {
                format!("Calendar · {}", timezone.as_str())
            }
        }
    }
}

pub fn details(frequency: &Frequency, now: i64) -> String {
    format!("{} | Next: {}", schedule(frequency), next(frequency, now))
}

pub fn next(frequency: &Frequency, now: i64) -> String {
    let Some(at) = frequency.next_at_ms else {
        return "No scheduled beat".into();
    };
    let Some(date) = chrono::DateTime::from_timestamp_millis(at) else {
        return "Date unavailable".into();
    };
    let seconds = at.saturating_sub(now).max(0).saturating_add(999) / 1000;
    let remaining = if seconds == 0 {
        "Due now".into()
    } else {
        format!(
            "{}d {:02}h {:02}m {:02}s",
            seconds / 86400,
            seconds / 3600 % 24,
            seconds / 60 % 60,
            seconds % 60
        )
    };
    format!(
        "{} · {remaining}",
        date.with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S%.3f %:z")
    )
}
