use std::num::NonZeroU64;

use chrono::{DateTime, LocalResult, NaiveDateTime, TimeZone};
use chrono_tz::{GapInfo, Tz};

use super::{
    CalendarSchedule, CivilDateTime, KarmaBoundaryError, LocalTimeResolution, TimeZoneId,
    TimeZoneProvider, TimestampMs, TzdbRevision, TzdbVersion, canonical_hash,
};

#[derive(Clone, Debug)]
pub struct BundledTimeZoneProvider {
    revision: TzdbRevision,
}

impl BundledTimeZoneProvider {
    pub fn new() -> Result<Self, KarmaBoundaryError> {
        Ok(Self {
            revision: TzdbRevision {
                version: TzdbVersion::new(format!(
                    "lince-iana.{}.1",
                    chrono_tz::IANA_TZDB_VERSION
                ))?,
                digest: canonical_hash(
                    "lince.bundled-timezone.v1",
                    &(
                        chrono_tz::IANA_TZDB_VERSION,
                        "chrono-tz=0.10.4",
                        include_str!("timezone_builtin.rs"),
                    ),
                )?,
            },
        })
    }
}

fn zone(timezone: &TimeZoneId) -> Result<Tz, KarmaBoundaryError> {
    if matches!(timezone.as_str(), "Etc/GMT" | "Etc/UTC" | "GMT" | "UTC") {
        return Err(KarmaBoundaryError::invalid_input(
            "Use the UTC timezone provider for this zone",
        ));
    }
    timezone.as_str().parse().map_err(|_| {
        KarmaBoundaryError::invalid_input("This timezone is absent from the bundled IANA rules")
    })
}

fn instant(value: DateTime<Tz>) -> Result<TimestampMs, KarmaBoundaryError> {
    TimestampMs::from_millis(value.timestamp_millis())
}

fn gap(zone: Tz, local: NaiveDateTime) -> Result<LocalTimeResolution, KarmaBoundaryError> {
    let after = GapInfo::new(&local, &zone)
        .and_then(|info| info.end)
        .ok_or_else(|| {
            KarmaBoundaryError::invalid_input("Gap's following instant is unavailable")
        })?;
    let first_valid_after = instant(after)?;
    let before = first_valid_after
        .as_millis()
        .checked_sub(1)
        .ok_or_else(|| {
            KarmaBoundaryError::invalid_input("Gap's preceding instant is out of range")
        })?;
    Ok(LocalTimeResolution::Gap {
        before: TimestampMs::from_millis(before)?,
        first_valid_after,
    })
}

impl TimeZoneProvider for BundledTimeZoneProvider {
    fn revision(&self) -> &TzdbRevision {
        &self.revision
    }

    fn resolve_local(
        &self,
        timezone: &TimeZoneId,
        local: CivilDateTime,
    ) -> Result<LocalTimeResolution, KarmaBoundaryError> {
        let zone = zone(timezone)?;
        match zone.from_local_datetime(&local.as_naive()) {
            LocalResult::Single(value) => Ok(LocalTimeResolution::Unique {
                instant: instant(value)?,
            }),
            LocalResult::Ambiguous(first, second) => Ok(LocalTimeResolution::Fold {
                first: instant(first.min(second))?,
                second: instant(first.max(second))?,
            }),
            LocalResult::None => gap(zone, local.as_naive()),
        }
    }

    fn minimum_interval_ms(
        &self,
        schedule: &CalendarSchedule,
    ) -> Result<NonZeroU64, KarmaBoundaryError> {
        if schedule.tzdb != self.revision {
            return Err(KarmaBoundaryError::invalid_definition(
                "Calendar timezone revision does not match the bundled provider",
            ));
        }
        zone(&schedule.timezone)?;
        Ok(NonZeroU64::new(1)
            .expect("Distinct millisecond boundaries are at least one millisecond apart"))
    }
}
