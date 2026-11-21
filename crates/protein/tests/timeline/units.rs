use super::fixtures::{capture, concept, context, engine, record, rows_of, timeline, window};
use chrono::{Duration, Utc};
use protein::execute;
use serde_json::Value;

#[tokio::test]
async fn two_units_under_one_concept_never_contaminate_each_other() {
    let e = engine().await;
    let stock = concept(&e, "stock", &[]).await;
    let kg = concept(&e, "kg", &[]).await;
    let coin = concept(&e, "coin", &[]).await;

    let jar = record(&e, "jar").await;
    let till = record(&e, "till").await;
    store::records::set_unit(&e.store.pool, &jar, Some(&kg))
        .await
        .unwrap();
    store::records::set_unit(&e.store.pool, &till, Some(&coin))
        .await
        .unwrap();

    let now = Utc::now();
    capture(
        &e,
        &jar,
        "-3",
        &stock,
        &(now - Duration::days(5)).to_rfc3339(),
    )
    .await;
    capture(
        &e,
        &till,
        "-100",
        &stock,
        &(now - Duration::days(5)).to_rfc3339(),
    )
    .await;

    let (since, before) = window();
    let rows = execute(&e.store, &timeline(&stock, since, before))
        .await
        .unwrap();
    let context = context(&rows);

    assert!(
        context["current"].is_null(),
        "a concept spanning two units has no one running total"
    );
    let by_unit = &context["current_by_unit"];
    assert_eq!(by_unit[&kg], "-3");
    assert_eq!(by_unit[&coin], "-100");

    let points = rows_of(&rows, "timeline_point");
    let kg_points: Vec<&Value> = points.iter().copied().filter(|p| p["unit"] == kg).collect();
    let coin_points: Vec<&Value> = points
        .iter()
        .copied()
        .filter(|p| p["unit"] == coin)
        .collect();
    assert_eq!(kg_points.last().unwrap()["cumulative"], "-3");
    assert_eq!(coin_points.last().unwrap()["cumulative"], "-100");
}

#[tokio::test]
async fn one_unit_still_gets_a_plain_running_total() {
    let e = engine().await;
    let rent = concept(&e, "rent", &[]).await;
    let coin = concept(&e, "coin", &[]).await;
    let checking = record(&e, "checking").await;
    store::records::set_unit(&e.store.pool, &checking, Some(&coin))
        .await
        .unwrap();

    let now = Utc::now();
    capture(
        &e,
        &checking,
        "-1200",
        &rent,
        &(now - Duration::days(5)).to_rfc3339(),
    )
    .await;

    let (since, before) = window();
    let rows = execute(&e.store, &timeline(&rent, since, before))
        .await
        .unwrap();
    assert_eq!(context(&rows)["current"], "-1200");
    assert_eq!(context(&rows)["current_by_unit"][&coin], "-1200");
}
