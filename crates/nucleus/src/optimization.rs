//! Linear optimization models and assignment verification.
//!
//! Enable `optimization-solver` to run the time-bounded solver.
// non-lean-yet

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::DecimalValue;

#[cfg(feature = "optimization-solver")]
mod solver;
#[cfg(feature = "optimization-solver")]
pub use solver::solve;

pub const MAX_VARIABLES: usize = 20_000;
pub const MAX_CONSTRAINTS: usize = 30_000;
pub const MAX_TERMS: usize = 500_000;

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Variable {
    pub name: String,
    pub cost: DecimalValue,
    pub lower: Option<DecimalValue>,
    pub upper: Option<DecimalValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Term {
    pub variable: usize,
    pub coefficient: DecimalValue,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Constraint {
    pub name: String,
    pub terms: Vec<Term>,
    pub lower: Option<DecimalValue>,
    pub upper: Option<DecimalValue>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub variables: Vec<Variable>,
    pub constraints: Vec<Constraint>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub time_limit_ms: u64,
    pub slice_ms: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            time_limit_ms: 5000,
            slice_ms: 100,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Optimal,
    Feasible,
    Infeasible,
    Interrupted,
    InvalidModel,
    Unbounded,
    NumericalFailure,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Achieved {
    pub name: String,
    pub value: f64,
    pub lower: Option<f64>,
    pub upper: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Diagnostic {
    pub constraint: String,
    pub reason: String,
    pub shortfall: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Report {
    pub status: Status,
    pub values: Vec<f64>,
    pub objective: Option<f64>,
    pub achieved: Vec<Achieved>,
    pub diagnostics: Vec<Diagnostic>,
    pub notes: Vec<String>,
    pub message: Option<String>,
    pub elapsed_ms: u64,
    pub solver: String,
}

pub fn decimal(value: f64) -> Result<DecimalValue, String> {
    if !value.is_finite() {
        return Err("Numeric values must be finite".into());
    }
    DecimalValue::from_f64_lossy(value)
        .or_else(|_| DecimalValue::parse_inferred(&format!("{value:.18}")))
        .map_err(|e| e.to_string())
}

fn number(value: DecimalValue) -> Result<f64, String> {
    let value = value.to_f64();
    if !value.is_finite() || value.abs() > 1e15 {
        return Err("Numeric values must be finite and at most 10^15 in magnitude".into());
    }
    Ok(value)
}

fn bounds(
    lower: Option<DecimalValue>,
    upper: Option<DecimalValue>,
    nonnegative: bool,
) -> Result<(f64, f64), String> {
    let lo =
        lower
            .map(number)
            .transpose()?
            .unwrap_or(if nonnegative { 0.0 } else { f64::NEG_INFINITY });
    let hi = upper.map(number).transpose()?.unwrap_or(f64::INFINITY);
    if lo > hi || (nonnegative && lo < 0.0) {
        return Err("Contradictory or negative variable bounds".into());
    }
    Ok((lo, hi))
}

fn name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 192
        && name.trim() == name
        && !name.chars().any(char::is_control)
}

impl Model {
    pub fn validate(&self) -> Result<(), String> {
        if self.variables.len() > MAX_VARIABLES
            || self.constraints.len() > MAX_CONSTRAINTS
            || self
                .constraints
                .iter()
                .map(|c| c.terms.len())
                .sum::<usize>()
                > MAX_TERMS
            || self.notes.len() > 128
            || self.notes.iter().any(|n| n.len() > 2048)
        {
            return Err("Optimization model exceeds its size limits".into());
        }
        let mut names = BTreeSet::new();
        for v in &self.variables {
            if !name(&v.name) || !names.insert(&v.name) {
                return Err("Variable names must be unique and nonempty".into());
            }
            number(v.cost)?;
            bounds(v.lower, v.upper, true)?;
        }
        names.clear();
        for c in &self.constraints {
            if !name(&c.name) || !names.insert(&c.name) {
                return Err("Constraint names must be unique and nonempty".into());
            }
            bounds(c.lower, c.upper, false)?;
            let mut indices = BTreeSet::new();
            for t in &c.terms {
                if t.variable >= self.variables.len() || !indices.insert(t.variable) {
                    return Err("Constraint has an unknown or repeated variable".into());
                }
                number(t.coefficient)?;
            }
        }
        Ok(())
    }

    pub fn verify(&self, values: &[f64]) -> Result<Vec<Achieved>, String> {
        self.validate()?;
        if values.len() != self.variables.len() || values.iter().any(|v| !v.is_finite()) {
            return Err("Solver assignment has invalid dimensions or values".into());
        }
        for (v, value) in self.variables.iter().zip(values) {
            let (lo, hi) = bounds(v.lower, v.upper, true)?;
            if !within(*value, lo, hi) {
                return Err(format!("Assignment violates variable {}", v.name));
            }
        }
        self.constraints
            .iter()
            .map(|c| {
                let value: f64 = c
                    .terms
                    .iter()
                    .map(|t| t.coefficient.to_f64() * values[t.variable])
                    .sum();
                let (lo, hi) = bounds(c.lower, c.upper, false)?;
                if !value.is_finite() || !within(value, lo, hi) {
                    return Err(format!("Assignment violates constraint {}", c.name));
                }
                Ok(Achieved {
                    name: c.name.clone(),
                    value,
                    lower: c.lower.map(DecimalValue::to_f64),
                    upper: c.upper.map(DecimalValue::to_f64),
                })
            })
            .collect()
    }
}

fn within(value: f64, lo: f64, hi: f64) -> bool {
    let tolerance = 1e-7 * (1.0 + value.abs());
    value >= lo - tolerance && value <= hi + tolerance
}
