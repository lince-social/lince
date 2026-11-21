use std::collections::HashMap;

use super::*;

#[derive(Default)]
struct Map(HashMap<String, DecimalValue>);

impl Map {
    fn set(&mut self, func: &str, slug: &str, value: &str) {
        self.0.insert(
            format!("{func}:{slug}"),
            DecimalValue::parse_inferred(value).unwrap(),
        );
    }
}

impl ExactResolver for Map {
    fn lookup(
        &mut self,
        func: &str,
        slug: &str,
        _window: Option<i64>,
    ) -> Result<DecimalValue, ConditionError> {
        self.0
            .get(&format!("{func}:{slug}"))
            .copied()
            .ok_or_else(|| ConditionError::UnknownReference(slug.to_string()))
    }
}

fn eval(source: &str, map: &mut Map) -> DecimalValue {
    Condition::parse(source).unwrap().evaluate(map).unwrap()
}

#[test]
fn a_tenth_plus_two_tenths_is_exactly_three_tenths() {
    let mut map = Map::default();
    map.set("quantity", "opening", "0.1");
    assert_eq!(eval("@opening + 0.2", &mut map).to_string(), "0.3");
}

#[test]
fn two_place_arithmetic_stays_exact() {
    let mut map = Map::default();
    map.set("quantity", "flour.jar", "1000.10");
    assert_eq!(eval("@flour.jar - 0.20", &mut map).to_string(), "999.90");
    map.set("quantity", "cent", "0.01");
    assert_eq!(eval("@cent * 3", &mut map).to_string(), "0.03");
}

#[test]
fn the_old_operators_survive_the_port() {
    let mut map = Map::default();
    map.set("freq", "daily-7am", "0");
    let condition = Condition::parse("-1 * freq(@daily-7am)").unwrap();
    assert_eq!(
        decide(&condition, &Gate::NonZero, &Carry::Value, &mut map).unwrap(),
        None,
        "a timer that did not fire must not fire the rule"
    );

    map.set("freq", "daily-7am", "1");
    let carried = decide(&condition, &Gate::NonZero, &Carry::Value, &mut map)
        .unwrap()
        .unwrap();
    assert_eq!(carried.to_string(), "-1");
}

#[test]
fn a_gate_decides_whether_and_a_carry_decides_what() {
    let mut map = Map::default();
    map.set("quantity", "apples.stock", "8");
    let condition = Condition::parse("@apples.stock").unwrap();
    let gate = Gate::parse("<3").unwrap();
    assert_eq!(
        decide(&condition, &gate, &Carry::One, &mut map).unwrap(),
        None
    );

    map.set("quantity", "apples.stock", "2");
    let carried = decide(&condition, &gate, &Carry::One, &mut map)
        .unwrap()
        .unwrap();
    assert_eq!(carried.to_string(), "1", "carry=one decouples the payload");
    let carried = decide(&condition, &gate, &Carry::Value, &mut map)
        .unwrap()
        .unwrap();
    assert_eq!(carried.to_string(), "2", "carry=value keeps the number");
    let carried = decide(
        &condition,
        &gate,
        &Carry::parse("const:7.5").unwrap(),
        &mut map,
    )
    .unwrap()
    .unwrap();
    assert_eq!(carried.to_string(), "7.5");
}

#[test]
fn comparisons_and_booleans_are_numbers() {
    let mut map = Map::default();
    map.set("quantity", "a", "5");
    map.set("quantity", "b", "2");
    assert_eq!(eval("@a > @b", &mut map).to_string(), "1");
    assert_eq!(eval("@a < @b", &mut map).to_string(), "0");
    assert_eq!(eval("(@a > @b) && (@b > 10)", &mut map).to_string(), "0");
    assert_eq!(eval("(@a > @b) || (@b > 10)", &mut map).to_string(), "1");
    assert_eq!(
        eval("(@b > 10) && @nothing.here", &mut map).to_string(),
        "0"
    );
}

#[test]
fn division_rounds_once_and_says_so() {
    let mut map = Map::default();
    map.set("quantity", "one", "1");
    map.set("quantity", "two", "2");
    map.set("quantity", "ten", "10");
    assert_eq!(eval("@one / 3", &mut map).to_string(), "0.333333");
    assert_eq!(eval("@two / 3", &mut map).to_string(), "0.666667");
    assert_eq!(eval("@ten / 4", &mut map).to_string(), "2.500000");
}

#[test]
fn dividing_by_zero_is_refused_rather_than_infinite() {
    let mut map = Map::default();
    map.set("quantity", "one", "1");
    assert_eq!(
        Condition::parse("@one / 0").unwrap().evaluate(&mut map),
        Err(ConditionError::DivisionByZero)
    );
}

