use std::{collections::BTreeMap, num::NonZeroU32};

use super::{DslErrorKind, KarmaDslError, Parser};
use crate::{
    CadenceAst, CadenceBound, CadenceStepAst, CanonicalHash, CivilDateTime, DurationBinding,
    DurationMs, FrequencyAst, FrequencyCadenceAst, FrequencyParameterDefinition, FrequencyTimerAst,
    LocalId, MissedPolicy, PositiveIntegerBinding, Slug, TimeZoneId, TimestampMs, TzdbRevision,
    TzdbVersion, WeekdaySet,
};

impl Parser<'_> {
    pub(super) fn parse_frequency(mut self) -> Result<FrequencyAst, KarmaDslError> {
        self.expect_word("karma-frequency")?;
        self.expect_word("1")?;
        self.expect_symbol(';')?;
        self.expect_word("frequency")?;
        let slug =
            Slug::new(self.take_word("frequency slug")?).map_err(|error| self.atom(error))?;
        self.expect_symbol('{')?;

        let mut schema = None;
        let mut purpose = None;
        let mut tags = None;
        let mut parameters = BTreeMap::new();
        let mut cadence = None;
        let mut anchor = None;
        let mut timezone = None;
        let mut tzdb = None;
        let mut gap = None;
        let mut fold = None;
        let mut timer = None;
        let mut missed = None;
        let mut inactive_gap = None;
        let mut rephase = None;
        let mut overload = None;

        while !self.consume_symbol('}') {
            let declaration = self.take_word("frequency declaration")?;
            match declaration {
                "schema" => {
                    let value = self.parse_enum_atom("frequency schema")?;
                    self.set_once(&mut schema, value, "schema")?;
                }
                "purpose" => {
                    let value = self.take_string("frequency purpose")?;
                    self.set_once(&mut purpose, value, "purpose")?;
                }
                "tags" => {
                    let value = self.parse_slug_set()?;
                    self.set_once(&mut tags, value, "tags")?;
                }
                "param" => {
                    let (id, definition) = self.parse_frequency_parameter()?;
                    self.insert_unique(&mut parameters, id, definition)?;
                    continue;
                }
                "every" => {
                    let value = self.parse_frequency_cadence()?;
                    self.set_once(&mut cadence, value, "cadence")?;
                }
                "anchor" => {
                    let value = self.parse_frequency_anchor()?;
                    self.set_once(&mut anchor, value, "anchor")?;
                }
                "timezone" => {
                    let value = TimeZoneId::new(self.take_word("timezone")?)
                        .map_err(|error| self.atom(error))?;
                    self.set_once(&mut timezone, value, "timezone")?;
                }
                "tzdb" => {
                    self.expect_word("version")?;
                    let version = TzdbVersion::new(self.take_string("tzdb version")?)
                        .map_err(|error| self.atom(error))?;
                    self.expect_word("digest")?;
                    let digest = CanonicalHash::parse(self.take_string("tzdb digest")?)
                        .map_err(|error| self.atom(error))?;
                    self.set_once(&mut tzdb, TzdbRevision { version, digest }, "tzdb")?;
                }
                "gap" => {
                    let value = self.parse_enum_atom("gap policy")?;
                    self.set_once(&mut gap, value, "gap policy")?;
                }
                "fold" => {
                    let value = self.parse_enum_atom("fold policy")?;
                    self.set_once(&mut fold, value, "fold policy")?;
                }
                "timer" => {
                    let value = self.parse_frequency_timer()?;
                    self.set_once(&mut timer, value, "timer")?;
                    continue;
                }
                "missed" => {
                    let value = self.parse_missed_policy()?;
                    self.set_once(&mut missed, value, "missed policy")?;
                }
                "inactive-gap" => {
                    let value = self.parse_enum_atom("inactive-gap policy")?;
                    self.set_once(&mut inactive_gap, value, "inactive-gap policy")?;
                }
                "rephase" => {
                    let value = self.parse_enum_atom("rephase policy")?;
                    self.set_once(&mut rephase, value, "rephase policy")?;
                }
                "overload" => {
                    let value = self.parse_enum_atom("overload policy")?;
                    self.set_once(&mut overload, value, "overload policy")?;
                }
                _ => {
                    return Err(
                        self.unexpected(format!("unknown Frequency declaration {declaration:?}"))
                    );
                }
            }
            self.expect_symbol(';')?;
        }
        self.finished("trailing input after Frequency")?;

        let cadence = match (
            cadence.ok_or_else(|| self.missing("cadence"))?,
            anchor.ok_or_else(|| self.missing("anchor"))?,
        ) {
            (
                ParsedFrequencyCadence::Elapsed(interval),
                ParsedFrequencyAnchor::Timestamp(anchor),
            ) => {
                if timezone.is_some() || tzdb.is_some() || gap.is_some() || fold.is_some() {
                    return Err(self.unexpected(
                        "elapsed Frequency cannot carry timezone/tzdb/gap/fold declarations",
                    ));
                }
                FrequencyCadenceAst::Elapsed { interval, anchor }
            }
            (ParsedFrequencyCadence::Calendar(cadence), ParsedFrequencyAnchor::Civil(anchor)) => {
                FrequencyCadenceAst::Calendar {
                    cadence,
                    anchor,
                    timezone: timezone.ok_or_else(|| self.missing("timezone"))?,
                    tzdb: tzdb.ok_or_else(|| self.missing("tzdb"))?,
                    gap: gap.ok_or_else(|| self.missing("gap policy"))?,
                    fold: fold.ok_or_else(|| self.missing("fold policy"))?,
                }
            }
            (ParsedFrequencyCadence::Elapsed(_), ParsedFrequencyAnchor::Civil(_)) => {
                return Err(self.unexpected("elapsed Frequency requires a UTC timestamp anchor"));
            }
            (ParsedFrequencyCadence::Calendar(_), ParsedFrequencyAnchor::Timestamp(_)) => {
                return Err(self.unexpected("calendar Frequency requires a civil anchor"));
            }
        };

        let frequency = FrequencyAst {
            schema: schema.ok_or_else(|| self.missing("schema"))?,
            slug,
            purpose: purpose.ok_or_else(|| self.missing("purpose"))?,
            tags: tags.ok_or_else(|| self.missing("tags"))?,
            parameters,
            cadence,
            timer: timer.ok_or_else(|| self.missing("timer"))?,
            missed: missed.ok_or_else(|| self.missing("missed policy"))?,
            inactive_gap: inactive_gap.ok_or_else(|| self.missing("inactive-gap policy"))?,
            rephase: rephase.ok_or_else(|| self.missing("rephase policy"))?,
            overload: overload.ok_or_else(|| self.missing("overload policy"))?,
        };
        frequency
            .compile(&BTreeMap::new())
            .map_err(|error| self.error_here(DslErrorKind::InvalidAtom, error.to_string()))?;
        Ok(frequency)
    }

    fn parse_frequency_parameter(
        &mut self,
    ) -> Result<(LocalId, FrequencyParameterDefinition), KarmaDslError> {
        let id = self.parse_local_id("Frequency parameter id")?;
        self.expect_symbol(':')?;
        let kind = self.take_word("Frequency parameter type")?;
        self.expect_word("default")?;
        let definition = match kind {
            "duration" => {
                let default = self.parse_duration_literal()?;
                let (minimum, maximum) = self.parse_range(Self::parse_duration_literal)?;
                FrequencyParameterDefinition::Duration {
                    default,
                    minimum,
                    maximum,
                }
            }
            "positive-integer" => {
                let default = self.parse_positive_integer_literal()?;
                let (minimum, maximum) = self.parse_range(Self::parse_positive_integer_literal)?;
                FrequencyParameterDefinition::PositiveInteger {
                    default,
                    minimum,
                    maximum,
                }
            }
            _ => {
                return Err(self.unexpected(format!("unknown Frequency parameter type {kind:?}")));
            }
        };
        self.expect_symbol(';')?;
        Ok((id, definition))
    }

    fn parse_range<T>(
        &mut self,
        literal: impl Fn(&mut Self) -> Result<T, KarmaDslError>,
    ) -> Result<(T, T), KarmaDslError> {
        self.expect_word("range")?;
        self.expect_symbol('[')?;
        let minimum = literal(self)?;
        self.comma()?;
        let maximum = literal(self)?;
        self.expect_symbol(']')?;
        Ok((minimum, maximum))
    }

    fn parse_frequency_cadence(&mut self) -> Result<ParsedFrequencyCadence, KarmaDslError> {
        match self.take_word("Frequency cadence kind")? {
            "elapsed" => self
                .parse_duration_binding()
                .map(ParsedFrequencyCadence::Elapsed),
            "calendar" => self
                .parse_cadence_ast()
                .map(ParsedFrequencyCadence::Calendar),
            value => Err(self.unexpected(format!("unknown Frequency cadence {value:?}"))),
        }
    }

    fn parse_frequency_anchor(&mut self) -> Result<ParsedFrequencyAnchor, KarmaDslError> {
        match self.take_word("Frequency anchor kind")? {
            "timestamp" => {
                self.expect_symbol('(')?;
                let value = TimestampMs::parse_canonical(&self.take_string("UTC anchor")?)
                    .map_err(|error| self.atom(error))?;
                self.expect_symbol(')')?;
                Ok(ParsedFrequencyAnchor::Timestamp(value))
            }
            "civil" => {
                self.expect_symbol('(')?;
                let value = CivilDateTime::parse_canonical(&self.take_string("civil anchor")?)
                    .map_err(|error| self.atom(error))?;
                self.expect_symbol(')')?;
                Ok(ParsedFrequencyAnchor::Civil(value))
            }
            value => Err(self.unexpected(format!("unknown Frequency anchor {value:?}"))),
        }
    }

    fn parse_frequency_timer(&mut self) -> Result<FrequencyTimerAst, KarmaDslError> {
        self.expect_symbol('{')?;
        let mut resolution = None;
        let mut max_lateness = None;
        let mut coalesce_window = None;
        while !self.consume_symbol('}') {
            let field = self.take_word("timer field")?;
            let (slot, label) = match field {
                "resolution" => (&mut resolution, "timer resolution"),
                "max-lateness" => (&mut max_lateness, "timer maximum lateness"),
                "coalesce-window" => (&mut coalesce_window, "timer coalescing window"),
                _ => return Err(self.unexpected(format!("unknown timer field {field:?}"))),
            };
            let value = self.parse_duration_binding()?;
            self.set_once(slot, value, label)?;
            self.expect_symbol(';')?;
        }
        Ok(FrequencyTimerAst {
            required_resolution: resolution.ok_or_else(|| self.missing("timer resolution"))?,
            max_lateness: max_lateness.ok_or_else(|| self.missing("timer maximum lateness"))?,
            coalesce_window: coalesce_window
                .ok_or_else(|| self.missing("timer coalescing window"))?,
        })
    }

    fn parse_duration_binding(&mut self) -> Result<DurationBinding, KarmaDslError> {
        match self.take_word("duration binding")? {
            "duration" => self
                .parse_duration_argument("duration milliseconds")
                .map(|value| DurationBinding::Literal { value }),
            "parameter" => {
                self.expect_symbol('(')?;
                let parameter = self.parse_local_id("duration parameter")?;
                self.expect_symbol(')')?;
                Ok(DurationBinding::Parameter { parameter })
            }
            value => Err(self.unexpected(format!("unknown duration binding {value:?}"))),
        }
    }

    fn parse_positive_integer_binding(&mut self) -> Result<PositiveIntegerBinding, KarmaDslError> {
        match self.take_word("positive-integer binding")? {
            "integer" => self
                .parse_nonzero("positive integer")
                .map(|value| PositiveIntegerBinding::Literal { value }),
            "parameter" => {
                self.expect_symbol('(')?;
                let parameter = self.parse_local_id("positive-integer parameter")?;
                self.expect_symbol(')')?;
                Ok(PositiveIntegerBinding::Parameter { parameter })
            }
            value => Err(self.unexpected(format!("unknown positive-integer binding {value:?}"))),
        }
    }

    fn parse_duration_literal(&mut self) -> Result<DurationMs, KarmaDslError> {
        self.parse_duration_constructor("duration milliseconds")
    }

    fn parse_positive_integer_literal(&mut self) -> Result<NonZeroU32, KarmaDslError> {
        self.expect_word("integer")?;
        self.parse_nonzero("positive integer")
    }

    fn parse_nonzero(&mut self, label: &str) -> Result<NonZeroU32, KarmaDslError> {
        self.expect_symbol('(')?;
        let value = self.take_word(label)?;
        let value = value.parse::<u32>().map_err(|_| {
            self.error_here(DslErrorKind::InvalidAtom, format!("{label} must fit u32"))
        })?;
        let value = NonZeroU32::new(value).ok_or_else(|| {
            self.error_here(
                DslErrorKind::InvalidAtom,
                format!("{label} must be non-zero"),
            )
        })?;
        self.expect_symbol(')')?;
        Ok(value)
    }

    fn parse_cadence_ast(&mut self) -> Result<CadenceAst, KarmaDslError> {
        self.expect_symbol('(')?;
        self.expect_word("step")?;
        self.expect_symbol('(')?;
        let mut every = CadenceStepAst::default();
        if !self.consume_symbol(')') {
            loop {
                let component = self.take_word("step component")?;
                self.expect_symbol('(')?;
                let binding = self.parse_positive_integer_binding()?;
                self.expect_symbol(')')?;
                let slot = match component {
                    "years" => &mut every.years,
                    "months" => &mut every.months,
                    "weeks" => &mut every.weeks,
                    "days" => &mut every.days,
                    "hours" => &mut every.hours,
                    "minutes" => &mut every.minutes,
                    "seconds" => &mut every.seconds,
                    "milliseconds" => &mut every.milliseconds,
                    other => {
                        return Err(self.unexpected(format!("unknown step component {other:?}")));
                    }
                };
                self.set_once(slot, binding, "step component")?;
                if !self.consume_symbol(',') {
                    break;
                }
            }
            self.expect_symbol(')')?;
        }
        self.comma()?;
        let land_on = if self.consume_word("land") {
            self.expect_symbol('(')?;
            let days = self.parse_weekday_set()?;
            self.expect_symbol(')')?;
            self.comma()?;
            Some(days)
        } else {
            None
        };
        self.expect_word("invalid-day")?;
        self.expect_symbol('(')?;
        let invalid_day = self.parse_enum_atom("invalid month-day policy")?;
        self.expect_symbol(')')?;
        self.comma()?;
        self.expect_word("bound")?;
        self.expect_symbol('(')?;
        let bound = self.parse_cadence_bound()?;
        self.expect_symbol(')')?;
        self.expect_symbol(')')?;
        Ok(CadenceAst {
            every,
            land_on,
            invalid_day,
            bound,
        })
    }

    fn parse_cadence_bound(&mut self) -> Result<CadenceBound, KarmaDslError> {
        match self.take_word("cadence bound")? {
            "unbounded" => Ok(CadenceBound::Unbounded),
            "count" => {
                self.expect_symbol('(')?;
                let occurrences = self.parse_atom("occurrence count")?;
                self.expect_symbol(')')?;
                Ok(CadenceBound::Count { occurrences })
            }
            "until" => {
                self.expect_symbol('(')?;
                let at = CivilDateTime::parse_canonical(&self.take_string("bound instant")?)
                    .map_err(|error| self.atom(error))?;
                self.expect_symbol(')')?;
                Ok(CadenceBound::Until { at })
            }
            other => Err(self.unexpected(format!("unknown cadence bound {other:?}"))),
        }
    }

    fn parse_weekday_set(&mut self) -> Result<WeekdaySet, KarmaDslError> {
        let days = self.parse_set("civil weekday", |parser| {
            parser.parse_enum_atom("civil weekday")
        })?;
        WeekdaySet::new(days).map_err(|error| self.atom(error))
    }

    fn parse_missed_policy(&mut self) -> Result<MissedPolicy, KarmaDslError> {
        match self.take_word("missed policy")? {
            "skip" => Ok(MissedPolicy::Skip),
            "coalesce" => Ok(MissedPolicy::Coalesce),
            "pause-on-lag" => Ok(MissedPolicy::PauseOnLag),
            "replay" => Ok(MissedPolicy::Replay {
                max: self.parse_nonzero("replay maximum")?,
            }),
            value => Err(self.unexpected(format!("unknown missed policy {value:?}"))),
        }
    }
}

#[expect(
    clippy::large_enum_variant,
    reason = "Keeps the one parsed cadence inline without allocating a temporary box"
)]
enum ParsedFrequencyCadence {
    Elapsed(DurationBinding),
    Calendar(CadenceAst),
}

enum ParsedFrequencyAnchor {
    Timestamp(TimestampMs),
    Civil(CivilDateTime),
}
