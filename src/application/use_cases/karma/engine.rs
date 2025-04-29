use rhai::Engine;

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
