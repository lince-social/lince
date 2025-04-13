use crate::application::{
    providers::{
        karma::get::provider_karma_get,
        record::{
            get_quantity_by_id::provider_record_get_quantity_by_id_sync,
            set_quantity::provider_record_set_quantity_sync,
        },
    },
    use_cases::karma::frequency::use_case_frequency_check,
};
use regex::Regex;
use rhai::Engine;
pub async fn use_case_karma_deliver() {
    println!("Delivering Karma...");

    let engine = return_engine();
    let vec_karma = provider_karma_get();
    let regex_rq = Regex::new(r"rq(\d+)").unwrap();
    let regex_f = Regex::new(r"f(\d+)").unwrap();

    for karma in &vec_karma {
        let mut karma_condition = karma.condition.clone();
        println!("{}", karma_condition);

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
            let replacement_f = use_case_frequency_check(id).to_string();
            karma_condition.replace_range(range, &replacement_f);
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

    println!("Karma delivered");
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
    engine.register_fn("/", |a: i64, b: bool| a / if b { 1 } else { 1 });
    engine.register_fn("/", |a: bool, b: i64| if a { 1 } else { 0 } / b.max(1));

    engine.register_fn("%", |a: f64, b: bool| {
        a % if b { 1.0 } else { f64::MIN_POSITIVE }
    });
    engine.register_fn(
        "%",
        |a: bool, b: f64| if a { 1.0 } else { 0.0 } % b.max(f64::MIN_POSITIVE),
    );
    engine.register_fn("%", |a: i64, b: bool| a % if b { 1 } else { 1 });
    engine.register_fn("%", |a: bool, b: i64| if a { 1 } else { 0 } % b.max(1));
    engine
}
