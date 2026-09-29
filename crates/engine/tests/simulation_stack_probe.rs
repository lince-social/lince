use chrono::{TimeDelta, TimeZone, Utc};
use engine::Engine;
use engine::actions::Action;
use engine::sync::OpBatch;
use engine::trust::Signer;
use nucleus::karma::{Cadence, CadenceStep, Consequence};

mod support;

async fn four_engines() -> Vec<(Engine, String)> {
    let mut cells = Vec::new();
    for index in 0..4 {
        let engine = Engine::open_memory().await.unwrap();
        let organ = store::organs::ensure_local(
            &engine.store.pool,
            &format!("http://simulation-{index}.test"),
        )
        .await
        .unwrap()
        .uid;
        engine
            .set_signer(Signer::generate(&organ, "probe"))
            .await
            .unwrap();
        cells.push((engine, organ));
    }
    cells
}

async fn quantity(engine: &Engine, record: &str) -> String {
    store::records::quantity(&engine.store.pool, record)
        .await
        .unwrap()
        .unwrap()
        .to_string()
}

async fn run_rules() -> (Vec<Vec<String>>, Vec<String>) {
    let cells = four_engines().await;
    let start = Utc.with_ymd_and_hms(2030, 1, 1, 0, 0, 0).unwrap();
    let mut records = Vec::new();
    for (index, (engine, _)) in cells.iter().enumerate() {
        let record = support::plain(engine, &format!("stock-{index}"), 10.0).await;
        engine
            .act(
                Action::CreateFrequency {
                    slug: "daily".into(),
                    head: None,
                    every: CadenceStep {
                        days: 1,
                        ..Default::default()
                    },
                    anchor_at: Some((start + TimeDelta::days(1)).to_rfc3339()),
                    request_id: Some(format!("frequency-{index}")),
                },
                None,
            )
            .await
            .unwrap();
        support::declare_rule(
            engine,
            &record,
            Cadence::every_days(1),
            &start.to_rfc3339(),
            Some("freq(@daily)"),
            Some("!=0"),
            Some("value"),
            vec![Consequence::AddQuantity {
                delta: Some(nucleus::DecimalValue::parse_inferred("-3").unwrap()),
            }],
        )
        .await;
        engine.advance_karma_time(start).await.unwrap();
        records.push(record);
    }
    let mut trace = Vec::new();
    for day in 1..=4 {
        let mut values = Vec::new();
        for (index, (engine, _)) in cells.iter().enumerate() {
            engine
                .advance_karma_time(start + TimeDelta::days(day))
                .await
                .unwrap();
            if day == 2 && index == 0 {
                engine.append_user(&records[index], 5.0).await.unwrap();
            }
            values.push(quantity(engine, &records[index]).await);
        }
        trace.push(values);
    }
    (trace, records)
}

#[tokio::test]
#[ignore = "manual simulation stack experiment"]
async fn four_real_engines_follow_virtual_rule_steps() {
    let (first, first_ids) = run_rules().await;
    let (second, second_ids) = run_rules().await;
    assert_eq!(first, second);
    let expected = [
        ["7", "7", "7", "7"],
        ["9", "4", "4", "4"],
        ["6", "1", "1", "1"],
        ["3", "-2", "-2", "-2"],
    ]
    .map(|row| row.map(str::to_owned).to_vec())
    .to_vec();
    assert_eq!(first, expected);
    println!(
        "owned boundary: four real Engines, identical quantity traces across two runs: {first:?}"
    );
    println!(
        "ambient IDs identical across those runs: {}",
        first_ids == second_ids
    );
}

#[tokio::test]
#[ignore = "manual simulation stack experiment"]
async fn four_real_engines_sync_after_delayed_duplicate_delivery() {
    let cells = four_engines().await;
    let source = &cells[0].0;
    let introduction = source.introduction().await.unwrap();
    for (engine, _) in cells.iter().skip(1) {
        engine.adopt_introduction(&introduction, 1).await.unwrap();
    }
    let record = support::plain(source, "shared-probe", 10.0).await;
    let (ops, _) = source.ops_after(0, 100_000).await.unwrap();
    let initial = OpBatch {
        from_organ: cells[0].1.clone(),
        ops,
    };
    for (engine, _) in cells.iter().skip(1).take(2) {
        engine.import_op_batch(&initial).await.unwrap();
        assert_eq!(engine.import_op_batch(&initial).await.unwrap(), 0);
        assert_eq!(quantity(engine, &record).await, "10");
    }
    assert!(
        store::records::get(&cells[3].0.store.pool, &record)
            .await
            .unwrap()
            .is_none()
    );
    source.append_user(&record, -3.0).await.unwrap();
    let (ops, _) = source.ops_after(0, 100_000).await.unwrap();
    let recovered = OpBatch {
        from_organ: cells[0].1.clone(),
        ops,
    };
    for (engine, _) in cells.iter().skip(1) {
        engine.import_op_batch(&recovered).await.unwrap();
        assert_eq!(engine.import_op_batch(&recovered).await.unwrap(), 0);
        assert_eq!(quantity(engine, &record).await, "7");
    }
    println!(
        "owned boundary: four real Engines converged to 7; duplicate delivery did not reapply; withheld peer caught up"
    );
}
