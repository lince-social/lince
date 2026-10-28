use nucleus::optimization::{self, Constraint, Model, Options, Status, Term, Variable, decimal};
use std::sync::atomic::AtomicBool;

fn variable(name: &str, cost: f64, upper: Option<f64>) -> Variable {
    Variable {
        name: name.into(),
        cost: decimal(cost).unwrap(),
        lower: None,
        upper: upper.map(|v| decimal(v).unwrap()),
    }
}

fn constraint(
    name: &str,
    coefficients: &[(usize, f64)],
    lower: Option<f64>,
    upper: Option<f64>,
) -> Constraint {
    Constraint {
        name: name.into(),
        terms: coefficients
            .iter()
            .map(|(variable, value)| Term {
                variable: *variable,
                coefficient: decimal(*value).unwrap(),
            })
            .collect(),
        lower: lower.map(|v| decimal(v).unwrap()),
        upper: upper.map(|v| decimal(v).unwrap()),
    }
}

fn solve(model: &Model) -> optimization::Report {
    optimization::solve(model, Options::default(), &AtomicBool::new(false))
}

#[test]
fn minimizes_cost_and_rechecks_the_known_optimum() {
    let model = Model {
        variables: vec![variable("x", 3.0, None), variable("y", 2.0, None)],
        constraints: vec![
            constraint("need", &[(0, 1.0), (1, 1.0)], Some(5.0), None),
            constraint("y-capacity", &[(1, 1.0)], None, Some(3.0)),
        ],
        ..Default::default()
    };
    let result = solve(&model);
    assert_eq!(result.status, Status::Optimal);
    assert!((result.objective.unwrap() - 12.0).abs() < 1e-7);
    assert!((result.values[0] - 2.0).abs() < 1e-7);
    assert!((result.values[1] - 3.0).abs() < 1e-7);
    assert!(model.verify(&result.values).is_ok());
    assert!(model.verify(&[0.0, 0.0]).is_err());
}

#[test]
fn supports_equalities_fixed_variables_and_zero_cost() {
    let mut x = variable("stock", 0.0, Some(2.0));
    x.lower = Some(decimal(2.0).unwrap());
    let model = Model {
        variables: vec![x, variable("buy", 4.0, None)],
        constraints: vec![constraint(
            "fixed-total",
            &[(0, 1.0), (1, 1.0)],
            Some(5.0),
            Some(5.0),
        )],
        ..Default::default()
    };
    let result = solve(&model);
    assert_eq!(result.status, Status::Optimal);
    assert_eq!(result.objective, Some(12.0));
    assert_eq!(result.values, vec![2.0, 3.0]);
}

#[test]
fn reports_impossible_missing_coefficients_without_a_fake_solution() {
    let model = Model {
        variables: vec![variable("food", 1.0, None)],
        constraints: vec![constraint("iodine", &[], Some(150.0), None)],
        ..Default::default()
    };
    let result = solve(&model);
    assert_eq!(result.status, Status::Infeasible);
    assert!(result.values.is_empty());
    assert!(result.objective.is_none());
    assert!(result.diagnostics.iter().any(|d| d.constraint == "iodine"));
}

#[test]
fn labels_relaxation_of_conflicting_constraints_as_diagnostic() {
    let model = Model {
        variables: vec![variable("x", 1.0, None)],
        constraints: vec![
            constraint("minimum", &[(0, 1.0)], Some(5.0), None),
            constraint("maximum", &[(0, 1.0)], None, Some(3.0)),
        ],
        ..Default::default()
    };
    let result = solve(&model);
    assert_eq!(result.status, Status::Infeasible);
    assert!(result.values.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.reason.contains("not a valid plan"))
    );
}

#[test]
fn rejects_bad_indices_duplicates_and_contradictory_bounds() {
    let mut model = Model {
        variables: vec![variable("x", 1.0, Some(1.0))],
        constraints: vec![constraint("row", &[(2, 1.0)], None, Some(2.0))],
        ..Default::default()
    };
    assert_eq!(solve(&model).status, Status::InvalidModel);
    model.constraints[0].terms = vec![
        Term {
            variable: 0,
            coefficient: decimal(1.0).unwrap()
        };
        2
    ];
    assert_eq!(solve(&model).status, Status::InvalidModel);
    model.constraints.clear();
    model.variables[0].lower = Some(decimal(2.0).unwrap());
    assert_eq!(solve(&model).status, Status::InvalidModel);
    assert!(decimal(f64::NAN).is_err());
    assert!(decimal(f64::INFINITY).is_err());
}

#[test]
fn interrupts_before_solving_and_enforces_the_budget_ceiling() {
    let model = Model {
        variables: vec![variable("x", 1.0, None)],
        ..Default::default()
    };
    assert_eq!(
        optimization::solve(&model, Options::default(), &AtomicBool::new(true)).status,
        Status::Interrupted
    );
    assert_eq!(
        optimization::solve(
            &model,
            Options {
                time_limit_ms: 0,
                slice_ms: 1
            },
            &AtomicBool::new(false)
        )
        .status,
        Status::Interrupted
    );
    assert_eq!(
        optimization::solve(
            &model,
            Options {
                time_limit_ms: 30_001,
                slice_ms: 100
            },
            &AtomicBool::new(false)
        )
        .status,
        Status::InvalidModel
    );
}

#[test]
fn identifies_an_unbounded_objective() {
    let model = Model {
        variables: vec![variable("x", -1.0, None)],
        ..Default::default()
    };
    assert_eq!(solve(&model).status, Status::Unbounded);
}

#[test]
fn exact_inputs_round_trip_and_solving_is_repeatable() {
    let model = Model {
        variables: vec![variable("x", 0.125, None)],
        constraints: vec![constraint("need", &[(0, 0.2)], Some(1.0), None)],
        ..Default::default()
    };
    let text = serde_json::to_string(&model).unwrap();
    let decoded: Model = serde_json::from_str(&text).unwrap();
    assert_eq!(serde_json::to_string(&decoded).unwrap(), text);
    let first = solve(&decoded);
    let second = solve(&decoded);
    assert_eq!(first.status, second.status);
    assert_eq!(first.values, second.values);
    assert_eq!(first.objective, second.objective);
}
