use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Predicate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    Less,
    AtMost,
    Equal,
    AtLeast,
    Greater,
}

impl Comparison {
    pub fn accepts(self, ordering: std::cmp::Ordering) -> bool {
        match self {
            Self::Less => ordering.is_lt(),
            Self::AtMost => !ordering.is_gt(),
            Self::Equal => ordering.is_eq(),
            Self::AtLeast => !ordering.is_lt(),
            Self::Greater => ordering.is_gt(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Evaluation {
    #[default]
    Default,
    End,
    At {
        at_ms: i64,
    },
    EveryChange,
    EveryEvents {
        every: u64,
    },
    EveryDuration {
        millis: u64,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct CheckWindow {
    pub from_ms: Option<i64>,
    pub until_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct CheckOptions {
    pub quantity: QuantityBasis,
    pub name: String,
    pub enabled: bool,
    pub evaluation: Evaluation,
    pub window: CheckWindow,
}

impl Default for CheckOptions {
    fn default() -> Self {
        Self {
            quantity: QuantityBasis::Stored,
            name: String::new(),
            enabled: true,
            evaluation: Evaluation::Default,
            window: CheckWindow::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuantityBasis {
    #[default]
    Stored,
    Available,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CheckDefinition {
    pub id: String,
    pub predicate: Predicate,
    #[serde(default)]
    pub options: CheckOptions,
}

impl CheckDefinition {
    pub fn evaluation(&self) -> Evaluation {
        match self.options.evaluation {
            Evaluation::Default => match self.predicate {
                Predicate::QuantityEquals { at_ms, .. } | Predicate::Converged { at_ms, .. } => {
                    Evaluation::At { at_ms }
                }
                Predicate::Quantity { .. } => Evaluation::End,
                _ => Evaluation::EveryChange,
            },
            ref evaluation => evaluation.clone(),
        }
    }

    pub fn interval(&self, start_ms: i64, end_ms: i64) -> (i64, i64) {
        (
            self.options.window.from_ms.unwrap_or(start_ms),
            self.options.window.until_ms.unwrap_or(end_ms),
        )
    }

    pub fn next_time(&self, after: i64, start_ms: i64, end_ms: i64) -> Option<i64> {
        if !self.options.enabled {
            return None;
        }
        let (from, until) = self.interval(start_ms, end_ms);
        let next = match self.evaluation() {
            Evaluation::At { at_ms } => at_ms,
            Evaluation::End => until,
            Evaluation::EveryDuration { millis } if millis > 0 => {
                if after < from {
                    from
                } else {
                    let interval = i64::try_from(millis).ok()?;
                    ((after - from) / interval + 1)
                        .checked_mul(interval)
                        .and_then(|delta| from.checked_add(delta))
                        .unwrap_or(until)
                        .min(until)
                }
            }
            _ => {
                if after < from {
                    from
                } else {
                    until
                }
            }
        };
        (next > after && next <= until).then_some(next)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CheckSet {
    pub uid: String,
    pub name: String,
    pub revision: u64,
    pub checks: Vec<CheckDefinition>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FailureMode {
    #[default]
    Stop,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Checking {
    pub on_failure: FailureMode,
    pub evaluations: u64,
}

impl Default for Checking {
    fn default() -> Self {
        Self {
            on_failure: FailureMode::Stop,
            evaluations: 100_000,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Passed,
    Failed,
    #[default]
    Incomplete,
    Skipped,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoverageKind {
    #[default]
    Instant,
    Continuous,
    Sampled,
    History,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoverageReason {
    Stopped,
    Inaccessible,
    UnsupportedUnit,
    MissingCommitEvidence,
    CheckBudget,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct CheckCost {
    pub check: String,
    pub evaluations: u64,
    pub micros: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RunCost {
    pub execution_micros: u64,
    pub evidence_micros: u64,
    pub checking_micros: u64,
    pub checks: Vec<CheckCost>,
}
