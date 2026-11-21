//! The Karma editor’s rule fields and consequence text format.
//!
//! [RuleFieldKind] names the Condition, Threshold, and Consequence fields.
//! [RuleFieldInput] holds user-authored text or a reference at a fixed revision.
//! [RuleIdentity] supplies the name and slug stored with the fields.

use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_string};

use super::{
    Consequence::{
        self, ActivateFiote, AddQuantity, InvokeCommand, RunCommand, SetQuantity, ShowComponent,
    },
    Consequences, transfer_consequence,
};
use crate::{DecimalValue, component::ComponentState};

const MAX_CONDITION_BYTES: usize = 4096;
const MAX_CONDITION_DEPTH: usize = 32;
const MAX_CONDITION_OPERATORS: usize = 128;
const CONSEQUENCE_SYNTAX: &str =
    "Use @record to save the calculated quantity, or @record: command(\"…\")";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RuleIdentity {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub slug: String,
}

/// Bounds the work of parsing an interactive condition. Operators and parentheses in strings
/// do not count toward the complexity limits.
///
/// # Errors
///
/// Returns an editor message if the byte length, depth, or operator count is too large.
pub fn check_condition_source(source: &str) -> Result<(), String> {
    if source.len() > MAX_CONDITION_BYTES {
        return Err("A condition can contain at most 4096 bytes".into());
    }
    let (mut depth, mut operators) = (0usize, 0);
    let mut quoted = false;
    let mut escaped = false;
    for byte in source.bytes() {
        if quoted {
            if byte == b'"' && !escaped {
                quoted = false;
            }
            escaped = byte == b'\\' && !escaped;
            continue;
        }
        match byte {
            b'"' => quoted = true,
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            byte if b"+-*/%^!<>=&|".contains(&byte) => operators += 1,
            _ => {}
        }
        if depth > MAX_CONDITION_DEPTH || operators > MAX_CONDITION_OPERATORS {
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
    /// The fields in evaluation order.
    pub const ALL: [Self; 3] = [Self::Condition, Self::Threshold, Self::Consequence];

    /// Returns the stable wire spelling.
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

/// A target Record and its ordered consequences.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuleConsequence {
    /// The target Record or Transfer reference.
    pub target: String,
    /// The effects in execution order.
    pub consequences: Vec<Consequence>,
}

impl RuleConsequence {
    /// Parses a compact `@record` expression or a JSON object for a list of effects.
    ///
    /// # Errors
    ///
    /// Returns an editor message for malformed input, an invalid target, or invalid consequences.
    pub fn parse(source: &str) -> Result<Self, String> {
        let source = source.trim();
        let result = if source.starts_with('{') {
            from_str(source).map_err(|error| error.to_string())?
        } else {
            let source = source.strip_prefix('@').ok_or(CONSEQUENCE_SYNTAX)?;
            let (target, operation) = source
                .find(|character: char| {
                    character.is_whitespace() || matches!(character, '=' | '+' | ':')
                })
                .map_or((source, ""), |at| source.split_at(at));
            let consequence = parse_consequence(target, operation.trim())?;
            Self {
                target: target.into(),
                consequences: vec![consequence],
            }
        };
        if result.target.trim().is_empty() {
            return Err("Choose a target Record".into());
        }
        let mut targets = result.consequences.iter().map(Consequence::transfer_target);
        if let Some(first) = targets.next()
            && (first.is_some_and(|target| target != result.target)
                || targets.any(|target| target != first))
        {
            return Err("Choose the same Transfer in the destination and each consequence".into());
        }
        Consequences::validate(&result.consequences).map_err(|error| error.to_string())?;
        Ok(result)
    }

    /// Formats an editor expression where one is available, or JSON for the full list.
    pub fn as_text(&self) -> String {
        let target = &self.target;
        if let [effect] = self.consequences.as_slice()
            && let Some(source) = transfer_consequence::display(target, effect)
        {
            return source;
        }
        match self.consequences.as_slice() {
            [ActivateFiote] => format!("@{target}: activate-fiote"),
            [InvokeCommand { command }] => format!("@{target}: run(@{command})"),
            [ShowComponent { component }] => format!("@{target}: show({})", editor_json(component)),
            [SetQuantity { value: None }] => format!("@{target}"),
            [SetQuantity { value }] => format!("@{target} = {}", display_amount(value.as_ref())),
            [AddQuantity { delta }] => format!("@{target} += {}", display_amount(delta.as_ref())),
            [RunCommand { command }] => format!("@{target}: command({})", editor_json(command)),
            _ => editor_json(self),
        }
    }
}

fn parse_consequence(target: &str, operation: &str) -> Result<Consequence, String> {
    if operation.is_empty() {
        return Ok(SetQuantity { value: None });
    }
    if let Some(value) = operation.strip_prefix("+=") {
        return Ok(AddQuantity {
            delta: amount(value)?,
        });
    }
    if let Some(value) = operation.strip_prefix('=') {
        return Ok(SetQuantity {
            value: amount(value)?,
        });
    }
    let command = operation
        .strip_prefix(':')
        .ok_or(CONSEQUENCE_SYNTAX)?
        .trim();
    if let Some(effect) = transfer_consequence::parse(target, command)? {
        return Ok(effect);
    }
    if command == "activate-fiote" {
        return Ok(ActivateFiote);
    }
    if let Some(command) = command
        .strip_prefix("run(@")
        .and_then(|source| source.strip_suffix(')'))
    {
        return Ok(InvokeCommand {
            command: command.trim().into(),
        });
    }
    if let Some(state) = command
        .strip_prefix("show(")
        .and_then(|source| source.strip_suffix(')'))
    {
        let mut component: ComponentState =
            from_str(state).map_err(|error| format!("Invalid component state: {error}"))?;
        if let Some(record) = component.record_mut()
            && record.is_empty()
        {
            *record = target.into();
        }
        return Ok(ShowComponent { component });
    }
    let command = command
        .strip_prefix("command(")
        .and_then(|source| source.strip_suffix(')'))
        .ok_or("Use command(\"shell command\")")?;
    Ok(RunCommand {
        command: from_str(command).map_err(|error| format!("Quote the command: {error}"))?,
    })
}

#[expect(
    clippy::expect_used,
    reason = "the editor values use JSON primitives and string keys"
)]
fn editor_json(value: &impl Serialize) -> String {
    to_string(value).expect("editor values have an infallible JSON representation")
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

fn display_amount(value: Option<&DecimalValue>) -> String {
    value.map_or_else(|| "result".into(), ToString::to_string)
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
    fn invalid_effects_are_reported_before_duplicates() {
        let input = r#"{"target":"balance","consequences":[{"kind":"set-quantity"},{"kind":"set-quantity"},{"kind":"run-command","command":""}]}"#;
        assert_eq!(
            RuleConsequence::parse(input).unwrap_err(),
            "parse error: `run-command` needs something to run"
        );
    }

    #[test]
    fn interactive_conditions_are_bounded_before_recursive_parsing() {
        let quoted_operators = format!(r#""é\"{}""#, "(-".repeat(200));
        assert!(check_condition_source(&quoted_operators).is_ok());
        assert!(check_condition_source(&"é".repeat(MAX_CONDITION_BYTES / 2)).is_ok());
        assert!(check_condition_source(&"é".repeat(MAX_CONDITION_BYTES / 2 + 1)).is_err());
        assert!(check_condition_source("-1 * freq(@daily) * @balance").is_ok());
        assert!(
            check_condition_source(&format!("{}@balance{}", "(".repeat(40), ")".repeat(40)))
                .is_err()
        );
        assert!(check_condition_source(&format!("{}@balance", "-".repeat(200))).is_err());
    }
}
