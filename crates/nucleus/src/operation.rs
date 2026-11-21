use std::fmt::from_fn;

use serde::{Deserialize, Serialize};

use crate::{execution::now, new_uid};

/// Token, context, and cost information reported by a provider.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Usage {
    /// Identifier of the usage report or session.
    pub id: String,
    /// Provider or model that supplied the report.
    pub source: String,
    /// Whether the measurements cover a request or a cumulative session.
    pub scope: String,
    /// Report update time in Unix milliseconds.
    pub updated_ms: u64,
    /// Input tokens reported by the provider.
    pub input_tokens: Option<u64>,
    /// Output tokens reported by the provider.
    pub output_tokens: Option<u64>,
    /// Total tokens reported by the provider.
    pub total_tokens: Option<u64>,
    /// Context size currently in use, in tokens.
    pub context_used: Option<u64>,
    /// Total context capacity, in tokens.
    pub context_capacity: Option<u64>,
    /// Cost of the reported work when available.
    pub cost: Option<Cost>,
}

/// An actual or estimated provider charge.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Cost {
    /// Charge in the reported currency.
    pub amount: f64,
    /// Currency code supplied by the provider.
    pub currency: String,
    /// Whether the charge is estimated.
    pub estimated: bool,
}

/// Current execution time in Unix milliseconds, clamped at the epoch.
///
/// # Panics
///
/// Panics if the active execution's state lock is poisoned.
pub fn now_ms() -> u64 {
    u64::try_from(now().timestamp_millis()).unwrap_or_default()
}

impl Usage {
    /// Creates a new per-request report with the supplied provider token counts.
    ///
    /// # Panics
    ///
    /// Panics if the active execution's lock is poisoned or its entropy is exhausted.
    pub fn request(
        source: String,
        input: Option<u64>,
        output: Option<u64>,
        total: Option<u64>,
    ) -> Self {
        Self {
            id: new_uid("usage"),
            source,
            scope: "request".into(),
            updated_ms: now_ms(),
            input_tokens: input,
            output_tokens: output,
            total_tokens: total,
            ..Default::default()
        }
    }
}

#[expect(
    missing_docs,
    clippy::exhaustive_enums,
    reason = "Rustdoc feeds the published schema and callers match every variant"
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Pending,
    InProgress,
    Completed,
}

#[expect(
    missing_docs,
    clippy::exhaustive_enums,
    reason = "Rustdoc feeds the published schema and callers match every variant"
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Medium,
    High,
}

#[expect(missing_docs, reason = "Rustdoc feeds the published step schema")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub content: String,
    pub status: StepState,
    pub priority: Priority,
}

const MAX_STEPS: usize = 64;
const MAX_STEP_TEXT_BYTES: usize = 2048;

/// Checks the number of steps and each step's text.
///
/// # Errors
///
/// Returns a diagnostic for an empty or oversized list or step text.
pub fn validate_steps(steps: &[Step]) -> Result<(), String> {
    if steps.is_empty()
        || steps.len() > MAX_STEPS
        || steps
            .iter()
            .any(|step| step.content.len() > MAX_STEP_TEXT_BYTES || step.content.trim().is_empty())
    {
        return Err("Use 1–64 steps, each with 1–2048 bytes of text.".into());
    }
    Ok(())
}

/// Formats each step on its own line for display and provider messages.
pub fn steps_text(steps: &[Step]) -> String {
    from_fn(|output| {
        for (
            index,
            Step {
                content,
                status,
                priority,
            },
        ) in steps.iter().enumerate()
        {
            let separator = if index == 0 { "" } else { "\n" };
            write!(output, "{separator}[{status:?}] {content} ({priority:?})")?;
        }
        Ok(())
    })
    .to_string()
}
