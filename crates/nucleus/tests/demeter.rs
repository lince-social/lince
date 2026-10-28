use nucleus::{
    DecimalValue,
    demeter::*,
    optimization::{Options, Status},
};
use serde_json::json;
use std::{collections::BTreeMap, sync::atomic::AtomicBool};

fn d(value: &str) -> DecimalValue {
    exact(value).unwrap()
}

fn fixture() -> Input {
    Input {
        start: "2026-10-25".into(),
        days: 1,
        currency: "brl".into(),
        requirements: Requirements {
            bounds: vec![NutrientBound {
                nutrient: "energy_kcal".into(),
                unit: Unit::Kcal,
                bounds: Bounds {
                    lower: Some(d("100")),
                    upper: None,
                },
                scope: "total".into(),
                source: "test".into(),
            }],
            ..Default::default()
        },
        foods: vec![Food {
            choice: FoodChoice {
                food: "rice".into(),
                bounds: Vec::new(),
                acquisition: Acquisition::Buy,
                volume_ml_per_g: None,
            },
            label: "Measured cooked rice".into(),
            basis_g: d("100"),
            nutrients: BTreeMap::from([
                ("energy_kcal".into(), json!(100)),
                ("protein_g".into(), json!(10)),
            ]),
        }],
        offers: vec![Offer {
            id: "shop-rice".into(),
            food: "rice".into(),
            currency: "brl".into(),
            price: d("2"),
            package_g: d("100"),
            usable_yield: d("1"),
            available_on: "2026-10-25".into(),
            valid_until: None,
            max_packages: None,
        }],
        stock: Vec::new(),
        forecasts: Vec::new(),
        volume_ml_per_day: None,
    }
}

fn solve(input: &Input) -> Outcome {
    input
        .solve(Options::default(), &AtomicBool::new(false))
        .unwrap()
}

#[test]
fn no_priced_or_available_supply_is_infeasible_instead_of_free_food() {
    let mut input = fixture();
    input.offers.clear();
    let out = solve(&input);
    assert_eq!(out.solution.status, Status::Infeasible);
    assert!(out.solution.objective.is_none());
    assert!(
        out.solution
            .diagnostics
            .iter()
            .any(|d| d.constraint.contains("energy_kcal"))
    );
    input.requirements = Requirements::default();
    let empty = solve(&input);
    assert_eq!(empty.solution.status, Status::Optimal);
    assert_eq!(empty.solution.objective, Some(0.0));
}

#[test]
fn stock_is_finite_free_and_package_rounding_is_a_separate_bill() {
    let mut input = fixture();
    input.stock.push(Stock {
        id: "pantry".into(),
        food: "rice".into(),
        usable_g: d("50"),
        available_on: input.start.clone(),
        expires_on: None,
    });
    let out = solve(&input);
    assert_eq!(out.solution.status, Status::Optimal);
    assert!((out.solution.objective.unwrap() - 1.0).abs() < 1e-7);
    assert_eq!(out.rounded_bill, 2.0);
    assert_eq!(out.shopping[0].packages, 1);
    assert!((out.shopping[0].leftover_usable_g - 50.0).abs() < 1e-7);
}

#[test]
fn expired_inventory_and_future_harvests_cannot_supply_a_different_day() {
    let mut input = fixture();
    input.days = 2;
    input.stock.push(Stock {
        id: "pantry".into(),
        food: "rice".into(),
        usable_g: d("150"),
        available_on: input.start.clone(),
        expires_on: Some(input.start.clone()),
    });
    let out = solve(&input);
    assert!((out.solution.objective.unwrap() - 2.0).abs() < 1e-7);
    input.stock.clear();
    input.foods[0].choice.acquisition = Acquisition::Grow;
    input.forecasts.push(Forecast {
        stock: Stock {
            id: "harvest".into(),
            food: "rice".into(),
            usable_g: d("100"),
            available_on: "2026-10-26".into(),
            expires_on: None,
        },
        cost_per_usable_g: d("0.001"),
        currency: "brl".into(),
        process: "test-process".into(),
    });
    let out = solve(&input);
    assert!((out.solution.objective.unwrap() - 2.1).abs() < 1e-7);
    assert!((out.forecast_cost - 0.1).abs() < 1e-7);
    assert_eq!(out.rounded_bill, 2.0);
}

