use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Usage {
    pub id: String,
    pub source: String,
    pub scope: String,
    pub updated_ms: u64,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub context_used: Option<u64>,
    pub context_capacity: Option<u64>,
    pub cost: Option<Cost>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Cost {
    pub amount: f64,
    pub currency: String,
    pub estimated: bool,
}

pub fn now_ms() -> u64 {
    crate::execution::now().timestamp_millis().max(0) as u64
}

impl Usage {
    pub fn request(
        source: String,
        input: Option<u64>,
        output: Option<u64>,
        total: Option<u64>,
    ) -> Self {
        Self {
            id: crate::new_uid("usage"),
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub content: String,
    pub status: StepState,
    pub priority: Priority,
}

pub fn validate_steps(steps: &[Step]) -> Result<(), String> {
    if steps.is_empty()
        || steps.len() > 64
        || steps
            .iter()
            .any(|step| step.content.trim().is_empty() || step.content.len() > 2048)
    {
        return Err("Use 1–64 steps, each with 1–2048 bytes of text.".into());
    }
    Ok(())
}

pub fn steps_text(steps: &[Step]) -> String {
    steps
        .iter()
        .map(|step| format!("[{:?}] {} ({:?})", step.status, step.content, step.priority))
        .collect::<Vec<_>>()
        .join("\n")
}
