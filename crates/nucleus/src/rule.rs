//! The rule pipeline (blueprint VI.3): condition -> gate -> carry -> consequences.
//! Trigger and payload finally separate, without losing the old behavior:
//! gate `!=0` is today's `=`, gate `always` is today's `=*`.

use crate::error::NucleusError;
use crate::expr::{Expr, Resolver};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gate {
    NonZero,
    Always,
    Lt(f64),
    Le(f64),
    Gt(f64),
    Ge(f64),
    Eq(f64),
}

impl Gate {
    pub fn parse(s: &str) -> Result<Gate, NucleusError> {
        let s = s.trim();
        if s == "always" {
            return Ok(Gate::Always);
        }
        if s == "!=0" {
            return Ok(Gate::NonZero);
        }
        let (op, rest) = if let Some(r) = s.strip_prefix("<=") {
            ("<=", r)
        } else if let Some(r) = s.strip_prefix(">=") {
            (">=", r)
        } else if let Some(r) = s.strip_prefix("==") {
            ("==", r)
        } else if let Some(r) = s.strip_prefix('<') {
            ("<", r)
        } else if let Some(r) = s.strip_prefix('>') {
            (">", r)
        } else {
            return Err(NucleusError::InvalidGate(s.into()));
        };
        let n: f64 = rest
            .trim()
            .parse()
            .map_err(|_| NucleusError::InvalidGate(s.into()))?;
        Ok(match op {
            "<" => Gate::Lt(n),
            "<=" => Gate::Le(n),
            ">" => Gate::Gt(n),
            ">=" => Gate::Ge(n),
            "==" => Gate::Eq(n),
            _ => unreachable!(),
        })
    }

    pub fn passes(&self, v: f64) -> bool {
        match self {
            Gate::NonZero => v != 0.0,
            Gate::Always => true,
            Gate::Lt(n) => v < *n,
            Gate::Le(n) => v <= *n,
            Gate::Gt(n) => v > *n,
            Gate::Ge(n) => v >= *n,
            Gate::Eq(n) => v == *n,
        }
    }