#[test]
fn daily_requirements_cannot_be_satisfied_only_by_a_weekly_total() {
    let mut input = fixture();
    input.days = 7;
    input.foods[0].choice.bounds.push(FoodBound {
        period: Period::Week,
        bounds: Bounds {
            lower: Some(d("700")),
            upper: Some(d("700")),
        },
    });
    let out = solve(&input);
    assert_eq!(out.solution.status, Status::Optimal);
    assert!(
        out.days
            .iter()
            .all(|day| (day.nutrients["energy_kcal"] - 100.0).abs() < 1e-7)
    );
    input.foods[0].choice.bounds.push(FoodBound {
        period: Period::Day,
        bounds: Bounds {
            lower: None,
            upper: Some(d("90")),
        },
    });
    assert_eq!(solve(&input).solution.status, Status::Infeasible);
}

#[test]
fn fixed_and_excluded_foods_have_explicit_bounds() {
    let mut input = fixture();
    input.foods[0].choice.bounds.push(FoodBound {
        period: Period::Day,
        bounds: Bounds {
            lower: Some(d("120")),
            upper: Some(d("120")),
        },
    });
    assert!((solve(&input).totals_g["rice"] - 120.0).abs() < 1e-7);
    input.foods[0].choice.bounds[0].bounds = Bounds {
        lower: None,
        upper: Some(d("0")),
    };
    assert_eq!(solve(&input).solution.status, Status::Infeasible);
}

#[test]
fn missing_trace_and_null_contribute_zero_and_remain_identifiable() {
    let mut input = fixture();
    input.foods[0]
        .nutrients
        .insert("iodine_ug".into(), json!("Tr"));
    input.requirements.bounds.push(NutrientBound {
        nutrient: "iodine_ug".into(),
        unit: Unit::Ug,
        bounds: Bounds {
            lower: Some(d("150")),
            upper: None,
        },
        scope: "total".into(),
        source: "test".into(),
    });
    let out = solve(&input);
    assert_eq!(out.solution.status, Status::Infeasible);
    assert_eq!(out.missing[0].original, Some(json!("Tr")));
    input.foods[0]
        .nutrients
        .insert("iodine_ug".into(), json!(null));
    assert_eq!(solve(&input).missing[0].original, Some(json!(null)));
    input.foods[0].nutrients.remove("iodine_ug");
    assert!(solve(&input).missing[0].original.is_none());
}

#[test]
fn measured_units_and_preparation_yields_are_converted_explicitly() {
    let mut input = fixture();
    input.requirements.bounds.push(NutrientBound {
        nutrient: "protein_g".into(),
        unit: Unit::Mg,
        bounds: Bounds {
            lower: Some(d("10000")),
            upper: None,
        },
        scope: "total".into(),
        source: "test".into(),
    });
    input.offers[0].package_g = d("200");
    input.offers[0].usable_yield = d("0.5");
    let out = solve(&input);
    assert!((out.totals_g["rice"] - 100.0).abs() < 1e-7);
    assert_eq!(out.shopping[0].bought_g, 200.0);
    input.offers[0].food = "raw-rice".into();
    assert!(input.compile().is_err());
    input.offers[0].food = "rice".into();
    input.offers[0].currency = "usd".into();
    assert!(input.compile().is_err());
}

#[test]
fn a_supplement_only_ceiling_does_not_cap_natural_food_magnesium() {
    let mut input = fixture();
    input.foods[0]
        .nutrients
        .insert("magnesium_mg".into(), json!(500));
    input.requirements.bounds.push(NutrientBound {
        nutrient: "magnesium_supplemental_mg".into(),
        unit: Unit::Mg,
        bounds: Bounds {
            lower: None,
            upper: Some(d("350")),
        },
        scope: "supplemental".into(),
        source: "test".into(),
    });
    assert_eq!(solve(&input).solution.status, Status::Optimal);
    input.requirements.bounds[1].nutrient = "magnesium_mg".into();
    assert!(input.compile().is_err());
}

#[test]
fn container_capacity_requires_a_measured_density() {
    let mut input = fixture();
    input.volume_ml_per_day = Some(d("50"));
    assert!(input.compile().is_err());
    input.foods[0].choice.volume_ml_per_g = Some(d("1"));
    assert_eq!(solve(&input).solution.status, Status::Infeasible);
}

