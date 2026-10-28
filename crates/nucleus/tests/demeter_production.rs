use nucleus::{
    demeter::{production::*, *},
    optimization::{Options, decimal},
};
use serde_json::json;
use std::{collections::BTreeMap, sync::atomic::AtomicBool};

fn fixture() -> (Input, Outcome, Template, Choice) {
    let input = Input {
        start: "2026-12-28".into(),
        days: 7,
        currency: "brl".into(),
        requirements: Requirements {
            bounds: vec![NutrientBound {
                nutrient: "energy_kcal".into(),
                unit: Unit::Kcal,
                bounds: Bounds {
                    lower: Some(exact("100").unwrap()),
                    upper: None,
                },
                scope: "total".into(),
                source: "test".into(),
            }],
            ..Default::default()
        },
        foods: vec![
            Food::from_nutrition(
                FoodChoice {
                    food: "food".into(),
                    bounds: Vec::new(),
                    acquisition: Acquisition::Grow,
                    volume_ml_per_g: None,
                },
                "Food".into(),
                &json!({"basis_g":1,"nutrients":{"energy_kcal":1}}),
            )
            .unwrap(),
        ],
        offers: vec![Offer {
            id: "offer".into(),
            food: "food".into(),
            currency: "brl".into(),
            price: exact("1").unwrap(),
            package_g: exact("100").unwrap(),
            usable_yield: exact("1").unwrap(),
            available_on: "2026-12-28".into(),
            valid_until: None,
            max_packages: None,
        }],
        stock: Vec::new(),
        forecasts: Vec::new(),
        volume_ml_per_day: None,
    };
    let out = input
        .solve(Options::default(), &AtomicBool::new(false))
        .unwrap();
    let template = Template {
        food_outputs: Vec::new(),
        id: "process".into(),
        species: "any-authored-species".into(),
        method: "test".into(),
        first_harvest_days: 14,
        harvest_interval_days: 0,
        harvest_count: 1,
        usable_g_per_harvest: "250".into(),
        season_months: (1..=12).collect(),
        requires_confirmation: true,
        assumption: "Test assumption".into(),
        inputs: vec![Material {
            item: "seed".into(),
            quantity: "1".into(),
            unit: "each".into(),
        }],
        stages: vec![Stage {
            ongoing: false,
            kind: StageKind::Care,
            offset_days: 1,
            title: "Inspect establishment".into(),
            repeat_every_days: Some(7),
            until_days: None,
        }],
    };
    let choice = Choice {
        food: "food".into(),
        template: "process".into(),
        confirmed: true,
        start_on: input.start.clone(),
        yield_override_g: None,
        lead_days_override: None,
        season_months: (1..=12).collect(),
        usable_yield: exact("1").unwrap(),
        shelf_life_days: 7,
        max_units: None,
        existing_units: None,
        cost_per_usable_g: exact("0.01").unwrap(),
        currency: "brl".into(),
        supermarket_fallback: true,
        resources_per_unit: BTreeMap::new(),
        resource_capacity: BTreeMap::new(),
    };
    (input, out, template, choice)
}

#[test]
fn rounds_plants_and_buys_the_startup_gap_across_the_new_year() {
    let (input, out, template, choice) = fixture();
    let p = project(&input, &out, &[choice], &[template], 35, false).unwrap();
    assert_eq!(p.earliest_harvest["food"], "2027-01-11");
    assert_eq!(
        p.tasks
            .iter()
            .find(|t| t.kind == "establish")
            .unwrap()
            .amount,
        exact("3").unwrap()
    );
    assert_eq!(
        p.tasks
            .iter()
            .filter(|t| t.kind == "buy")
            .map(|t| t.amount.to_f64())
            .sum::<f64>(),
        1400.0
    );
    assert!(
        p.forecasts
            .iter()
            .all(|f| f.stock.available_on.as_str() >= "2027-01-11")
    );
    assert!(p.surplus_g["food"] > 0.0);
}

#[test]
fn seasonal_delays_and_capacity_shortfalls_remain_visible() {
    let (input, out, template, mut choice) = fixture();
    choice.season_months = vec![2];
    choice.max_units = Some(1);
    let p = project(&input, &out, &[choice], &[template], 90, false).unwrap();
    assert_eq!(
        p.tasks.iter().find(|t| t.kind == "establish").unwrap().on,
        "2027-02-01"
    );
    assert!(!p.gaps.is_empty());
    assert!(
        p.tasks
            .iter()
            .any(|t| t.kind == "buy" && t.on.as_str() >= "2027-02-01")
    );
}

#[test]
fn repeated_livestock_outputs_and_feed_use_the_same_template_interpreter() {
    let (input, out, mut template, mut choice) = fixture();
    template.first_harvest_days = 3;
    template.harvest_interval_days = 1;
    template.harvest_count = 14;
    template.usable_g_per_harvest = "50".into();
    template.inputs = vec![Material {
        item: "ration".into(),
        quantity: "100".into(),
        unit: "g/day".into(),
    }];
    choice.shelf_life_days = 2;
    let p = project(&input, &out, &[choice], &[template], 14, false).unwrap();
    assert_eq!(p.forecasts.len(), 11);
    assert!(
        p.tasks
            .iter()
            .filter(|t| t.kind == "input")
            .all(|t| t.unit == "g")
    );
    assert!(p.tasks.iter().any(|t| t.kind == "care"));
}