    pub fn as_str(&self) -> String {
        match self {
            Gate::NonZero => "!=0".into(),
            Gate::Always => "always".into(),
            Gate::Lt(n) => format!("<{n}"),
            Gate::Le(n) => format!("<={n}"),
            Gate::Gt(n) => format!(">{n}"),
            Gate::Ge(n) => format!(">={n}"),
            Gate::Eq(n) => format!("=={n}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Carry {
    Value,
    One,
    Const(f64),
}

impl Carry {
    pub fn parse(s: &str) -> Result<Carry, NucleusError> {
        let s = s.trim();
        match s {
            "value" => Ok(Carry::Value),
            "one" => Ok(Carry::One),
            _ => s
                .strip_prefix("const:")
                .and_then(|n| n.trim().parse().ok())
                .map(Carry::Const)
                .ok_or_else(|| NucleusError::InvalidCarry(s.into())),
        }
    }

    pub fn apply(&self, condition_value: f64) -> f64 {
        match self {
            Carry::Value => condition_value,
            Carry::One => 1.0,
            Carry::Const(c) => *c,
        }
    }

    pub fn as_str(&self) -> String {
        match self {
            Carry::Value => "value".into(),
            Carry::One => "one".into(),
            Carry::Const(c) => format!("const:{c}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsequenceKind {
    SetQuantity,
    AddQuantity,
    EmitPromise,
    RunCommand,
    RunQuery,
    RunAction,
    SetVisibility,
    AdvanceTransfer,
    Activate,
    Deactivate,
    Ask,
    Notify,
}

impl ConsequenceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SetQuantity => "set_quantity",
            Self::AddQuantity => "add_quantity",
            Self::EmitPromise => "emit_promise",
            Self::RunCommand => "run_command",
            Self::RunQuery => "run_query",
            Self::RunAction => "run_action",
            Self::SetVisibility => "set_visibility",
            Self::AdvanceTransfer => "advance_transfer",
            Self::Activate => "activate",
            Self::Deactivate => "deactivate",
            Self::Ask => "ask",
            Self::Notify => "notify",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "set_quantity" => Self::SetQuantity,
            "add_quantity" => Self::AddQuantity,
            "emit_promise" => Self::EmitPromise,
            "run_command" => Self::RunCommand,
            "run_query" => Self::RunQuery,
            "run_action" => Self::RunAction,
            "set_visibility" => Self::SetVisibility,
            "advance_transfer" => Self::AdvanceTransfer,
            "activate" => Self::Activate,
            "deactivate" => Self::Deactivate,
            "ask" => Self::Ask,
            "notify" => Self::Notify,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConsequenceSpec {
    pub kind: ConsequenceKind,
    /// Target record/transfer by `@slug` or uid; None for e.g. notify with only params.
    pub target: Option<String>,
    pub params: Option<serde_json::Value>,
    pub position: i64,
}

/// A parsed, ready-to-evaluate rule. `uid` is the rule's *record* uid
/// (everything is a record); activation is that record's quantity.
#[derive(Debug, Clone)]
pub struct RuleDef {
    pub uid: String,
    pub slug: Option<String>,
    pub condition_src: String,
    pub condition: Expr,
    pub gate: Gate,
    pub carry: Carry,
    /// Minimum interval between firings (blueprint VI.1 `debounce`), parsed
    /// from a duration literal ('90s', '2h'). None = fire on every delivery.
    pub debounce_secs: Option<i64>,
    pub consequences: Vec<ConsequenceSpec>,
}

impl RuleDef {
    #[allow(clippy::too_many_arguments)]
    pub fn parse(
        uid: impl Into<String>,
        slug: Option<String>,
        condition: &str,
        gate: &str,
        carry: &str,
        debounce: Option<&str>,
        consequences: Vec<ConsequenceSpec>,
    ) -> Result<RuleDef, NucleusError> {
        let debounce_secs = match debounce.map(str::trim).filter(|s| !s.is_empty()) {
            Some(s) => Some(
                crate::frequency::parse_duration(s)
                    .ok_or_else(|| NucleusError::Parse(format!("bad debounce `{s}`")))?,
            ),
            None => None,
        };
        Ok(RuleDef {
            uid: uid.into(),
            slug,
            condition_src: condition.to_string(),
            condition: Expr::parse(condition)?,
            gate: Gate::parse(gate)?,
            carry: Carry::parse(carry)?,
            debounce_secs,
            consequences,
        })
    }

    /// A rule with zero consequences is a named derived value (blueprint VI.3):
    /// a spreadsheet cell other rules read via `value(@slug)`.
    pub fn is_derived_value(&self) -> bool {
        self.consequences.is_empty()
    }

    /// The pipeline. Returns None when the gate blocks.
    pub fn evaluate(&self, r: &mut dyn Resolver) -> Result<Option<Firing>, NucleusError> {
        let v = self.condition.eval(r)?;
        if !self.gate.passes(v) {
            return Ok(None);
        }
        Ok(Some(Firing {
            rule_uid: self.uid.clone(),
            condition_value: v,
            carried: self.carry.apply(v),
        }))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Firing {
    pub rule_uid: String,
    pub condition_value: f64,
    pub carried: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::MapResolver;

    fn rule(cond: &str, gate: &str, carry: &str) -> RuleDef {
        RuleDef::parse(
            "r_R1",
            Some("rules.t".into()),
            cond,
            gate,
            carry,
            None,
            vec![],
        )
        .unwrap()
    }

    #[test]
    fn gate_parsing() {
        assert_eq!(Gate::parse("!=0").unwrap(), Gate::NonZero);
        assert_eq!(Gate::parse("always").unwrap(), Gate::Always);
        assert_eq!(Gate::parse("<3").unwrap(), Gate::Lt(3.0));
        assert_eq!(Gate::parse(">= 10").unwrap(), Gate::Ge(10.0));
        assert!(Gate::parse("~5").is_err());
    }

    #[test]
    fn pipeline_gate_blocks_and_carry_separates_payload() {
        let mut r = MapResolver::default();
        r.set("quantity", "apples.stock", None, 8.0);
        // gate <3 blocks at 8
        assert!(
            rule("@apples.stock", "<3", "one")
                .evaluate(&mut r)
                .unwrap()
                .is_none()
        );
        // at 2 it fires, and carry=one decouples the payload from the value
        r.set("quantity", "apples.stock", None, 2.0);
        let f = rule("@apples.stock", "<3", "one")
            .evaluate(&mut r)
            .unwrap()
            .unwrap();
        assert_eq!(f.condition_value, 2.0);
        assert_eq!(f.carried, 1.0);
        // carry=value keeps today's semantics
        let f = rule("@apples.stock", "always", "value")
            .evaluate(&mut r)
            .unwrap()
            .unwrap();
        assert_eq!(f.carried, 2.0);
        // carry=const
        let f = rule("@apples.stock", "always", "const:7.5")
            .evaluate(&mut r)
            .unwrap()
            .unwrap();
        assert_eq!(f.carried, 7.5);
    }

    #[test]
    fn old_operators_survive() {
        let mut r = MapResolver::default();
        r.set("freq", "daily-7am", None, 0.0);
        // today's '=': non-zero passes. -1 * 0 = 0 -> blocked.
        assert!(
            rule("-1 * freq(@daily-7am)", "!=0", "value")
                .evaluate(&mut r)
                .unwrap()
                .is_none()
        );
        r.set("freq", "daily-7am", None, 1.0);
        let f = rule("-1 * freq(@daily-7am)", "!=0", "value")
            .evaluate(&mut r)
            .unwrap()
            .unwrap();
        assert_eq!(f.carried, -1.0);
    }
}
