use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap, btree_map::Entry},
    num::NonZeroU32,
};

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{
    CanonicalHash, DurationMs, KarmaBoundaryError, OverloadPolicy, TimerPolicy, TimestampMs,
};

/// A cursor revision with a nominal wake time and validated timer bounds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeadlineEntry {
    activation_hash: CanonicalHash,
    cursor_revision: u64,
    next_intended_at: TimestampMs,
    required_resolution_ms: NonZeroU32,
    max_lateness_ms: u32,
    coalesce_window_ms: u32,
    overload: OverloadPolicy,
}

#[derive(Deserialize)]
struct DeadlineEntryWire {
    activation_hash: CanonicalHash,
    cursor_revision: u64,
    next_intended_at: TimestampMs,
    required_resolution_ms: NonZeroU32,
    max_lateness_ms: u32,
    coalesce_window_ms: u32,
    overload: OverloadPolicy,
}

impl DeadlineEntry {
    /// Builds an entry for a positive cursor revision and representable wake window.
    ///
    /// # Errors
    ///
    /// Returns a boundary error for an invalid revision or wake window.
    pub fn new(
        activation_hash: CanonicalHash,
        cursor_revision: u64,
        next_intended_at: TimestampMs,
        timer: TimerPolicy,
        overload: OverloadPolicy,
    ) -> Result<Self, KarmaBoundaryError> {
        let entry = Self {
            activation_hash,
            cursor_revision,
            next_intended_at,
            required_resolution_ms: timer_resolution(timer),
            max_lateness_ms: timer.max_lateness_ms(),
            coalesce_window_ms: timer.coalesce_window_ms(),
            overload,
        };
        entry.validate()?;
        Ok(entry)
    }

    pub fn activation_hash(&self) -> &CanonicalHash {
        &self.activation_hash
    }

    pub(super) fn into_activation_hash(self) -> CanonicalHash {
        self.activation_hash
    }

    pub const fn cursor_revision(&self) -> u64 {
        self.cursor_revision
    }

    pub const fn next_intended_at(&self) -> TimestampMs {
        self.next_intended_at
    }

    pub const fn required_resolution_ms(&self) -> NonZeroU32 {
        self.required_resolution_ms
    }

    pub const fn max_lateness_ms(&self) -> u32 {
        self.max_lateness_ms
    }

    pub const fn coalesce_window_ms(&self) -> u32 {
        self.coalesce_window_ms
    }

    pub const fn overload_policy(&self) -> OverloadPolicy {
        self.overload
    }

    fn validate(&self) -> Result<(), KarmaBoundaryError> {
        if self.cursor_revision == 0 {
            return Err(KarmaBoundaryError::invalid_input(
                "deadline cursor revision must be positive",
            ));
        }
        if self.coalesce_window_ms > self.max_lateness_ms {
            return Err(KarmaBoundaryError::invalid_input(
                "deadline coalescing window cannot exceed maximum lateness",
            ));
        }
        self.next_intended_at
            .checked_add(DurationMs::new(i64::from(self.coalesce_window_ms)))
            .ok_or_else(|| {
                KarmaBoundaryError::invalid_input("deadline arm window overflows timestamp")
            })?;
        Ok(())
    }
}

pub(super) fn timer_resolution(timer: TimerPolicy) -> NonZeroU32 {
    NonZeroU32::new(timer.required_resolution_ms()).expect("validated timer has nonzero resolution")
}

impl<'de> Deserialize<'de> for DeadlineEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = DeadlineEntryWire::deserialize(deserializer)?;
        let entry = Self {
            activation_hash: wire.activation_hash,
            cursor_revision: wire.cursor_revision,
            next_intended_at: wire.next_intended_at,
            required_resolution_ms: wire.required_resolution_ms,
            max_lateness_ms: wire.max_lateness_ms,
            coalesce_window_ms: wire.coalesce_window_ms,
            overload: wire.overload,
        };
        entry.validate().map_err(de::Error::custom)?;
        Ok(entry)
    }
}

/// An admitted deadline assigned to a host timer resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmedDeadline {
    pub entry: DeadlineEntry,
    pub lane_resolution_ms: NonZeroU32,
    pub degraded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DeadlineKey {
    next_intended_at: TimestampMs,
    lane_resolution_ms: NonZeroU32,
    activation_hash: CanonicalHash,
    cursor_revision: u64,
}

