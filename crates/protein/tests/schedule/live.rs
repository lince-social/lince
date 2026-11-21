use super::fixtures::task;
use chrono::{DateTime, TimeDelta, Utc};
use engine::{Engine, actions::Action};
use nucleus::RecordKind;
use nucleus::projection::Window;
use protein::{Predicate, execute, execute_for, schedule::query};
use serde_json::json;

#[tokio::test]
async fn current_undated_needs_use_live_quantity_and_obey_source_and_visibility() {
    let engine = Engine::open_memory().await.unwrap();
    let record = engine
        .act(
            Action::CreateRecord {
                slug: Some("floss".into()),
                kind: RecordKind::Plain,
                head: "Passar Fio Dental".into(),
                body: String::new(),
                quantity: -1.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let now = Utc::now().timestamp_millis();
    let window = Window {
        from_ms: now,
        until_ms: now + 3_600_000,
        timezone: "UTC".into(),
    };
    let request = query(window.clone(), vec![Predicate::UidEq(record.clone())]);
    let rows = execute(&engine.store, &request).await.unwrap();
    let need = rows.iter().find(|row| row["record_uid"] == record).unwrap();
    assert_eq!(need["origin"]["kind"], "need");
    assert_eq!(need["category"], "overdue");
    assert_eq!(need["quantity"], "-1");
    assert!(need["time"].is_null());
    assert_eq!(need["preview"], false);
    let filtered = query(window.clone(), vec![Predicate::SlugEq("other".into())]);
    assert!(
        execute(&engine.store, &filtered)
            .await
            .unwrap()
            .iter()
            .all(|row| row["record_uid"] != record)
    );
    let visitor = task(&engine, "visitor", json!({}), 0.0).await;
    assert!(
        execute_for(&engine.store, &request, Some(&visitor))
            .await
            .unwrap()
            .iter()
            .all(|row| row["record_uid"] != record)
    );
    engine
        .act(
            Action::AddQuantity {
                target: record.clone(),
                delta: 1.0,
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        execute(&engine.store, &request)
            .await
            .unwrap()
            .iter()
            .all(|row| row["record_uid"] != record)
    );
    let dated = task(
        &engine,
        "missed-range",
        json!({
            "start": DateTime::from_timestamp_millis(now - 86_400_000).unwrap().to_rfc3339(),
            "estimate_min": 10,
        }),
        -1.0,
    )
    .await;
    let request = query(window, vec![Predicate::UidEq(dated.clone())]);
    let rows = execute(&engine.store, &request).await.unwrap();
    let missed = rows.iter().find(|row| row["record_uid"] == dated).unwrap();
    assert_eq!(missed["category"], "overdue");
    assert_eq!(missed["time"]["from_ms"], now - 86_400_000);
    engine
        .act(
            Action::AddQuantity {
                target: dated.clone(),
                delta: 1.0,
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        execute(&engine.store, &request)
            .await
            .unwrap()
            .iter()
            .all(|row| row["record_uid"] != dated)
    );
}

#[tokio::test]
async fn schedule_keeps_points_intervals_all_day_and_overdue_separate() {
    let engine = Engine::open_memory().await.unwrap();
    let now = Utc::now();
    let at = now + TimeDelta::minutes(20);
    let point = task(&engine, "point", json!({"start":at.to_rfc3339()}), -12.0).await;
    let interval = task(
        &engine,
        "interval",
        json!({"due":at.to_rfc3339(),"estimate_min":10}),
        -100.0,
    )
    .await;
    task(
        &engine,
        "all-day",
        json!({"due":now.format("%Y-%m-%d").to_string()}),
        -1.0,
    )
    .await;
    task(
        &engine,
        "overdue",
        json!({"due":(now - TimeDelta::minutes(1)).to_rfc3339()}),
        -1.0,
    )
    .await;
    task(
        &engine,
        "past-finished",
        json!({"due":(now - TimeDelta::minutes(1)).to_rfc3339()}),
        0.0,
    )
    .await;
    let request = query(
        Window {
            from_ms: now.timestamp_millis(),
            until_ms: now.timestamp_millis() + 3_600_000,
            timezone: "UTC".into(),
        },
        Vec::new(),
    );
    let rows = execute(&engine.store, &request).await.unwrap();
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "schedule-entry")
            .count(),
        4
    );
    let point = rows.iter().find(|row| row["record_uid"] == point).unwrap();
    assert!(point["time"]["until_ms"].is_null());
    assert_eq!(point["quantity"], "-12");
    let interval = rows
        .iter()
        .find(|row| row["record_uid"] == interval)
        .unwrap();
    assert_eq!(
        interval["time"]["until_ms"].as_i64().unwrap()
            - interval["time"]["from_ms"].as_i64().unwrap(),
        600_000
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row["category"] == "all-day")
            .count(),
        1
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row["category"] == "overdue")
            .count(),
        1
    );
}

#[tokio::test]
async fn scoped_schedule_cannot_read_another_actors_private_work() {
    let engine = Engine::open_memory().await.unwrap();
    let now = Utc::now();
    task(
        &engine,
        "private",
        json!({"due":(now + TimeDelta::minutes(10)).to_rfc3339()}),
        -1.0,
    )
    .await;
    let person = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: RecordKind::Person,
                head: "Visitor".into(),
                body: String::new(),
                quantity: 1.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let request = query(
        Window {
            from_ms: now.timestamp_millis(),
            until_ms: now.timestamp_millis() + 3_600_000,
            timezone: "UTC".into(),
        },
        Vec::new(),
    );
    let rows = execute_for(&engine.store, &request, Some(&person))
        .await
        .unwrap();
    assert!(rows.iter().all(|row| row["kind"] != "schedule-entry"));
    assert!(
        rows.is_empty()
            || rows
                .last()
                .is_some_and(|row| row["status"]["kind"] == "incomplete")
    );
}
