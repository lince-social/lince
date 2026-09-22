use serde::{Deserialize, Serialize};

use super::{Consequence, Consequences};
use crate::DecimalValue;

pub fn check_condition_source(source: &str) -> Result<(), String> {
    if source.len() > 4096 {
        return Err("A condition can contain at most 4096 bytes".into());
    }
    let mut depth = 0u32;
    let mut operators = 0u32;
    for character in source.chars() {
        if character == '(' {
            depth += 1;
        } else if character == ')' {
            depth = depth.saturating_sub(1);
        } else if "+-*/%^!<>=&|".contains(character) {
            operators += 1;
        }
        if depth > 32 || operators > 128 {
            return Err("This condition is too deeply nested or has too many operations".into());
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RuleFieldKind {
    Condition,
    Threshold,
    Consequence,
}

impl RuleFieldKind {
    pub const ALL: [Self; 3] = [Self::Condition, Self::Threshold, Self::Consequence];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Condition => "condition",
            Self::Threshold => "threshold",
            Self::Consequence => "consequence",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RuleFieldInput {
    Text { source: String },
    Reference { uid: String, revision: i64 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuleConsequence {
    pub target: String,
    pub consequences: Vec<Consequence>,
}

impl RuleConsequence {
    pub fn parse(source: &str) -> Result<Self, String> {
        let source = source.trim();
        let result: Self = if source.starts_with('{') {
            serde_json::from_str(source).map_err(|error| error.to_string())?
        } else {
            let source = source
                .strip_prefix('@')
                .ok_or("Use @record to save the calculated quantity, or @record: command(\"…\")")?;
            let (target, operation) = source
                .find(|character: char| {
                    character.is_whitespace() || matches!(character, '=' | '+' | ':')
                })
                .map_or((source, ""), |at| source.split_at(at));
            let operation = operation.trim();
            let consequences = if operation.is_empty() {
                vec![Consequence::SetQuantity { value: None }]
            } else if let Some(value) = operation.strip_prefix("+=") {
                vec![Consequence::AddQuantity {
                    delta: amount(value)?,
                }]
            } else if let Some(value) = operation.strip_prefix('=') {
                vec![Consequence::SetQuantity {
                    value: amount(value)?,
                }]
            } else if let Some(command) = operation.strip_prefix(':').map(str::trim) {
                let command = command
                    .strip_prefix("command(")
                    .and_then(|v| v.strip_suffix(')'))
                    .ok_or("Use command(\"shell command\")")?;
                vec![Consequence::RunCommand {
                    command: serde_json::from_str(command)
                        .map_err(|error| format!("Quote the command: {error}"))?,
                }]
            } else {
                return Err(
                    "Use @record to save the calculated quantity, or @record: command(\"…\")"
                        .into(),
                );
            };
            Self {
                target: target.into(),
                consequences,
            }
        };
        if result.target.trim().is_empty() {
            return Err("Choose a target Record".into());
        }
        Consequences::new(result.consequences.clone()).map_err(|error| error.to_string())?;
        Ok(result)
    }

    pub fn as_text(&self) -> String {
        match self.consequences.as_slice() {
            [Consequence::SetQuantity { value: None }] => format!("@{}", self.target),
            [Consequence::SetQuantity { value }] => {
                format!("@{} = {}", self.target, display_amount(value))
            }
            [Consequence::AddQuantity { delta }] => {
                format!("@{} += {}", self.target, display_amount(delta))
            }
            [Consequence::RunCommand { command }] => format!(
                "@{}: command({})",
                self.target,
                serde_json::to_string(command).unwrap()
            ),
            _ => serde_json::to_string(self).unwrap(),
        }
    }
}

fn amount(value: &str) -> Result<Option<DecimalValue>, String> {
    let value = value.trim();
    if value == "result" {
        Ok(None)
    } else {
        DecimalValue::parse_inferred(value)
            .map(Some)
            .map_err(|error| error.to_string())
    }
}

fn display_amount(value: &Option<DecimalValue>) -> String {
    value.map_or_else(|| "result".into(), |value| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consequence_text_round_trips_without_losing_precision_or_command_quotes() {
        for source in [
            "@balance",
            "@balance = -0.000001",
            "@balance += -0.000001",
            "@log: command(\"printf \\\"ok\\\"\")",
        ] {
            let parsed = RuleConsequence::parse(source).unwrap();
            assert_eq!(parsed.as_text(), source);
        }
        for source in [
            "@",
            "balance",
            "@balance @other",
            "@balance =",
            "@balance = @secret",
            "@log: command(rm)",
            "@ = 1",
        ] {
            assert!(RuleConsequence::parse(source).is_err());
        }
    }

    #[test]
    fn a_record_destination_sets_the_calculated_quantity() {
        for source in [
            "@escovar-dentes",
            "  @escovar-dentes  ",
            "@escovar-dentes = result",
        ] {
            let parsed = RuleConsequence::parse(source).unwrap();
            assert_eq!(parsed.target, "escovar-dentes");
            assert!(matches!(
                parsed.consequences.as_slice(),
                [Consequence::SetQuantity { value: None }]
            ));
            assert_eq!(parsed.as_text(), "@escovar-dentes");
        }
    }

    #[test]
    fn interactive_conditions_are_bounded_before_recursive_parsing() {
        assert!(check_condition_source("-1 * freq(@daily) * @balance").is_ok());
        assert!(
            check_condition_source(&format!("{}@balance{}", "(".repeat(40), ")".repeat(40)))
                .is_err()
        );
        assert!(check_condition_source(&format!("{}@balance", "-".repeat(200))).is_err());
    }
}
