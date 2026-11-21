use nucleus::{DecimalValue, RecordKind};
use protein::{Predicate, Protein, Source, matching_records};
use serde_json::{from_value, json};
use store::{
    Store,
    records::{NewRecord, create},
};

fn decimal(value: &str) -> DecimalValue {
    DecimalValue::parse_inferred(value).expect("fixture decimal")
}

fn predicate(name: &str, value: &str) -> Predicate {
    from_value(json!({ (name): value })).expect("valid quantity predicate JSON")
}

async fn record(store: &Store, slug: &str, quantity: DecimalValue) {
    create(
        &store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head: slug,
            body: "",
            quantity,
        },
    )
    .await
    .expect("create fixture Record");
}

async fn matching_slugs(store: &Store, quantity: Predicate) -> Vec<String> {
    let query = Protein {
        source: Source::Record,
        filter: vec![quantity, Predicate::KindEq("plain".into())],
        fields: None,
        include: Default::default(),
        aggregate: None,
        order: Vec::new(),
        limit: None,
    };
    let rows = matching_records(store, &query, None)
        .await
        .expect("quantity query");
    let mut slugs = rows
        .into_iter()
        .map(|row| row.slug.expect("fixture Record slug"))
        .collect::<Vec<_>>();
    slugs.sort_unstable();
    slugs
}

#[tokio::test]
async fn all_five_record_comparisons_include_their_exact_boundaries() {
    let store = Store::open_memory().await.expect("quantity fixture store");
    for (slug, amount) in [
        ("low", "-1"),
        ("zero", "0"),
        ("scaled-zero", "0.0"),
        ("high", "1"),
    ] {
        record(&store, slug, decimal(amount)).await;
    }
    let cases: [(&str, &[&str]); 5] = [
        ("quantity_lt", &["low"]),
        ("quantity_lte", &["low", "scaled-zero", "zero"]),
        ("quantity_gt", &["high"]),
        ("quantity_gte", &["high", "scaled-zero", "zero"]),
        ("quantity_eq", &["scaled-zero", "zero"]),
    ];
    for (name, expected) in cases {
        assert_eq!(
            matching_slugs(&store, predicate(name, "0.00")).await,
            expected,
            "predicate {name}"
        );
    }
}

#[tokio::test]
async fn comparisons_distinguish_integers_that_binary64_collapses() {
    let store = Store::open_memory().await.expect("quantity fixture store");
    record(&store, "lower", decimal("9007199254740992")).await;
    record(&store, "higher", decimal("9007199254740993")).await;

    assert_eq!(
        matching_slugs(&store, predicate("quantity_eq", "9007199254740992")).await,
        ["lower"]
    );
    assert_eq!(
        matching_slugs(&store, predicate("quantity_gt", "9007199254740992")).await,
        ["higher"]
    );
}

#[tokio::test]
async fn comparisons_distinguish_the_smallest_supported_fraction() {
    let store = Store::open_memory().await.expect("quantity fixture store");
    record(&store, "zero", decimal("0")).await;
    record(&store, "tiny", decimal("0.000000000000000001")).await;

    assert_eq!(
        matching_slugs(&store, predicate("quantity_gt", "0.000000000000000000")).await,
        ["tiny"]
    );
}

#[tokio::test]
async fn comparisons_cover_negative_i128_extremes() {
    let store = Store::open_memory().await.expect("quantity fixture store");
    let minimum = DecimalValue::from_mantissa(0, i128::MIN).expect("minimum mantissa");
    let next = DecimalValue::from_mantissa(0, i128::MIN + 1).expect("next mantissa");
    record(&store, "minimum", minimum).await;
    record(&store, "next", next).await;

    let boundary = minimum.to_string();
    assert_eq!(
        matching_slugs(&store, predicate("quantity_gt", &boundary)).await,
        ["next"]
    );
    assert_eq!(
        matching_slugs(&store, predicate("quantity_lte", &boundary)).await,
        ["minimum"]
    );
}

#[tokio::test]
async fn numeric_equality_matches_equivalent_values_at_every_scale() {
    let store = Store::open_memory().await.expect("quantity fixture store");
    for scale in 0..=18 {
        let factor = 10_i128.pow(u32::from(scale));
        record(
            &store,
            &format!("scale.{scale}"),
            DecimalValue::from_mantissa(scale, factor).expect("supported scale"),
        )
        .await;
    }

    let matches = matching_slugs(&store, predicate("quantity_eq", "1")).await;
    assert_eq!(matches.len(), 19);
}
