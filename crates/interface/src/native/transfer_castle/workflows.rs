use super::model::{self, Form};
pub(super) use crate::transfers::workflows::{ACTIVITIES, alternative, smaller_need};
use serde_json::{Value, json};

#[path = "workflows_tests.rs"]
#[allow(
    clippy::unwrap_used,
    reason = "These Laboratory workflow fixtures must fail the case on incomplete setup"
)]
pub(super) mod tests;