#[test]
fn adult_selection_changes_with_age_and_applies_personal_overrides() {
    let publication = anicca::project(
        &anicca::parse(include_str!(
            "../../../institute/trails/health/Nutrition-needs.lingua"
        ))
        .unwrap(),
    )
    .unwrap();
    let references: Vec<Reference> = publication
        .extensions
        .into_iter()
        .filter(|e| e.namespace == "demeter.reference")
        .map(|e| serde_json::from_value(e.fields).unwrap())
        .collect();
    let mut profile = Profile {
        age: 30,
        group: ReferenceGroup::Female,
        weight_kg: d("60"),
        height_cm: d("165"),
        activity_factor: d("1.4"),
        energy_override_kcal: None,
    };
    let young = select_references(&references, &profile, &[], &[]).unwrap();
    assert_eq!(
        young
            .bounds
            .iter()
            .find(|b| b.nutrient == "iron_mg")
            .unwrap()
            .bounds
            .lower,
        Some(d("18"))
    );
    profile.age = 51;
    let older = select_references(&references, &profile, &[], &[]).unwrap();
    let without_energy =
        select_references(&references, &profile, &[], &["energy_kcal".into()]).unwrap();
    assert!(
        !without_energy
            .bounds
            .iter()
            .any(|b| b.nutrient == "energy_kcal")
    );
    assert!(
        without_energy
            .bounds
            .iter()
            .any(|b| b.nutrient == "protein_g")
    );
    assert_eq!(
        older
            .bounds
            .iter()
            .find(|b| b.nutrient == "iron_mg")
            .unwrap()
            .bounds
            .lower,
        Some(d("8"))
    );
    assert_eq!(
        older
            .bounds
            .iter()
            .find(|b| b.nutrient == "calcium_mg")
            .unwrap()
            .bounds
            .lower,
        Some(d("1200"))
    );
    let override_bound = NutrientBound {
        nutrient: "protein_g".into(),
        unit: Unit::G,
        bounds: Bounds {
            lower: Some(d("90")),
            upper: Some(d("130")),
        },
        scope: "total".into(),
        source: "personal".into(),
    };
    let overridden = select_references(&references, &profile, &[override_bound], &[]).unwrap();
    assert_eq!(
        overridden
            .bounds
            .iter()
            .find(|b| b.nutrient == "protein_g")
            .unwrap()
            .bounds
            .lower,
        Some(d("90"))
    );
}

#[test]
fn energy_shares_constrain_actual_energy_not_only_a_reference_calorie_number() {
    let mut input = fixture();
    input.requirements.energy_shares.push(EnergyShare {
        nutrient: "protein_g".into(),
        lower: d("0.10"),
        upper: d("0.35"),
        kcal_per_g: d("4"),
    });
    assert_eq!(solve(&input).solution.status, Status::Infeasible);
}

#[test]
fn all_463_food_profiles_can_participate_in_a_bounded_weekly_solve() {
    let started = std::time::Instant::now();
    let publication = anicca::project(
        &anicca::parse(include_str!(
            "../../../institute/trails/health/Nutrients.lingua"
        ))
        .unwrap(),
    )
    .unwrap();
    let mut input = fixture();
    input.days = 7;
    input.foods.clear();
    input.offers.clear();
    for extension in publication
        .extensions
        .into_iter()
        .filter(|e| e.namespace == "nutrition")
    {
        let id = extension.target_slug;
        let choice = FoodChoice {
            food: id.clone(),
            bounds: Vec::new(),
            acquisition: Acquisition::Buy,
            volume_ml_per_g: None,
        };
        input
            .foods
            .push(Food::from_nutrition(choice, id.clone(), &extension.fields).unwrap());
        input.offers.push(Offer {
            id: format!("offer:{id}"),
            food: id,
            currency: input.currency.clone(),
            price: d("1"),
            package_g: d("100"),
            usable_yield: d("1"),
            available_on: input.start.clone(),
            valid_until: None,
            max_packages: None,
        });
    }
    assert_eq!(input.foods.len(), 463);
    let out = solve(&input);
    assert_eq!(out.solution.status, Status::Optimal);
    assert_eq!(out.days.len(), 7);
    assert!(
        out.days
            .iter()
            .all(|day| day.nutrients["energy_kcal"] >= 100.0 - 1e-6)
    );
    assert!(started.elapsed().as_secs() < 30);
}
