//! Time-sliced simplex execution and infeasibility diagnostics.
// non-lean-yet

use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use microlp::{ComparisonOp, OptimizationDirection, Problem, ResumeOptions, SolutionStatus};

use super::{
    Constraint, Diagnostic, Model, Options, Report, Status, Term, Variable, bounds, decimal,
};
use crate::DecimalValue;

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

/// Validates and solves a model within a time budget and supports cancellation.
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
        .map(|v| {
            let bounds =
                bounds(v.lower, v.upper, true).expect("the solver validates variable bounds");
            problem.add_var(v.cost.to_f64(), bounds)
        })
        .collect();
    for c in &model.constraints {
        let terms: Vec<_> = c
            .terms
            .iter()
            .map(|t| (variables[t.variable], t.coefficient.to_f64()))
            .collect();
        if let Some(lower) = c.lower
            && Some(lower) == c.upper
        {
            problem.add_constraint(&terms, ComparisonOp::Eq, lower.to_f64());
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
                let (lo, hi) =
                    bounds(v.lower, v.upper, true).expect("the solver validates variable bounds");
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
                v.cost = DecimalValue::from_integer(0);
                v
            })
            .collect(),
        ..Default::default()
    };
    let mut slacks = Vec::new();
    for (index, c) in model.constraints.iter().enumerate() {
        for (suffix, target, coefficient) in [("lower", c.lower, 1), ("upper", c.upper, -1)] {
            let Some(target) = target else {
                continue;
            };
            let variable = relaxed.variables.len();
            relaxed.variables.push(Variable {
                name: format!("relax:{index}:{suffix}"),
                cost: decimal(1.0 / target.to_f64().abs().max(1.0))
                    .expect("the reciprocal of a validated bound is finite and at most one"),
                lower: None,
                upper: None,
            });
            let mut terms = c.terms.clone();
            terms.push(Term {
                variable,
                coefficient: DecimalValue::from_integer(coefficient),
            });
            relaxed.constraints.push(Constraint {
                name: format!("relax:{index}:{suffix}"),
                terms,
                lower: (coefficient > 0).then_some(target),
                upper: (coefficient < 0).then_some(target),
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
