//! Contracts for reviewed remote evidence queries and background jobs.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::karma::CanonicalHash;

mod draft;
mod review;

pub use draft::{Draft, ReceiptSource, Series};
pub use review::{Endpoint, Review};

pub const MAX_PEERS: usize = 32;
pub const MAX_JOBS: usize = 8;
pub const MAX_REVIEWS: usize = 32;
pub const LEASE_SECONDS: u64 = 5;
pub const DEADLINE_SECONDS: u64 = 15;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Preview {
        draft: Draft,
    },
    Start {
        review: Box<Review>,
        expected_review: CanonicalHash,
    },
    Poll {
        job: String,
    },
    Verify {
        job: String,
    },
    Cancel {
        job: String,
    },
    Inspect {
        job: String,
        artifact: CanonicalHash,
    },
    Forget {
        job: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Queued,
    Running,
    Completed,
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verification {
    Accepted,
    UnknownIdentity,
    WithheldAuthority,
    Incomplete,
    Invalid,
}

#[cfg(test)]
mod tests;
