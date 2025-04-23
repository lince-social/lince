use std::collections::HashMap;

use crate::{
    application::{
        providers::{
            karma::get::provider_karma_get,
            record::{
                get_quantity_by_id::provider_record_get_quantity_by_id_sync,
                set_quantity::provider_record_set_quantity_sync,
            },
        },
        use_cases::frequency::{
            check::use_case_frequency_check, update::use_case_frequency_update,
        },
    },
    domain::entities::frequency::Frequency,
};
use regex::Regex;
use rhai::Engine;
pub async fn use_case_karma_deliver() {
    let engine = return_engine();
    let vec_karma = provider_karma_get();
    let regex_rq = Regex::new(r"rq(\d+)").unwrap();
    let regex_f = Regex::new(r"f(\d+)").unwrap();
    let mut frequencies_to_update: HashMap<u32, Frequency> = HashMap::new();

    for karma in &vec_karma {
        let mut karma_condition = karma.condition.clone();

        let mut replacements_rq = Vec::new();
        for caps in regex_rq.captures_iter(&karma.condition) {
            let id = caps[1].parse::<u32>().unwrap();
            replacements_rq.push((caps.get(0).unwrap().range(), id));
        }
        for (range, id) in replacements_rq.into_iter().rev() {
            let replacement_rq = provider_record_get_quantity_by_id_sync(id).to_string();
            karma_condition.replace_range(range, &replacement_rq);
        }

        let mut replacements_f = Vec::new();
        for caps in regex_f.captures_iter(&karma.condition) {
            let id = caps[1].parse::<u32>().unwrap();
            replacements_f.push((caps.get(0).unwrap().range(), id));
        }
        for (range, id) in replacements_f.into_iter().rev() {
            let (replacement_f, frequency_to_update) = use_case_frequency_check(id);

            if let Some(frequency_to_update) = frequency_to_update {
                frequencies_to_update.insert(frequency_to_update.id, frequency_to_update);
            }

            karma_condition.replace_range(range, &replacement_f.to_string());
        }

        let karma_condition = format!("({}) * 1.0", karma_condition);
        let karma_condition: f64 = engine.eval(&karma_condition).unwrap();

        match karma.operator.as_str() {
            "=" => {
                if karma_condition != 0.0 {
                    if let Some(caps) = regex_rq.captures(&karma.consequence) {
                        let record_id = &caps[1];
                        provider_record_set_quantity_sync(record_id.to_string(), karma_condition)
                    }
                }
            }
            _ => println!("Invalid operator for Karma with id: {}", karma.id),
        }
    }

    use_case_frequency_update(frequencies_to_update.into_values().collect());
}

pub fn return_engine() -> Engine {
    let mut engine = Engine::new();
    engine.set_fast_operators(false);

    engine.register_fn("+", |a: f64, b: bool| a + if b { 1.0 } else { 0.0 });
    engine.register_fn("+", |a: bool, b: f64| if a { 1.0 } else { 0.0 } + b);
    engine.register_fn("+", |a: i64, b: bool| a + if b { 1 } else { 0 });
    engine.register_fn("+", |a: bool, b: i64| if a { 1 } else { 0 } + b);

    engine.register_fn("-", |a: f64, b: bool| a - if b { 1.0 } else { 0.0 });
    engine.register_fn("-", |a: bool, b: f64| if a { 1.0 } else { 0.0 } - b);
    engine.register_fn("-", |a: i64, b: bool| a - if b { 1 } else { 0 });
    engine.register_fn("-", |a: bool, b: i64| if a { 1 } else { 0 } - b);

    engine.register_fn("*", |a: f64, b: bool| a * if b { 1.0 } else { 0.0 });
    engine.register_fn("*", |a: bool, b: f64| if a { 1.0 } else { 0.0 } * b);
    engine.register_fn("*", |a: i64, b: bool| a * if b { 1 } else { 0 });
    engine.register_fn("*", |a: bool, b: i64| if a { 1 } else { 0 } * b);

    engine.register_fn("/", |a: f64, b: bool| {
        a / if b { 1.0 } else { f64::MIN_POSITIVE }
    });
    engine.register_fn(
        "/",
        |a: bool, b: f64| if a { 1.0 } else { 0.0 } / b.max(f64::MIN_POSITIVE),
    );
    engine.register_fn("/", |a: i64, _b: bool| a);
    engine.register_fn("/", |a: bool, b: i64| if a { 1 } else { 0 } / b.max(1));

    engine.register_fn("%", |a: f64, b: bool| {
        a % if b { 1.0 } else { f64::MIN_POSITIVE }
    });
    engine.register_fn(
        "%",
        |a: bool, b: f64| if a { 1.0 } else { 0.0 } % b.max(f64::MIN_POSITIVE),
    );
    engine.register_fn("%", |a: i64, _b: bool| a);
    engine.register_fn("%", |a: bool, b: i64| if a { 1 } else { 0 } % b.max(1));
    engine
}
