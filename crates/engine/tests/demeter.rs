mod support;

use engine::{
    Engine,
    actions::Action,
    demeter::{Command, Config, OfferDraft},
};
use nucleus::{
    demeter::*,
    optimization::{self, Options, decimal},
};
use serde_json::{Value, json};

async fn command(engine: &Engine, request: Command) -> Value {
    engine
        .act(Action::Demeter { request }, None)
        .await
        .unwrap()
        .data
        .unwrap()
}

async fn ready(engine: &Engine) -> (String, String, Config) {
    let food = support::plain(engine, "meal-food", 0.0).await;
    engine
        .act(
            Action::SetExtension {
                target: food.clone(),
                namespace: "nutrition".into(),
                fds: json!({"basis_g":100,"nutrients":{"energy_kcal":100,"protein_g":10}}),
            },
            None,
        )
        .await
        .unwrap();
    let terms = command(
        engine,
        Command::Vocabulary {
            currency: "brl".into(),
        },
    )
    .await;
    let offer = nucleus::new_uid("r");
    command(
        engine,
        Command::SaveOffer {
            offer: OfferDraft {
                id: offer.clone(),
                food: food.clone(),
                currency: terms["currency"].as_str().unwrap().into(),
                price_predicate: terms["price_predicate"].as_str().unwrap().into(),
                price: decimal(2.0).unwrap(),
                package_g: decimal(100.0).unwrap(),
                usable_yield: decimal(1.0).unwrap(),
                vendor: "Local market".into(),
                observed_on: "2026-10-25".into(),
                source_url: String::new(),
                available_on: "2026-10-25".into(),
                valid_until: None,
                max_packages: None,
            },
        },
    )
    .await;
    let mut config = Config::new("2026-10-25".into());
    config.days = 2;
    config.horizon_days = 14;
    config.foods.push(FoodChoice {
        food: food.clone(),
        bounds: Vec::new(),
        acquisition: Acquisition::Buy,
        volume_ml_per_g: None,
    });
    config.offers.push(offer.clone());
    config.manual.bounds.push(NutrientBound {
        nutrient: "energy_kcal".into(),
        unit: Unit::Kcal,
        bounds: Bounds {
            lower: Some(decimal(100.0).unwrap()),
            upper: None,
        },
        scope: "total".into(),
        source: "manual test target".into(),
    });
    let saved = command(
        engine,
        Command::Save {
            plan: None,
            expected_revision: 0,
            config: config.clone(),
        },
    )
    .await;
    (saved["plan"].as_str().unwrap().into(), offer, config)
}

