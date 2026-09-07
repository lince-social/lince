use nucleus::{DecimalValue, RecordKind};
use protein::{Include, Predicate, Protein, Source};
use serde_json::{Map, Value, json};
use store::Store;

fn decimal(value: &str) -> DecimalValue {
    DecimalValue::parse_inferred(value).unwrap()
}

fn predicate(name: &str, value: Value) -> Predicate {
    let mut object = Map::new();
    object.insert(name.to_string(), value);
    serde_json::from_value(Value::Object(object)).unwrap()
}

fn query(filter: Vec<Predicate>) -> Protein {
    Protein {
        source: Source::Record,
        filter,
        fields: None,
        include: Include::default(),
        aggregate: None,
        order: Vec::new(),
        limit: None,
    }
}

async fn record(store: &Store, slug: &str, quantity: DecimalValue) {
    store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head: slug,
            body: "",
            quantity,
        },
    )
    .await
    .unwrap();
}

async fn matching_slugs(store: &Store, quantity: Predicate) -> Vec<String> {
    let rows = protein::matching_records(
        store,
        &query(vec![quantity, Predicate::KindEq("plain".into())]),
        None,
    )
    .await
    .unwrap();
    let mut slugs = rows
        .into_iter()
        .map(|row| row.slug.unwrap())
        .collect::<Vec<_>>();
    slugs.sort();
    slugs
}

#[test]
fn every_quantity_predicate_uses_a_bare_canonical_string() {
    for name in [
        "quantity_lt",
        "quantity_lte",
        "quantity_gt",
        "quantity_gte",
        "quantity_eq",
    ] {
        let mut raw = Map::new();
        raw.insert(name.into(), json!("-12.340"));
        let raw = Value::Object(raw);
        assert_eq!(
            serde_json::to_value(predicate(name, json!("-12.340"))).unwrap(),
            raw
        );
    }
}

#[test]
fn nested_boolean_quantity_predicates_round_trip_exactly() {
    let raw = json!({
        "all": [
            {"quantity_gt": "-1"},
            {"any": [
                {"quantity_lt": "2.00"},
                {"quantity_gte": "3.000"}
            ]},
            {"not": {"quantity_eq": "1.0"}}
        ]
    });
    let parsed: Predicate = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), raw);
}

#[test]
fn every_scale_and_i128_extreme_round_trip() {
    for scale in 0..=18 {
        let value = DecimalValue::from_mantissa(scale, i128::MIN).unwrap();
        let raw = json!({"quantity_eq": value.to_string()});
        let parsed: Predicate = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), raw);

        let value = DecimalValue::from_mantissa(scale, i128::MAX).unwrap();
        let raw = json!({"quantity_eq": value.to_string()});
        let parsed: Predicate = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), raw);
    }
}

#[test]
fn non_string_operands_refuse() {
    for value in [
        json!(0),
        json!(0.0),
        json!(true),
        Value::Null,
        json!([]),
        json!({}),
    ] {
        let mut object = Map::new();
        object.insert("quantity_eq".into(), value);
        assert!(serde_json::from_value::<Predicate>(Value::Object(object)).is_err());
    }
}

#[test]
fn malformed_and_noncanonical_decimal_strings_refuse() {
    for value in [
        "",
        " 1",
        "1 ",
        "+1",
        "01",
        "00.0",
        "-0",
        "-0.0",
        ".1",
        "1.",
        "1e0",
        "1E+0",
        "0.0000000000000000000",
        "170141183460469231731687303715884105728",
        "-170141183460469231731687303715884105729",
    ] {
        let mut object = Map::new();
        object.insert("quantity_eq".into(), json!(value));
        assert!(serde_json::from_value::<Predicate>(Value::Object(object)).is_err());
    }
}

#[test]
fn decimal_string_limit_refuses_before_parsing() {
    let oversized = "1".repeat(65);
    let mut object = Map::new();
    object.insert("quantity_eq".into(), json!(oversized));
    let error = serde_json::from_value::<Predicate>(Value::Object(object)).unwrap_err();
    assert!(error.to_string().contains("byte limit"));
}

#[tokio::test]
async fn all_five_record_comparisons_include_their_exact_boundaries() {
    let store = Store::open_memory().await.unwrap();
    record(&store, "low", decimal("-1")).await;
    record(&store, "zero", decimal("0")).await;
    record(&store, "scaled-zero", decimal("0.0")).await;
    record(&store, "high", decimal("1")).await;

    for (name, expected) in [
        ("quantity_lt", vec!["low"]),
        ("quantity_lte", vec!["low", "scaled-zero", "zero"]),
        ("quantity_gt", vec!["high"]),
        ("quantity_gte", vec!["high", "scaled-zero", "zero"]),
        ("quantity_eq", vec!["scaled-zero", "zero"]),
    ] {
        assert_eq!(
            matching_slugs(&store, predicate(name, json!("0.00"))).await,
            expected
        );
    }
}

#[tokio::test]
async fn comparisons_distinguish_integers_that_binary64_collapses() {
    let store = Store::open_memory().await.unwrap();
    record(&store, "lower", decimal("9007199254740992")).await;
    record(&store, "higher", decimal("9007199254740993")).await;

    assert_eq!(
        matching_slugs(&store, predicate("quantity_eq", json!("9007199254740992"))).await,
        vec!["lower"]
    );
    assert_eq!(
        matching_slugs(&store, predicate("quantity_gt", json!("9007199254740992"))).await,
        vec!["higher"]
    );
}

#[tokio::test]
async fn comparisons_distinguish_the_smallest_supported_fraction() {
    let store = Store::open_memory().await.unwrap();
    record(&store, "zero", decimal("0")).await;
    record(&store, "tiny", decimal("0.000000000000000001")).await;

    assert_eq!(
        matching_slugs(
            &store,
            predicate("quantity_gt", json!("0.000000000000000000"))
        )
        .await,
        vec!["tiny"]
    );
}

#[tokio::test]
async fn comparisons_cover_negative_i128_extremes() {
    let store = Store::open_memory().await.unwrap();
    let minimum = DecimalValue::from_mantissa(0, i128::MIN).unwrap();
    let next = DecimalValue::from_mantissa(0, i128::MIN + 1).unwrap();
    record(&store, "minimum", minimum).await;
    record(&store, "next", next).await;

    assert_eq!(
        matching_slugs(&store, predicate("quantity_gt", json!(minimum.to_string()))).await,
        vec!["next"]
    );
    assert_eq!(
        matching_slugs(
            &store,
            predicate("quantity_lte", json!(minimum.to_string()))
        )
        .await,
        vec!["minimum"]
    );
}

#[tokio::test]
async fn numeric_equality_matches_equivalent_values_at_every_scale() {
    let store = Store::open_memory().await.unwrap();
    for scale in 0..=18 {
        let factor = 10_i128.pow(u32::from(scale));
        record(
            &store,
            &format!("scale.{scale}"),
            DecimalValue::from_mantissa(scale, factor).unwrap(),
        )
        .await;
    }

    let matches = matching_slugs(&store, predicate("quantity_eq", json!("1"))).await;
    assert_eq!(matches.len(), 19);
}
