use super::*;

fn eval(src: &str, setup: impl FnOnce(&mut MapResolver)) -> f64 {
    let e = Expr::parse(src).unwrap();
    let mut r = MapResolver::default();
    setup(&mut r);
    e.eval(&mut r).unwrap()
}

#[test]
fn plain_math() {
    assert_eq!(eval("1 + 2 * 3", |_| {}), 7.0);
    assert_eq!(eval("-(2 + 3) * 2", |_| {}), -10.0);
    assert_eq!(eval("10 % 3", |_| {}), 1.0);
}

#[test]
fn booleans_are_numbers_no_times_one_workaround() {
    assert_eq!(
        eval("(@apples.stock < 3) + 2", |r| r.set(
            "quantity",
            "apples.stock",
            None,
            2.0
        )),
        3.0
    );
    assert_eq!(eval("!0", |_| {}), 1.0);
    assert_eq!(eval("1 && 0 || 1", |_| {}), 1.0);
}

#[test]
fn bare_ref_is_quantity_sugar() {
    assert_eq!(
        eval("@apples.stock + quantity(@apples.stock)", |r| {
            r.set("quantity", "apples.stock", None, 4.0)
        }),
        8.0
    );
}

#[test]
fn full_math_composition_from_the_blueprint() {
    assert_eq!(
        eval("-1 * freq(@daily-7am)", |r| r.set(
            "freq",
            "daily-7am",
            None,
            1.0
        )),
        -1.0
    );
    assert_eq!(
        eval("(@checking - value(@rules.monthly-burn)) < 500", |r| {
            r.set("quantity", "checking", None, 900.0);
            r.set("value", "rules.monthly-burn", None, 600.0);
        }),
        1.0
    );
}

#[test]
fn durations() {
    assert_eq!(
        eval("sum(@checking, 30d) / 30", |r| {
            r.set("sum", "checking", Some(30 * 86400), 900.0)
        }),
        30.0
    );
    assert_eq!(eval("90s + 1m", |_| {}), 150.0);
}

#[test]
fn token_extraction() {
    let e = Expr::parse("freq(@weekly) * @books + sum(@reading.log, 7d)").unwrap();
    let t = e.tokens();
    assert!(t.contains(&TokenKey {
        func: "freq".into(),
        slug: "weekly".into(),
        dur_secs: None
    }));
    assert!(t.contains(&TokenKey {
        func: "quantity".into(),
        slug: "books".into(),
        dur_secs: None
    }));
    assert!(t.contains(&TokenKey {
        func: "sum".into(),
        slug: "reading.log".into(),
        dur_secs: Some(7 * 86400)
    }));
}

#[test]
fn unknown_token_errors() {
    let e = Expr::parse("@ghost").unwrap();
    let mut r = MapResolver::default();
    assert!(matches!(e.eval(&mut r), Err(NucleusError::UnknownToken(_))));
}

#[test]
fn the_four_units_read_as_seconds() {
    assert_eq!(parse_duration("90s"), Some(90));
    assert_eq!(parse_duration("5m"), Some(300));
    assert_eq!(parse_duration("2h"), Some(7_200));
    assert_eq!(parse_duration("30d"), Some(2_592_000));
}

#[test]
fn anything_that_is_not_a_duration_is_refused_rather_than_guessed() {
    assert_eq!(parse_duration(""), None);
    assert_eq!(parse_duration("30"), None);
    assert_eq!(parse_duration("30w"), None, "weeks are not in this grammar");
    assert_eq!(parse_duration("d"), None);
    assert_eq!(parse_duration("1🕒"), None);
}
