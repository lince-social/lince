use crate::application::providers::{
    karma::get::provider_karma_get,
    record::{
        get_quantity_by_id::provider_record_get_quantity_by_id_sync,
        set_quantity::provider_record_set_quantity_sync,
    },
};
use regex::Regex;
use rhai::Engine;
pub async fn use_case_karma_deliver() {
    println!("Delivering Karma...");

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

    let vec_karma = provider_karma_get();
    let regex_rq = Regex::new(r"rq(\d+)").unwrap();

    for karma in &vec_karma {
        println!("Karma Condition Original: {}", karma.condition);

        let mut replacements = Vec::new();
        for caps in regex_rq.captures_iter(&karma.condition) {
            let id = caps[1].parse::<u32>().unwrap();
            replacements.push((caps.get(0).unwrap().range(), id));
        }

        // Process async replacements
        let mut karma_condition = karma.condition.clone();
        for (range, id) in replacements.into_iter().rev() {
            let replacement = provider_record_get_quantity_by_id_sync(id).to_string();
            karma_condition.replace_range(range, &replacement);
        }
        let karma_condition = format!("({}) * 1.0", karma_condition);
        let karma_condition: f64 = engine.eval(&karma_condition).unwrap();
        println!("Result: {:?}", karma_condition);

        match karma.operator.as_str() {
            "=" => {
                if let Some(caps) = regex_rq.captures(&karma.consequence) {
                    let record_id = &caps[1];
                    provider_record_set_quantity_sync(record_id.to_string(), karma_condition)
                }
            }
            _ => println!("Invalid operator for Karma with id: {}", karma.id),
        }
    }

    println!("Karma delivered");
}
