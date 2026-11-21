use super::fixtures::task;
use chrono::{DateTime, Utc};
use engine::{Engine, actions::Action};
use nucleus::{
    DecimalValue,
    karma::{ReferenceKind, TypedUid},
    new_uid,
    projection::{Context, OccurrenceLink, Scheduled, Window},
    schedule::TimeRange,
    simulation::{Cause, Quantity, RuleOccurrence},
};
use protein::{Predicate, execute, schedule::query};
use serde_json::{from_value, json};
use store::{
    Store,
    projection::{publish_schedule, revision},
};

async fn publish(store: &Store, context: &Context, entries: &[Scheduled]) -> i64 {
    let source = revision(&store.pool).await.expect("projection revision");
    assert!(
        publish_schedule(
            store,
            context,
            source,
            context.window.from_ms,
            context.window.until_ms,
            None,
            &[],
            entries
        )
        .await
        .expect("publish fixture schedule"),
        "the fixture schedule uses the current source revision"
    );
    source
}

#[tokio::test]
async fn refreshing_forecast_preserves_future_work_and_applies_live_changes() {
    let engine = Engine::open_memory().await.unwrap();
    let now = Utc::now().timestamp_millis();
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
            value: DecimalValue::parse_inferred("-1").unwrap(),
            unit: None,
        },
        cause: Cause::Rule {
            occurrence: RuleOccurrence {
                rule_uid: new_uid("rec"),
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
    publish(&engine.store, &context, &[entry]).await;
    let request = query(context.window.clone(), Vec::new());
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
    let rows = execute(&engine.store, &request).await.unwrap();
    assert_eq!(rows.last().unwrap()["status"]["kind"], "updating");
    assert!(rows.iter().any(|row| row["uid"] == "next-floss"));
    assert!(rows.iter().all(|row| row["origin"]["kind"] != "need"));
    let filtered = query(
        context.window.clone(),
        vec![Predicate::SlugEq("other".into())],
    );
    assert!(
        execute(&engine.store, &filtered)
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
        execute(&engine.store, &request)
            .await
            .unwrap()
            .iter()
            .all(|row| row["record_uid"] != uid)
    );
    publish(&engine.store, &context, &[]).await;
    let rows = execute(&engine.store, &request).await.unwrap();
    assert_eq!(rows.last().unwrap()["status"]["kind"], "ready");
    assert!(rows.iter().all(|row| row["kind"] != "schedule-entry"));
    let unsupported = query(
        context.window,
        vec![Predicate::Any(vec![Predicate::SlugEq("floss".into())])],
    );
    let rows = execute(&engine.store, &unsupported).await.unwrap();
    assert_eq!(
        rows.last().unwrap()["status"]["reason"]["kind"],
        "unsupported-filter"
    );
}

#[tokio::test]
async fn schedule_cache_obeys_half_open_windows_and_source_invalidation() {
    let engine = Engine::open_memory().await.unwrap();
    let now = Utc::now().timestamp_millis();
    let uid = task(&engine, "cached", json!({"estimate_min":10}), -1.0).await;
    let context = Context {
        actor: None,
        window: Window {
            from_ms: now,
            until_ms: now + 3_600_000,
            timezone: "UTC".into(),
        },
    };
    let rule_uid = new_uid("rec");
    let occurrence = |at: i64| Scheduled {
        id: format!("event:{at}"),
        record: TypedUid::new(ReferenceKind::Record, &uid).unwrap(),
        time: TimeRange {
            from_ms: at,
            until_ms: None,
        },
        quantity: Quantity {
            value: DecimalValue::parse_inferred("-1").unwrap(),
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
    let source = publish(
        &engine.store,
        &context,
        &[occurrence(now), occurrence(now + 3_600_000)],
    )
    .await;
    let request = query(context.window.clone(), Vec::new());
    let rows = execute(&engine.store, &request).await.unwrap();
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
    } = &admitted.cause
    else {
        unreachable!()
    };
    let actual = task(&engine, "materialized", json!({
        "start": DateTime::from_timestamp_millis(now).unwrap().to_rfc3339(),
        "projection_occurrence": OccurrenceLink { record: admitted.record.clone(), occurrence: event.clone(), consequence: 0 }
    }), -1.0).await;
    publish(&engine.store, &context, &[admitted]).await;
    let rows = execute(&engine.store, &request).await.unwrap();
    let scheduled: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "schedule-entry" && row["origin"]["kind"] != "need")
        .collect();
    assert_eq!(scheduled.len(), 1);
    assert_eq!(scheduled[0]["record_uid"], actual);
    assert_eq!(scheduled[0]["origin"]["kind"], "manual");
    let link: OccurrenceLink = from_value(scheduled[0]["origin"]["occurrence"].clone()).unwrap();
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
        store::projection::read(&engine.store, &context, now)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !store::projection::publish_schedule(
            &engine.store,
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
