use crate::transfer::transport::Activity;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Area {
    pub latitude_e5: i32,
    pub longitude_e5: i32,
    pub radius_metres: u32,
}

impl Area {
    pub fn validate(&self) -> Result<(), String> {
        if self.latitude_e5.unsigned_abs() > 9_000_000
            || self.longitude_e5.unsigned_abs() > 18_000_000
            || !(1000..=1_000_000).contains(&self.radius_metres)
        {
            return Err(
                "Choose a public area between 1 and 1000 km within valid coordinates".into(),
            );
        }
        Ok(())
    }

    pub fn approximate(latitude: f64, longitude: f64, radius_metres: u32) -> Result<Self, String> {
        if !latitude.is_finite()
            || !longitude.is_finite()
            || latitude.abs() > 90.0
            || longitude.abs() > 180.0
        {
            return Err("Choose valid geographic coordinates".into());
        }
        let radius = radius_metres.max(1000);
        let grid = f64::from(radius) / 111_195.0;
        let latitude = ((latitude / grid).round() * grid).clamp(-90.0, 90.0);
        let longitude = ((longitude / grid).round() * grid + 180.0).rem_euclid(360.0) - 180.0;
        let area = Self {
            latitude_e5: (latitude * 100_000.0).round() as i32,
            longitude_e5: (longitude * 100_000.0).round() as i32,
            radius_metres: radius,
        };
        area.validate()?;
        Ok(area)
    }

    pub fn intersects(&self, other: &Self) -> bool {
        let lat = f64::from(self.latitude_e5) / 100_000.0;
        let other_lat = f64::from(other.latitude_e5) / 100_000.0;
        let delta_lat = (lat - other_lat).to_radians();
        let delta_lon =
            (f64::from(self.longitude_e5) - f64::from(other.longitude_e5)).to_radians() / 100_000.0;
        let a = (delta_lat / 2.0).sin().powi(2)
            + lat.to_radians().cos()
                * other_lat.to_radians().cos()
                * (delta_lon / 2.0).sin().powi(2);
        12_742_000.0 * a.clamp(0.0, 1.0).sqrt().asin()
            <= f64::from(self.radius_metres) + f64::from(other.radius_metres)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Announcement {
    pub activity: Activity,
    pub origin: Area,
    pub destination: Area,
    pub window_start: i64,
    pub window_end: i64,
    pub immediate: bool,
}

impl Announcement {
    pub fn validate(&self) -> Result<(), String> {
        self.origin.validate()?;
        self.destination.validate()?;
        if self.window_start <= 0
            || self.window_end <= self.window_start
            || self.window_end > 8_640_000_000_000
        {
            return Err("Choose an ordered transport pickup window".into());
        }
        Ok(())
    }

    pub fn expires_at(&self, published: i64) -> i64 {
        if self.immediate {
            published.saturating_add(900)
        } else {
            self.window_end
                .saturating_add(900)
                .min(published.saturating_add(super::MAX_LIFETIME))
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Filter {
    pub activity: Option<Activity>,
    pub origin: Option<Area>,
    pub destination: Option<Area>,
    pub window_start: Option<i64>,
    pub window_end: Option<i64>,
}

impl Filter {
    pub fn validate(&self) -> Result<(), String> {
        for area in [&self.origin, &self.destination].into_iter().flatten() {
            area.validate()?;
        }
        if self.window_start.is_some_and(|start| start < 0)
            || self.window_end.is_some_and(|end| end < 0)
            || self
                .window_start
                .zip(self.window_end)
                .is_some_and(|(start, end)| start >= end)
        {
            return Err("Choose an ordered search time window".into());
        }
        Ok(())
    }

    pub fn matches(&self, announcement: Option<&Announcement>) -> bool {
        announcement.is_some_and(|announcement| {
            self.activity
                .is_none_or(|activity| activity == announcement.activity)
                && self
                    .origin
                    .as_ref()
                    .is_none_or(|area| area.intersects(&announcement.origin))
                && self
                    .destination
                    .as_ref()
                    .is_none_or(|area| area.intersects(&announcement.destination))
                && self
                    .window_start
                    .is_none_or(|start| announcement.window_end >= start)
                && self
                    .window_end
                    .is_none_or(|end| announcement.window_start <= end)
        })
    }
}
