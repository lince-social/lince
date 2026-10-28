use crate::DecimalValue;
use microlp::{ComparisonOp, OptimizationDirection, Problem, ResumeOptions, SolutionStatus};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

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

fn report(status: Status, model: &Model, started: Instant, message: Option<String>) -> Report {
    Report {
        status,
        values: Vec::new(),
        objective: None,
        achieved: Vec::new(),
        diagnostics: Vec::new(),
        notes: model.notes.clone(),
        message,
        elapsed_ms: started.elapsed().as_millis() as u64,
        solver: "microlp 0.6.0; continuous simplex".into(),
    }
}

pub fn solve(model: &Model, options: Options, cancelled: &AtomicBool) -> Report {
    let started = Instant::now();
    if let Err(error) = model.validate() {
        return report(Status::InvalidModel, model, started, Some(error));
    }
    if options.time_limit_ms > 30_000 || options.slice_ms == 0 || options.slice_ms > 200 {
        return report(
            Status::InvalidModel,
            model,
            started,
            Some("Use at most 30000 ms and slices of 1–200 ms".into()),
        );
    }
    let mut result = solve_inner(model, options, cancelled, started);
    if result.status == Status::Infeasible {
        result.diagnostics = impossible_requirements(model);
        let remaining = options
            .time_limit_ms
            .saturating_sub(started.elapsed().as_millis() as u64)
            .min(1000);
        if remaining > 0 && !cancelled.load(Ordering::Relaxed) {
            result
                .diagnostics
                .extend(relaxation(model, remaining, cancelled));
        }
    }
    result.elapsed_ms = started.elapsed().as_millis() as u64;
    result
}

fn solve_inner(
    model: &Model,
    options: Options,
    cancelled: &AtomicBool,
    started: Instant,
) -> Report {
    if cancelled.load(Ordering::Relaxed)
        || started.elapsed() >= Duration::from_millis(options.time_limit_ms)
    {
        return report(
            Status::Interrupted,
            model,
            started,
            Some("Cancelled or time budget reached".into()),
        );
    }
    let mut problem = Problem::new(OptimizationDirection::Minimize);
    let variables: Vec<_> = model
        .variables
        .iter()
        .map(|v| problem.add_var(v.cost.to_f64(), bounds(v.lower, v.upper, true).unwrap()))
        .collect();
    for c in &model.constraints {
        let terms: Vec<_> = c
            .terms
            .iter()
            .map(|t| (variables[t.variable], t.coefficient.to_f64()))
            .collect();
        if c.lower == c.upper && c.lower.is_some() {
            problem.add_constraint(&terms, ComparisonOp::Eq, c.lower.unwrap().to_f64());
        } else {
            if let Some(lower) = c.lower {
                problem.add_constraint(&terms, ComparisonOp::Ge, lower.to_f64());
            }
            if let Some(upper) = c.upper {
                problem.add_constraint(&terms, ComparisonOp::Le, upper.to_f64());
            }
        }
    }
    let slice = || {
        Duration::from_millis(
            options.slice_ms.min(
                options
                    .time_limit_ms
                    .saturating_sub(started.elapsed().as_millis() as u64),
            ),
        )
    };
    if cancelled.load(Ordering::Relaxed) || slice().is_zero() {
        return report(Status::Interrupted, model, started, None);
    }
    problem.set_time_limit(slice());
    let mut outcome = problem.solve();
    loop {
        let current = match outcome {
            Ok(value) => value,
            Err(error) => {
                let status = match error {
                    microlp::Error::Infeasible => Status::Infeasible,
                    microlp::Error::Unbounded => Status::Unbounded,
                    _ => Status::NumericalFailure,
                };
                return report(status, model, started, Some(error.to_string()));
            }
        };
        if current.is_optimal() || cancelled.load(Ordering::Relaxed) || slice().is_zero() {
            let Ok(solution) = current.into_solution() else {
                return report(
                    Status::Interrupted,
                    model,
                    started,
                    Some("Stopped before a feasible assignment was found".into()),
                );
            };
            let values: Vec<_> = variables.iter().map(|v| solution.var_value(*v)).collect();
            let achieved = match model.verify(&values) {
                Ok(value) => value,
                Err(error) => return report(Status::NumericalFailure, model, started, Some(error)),
            };
            let objective: f64 = model
                .variables
                .iter()
                .zip(&values)
                .map(|(v, value)| v.cost.to_f64() * value)
                .sum();
            if !objective.is_finite() {
                return report(
                    Status::NumericalFailure,
                    model,
                    started,
                    Some("Non-finite objective".into()),
                );
            }
            let status = if solution.status() == SolutionStatus::Optimal {
                Status::Optimal
            } else {
                Status::Feasible
            };
            let mut result = report(status, model, started, None);
            result.values = values;
            result.achieved = achieved;
            result.objective = Some(objective);
            return result;
        }
        let mut resume = ResumeOptions::default();
        resume.time_limit = Some(slice());
        outcome = current.resume_with(resume);
    }
}

