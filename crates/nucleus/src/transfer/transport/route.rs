//! Ordered Transport routes, booking terms, and projected capacity.

use std::{collections::BTreeSet, iter::once, ops::Range};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::valid_uid;

/// The largest reviewed Transport capacity.
pub const MAX_CAPACITY: u32 = 10_000;
const MAX_STOPS: usize = 64;

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
    fn ordered_stops(&self) -> impl Iterator<Item = &str> + Clone {
        once(self.origin.as_str())
            .chain(self.via.iter().map(String::as_str))
            .chain(once(self.destination.as_str()))
    }

    /// Return the origin, intermediate stops, and destination in travel order.
    pub fn stops(&self) -> Vec<&str> {
        self.ordered_stops().collect()
    }

    /// Locate a booking's half-open range of segments on this route.
    ///
    /// # Errors
    ///
    /// Returns an error if the endpoints are missing or reversed, or if
    /// supplied intermediate stops differ from the route.
    pub fn segment(&self, part: &Self) -> Result<Range<usize>, &'static str> {
        let stops = self.ordered_stops();
        let start = stops
            .clone()
            .position(|stop| stop == part.origin)
            .ok_or("Part origin is outside the larger Transfer route")?;
        let end = stops
            .clone()
            .position(|stop| stop == part.destination)
            .ok_or("Part destination is outside the larger Transfer route")?;
        if start >= end
            || !part.via.is_empty()
                && part
                    .ordered_stops()
                    .ne(stops.skip(start).take(end - start + 1))
        {
            return Err("Transfer parts must follow their larger route's stop order");
        }
        Ok(start..end)
    }

    /// Validate the stop identifiers and the reviewed counts.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid, repeated, or excess stops, or a count
    /// outside the permitted capacity.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_uid(&self.origin, "r") || !valid_uid(&self.destination, "r") {
            return Err("Choose origin and destination Records with saved locations");
        }
        let mut seen = BTreeSet::new();
        if self.via.len() > MAX_STOPS - 2
            || !self
                .ordered_stops()
                .all(|uid| valid_uid(uid, "r") && seen.insert(uid))
        {
            return Err("Choose at most 64 distinct ordered stop Records");
        }
        if self.count == 0 || self.capacity > MAX_CAPACITY || self.count > self.capacity {
            return Err(
                "Choose a positive count within the reviewed transport capacity (at most 10000)",
            );
        }
        Ok(())
    }
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
    use super::{Activity, Direction, Terms};
    use crate::new_uid;

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
        let [a, b, c] = std::array::from_fn(|_| new_uid("r"));
        let journey = terms(&a, &c, vec![b.clone()]);
        assert_eq!(journey.validate(), Ok(()));
        assert_eq!(journey.segment(&terms(&a, &b, vec![])), Ok(0..1));
        assert_eq!(journey.segment(&terms(&b, &c, vec![])), Ok(1..2));
        assert_eq!(journey.segment(&terms(&a, &c, vec![])), Ok(0..2));
        assert_eq!(journey.segment(&journey), Ok(0..2));
        assert_eq!(
            journey.segment(&terms(&a, &c, vec![b, new_uid("r")])),
            Err("Transfer parts must follow their larger route's stop order"),
        );
        assert!(journey.segment(&terms(&c, &a, vec![])).is_err());
        assert!(terms(&a, &c, vec![a.clone()]).validate().is_err());
    }
}