async fn solved(engine: &Engine, plan: &str) -> (String, Value) {
    let run = command(
        engine,
        Command::Solve {
            plan: plan.into(),
            options: Options::default(),
        },
    )
    .await["run"]
        .as_str()
        .unwrap()
        .to_owned();
    for _ in 0..200 {
        let status = command(engine, Command::Poll { run: run.clone() }).await;
        if !["running", "cancelling"].contains(&status["state"].as_str().unwrap()) {
            return (run, status);
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("bounded solve did not finish");
}

#[tokio::test]
async fn prices_are_real_assertions_and_repeated_acceptance_preserves_completed_work() {
    let engine = support::engine().await;
    let (plan, offer, mut config) = ready(&engine).await;
    let assertions =
        store::assertions::for_subjects(&engine.store.pool, std::slice::from_ref(&offer))
            .await
            .unwrap();
    assert!(assertions.iter().any(|a| a.predicate == "price"
        && a.quantity == Some(decimal(2.0).unwrap())
        && a.unit_uid.is_some()));
    let (run, status) = solved(&engine, &plan).await;
    assert_eq!(status["state"], "optimal");
    assert_eq!(status["result"]["rounded_bill"], 4.0);
    let preview = command(
        &engine,
        Command::Project {
            plan: plan.clone(),
            run: run.clone(),
        },
    )
    .await;
    assert!(
        preview["projection"]["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["kind"] == "buy")
    );
    let accepted = command(
        &engine,
        Command::Accept {
            plan: plan.clone(),
            run: run.clone(),
        },
    )
    .await;
    assert_eq!(accepted["accepted"], true);
    let tasks = store::records::all_extensions(&engine.store.pool, engine::demeter::TASK)
        .await
        .unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(
        command(
            &engine,
            Command::Accept {
                plan: plan.clone(),
                run
            }
        )
        .await["already_accepted"],
        true
    );
    let completed = tasks.keys().next().unwrap().clone();
    engine
        .act(
            Action::SetQuantityExact {
                target: completed.clone(),
                amount: "0".into(),
            },
            None,
        )
        .await
        .unwrap();
    config.horizon_days = 7;
    command(
        &engine,
        Command::Save {
            plan: Some(plan.clone()),
            expected_revision: 1,
            config,
        },
    )
    .await;
    let (run, _) = solved(&engine, &plan).await;
    command(&engine, Command::Accept { plan, run }).await;
    assert_eq!(
        store::records::get(&engine.store.pool, &completed)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .to_f64(),
        0.0
    );
    assert_eq!(
        store::records::all_extensions(&engine.store.pool, engine::demeter::TASK)
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn price_and_composition_changes_reject_stale_acceptance() {
    let engine = support::engine().await;
    let (plan, offer, config) = ready(&engine).await;
    let (run, _) = solved(&engine, &plan).await;
    let market = store::records::get_extension(&engine.store.pool, &offer, engine::demeter::OFFER)
        .await
        .unwrap()
        .unwrap();
    engine
        .act(
            Action::AssertRecord {
                subject: offer,
                predicate: market["predicate"].as_str().unwrap().into(),
                object: None,
                quantity: Some("3".into()),
                unit: Some(market["offer"]["currency"].as_str().unwrap().into()),
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        engine
            .act(
                Action::Demeter {
                    request: Command::Accept {
                        plan: plan.clone(),
                        run
                    }
                },
                None
            )
            .await
            .is_err()
    );
    let (run, _) = solved(&engine, &plan).await;
    engine
        .act(
            Action::SetExtension {
                target: config.foods[0].food.clone(),
                namespace: "nutrition".into(),
                fds: json!({"basis_g":100,"nutrients":{"energy_kcal":200}}),
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        engine
            .act(
                Action::Demeter {
                    request: Command::Accept { plan, run }
                },
                None
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn managed_namespaces_and_foreign_plan_access_are_refused() {
    let engine = support::engine().await;
    let (plan, _, _) = ready(&engine).await;
    assert!(
        engine
            .act(
                Action::SetExtension {
                    target: plan.clone(),
                    namespace: engine::demeter::PLAN.into(),
                    fds: json!({})
                },
                None
            )
            .await
            .is_err()
    );
    let person = support::person(&engine, "intruder").await;
    assert!(
        engine
            .act(
                Action::Demeter {
                    request: Command::Inspect { plan }
                },
                Some(person.uid)
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn generic_optimization_runs_through_the_normal_action_path_and_can_be_cancelled() {
    let engine = support::engine().await;
    let model = optimization::Model {
        variables: vec![optimization::Variable {
            name: "quantity".into(),
            cost: decimal(1.0).unwrap(),
            lower: Some(decimal(2.0).unwrap()),
            upper: None,
        }],
        ..Default::default()
    };
    let run = engine
        .act(
            Action::Optimize {
                request: engine::optimization::Command::Start {
                    model,
                    options: Options::default(),
                },
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let result = engine
        .act(
            Action::Optimize {
                request: engine::optimization::Command::Cancel { run: run.clone() },
            },
            None,
        )
        .await
        .unwrap();
    assert!(result.data.is_some());
    for _ in 0..200 {
        let status = engine
            .act(
                Action::Optimize {
                    request: engine::optimization::Command::Poll { run: run.clone() },
                },
                None,
            )
            .await
            .unwrap()
            .data
            .unwrap();
        if !["running", "cancelling"].contains(&status["state"].as_str().unwrap()) {
            assert!(["optimal", "interrupted"].contains(&status["state"].as_str().unwrap()));
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("cancelled solve did not finish");
}

#[tokio::test]
async fn imports_the_complete_meal_trail_as_ordinary_sources() {
    let engine = support::engine().await;
    let uri = format!(
        "{}/../../institute/trails/health/Meal-preparation.lingua",
        env!("CARGO_MANIFEST_DIR")
    );
    let preview = engine
        .act(
            Action::Sources {
                request: engine::sources::Command::Preview {
                    uri: uri.clone(),
                    selected: Vec::new(),
                    bindings: Default::default(),
                    refresh: false,
                },
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
    let imported = engine
        .act(
            Action::Sources {
                request: engine::sources::Command::Import {
                    uri,
                    selected: Vec::new(),
                    bindings: Default::default(),
                    revision: preview["revision"].as_str().unwrap().into(),
                    update: false,
                },
            },
            None,
        )
        .await
        .unwrap();
    assert!(imported.data.is_some());
    let catalog = command(&engine, Command::Catalog).await;
    assert_eq!(catalog["foods"].as_array().unwrap().len(), 463);
    assert_eq!(catalog["references"].as_array().unwrap().len(), 41);
    assert_eq!(catalog["templates"].as_array().unwrap().len(), 6);
}

#[tokio::test]
async fn switching_to_supermarket_supersedes_pending_farm_work_and_preserves_done_tasks() {
    let engine = support::engine().await;
    let (plan, _, mut config) = ready(&engine).await;
    let species = support::plain(&engine, "test-species", 0.0).await;
    let process = support::plain(&engine, "test-process", 0.0).await;
    engine.act(Action::SetExtension {target:process.clone(),namespace:"demeter.production".into(),fds:json!({"id":process,"species":species,"method":"seed","first_harvest_days":1,"harvest_interval_days":0,"harvest_count":1,"usable_g_per_harvest":"100","season_months":[1,2,3,4,5,6,7,8,9,10,11,12],"requires_confirmation":true,"assumption":"Test measured yield","inputs":[{"item":"seed","quantity":"1","unit":"each"}],"stages":[]})},None).await.unwrap();
    config.foods[0].acquisition = Acquisition::Grow;
    config.production.push(serde_json::from_value(json!({"food":config.foods[0].food,"template":process,"confirmed":true,"start_on":config.start,"yield_override_g":null,"lead_days_override":null,"season_months":[1,2,3,4,5,6,7,8,9,10,11,12],"usable_yield":{"scale":0,"value":"1"},"shelf_life_days":7,"max_units":null,"existing_units":null,"cost_per_usable_g":{"scale":2,"value":"0.01"},"currency":"brl","supermarket_fallback":true})).unwrap());
    command(
        &engine,
        Command::Save {
            plan: Some(plan.clone()),
            expected_revision: 1,
            config,
        },
    )
    .await;
    let (run, _) = solved(&engine, &plan).await;
    command(
        &engine,
        Command::Accept {
            plan: plan.clone(),
            run,
        },
    )
    .await;
    let tasks = store::records::all_extensions(&engine.store.pool, engine::demeter::TASK)
        .await
        .unwrap();
    let completed = tasks
        .iter()
        .find(|(_, m)| m["task"]["kind"] == "establish")
        .unwrap()
        .0
        .clone();
    engine
        .act(
            Action::SetQuantityExact {
                target: completed.clone(),
                amount: "0".into(),
            },
            None,
        )
        .await
        .unwrap();
    command(
        &engine,
        Command::Acquisition {
            plan: plan.clone(),
            expected_revision: 2,
            food: None,
            supermarket: true,
        },
    )
    .await;
    let (run, _) = solved(&engine, &plan).await;
    command(&engine, Command::Accept { plan, run }).await;
    let after = store::records::all_extensions(&engine.store.pool, engine::demeter::TASK)
        .await
        .unwrap();
    assert_eq!(
        store::records::get(&engine.store.pool, &completed)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .to_f64(),
        0.0
    );
    assert!(after.values().any(|m| m["state"] == "superseded"));
    assert!(
        after
            .values()
            .any(|m| m["state"] == "pending" && m["task"]["kind"] == "buy")
    );
    assert!(
        after
            .values()
            .any(|m| m["state"] == "pending" && m["task"]["kind"] == "harvest")
    );
}

#[tokio::test]
async fn confirmed_existing_production_supplies_meals_only_from_its_harvest_date() {
    let engine = support::engine().await;
    let (plan, _, mut config) = ready(&engine).await;
    let species = support::plain(&engine, "existing-production-species", 0.0).await;
    let process = support::plain(&engine, "existing-production-process", 0.0).await;
    engine
        .act(
            Action::SetExtension {
                target: process.clone(),
                namespace: "demeter.production".into(),
                fds: json!({
                    "species":species,"id":process,"food_outputs":[config.foods[0].food],
                    "method":"measured-site","first_harvest_days":1,
                    "harvest_interval_days":0,"harvest_count":1,
                    "usable_g_per_harvest":"100","season_months":[1,2,3,4,5,6,7,8,9,10,11,12],
                    "requires_confirmation":true,"assumption":"A measured existing plant",
                    "inputs":[],"stages":[]
                }),
            },
            None,
        )
        .await
        .unwrap();
    config.foods[0].acquisition = Acquisition::Grow;
    config.production.push(
        serde_json::from_value(json!({
            "food":config.foods[0].food,"template":process,"confirmed":true,
            "start_on":config.start,"yield_override_g":null,"lead_days_override":null,
            "season_months":[1,2,3,4,5,6,7,8,9,10,11,12],"usable_yield":decimal(1.0).unwrap(),
            "shelf_life_days":2,"max_units":null,"existing_units":1,
            "cost_per_usable_g":decimal(0.005).unwrap(),"currency":"brl",
            "supermarket_fallback":true
        }))
        .unwrap(),
    );
    command(
        &engine,
        Command::Save {
            plan: Some(plan.clone()),
            expected_revision: 1,
            config,
        },
    )
    .await;
    let (run, status) = solved(&engine, &plan).await;
    let out = &status["result"];
    assert_eq!(out["solution"]["status"], "optimal");
    assert_eq!(out["solution"]["objective"].as_f64(), Some(2.5));
    assert_eq!(out["rounded_bill"].as_f64(), Some(2.0));
    assert_eq!(out["forecast_cost"].as_f64(), Some(0.5));
    let projected = command(&engine, Command::Project { plan, run }).await;
    let tasks = projected["projection"]["tasks"].as_array().unwrap();
    assert!(!tasks.iter().any(|t| t["kind"] == "establish"));
    assert!(
        tasks
            .iter()
            .any(|t| t["kind"] == "harvest" && t["on"] == "2026-10-26")
    );
}
