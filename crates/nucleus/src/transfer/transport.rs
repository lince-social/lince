use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MAX_CAPACITY: u32 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    Ride,
    Delivery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Need,
    Offer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Terms {
    pub activity: Activity,
    pub direction: Direction,
    pub origin: String,
    pub destination: String,
    #[serde(default)]
    pub via: Vec<String>,
    pub count: u32,
    pub capacity: u32,
}

impl Terms {
    pub fn stops(&self) -> Vec<&str> {
        std::iter::once(self.origin.as_str())
            .chain(self.via.iter().map(String::as_str))
            .chain(std::iter::once(self.destination.as_str()))
            .collect()
    }

    pub fn segment(&self, part: &Self) -> Result<std::ops::Range<usize>, &'static str> {
        let stops = self.stops();
        let start = stops
            .iter()
            .position(|stop| *stop == part.origin)
            .ok_or("Part origin is outside the larger Transfer route")?;
        let end = stops
            .iter()
            .position(|stop| *stop == part.destination)
            .ok_or("Part destination is outside the larger Transfer route")?;
        if start >= end || !part.via.is_empty() && part.stops() != stops[start..=end] {
            return Err("Transfer parts must follow their larger route's stop order");
        }
        Ok(start..end)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if !crate::valid_uid(&self.origin, "r") || !crate::valid_uid(&self.destination, "r") {
            return Err("Choose origin and destination Records with saved locations");
        }
        let stops = self.stops();
        if stops.len() > 64
            || stops.iter().any(|uid| !crate::valid_uid(uid, "r"))
            || stops
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != stops.len()
        {
            return Err("Choose at most 64 distinct ordered stop Records");
        }
        if self.count == 0
            || self.capacity == 0
            || self.capacity > MAX_CAPACITY
            || self.count > self.capacity
        {
            return Err(
                "Choose a positive count within the reviewed transport capacity (at most 10000)",
            );
        }
        Ok(())
    }
}

pub const JOURNEY_DOMAIN: &str = "lince.transfer-journey.v1";
pub const PICKUP_GRACE_MILLIS: i64 = 15 * 60 * 1000;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Arrived,
    Boarded,
    Departed,
    Destination,
    Finished,
    Cancelled,
}

impl Step {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Finished | Self::Cancelled)
    }
    pub fn caption(self) -> &'static str {
        match self {
            Self::Arrived => "Arrived at pickup",
            Self::Boarded => "Met / boarded",
            Self::Departed => "Departed",
            Self::Destination => "Reached destination",
            Self::Finished => "Finish my journey",
            Self::Cancelled => "Cancel my journey attendance",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub domain: String,
    pub transfer: String,
    pub revision: u64,
    pub person: String,
    pub step: Step,
    pub request_id: String,
}

impl Event {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.domain != JOURNEY_DOMAIN
            || self.revision == 0
            || !crate::valid_uid(&self.transfer, "r")
            || !crate::valid_uid(&self.person, "r")
            || self.request_id.is_empty()
            || self.request_id.len() > 256
            || self.request_id.chars().any(char::is_control)
        {
            return Err("Choose the current journey, acting Person and stable request ID");
        }
        Ok(())
    }
    pub fn parse(payload: &str) -> Option<Self> {
        if payload.len() > 4096 {
            return None;
        }
        let value: serde_json::Value = serde_json::from_str(payload).ok()?;
        let event: Self = serde_json::from_value(value.get("journey")?.clone()).ok()?;
        event.validate().ok()?;
        Some(event)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Progress {
    pub person: String,
    pub step: Option<Step>,
    pub fact: Option<String>,
    pub at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Journey {
    pub transfer: String,
    pub revision: u64,
    pub agreed: bool,
    pub participants: Vec<Progress>,
    pub started: bool,
    pub expired: bool,
    pub capacity_released: bool,
    pub all_finished: bool,
    pub pickup_grace_until: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SegmentCapacity {
    pub origin: String,
    pub destination: String,
    pub reserved: u32,
    pub available: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Capacity {
    pub offer: String,
    pub segments: Vec<SegmentCapacity>,
    pub total: u32,
    pub reserved: u32,
    pub available: u32,
}

#[cfg(test)]
mod segment_tests {
    use super::*;

    fn terms(origin: &str, destination: &str, via: Vec<String>) -> Terms {
        Terms {
            activity: Activity::Ride,
            direction: Direction::Offer,
            origin: origin.into(),
            destination: destination.into(),
            via,
            count: 1,
            capacity: 1,
        }
    }
    #[test]
    fn ordinary_transfer_parts_select_only_the_segments_they_use() {
        let a = crate::new_uid("r");
        let b = crate::new_uid("r");
        let c = crate::new_uid("r");
        let journey = terms(&a, &c, vec![b.clone()]);
        journey.validate().unwrap();
        assert_eq!(journey.segment(&terms(&a, &b, vec![])).unwrap(), 0..1);
        assert_eq!(journey.segment(&terms(&b, &c, vec![])).unwrap(), 1..2);
        assert_eq!(journey.segment(&terms(&a, &c, vec![])).unwrap(), 0..2);
        assert!(journey.segment(&terms(&c, &a, vec![])).is_err());
        assert!(terms(&a, &c, vec![a.clone()]).validate().is_err());
    }
}
