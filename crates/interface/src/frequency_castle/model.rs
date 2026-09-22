use nucleus::karma::{
    Cadence, CadenceStep, CanonicalHash, DurationBinding, FrequencyAst, FrequencyCadenceAst, Slug,
    TimestampMs,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Frequency {
    pub uid: String,
    pub slug: String,
    pub status: String,
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
    pub const ALL: [Self; 8] = [
        Self::Milliseconds,
        Self::Seconds,
        Self::Minutes,
        Self::Hours,
        Self::Days,
        Self::Weeks,
        Self::Months,
        Self::Years,
    ];

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

    fn step(self, count: u32) -> CadenceStep {
        let mut step = CadenceStep::default();
        *match self {
            Self::Milliseconds => &mut step.milliseconds,
            Self::Seconds => &mut step.seconds,
            Self::Minutes => &mut step.minutes,
            Self::Hours => &mut step.hours,
            Self::Days => &mut step.days,
            Self::Weeks => &mut step.weeks,
            Self::Months => &mut step.months,
            Self::Years => &mut step.years,
        } = count;
        step
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub uid: Option<String>,
    pub revision: Option<u64>,
    pub original: Option<FrequencyAst>,
    pub fields: [String; 4],
    pub unit: Unit,
    pub original_fields: Option<[String; 4]>,
    pub original_unit: Option<Unit>,
    pub cadence_editable: bool,
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
                "1".into(),
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            ],
            unit: Unit::Minutes,
            original_fields: None,
            original_unit: None,
            cadence_editable: true,
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
            for (unit, divisor) in [
                (Unit::Weeks, 604_800_000),
                (Unit::Days, 86_400_000),
                (Unit::Hours, 3_600_000),
                (Unit::Minutes, 60_000),
                (Unit::Seconds, 1000),
                (Unit::Milliseconds, 1),
            ] {
                if value.get() % divisor == 0 && u32::try_from(value.get() / divisor).is_ok() {
                    draft.unit = unit;
                    draft.fields[2] = (value.get() / divisor).to_string();
                    draft.fields[3] = chrono::DateTime::from_timestamp_millis(
                        row.next_at_ms.unwrap_or(anchor.as_millis()),
                    )
                    .unwrap()
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
                    draft.cadence_editable = true;
                    break;
                }
            }
        }
        if let Ok(compiled) = row.definition.compile(&Default::default())
            && let nucleus::karma::CompiledSchedule::Calendar { schedule } = compiled.schedule
        {
            let step = schedule.cadence.every;
            let components = [
                (Unit::Years, step.years),
                (Unit::Months, step.months),
                (Unit::Weeks, step.weeks),
                (Unit::Days, step.days),
                (Unit::Hours, step.hours),
                (Unit::Minutes, step.minutes),
                (Unit::Seconds, step.seconds),
                (Unit::Milliseconds, step.milliseconds),
            ];
            let nonzero: Vec<_> = components
                .into_iter()
                .filter(|(_, count)| *count > 0)
                .collect();
            if let [(unit, count)] = nonzero.as_slice() {
                draft.unit = *unit;
                draft.fields[2] = count.to_string();
                draft.fields[3] = schedule.anchor.to_string();
                if let Some(next) = &row.next_local {
                    draft.fields[3] = next.clone();
                }
                draft.cadence_editable = true;
            }
        }
        draft.original_fields = Some(draft.fields.clone());
        draft.original_unit = Some(draft.unit);
        draft
    }

    pub fn valid(&self) -> bool {
        self.fields.iter().all(|field| field.len() <= 32_768)
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
            let count = self.fields[2]
                .trim()
                .parse::<u32>()
                .ok()
                .filter(|count| *count > 0)
                .ok_or("Every must be a positive whole number")?;
            if let Some(original) = &self.original
                && self
                    .original_fields
                    .as_ref()
                    .is_some_and(|fields| fields[2..] == self.fields[2..])
                && self.original_unit == Some(self.unit)
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
                let step = self.unit.step(count);
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
                &Cadence::every(self.unit.step(count)),
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
                for (unit, divisor) in [
                    ("weeks", 604_800_000),
                    ("days", 86_400_000),
                    ("hours", 3_600_000),
                    ("minutes", 60_000),
                    ("seconds", 1000),
                ] {
                    if value.get() % divisor == 0 {
                        return format!("Every {} {unit}", value.get() / divisor);
                    }
                }
                format!("Every {} ms", value.get())
            }
            DurationBinding::Parameter { parameter } => format!("Every ${}", parameter.as_str()),
        },
        FrequencyCadenceAst::Calendar { timezone, .. } => {
            format!("Calendar · {}", timezone.as_str())
        }
    }
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
