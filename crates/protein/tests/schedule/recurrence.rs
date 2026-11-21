use super::fixtures::task;
use chrono::{DateTime, Utc};
use engine::{Engine, actions::Action};
use nucleus::RecordKind;
use nucleus::karma::Consequence;
use nucleus::projection::Window;
use protein::{execute, execute_for, schedule::query};
use serde_json::json;

#[tokio::test]
async fn admitted_recurring_work_is_confirmed_without_a_projection_cache() {
    let engine = Engine::open_memory().await.unwrap();
    let now = Utc::now().timestamp_millis();
    let record = task(&engine, "admitted", json!({"estimate_min":10}), 0.0).await;
    let rule = engine
        .act(
            Action::CreateRecurrence {
                target: record.clone(),
                consequences: vec![Consequence::AddQuantity {
                    delta: Some(nucleus::fact::zero_delta()),
                }],
                condition: None,
                gate: None,
                carry: None,
                note: None,
                cadence: nucleus::karma::Cadence::every_days(1),
                anchor_at: None,
                request_id: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let intended = DateTime::from_timestamp_millis(now - 120_000)
        .unwrap()
        .to_rfc3339();
    for attempt in 0..2 {
        store::sqlx::query("INSERT INTO karma_rule_application (event_id, rule_uid, rule_revision, status, at, intended_at, attempt) VALUES (?, ?, 1, 'applied', ?, ?, ?)")
            .bind("admitted-schedule-test").bind(&rule).bind(&intended).bind(&intended).bind(attempt)
            .execute(&engine.store.pool).await.unwrap();
    }
    let window = Window {
        from_ms: now,
        until_ms: now + 3_600_000,
        timezone: "UTC".into(),
    };
    let request = query(window.clone(), Vec::new());
    let rows = execute(&engine.store, &request).await.unwrap();
    let confirmed: Vec<_> = rows
        .iter()
        .filter(|row| row["record_uid"] == record)
        .collect();
    assert_eq!(confirmed.len(), 1);
    assert_eq!(confirmed[0]["origin"]["kind"], "manual");
    assert_eq!(
        confirmed[0]["origin"]["occurrence"]["occurrence"]["event_id"],
        "admitted-schedule-test"
    );
    assert_eq!(confirmed[0]["preview"], false);
    assert_eq!(confirmed[0]["time"]["from_ms"], now - 120_000);
    assert_eq!(confirmed[0]["time"]["until_ms"], now + 480_000);
    let boundary = query(
        Window {
            from_ms: now + 480_000,
            ..window
        },
        Vec::new(),
    );
    assert!(
        execute(&engine.store, &boundary)
            .await
            .unwrap()
            .iter()
            .all(|row| row["record_uid"] != record)
    );
    let past_at = DateTime::from_timestamp_millis(now - 1_200_000)
        .unwrap()
        .to_rfc3339();
    store::sqlx::query("INSERT INTO karma_rule_application (event_id, rule_uid, rule_revision, status, at, intended_at, attempt) VALUES (?, ?, 1, 'applied', ?, ?, 0)")
        .bind("past-admitted-schedule-test").bind(&rule).bind(&past_at).bind(&past_at)
        .execute(&engine.store.pool).await.unwrap();
    let finished = task(&engine, "finished-manual", json!({"due":past_at}), 0.0).await;
    let history = query(
        Window {
            from_ms: now - 3_600_000,
            until_ms: now + 3_600_000,
            timezone: "UTC".into(),
        },
        Vec::new(),
    );
    let history = execute(&engine.store, &history).await.unwrap();
    let past = history
        .iter()
        .find(|row| {
            row["origin"]["occurrence"]["occurrence"]["event_id"] == "past-admitted-schedule-test"
        })
        .unwrap();
    assert_eq!(past["preview"], false);
    assert_eq!(past["time"]["until_ms"], now - 600_000);
    let manual = history
        .iter()
        .find(|row| row["record_uid"] == finished)
        .unwrap();
    assert_eq!(manual["category"], "timed");
    assert_eq!(manual["preview"], false);
    assert_eq!(manual["time"]["from_ms"], now - 1_200_000);
    let visitor = engine
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
    assert!(
        execute_for(&engine.store, &request, Some(&visitor))
            .await
            .unwrap()
            .iter()
            .all(|row| row["kind"] != "schedule-entry")
    );
}
