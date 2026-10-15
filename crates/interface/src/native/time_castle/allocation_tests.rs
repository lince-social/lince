use super::{Entry, Settings, model};
use engine::{Engine, actions::Action};
use nucleus::projection::Window;
use serde_json::json;

async fn entries(engine: &Engine, query: &protein::Protein) -> Vec<Entry> {
    protein::execute(&engine.store, query)
        .await
        .unwrap()
        .into_iter()
        .filter(|row| row["kind"] == "schedule-entry")
        .map(|row| serde_json::from_value(row).unwrap())
        .collect()
}

fn visible(settings: &Settings, entries: &[Entry], now: i64) -> Vec<String> {
    model::clock_occurrences(settings, entries, now, now + settings.aperture_ms)
        .into_iter()
        .filter(|occurrence| !occurrence.historical)
        .map(|occurrence| entries[occurrence.index].record_uid.clone())
        .collect()
}

#[cfg_attr(test, tokio::test)]
async fn sorted_need_ranges_enter_the_aperture_after_completion_and_estimate_changes() {
    let engine = Engine::open_memory().await.unwrap();
    let predicate = engine
        .act(
            Action::CreateConcept {
                lingua: "g_local".into(),
                name: "queue-priority".into(),
                parents: Vec::new(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let mut records = Vec::new();
    for priority in [1000, 200, 10] {
        let uid = engine
            .act(
                Action::CreateRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: format!("Priority {priority}"),
                    body: String::new(),
                    quantity: -1.0,
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
                    fds: json!({"estimate_min":10}),
                },
                None,
            )
            .await
            .unwrap();
        engine
            .act(
                Action::AssertRecord {
                    subject: uid.clone(),
                    predicate: predicate.clone(),
                    object: None,
                    quantity: Some(priority.to_string()),
                    unit: None,
                },
                None,
            )
            .await
            .unwrap();
        records.push(uid);
    }
    let now = chrono::Utc::now().timestamp_millis();
    let settings = Settings {
        aperture_ms: 900_000,
        horizon_ms: 900_000,
        ..Default::default()
    };
    let mut query = protein::schedule::query(
        Window {
            from_ms: now,
            until_ms: now + 900_000,
            timezone: "UTC".into(),
        },
        Vec::new(),
    );
    query.order = vec![protein::Order::Desc("#queue-priority".into())];
    let initial = entries(&engine, &query).await;
    assert_eq!(visible(&settings, &initial, now), records[..2]);
    assert_eq!(
        model::clock_summaries(&settings, &initial, now, now + 900_000).len(),
        2
    );
    for offset in [120_000, 3_600_000, 86_400_000] {
        assert_eq!(visible(&settings, &initial, now + offset), records[..2]);
        assert_eq!(
            super::face::Face::rows(&settings, &initial, now + offset),
            2
        );
    }
    engine
        .act(
            Action::SetQuantity {
                target: records[0].clone(),
                value: 0.0,
            },
            None,
        )
        .await
        .unwrap();
    let compacted = entries(&engine, &query).await;
    assert_eq!(visible(&settings, &compacted, now), records[1..]);
    for (estimate, expected) in [
        (json!(20), vec![records[1].clone()]),
        (serde_json::Value::Null, records[1..].to_vec()),
        (json!(5), records[1..].to_vec()),
    ] {
        engine
            .act(
                Action::ChangeRecord {
                    request: engine::record_change::Request {
                        id: nucleus::new_uid("op"),
                        record_uid: records[1].clone(),
                        mutation: engine::record_change::Mutation::Work {
                            field: engine::record_change::WorkField::Estimate,
                            value: estimate,
                        },
                    },
                },
                None,
            )
            .await
            .unwrap();
        let refreshed = entries(&engine, &query).await;
        assert_eq!(visible(&settings, &refreshed, now), expected);
        let summary: Vec<_> = model::clock_summaries(&settings, &refreshed, now, now + 900_000)
            .into_iter()
            .map(|(index, _)| refreshed[index].record_uid.clone())
            .collect();
        assert_eq!(summary, expected);

        assert_eq!(
            model::clock_summaries(&settings, &refreshed, now, now + 900_000).len(),
            expected.len()
        );
    }
    let mut record_query = query;
    record_query.source = protein::Source::Record;
    record_query.filter.clear();
    let ordered: Vec<_> = protein::execute(&engine.store, &record_query)
        .await
        .unwrap()
        .into_iter()
        .filter_map(|row| row["uid"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(&ordered[..3], records);
    engine.store.pool.close().await;
}

crate::native::laboratory_cases! {
    async sorted_need_ranges_enter_the_aperture_after_completion_and_estimate_changes,
}
