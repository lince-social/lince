use engine::{Engine, actions::Action};
use nucleus::{
    karma::{ReferenceKind, TypedUid},
    projection::{Context, Scheduled, Window},
    schedule::TimeRange,
    simulation::{Cause, Quantity, RuleOccurrence},
};
use serde_json::json;

async fn task(engine: &Engine, slug: &str, work: serde_json::Value, quantity: f64) -> String {
    let uid = engine
        .act(
            Action::CreateRecord {
                slug: Some(slug.into()),
                kind: nucleus::RecordKind::Plain,
                head: slug.into(),
                body: String::new(),
                quantity,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine
        .act(
            Action::SetExtension {
                target: uid.clone(),
                namespace: "work".into(),
                fds: work,
            },
            None,
        )
        .await
        .unwrap();
    uid
}

#[tokio::test]
async fn current_undated_needs_use_live_quantity_and_obey_source_and_visibility() {
    let engine = Engine::open_memory().await.unwrap();
    let record = engine
        .act(
            Action::CreateRecord {
                slug: Some("floss".into()),
                kind: nucleus::RecordKind::Plain,
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
    let now = chrono::Utc::now().timestamp_millis();
    let window = Window {
        from_ms: now,
        until_ms: now + 3_600_000,
        timezone: "UTC".into(),
    };
    let query = protein::schedule::query(
        window.clone(),
        vec![protein::Predicate::UidEq(record.clone())],
    );
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    let need = rows.iter().find(|row| row["record_uid"] == record).unwrap();
    assert_eq!(need["origin"]["kind"], "need");
    assert_eq!(need["category"], "overdue");
    assert_eq!(need["quantity"], "-1");
    assert!(need["time"].is_null());
    assert_eq!(need["preview"], false);
    let filtered = protein::schedule::query(
        window.clone(),
        vec![protein::Predicate::SlugEq("other".into())],
    );
    assert!(
        protein::execute(&engine.store, &filtered)
            .await
            .unwrap()
            .iter()
            .all(|row| row["record_uid"] != record)
    );
    let visitor = task(&engine, "visitor", json!({}), 0.0).await;
    assert!(
        protein::execute_for(&engine.store, &query, Some(&visitor))
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
        protein::execute(&engine.store, &query)
            .await
            .unwrap()
            .iter()
            .all(|row| row["record_uid"] != record)
    );
    let dated = task(&engine, "missed-range", json!({
        "start": chrono::DateTime::from_timestamp_millis(now - 86_400_000).unwrap().to_rfc3339(),
        "estimate_min": 10,
    }), -1.0).await;
    let query = protein::schedule::query(window, vec![protein::Predicate::UidEq(dated.clone())]);
    let rows = protein::execute(&engine.store, &query).await.unwrap();
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
        protein::execute(&engine.store, &query)
            .await
            .unwrap()
            .iter()
            .all(|row| row["record_uid"] != dated)
    );
}

#[tokio::test]
async fn refreshing_forecast_preserves_future_work_and_applies_live_changes() {
    let engine = Engine::open_memory().await.unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let uid = task(&engine, "floss", json!({}), -1.0).await;
    let context = Context {
        actor: None,
        window: Window {
            from_ms: now,
            until_ms: now + 3_600_000,
            timezone: "UTC".into(),
        },
    };
    let entry = Scheduled {
        id: "next-floss".into(),
        record: TypedUid::new(ReferenceKind::Record, &uid).unwrap(),
        time: TimeRange {
            from_ms: now + 1_800_000,
            until_ms: None,
        },
        quantity: Quantity {
            value: nucleus::DecimalValue::parse_inferred("-1").unwrap(),
            unit: None,
        },
        cause: Cause::Rule {
            occurrence: RuleOccurrence {
                rule_uid: nucleus::new_uid("rec"),
                revision: 1,
                event_id: "next".into(),
                frequency: None,
                intended_at_ms: Some(now + 1_800_000),
            },
            consequence: 0,
        },
        head: "floss".into(),
        slug: Some("floss".into()),
        record_kind: "plain".into(),
        preview: false,
    };
    let source = store::projection::revision(&engine.store.pool)
        .await
        .unwrap();
    assert!(
        store::projection::publish_schedule(
            &engine.store.pool,
            &context,
            source,
            now,
            now + 3_600_000,
            None,
            &[],
            &[entry]
        )
        .await
        .unwrap()
    );
    let query = protein::schedule::query(context.window.clone(), Vec::new());
    engine
        .act(
            Action::AddQuantity {
                target: uid.clone(),
                delta: 1.0,
            },
            None,
        )
        .await
        .unwrap();
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    assert_eq!(rows.last().unwrap()["status"]["kind"], "updating");
    assert!(rows.iter().any(|row| row["uid"] == "next-floss"));
    assert!(rows.iter().all(|row| row["origin"]["kind"] != "need"));
    let filtered = protein::schedule::query(
        context.window.clone(),
        vec![protein::Predicate::SlugEq("other".into())],
    );
    assert!(
        protein::execute(&engine.store, &filtered)
            .await
            .unwrap()
            .iter()
            .all(|row| row["record_uid"] != uid)
    );
    engine
        .act(
            Action::DeleteRecord {
                target: uid.clone(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        protein::execute(&engine.store, &query)
            .await
            .unwrap()
            .iter()
            .all(|row| row["record_uid"] != uid)
    );
    let current = store::projection::revision(&engine.store.pool)
        .await
        .unwrap();
    assert!(
        store::projection::publish_schedule(
            &engine.store.pool,
            &context,
            current,
            now,
            now + 3_600_000,
            None,
            &[],
            &[]
        )
        .await
        .unwrap()
    );
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    assert_eq!(rows.last().unwrap()["status"]["kind"], "ready");
    assert!(rows.iter().all(|row| row["kind"] != "schedule-entry"));
    let unsupported = protein::schedule::query(
        context.window,
        vec![protein::Predicate::Any(vec![protein::Predicate::SlugEq(
            "floss".into(),
        )])],
    );
    let rows = protein::execute(&engine.store, &unsupported).await.unwrap();
    assert_eq!(
        rows.last().unwrap()["status"]["reason"]["kind"],
        "unsupported-filter"
    );
}

#[tokio::test]
async fn schedule_keeps_points_intervals_all_day_and_overdue_separate() {
    let engine = Engine::open_memory().await.unwrap();
    let now = chrono::Utc::now();
    let at = now + chrono::TimeDelta::minutes(20);
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
        json!({"due":(now - chrono::TimeDelta::minutes(1)).to_rfc3339()}),
        -1.0,
    )
    .await;
    task(
        &engine,
        "past-finished",
        json!({"due":(now - chrono::TimeDelta::minutes(1)).to_rfc3339()}),
        0.0,
    )
    .await;
    let query = protein::schedule::query(
        Window {
            from_ms: now.timestamp_millis(),
            until_ms: now.timestamp_millis() + 3_600_000,
            timezone: "UTC".into(),
        },
        Vec::new(),
    );
    let rows = protein::execute(&engine.store, &query).await.unwrap();
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
async fn schedule_cache_obeys_half_open_windows_and_source_invalidation() {
    let engine = Engine::open_memory().await.unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let uid = task(&engine, "cached", json!({"estimate_min":10}), -1.0).await;
    let context = Context {
        actor: None,
        window: Window {
            from_ms: now,
            until_ms: now + 3_600_000,
            timezone: "UTC".into(),
        },
    };
    let rule_uid = nucleus::new_uid("rec");
    let occurrence = |at: i64| Scheduled {
        id: format!("event:{at}"),
        record: TypedUid::new(ReferenceKind::Record, &uid).unwrap(),
        time: TimeRange {
            from_ms: at,
            until_ms: None,
        },
        quantity: Quantity {
            value: nucleus::DecimalValue::parse_inferred("-1").unwrap(),
            unit: None,
        },
        cause: Cause::Rule {
            occurrence: RuleOccurrence {
                rule_uid: rule_uid.clone(),
                revision: 1,
                event_id: format!("event:{at}"),
                frequency: None,
                intended_at_ms: Some(at),
            },
            consequence: 0,
        },
        head: "cached".into(),
        slug: Some("cached".into()),
        record_kind: "plain".into(),
        preview: false,
    };
    let source = store::projection::revision(&engine.store.pool)
        .await
        .unwrap();
    assert!(
        store::projection::publish_schedule(
            &engine.store.pool,
            &context,
            source,
            now,
            now + 3_600_000,
            None,
            &[],
            &[occurrence(now), occurrence(now + 3_600_000)]
        )
        .await
        .unwrap()
    );
    let query = protein::schedule::query(context.window.clone(), Vec::new());
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    assert!(
        rows.iter()
            .any(|row| row["record_uid"] == uid && row["origin"]["kind"] == "need")
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "schedule-entry" && row["origin"]["kind"] != "need")
            .count(),
        1
    );
    assert_eq!(rows.last().unwrap()["status"]["kind"], "ready");
    let admitted = occurrence(now);
    let Cause::Rule {
        occurrence: event, ..
    } = admitted.cause.clone()
    else {
        unreachable!()
    };
    let actual = task(&engine, "materialized", json!({
        "start":chrono::DateTime::from_timestamp_millis(now).unwrap().to_rfc3339(),
        "projection_occurrence":nucleus::projection::OccurrenceLink { record: admitted.record.clone(), occurrence: event, consequence: 0 }
    }), -1.0).await;
    let current = store::projection::revision(&engine.store.pool)
        .await
        .unwrap();
    assert!(
        store::projection::publish_schedule(
            &engine.store.pool,
            &context,
            current,
            now,
            now + 3_600_000,
            None,
            &[],
            &[admitted]
        )
        .await
        .unwrap()
    );
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    let scheduled: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "schedule-entry" && row["origin"]["kind"] != "need")
        .collect();
    assert_eq!(scheduled.len(), 1);
    assert_eq!(scheduled[0]["record_uid"], actual);
    assert_eq!(scheduled[0]["origin"]["kind"], "manual");
    let link: nucleus::projection::OccurrenceLink =
        serde_json::from_value(scheduled[0]["origin"]["occurrence"].clone()).unwrap();
    assert_eq!(link.occurrence.rule_uid, rule_uid);
    assert_eq!(link.occurrence.revision, 1);
    engine
        .act(
            Action::SetExtension {
                target: uid,
                namespace: "work".into(),
                fds: json!({"estimate_min":20}),
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        store::projection::read(&engine.store.pool, &context, now)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !store::projection::publish_schedule(
            &engine.store.pool,
            &context,
            source,
            now,
            now + 3_600_000,
            None,
            &[],
            &[]
        )
        .await
        .unwrap()
    );
}

#[tokio::test]
async fn scoped_schedule_cannot_read_another_actors_private_work() {
    let engine = Engine::open_memory().await.unwrap();
    let now = chrono::Utc::now();
    task(
        &engine,
        "private",
        json!({"due":(now + chrono::TimeDelta::minutes(10)).to_rfc3339()}),
        -1.0,
    )
    .await;
    let person = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Person,
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
    let query = protein::schedule::query(
        Window {
            from_ms: now.timestamp_millis(),
            until_ms: now.timestamp_millis() + 3_600_000,
            timezone: "UTC".into(),
        },
        Vec::new(),
    );
    let rows = protein::execute_for(&engine.store, &query, Some(&person))
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

#[tokio::test]
async fn admitted_recurring_work_is_confirmed_without_a_projection_cache() {
    let engine = Engine::open_memory().await.unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    let record = task(&engine, "admitted", json!({"estimate_min":10}), 0.0).await;
    let rule = engine
        .act(
            Action::CreateRecurrence {
                target: record.clone(),
                consequences: vec![nucleus::karma::Consequence::AddQuantity {
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
    let intended = chrono::DateTime::from_timestamp_millis(now - 120_000)
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
    let query = protein::schedule::query(window.clone(), Vec::new());
    let rows = protein::execute(&engine.store, &query).await.unwrap();
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
    let boundary = protein::schedule::query(
        Window {
            from_ms: now + 480_000,
            ..window
        },
        Vec::new(),
    );
    assert!(
        protein::execute(&engine.store, &boundary)
            .await
            .unwrap()
            .iter()
            .all(|row| row["record_uid"] != record)
    );
    let past_at = chrono::DateTime::from_timestamp_millis(now - 1_200_000)
        .unwrap()
        .to_rfc3339();
    store::sqlx::query("INSERT INTO karma_rule_application (event_id, rule_uid, rule_revision, status, at, intended_at, attempt) VALUES (?, ?, 1, 'applied', ?, ?, 0)")
        .bind("past-admitted-schedule-test").bind(&rule).bind(&past_at).bind(&past_at)
        .execute(&engine.store.pool).await.unwrap();
    let finished = task(&engine, "finished-manual", json!({"due":past_at}), 0.0).await;
    let history = protein::schedule::query(
        Window {
            from_ms: now - 3_600_000,
            until_ms: now + 3_600_000,
            timezone: "UTC".into(),
        },
        Vec::new(),
    );
    let history = protein::execute(&engine.store, &history).await.unwrap();
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
                kind: nucleus::RecordKind::Person,
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
        protein::execute_for(&engine.store, &query, Some(&visitor))
            .await
            .unwrap()
            .iter()
            .all(|row| row["kind"] != "schedule-entry")
    );
}