impl From<&ArmedDeadline> for DeadlineKey {
    fn from(deadline: &ArmedDeadline) -> Self {
        Self {
            next_intended_at: deadline.entry.next_intended_at,
            lane_resolution_ms: deadline.lane_resolution_ms,
            activation_hash: deadline.entry.activation_hash.clone(),
            cursor_revision: deadline.entry.cursor_revision,
        }
    }
}

impl DeadlineKey {
    fn matches(&self, deadline: &ArmedDeadline) -> bool {
        self.next_intended_at == deadline.entry.next_intended_at
            && self.lane_resolution_ms == deadline.lane_resolution_ms
            && self.cursor_revision == deadline.entry.cursor_revision
            && self.activation_hash == deadline.entry.activation_hash
    }
}

/// Active deadlines by activation, with lazy removal of superseded heap entries.
#[derive(Debug, Clone, Default)]
pub struct DeadlineIndex {
    heap: BinaryHeap<Reverse<DeadlineKey>>,
    active: BTreeMap<CanonicalHash, ArmedDeadline>,
    lanes: BTreeMap<NonZeroU32, usize>,
}

impl DeadlineIndex {
    /// Replaces the active deadline, returning the previous entry for its activation.
    pub fn upsert(&mut self, deadline: ArmedDeadline) -> Option<ArmedDeadline> {
        let key = DeadlineKey::from(&deadline);
        let lane = key.lane_resolution_ms;
        let previous = self.active.insert(key.activation_hash.clone(), deadline);
        if previous
            .as_ref()
            .is_none_or(|entry| entry.lane_resolution_ms != lane)
        {
            if let Some(previous) = &previous {
                self.remove_lane_member(previous.lane_resolution_ms);
            }
            *self.lanes.entry(lane).or_default() += 1;
        }
        self.heap.push(Reverse(key));
        previous
    }

    /// Removes the active deadline, leaving its heap entry for lazy removal.
    pub fn remove(&mut self, activation_hash: &CanonicalHash) -> Option<ArmedDeadline> {
        let previous = self.active.remove(activation_hash)?;
        self.remove_lane_member(previous.lane_resolution_ms);
        Some(previous)
    }

    pub fn len(&self) -> usize {
        self.active.len()
    }

    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    pub fn lane_count(&self) -> usize {
        self.lanes.len()
    }

    pub fn lane_sizes(&self) -> BTreeMap<NonZeroU32, usize> {
        self.lanes.clone()
    }

    pub(super) fn has_lane(&self, resolution: NonZeroU32) -> bool {
        self.lanes.contains_key(&resolution)
    }

    pub(super) fn compatible_lane(
        &self,
        finest: NonZeroU32,
        required: NonZeroU32,
    ) -> Option<NonZeroU32> {
        self.lanes
            .range(..=required)
            .next_back()
            .map(|(&lane, _)| lane)
            .filter(|&lane| lane >= finest)
    }

    /// Removes stale heap heads and returns the earliest remaining deadline.
    pub fn next_host_wake(&mut self) -> Option<TimestampMs> {
        loop {
            let Reverse(key) = self.heap.peek()?;
            if self
                .active
                .get(&key.activation_hash)
                .is_some_and(|deadline| key.matches(deadline))
            {
                return Some(key.next_intended_at);
            }
            self.heap.pop();
        }
    }

    /// Removes due active deadlines in heap order and counts discarded stale entries.
    pub fn pop_due(&mut self, observed_at: TimestampMs) -> DispatchBatch {
        let mut due = Vec::new();
        let mut stale_discarded = 0;
        while self
            .heap
            .peek()
            .is_some_and(|Reverse(key)| key.next_intended_at <= observed_at)
            && let Some(Reverse(key)) = self.heap.pop()
        {
            if self
                .active
                .get(&key.activation_hash)
                .is_some_and(|deadline| key.matches(deadline))
                && let Some(deadline) = self.remove(&key.activation_hash)
            {
                due.push(deadline);
            } else {
                stale_discarded += 1;
            }
        }
        DispatchBatch {
            observed_at,
            due,
            stale_discarded,
        }
    }

    fn remove_lane_member(&mut self, lane: NonZeroU32) {
        if let Entry::Occupied(mut entry) = self.lanes.entry(lane) {
            if *entry.get() == 1 {
                entry.remove();
            } else {
                *entry.get_mut() -= 1;
            }
        }
    }
}

/// Active deadlines removed at a wake, with the stale heap entries discarded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchBatch {
    pub observed_at: TimestampMs,
    pub due: Vec<ArmedDeadline>,
    pub stale_discarded: u64,
}