#[test]
fn a_perennial_is_planted_without_inventing_a_first_year_harvest() {
    let (input, out, mut template, choice) = fixture();
    template.first_harvest_days = 1460;
    template.harvest_interval_days = 365;
    template.harvest_count = 10;
    let p = project(&input, &out, &[choice], &[template], 365, false).unwrap();
    assert!(p.forecasts.is_empty());
    assert!(p.tasks.iter().any(|t| t.kind == "establish"));
    assert!(p.earliest_harvest["food"].as_str() > "2027-12-28");
    assert_eq!(
        p.tasks
            .iter()
            .filter(|t| t.kind == "buy")
            .map(|t| t.amount.to_f64())
            .sum::<f64>(),
        36500.0
    );
}

#[test]
fn supermarket_overrides_and_unconfirmed_inputs_do_not_create_planting_tasks() {
    let (input, out, template, mut choice) = fixture();
    let bought = project(
        &input,
        &out,
        &[choice.clone()],
        &[template.clone()],
        28,
        true,
    )
    .unwrap();
    assert!(bought.tasks.iter().all(|t| t.kind == "buy"));
    choice.confirmed = false;
    choice.supermarket_fallback = false;
    let p = project(&input, &out, &[choice], &[template], 28, false).unwrap();
    assert!(p.tasks.iter().all(|t| t.kind == "unresolved"));
    assert!(!p.gaps.is_empty());
}

#[test]
fn resource_capacity_is_explicit_and_replanning_has_stable_task_keys() {
    let (input, out, template, mut choice) = fixture();
    choice
        .resources_per_unit
        .insert("bed-m2".into(), decimal(0.5).unwrap());
    assert!(
        project(
            &input,
            &out,
            &[choice.clone()],
            &[template.clone()],
            28,
            false
        )
        .is_err()
    );
    choice
        .resource_capacity
        .insert("bed-m2".into(), decimal(1.0).unwrap());
    let first = project(
        &input,
        &out,
        &[choice.clone()],
        &[template.clone()],
        28,
        false,
    )
    .unwrap();
    let second = project(&input, &out, &[choice], &[template], 28, false).unwrap();
    assert_eq!(
        first.tasks.iter().map(|t| &t.key).collect::<Vec<_>>(),
        second.tasks.iter().map(|t| &t.key).collect::<Vec<_>>()
    );
    assert!(!first.gaps.is_empty());
}

#[test]
fn existing_livestock_still_need_daily_care_and_feed_after_buying_or_exclusion() {
    let (mut input, _, mut template, mut choice) = fixture();
    template.first_harvest_days = 1;
    template.harvest_interval_days = 1;
    template.harvest_count = 2;
    template.inputs = vec![Material {
        item: "ration".into(),
        quantity: "100".into(),
        unit: "g/day".into(),
    }];
    template.stages[0].ongoing = true;
    template.stages[0].offset_days = 0;
    template.stages[0].repeat_every_days = Some(1);
    choice.existing_units = Some(2);
    input.requirements = Requirements::default();
    input.foods[0].choice.bounds.push(FoodBound {
        period: Period::Day,
        bounds: Bounds {
            lower: None,
            upper: Some(exact("0").unwrap()),
        },
    });
    let out = input
        .solve(Options::default(), &AtomicBool::new(false))
        .unwrap();
    let p = project(&input, &out, &[choice], &[template], 14, true).unwrap();
    assert!(!p.tasks.iter().any(|t| t.kind == "establish"));
    assert_eq!(p.tasks.iter().filter(|t| t.kind == "care").count(), 14);
    assert_eq!(
        p.tasks
            .iter()
            .filter(|t| t.kind == "input")
            .map(|t| t.amount.to_f64())
            .sum::<f64>(),
        2800.0
    );
    assert_eq!(p.tasks.last().unwrap().on, "2027-01-10");
    assert!(p.surplus_g["food"] > 0.0);
}

#[test]
fn cleanup_after_harvest_remains_in_the_lifecycle() {
    let (input, out, mut template, choice) = fixture();
    template.first_harvest_days = 2;
    template.stages = vec![Stage {
        ongoing: false,
        kind: StageKind::Care,
        offset_days: 4,
        title: "Remove residues and clean the bed".into(),
        repeat_every_days: None,
        until_days: None,
    }];
    let p = project(&input, &out, &[choice], &[template], 7, false).unwrap();
    let cleanup = p.tasks.iter().find(|t| t.kind == "care").unwrap();
    assert_eq!(cleanup.on, "2027-01-01");
    assert!(cleanup.title.contains("Remove residues"));
}

#[test]
fn explicit_food_outputs_prevent_using_an_egg_process_for_meat() {
    let (input, out, mut template, choice) = fixture();
    template.food_outputs = vec!["eggs".into()];
    let error = project(&input, &out, &[choice], &[template], 14, false).unwrap_err();
    assert!(error.contains("does not declare the selected food"));
}

#[test]
fn observed_forecasts_replace_same_day_estimates_without_double_counting() {
    let (mut input, _, template, mut choice) = fixture();
    choice.existing_units = Some(1);
    choice.lead_days_override = Some(0);
    input.forecasts.push(Forecast {
        stock: Stock {
            id: "confirmed-harvest".into(),
            food: "food".into(),
            usable_g: exact("100").unwrap(),
            available_on: input.start.clone(),
            expires_on: Some("2027-01-10".into()),
        },
        process: "process".into(),
        currency: "brl".into(),
        cost_per_usable_g: exact("0.01").unwrap(),
    });
    let out = input
        .solve(Options::default(), &AtomicBool::new(false))
        .unwrap();
    let p = project(&input, &out, &[choice], &[template], 7, false).unwrap();
    assert_eq!(
        p.tasks
            .iter()
            .filter(|t| t.kind == "buy")
            .map(|t| t.amount.to_f64())
            .sum::<f64>(),
        600.0
    );
    assert_eq!(p.surplus_g["food"], 0.0);
}