fn impossible_requirements(model: &Model) -> Vec<Diagnostic> {
    model
        .constraints
        .iter()
        .filter_map(|c| {
            let (mut min, mut max) = (0.0, 0.0);
            for term in &c.terms {
                let coefficient = term.coefficient.to_f64();
                if coefficient == 0.0 {
                    continue;
                }
                let v = &model.variables[term.variable];
                let (lo, hi) = bounds(v.lower, v.upper, true).unwrap();
                if coefficient >= 0.0 {
                    min += coefficient * lo;
                    max += coefficient * hi;
                } else {
                    min += coefficient * hi;
                    max += coefficient * lo;
                }
            }
            let shortfall = c
                .lower
                .map(DecimalValue::to_f64)
                .filter(|lo| *lo > max)
                .map(|lo| lo - max)
                .or_else(|| {
                    c.upper
                        .map(DecimalValue::to_f64)
                        .filter(|hi| *hi < min)
                        .map(|hi| min - hi)
                });
            shortfall.map(|shortfall| Diagnostic {
                constraint: c.name.clone(),
                reason: "Impossible within individual variable bounds".into(),
                shortfall: Some(shortfall),
            })
        })
        .collect()
}

fn relaxation(model: &Model, budget: u64, cancelled: &AtomicBool) -> Vec<Diagnostic> {
    let mut relaxed = Model {
        variables: model
            .variables
            .iter()
            .cloned()
            .map(|mut v| {
                v.cost = decimal(0.0).unwrap();
                v
            })
            .collect(),
        ..Default::default()
    };
    let mut slacks = Vec::new();
    for (index, c) in model.constraints.iter().enumerate() {
        for (suffix, target, coefficient) in [("lower", c.lower, 1.0), ("upper", c.upper, -1.0)] {
            let Some(target) = target else {
                continue;
            };
            let variable = relaxed.variables.len();
            relaxed.variables.push(Variable {
                name: format!("relax:{index}:{suffix}"),
                cost: decimal(1.0 / target.to_f64().abs().max(1.0)).unwrap(),
                lower: None,
                upper: None,
            });
            let mut terms = c.terms.clone();
            terms.push(Term {
                variable,
                coefficient: decimal(coefficient).unwrap(),
            });
            relaxed.constraints.push(Constraint {
                name: format!("relax:{index}:{suffix}"),
                terms,
                lower: (coefficient > 0.0).then_some(target),
                upper: (coefficient < 0.0).then_some(target),
            });
            slacks.push((variable, c.name.clone(), suffix));
        }
    }
    if relaxed.validate().is_err() {
        return Vec::new();
    }
    let result = solve_inner(
        &relaxed,
        Options {
            time_limit_ms: budget,
            slice_ms: 100,
        },
        cancelled,
        Instant::now(),
    );
    if !matches!(result.status, Status::Optimal | Status::Feasible) {
        return Vec::new();
    }
    slacks
        .into_iter()
        .filter_map(|(variable, name, suffix)| {
            let value = result.values[variable];
            (value > 1e-7).then(|| Diagnostic {
                constraint: name,
                reason: format!(
                    "Diagnostic relaxation violates this {suffix}; this is not a valid plan"
                ),
                shortfall: Some(value),
            })
        })
        .collect()
}
