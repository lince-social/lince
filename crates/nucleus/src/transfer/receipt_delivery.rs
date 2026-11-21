//! Public receipt delivery envelopes and commands.

mod command;
mod envelope;

pub use command::{Command, Request, Source};
pub use envelope::{Artifact, Delivery};

pub const DOMAIN: &str = "lince.transfer-receipt-delivery.v1";
pub const MIME: &str = "application/vnd.lince.transfer-receipt+json";
pub const MAX_BYTES: usize = 850_000;
pub const MAX_SECONDS: i64 = 30 * 86400;

#[cfg(test)]
mod tests;
