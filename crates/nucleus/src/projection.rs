//! Projection cache inputs and outputs.
//!
//! A [`RollingWindow`] specifies local-day offsets around the current date. A concrete
//! [`Window`] carries millisecond bounds and an IANA timezone. [`Span`] values cover
//! intervals, [`Scheduled`] entries represent occurrences, and [`Status`] describes the
//! availability of the result, including why an [`Incomplete`] projection stopped.

mod window;

use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

use crate::{
    karma::{CanonicalHash, KarmaBoundaryError, TypedUid, canonical_hash},
    schedule::TimeRange,
    simulation::{Cause, Quantity, RuleOccurrence},
};

/// Canonical fingerprint of the runtime inputs used to compute a projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionIdentity(CanonicalHash);

impl ExecutionIdentity {
    /// Validates the canonical SHA-256 fingerprint.
    ///
    /// # Errors
    ///
    /// Returns an error if the value is not a lowercase SHA-256 fingerprint with a
    /// `sha256:` prefix.
    pub fn parse(value: impl Into<String>) -> Result<Self, KarmaBoundaryError> {
        CanonicalHash::parse(value).map(Self)
    }

    /// Borrows the canonical fingerprint, including its algorithm prefix.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Maximum number of spans or scheduled entries in a projection.
pub const MAX_SPANS: usize = 50_000;
/// Maximum encoded size of a cached projection.
pub const MAX_CACHE_BYTES: usize = 16 * 1024 * 1024;
/// Maximum simulation steps used for a projection.
pub const MAX_STEPS: u64 = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RollingWindow {
    pub past_days: u16,
    pub future_days: u16,
    pub timezone: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub from_ms: i64,
    pub until_ms: i64,
    pub timezone: String,
}

/// Local-date conversion for a timezone that has already been parsed.
#[derive(Debug, Clone, Copy)]
pub struct Dates(Tz);

/// The requested window and actor scope for a projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    /// The time range requested by the caller.
    pub window: Window,
    /// The actor whose projection is requested.
    pub actor: Option<String>,
}

impl Context {
    /// Identifies the actor and runtime. Cache readers check window coverage separately.
    ///
    /// # Errors
    ///
    /// Returns an error if the key inputs cannot be canonically encoded.
    pub fn key(&self, identity: &ExecutionIdentity) -> Result<CanonicalHash, KarmaBoundaryError> {
        canonical_hash(
            "lince.projection.v1",
            &(&self.actor, identity.as_str(), chrono_tz::IANA_TZDB_VERSION),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Span {
    pub id: String,
    pub record: TypedUid,
    pub from_ms: i64,
    pub until_ms: i64,
    pub quantity: Quantity,
    pub cause: Cause,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scheduled {
    pub id: String,
    pub record: TypedUid,
    pub time: TimeRange,
    pub quantity: Quantity,
    pub cause: Cause,
    pub head: String,
    pub slug: Option<String>,
    pub record_kind: String,
    pub preview: bool,
}

/// Locates the consequence that projected a Record occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OccurrenceLink {
    /// The Record produced by the consequence.
    pub record: TypedUid,
    /// The rule occurrence that produced the Record.
    pub occurrence: RuleOccurrence,
    /// The consequence index within that occurrence.
    pub consequence: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Incomplete {
    Budget {},
    ExternalEffects {},
    RuleFailure {},
    UnavailableRuntime {},
    PastWindow {},
    UnsupportedFilter {},
    UnsupportedUnit {},
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Status {
    Updating {},
    Ready { base_ms: i64, expires_ms: i64 },
    Incomplete { base_ms: i64, reason: Incomplete },
}