#[test]
fn a_window_is_required_where_it_is_meaningless_without_one() {
    let mut map = Map::default();
    map.set("sum", "spend", "10");
    assert!(matches!(
        Condition::parse("sum(@spend)").unwrap().evaluate(&mut map),
        Err(ConditionError::MissingWindow(_))
    ));
    assert_eq!(eval("sum(@spend, 30d)", &mut map).to_string(), "10");
}

#[test]
fn gates_and_carries_round_trip_through_their_text() {
    for text in ["!=0", "always", "<3", "<=3", ">3", ">=3", "==3"] {
        assert_eq!(Gate::parse(text).unwrap().as_text(), text);
    }
    for text in ["value", "one", "const:7.5"] {
        assert_eq!(Carry::parse(text).unwrap().as_text(), text);
    }
    assert!(Gate::parse("~5").is_err());
    assert!(Carry::parse("whatever").is_err());
}

#[test]
fn a_condition_that_reads_nothing_is_refused() {
    assert!(matches!(
        Condition::parse("1 + 1"),
        Err(ConditionError::Parse(_))
    ));
    assert!(matches!(
        Condition::parse("2 > 5"),
        Err(ConditionError::Parse(_))
    ));
    assert!(Condition::parse("@a > 5").is_ok());
}

#[test]
fn a_second_frequency_is_refused_at_parse_time() {
    assert!(Condition::parse("-1 * freq(@payday)").is_ok());
    assert!(matches!(
        Condition::parse("freq(@payday) + freq(@rent)"),
        Err(ConditionError::Parse(_))
    ));
    assert!(
        matches!(
            Condition::parse("freq(@payday) + freq(@payday)"),
            Err(ConditionError::Parse(_))
        ),
        "the same frequency named twice is still a second timer"
    );
    assert!(Condition::parse("freq(@payday) * @apples.stock").is_ok());
}

#[test]
fn a_condition_reports_what_it_reads() {
    let condition = Condition::parse("@a + freq(@b) * sum(@c, 30d)").unwrap();
    let reads = condition.reads();
    for slug in ["a", "b", "c"] {
        assert!(reads.iter().any(|reading| reading.slug == slug), "{slug}");
    }
}

#[test]
fn extension_literals_are_explicit_and_only_the_record_is_bound() {
    let condition =
        Condition::parse(r#"extension(@apples.stock, "shop.inventory", "price") * 2"#).unwrap();
    let [reading] = condition.reads_ref() else {
        panic!("expected a single extension reading: {condition:?}");
    };
    assert_eq!(reading.slug, "apples.stock");
    assert_eq!(
        crate::expr::extension_parts(&reading.func),
        Some(("shop.inventory".into(), "price".into()))
    );
    let bound = condition
        .map_references(|reading, record| {
            assert_eq!(reading, "extension");
            assert_eq!(record, "apples.stock");
            Ok("original".into())
        })
        .unwrap();
    let [bound_reading] = bound.reads_ref() else {
        panic!("expected a single bound reading: {bound:?}");
    };
    assert_eq!(bound_reading.slug, "original");
    assert_eq!(bound_reading.func, reading.func);
    for invalid in [
        r#"extension(@record)"#,
        r#"extension(@record, @namespace, @property)"#,
        r#"extension(@record, "", "price")"#,
        r#"extension(@record, "work", "nested.price")"#,
        r#"@record + "price""#,
    ] {
        assert!(Condition::parse(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn extension_properties_supply_distinct_exact_values_to_condition_math() {
    let mut readings = Map::default();
    readings.set(
        &crate::expr::extension_reading("inventory", "price"),
        "stock",
        "0.1",
    );
    readings.set(
        &crate::expr::extension_reading("inventory", "count"),
        "stock",
        "3",
    );
    assert_eq!(
        eval(
            r#"extension(@stock, "inventory", "price") * extension(@stock, "inventory", "count")"#,
            &mut readings
        )
        .to_string(),
        "0.3"
    );
}

#[test]
fn overflowing_remainders_are_condition_errors() {
    let mut readings = Map::default();
    readings.0.insert(
        "quantity:minimum".into(),
        DecimalValue::from_integer(i128::MIN),
    );
    readings.set("quantity", "divisor", "-1");
    let condition = Condition::parse("@minimum % @divisor").unwrap();
    assert_eq!(
        condition.evaluate(&mut readings),
        Err(ConditionError::Overflow)
    );
}
