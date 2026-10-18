use super::*;
use nucleus::{
    social::{
        Direction, Search, Snippet,
        transport::{Announcement, Area, Filter},
    },
    transfer::transport::Activity,
};
use std::collections::BTreeSet;

async fn publish(engine: &Engine, record: &str, state: PostState) -> Value {
    let preview = command(
        engine,
        Command::Preview {
            record: record.into(),
            state,
        },
    )
    .await;
    let document: Snippet = serde_json::from_value(preview["document"].clone()).unwrap();
    let signature = document.signature.clone();
    assert!(!signature.is_empty());
    let saved = command(
        engine,
        Command::Publish {
            record: record.into(),
            preview_hash: preview["preview_hash"].as_str().unwrap().into(),
            document,
        },
    )
    .await;
    assert_eq!(saved["document"]["signature"], signature);
    saved["document"].clone()
}

async fn all(engine: &Engine, mut query: Search, now: i64) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    let mut results = Vec::new();
    loop {
        let rows = store::social::search(&engine.store.pool, &query, now)
            .await
            .unwrap();
        assert!(rows.len() <= 50);
        for row in &rows {
            assert!(seen.insert(row["document"]["id"].as_str().unwrap().to_owned()));
        }
        let after = rows
            .last()
            .map(|row| row["document"]["id"].as_str().unwrap().to_owned());
        let len = rows.len();
        results.extend(rows);
        if len < 50 {
            break;
        }
        query.after = after;
    }
    results
}

#[tokio::test]
async fn signed_transport_announcements_page_filter_expire_withdraw_and_keep_routes_approximate() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-17T12:00:00Z")
        .unwrap()
        .timestamp();
    let clock = nucleus::execution::Execution::new([82; 32], now * 1000).unwrap();
    clock
        .scope(async {
            let engine = Engine::open_memory().await.unwrap();
            let mut posts = Vec::new();
            for index in 0..52 {
                let route = Announcement {
                    activity: if index % 2 == 0 {
                        Activity::Ride
                    } else {
                        Activity::Delivery
                    },
                    origin: Area::approximate(0.0011, 179.9961, 10000).unwrap(),
                    destination: Area::approximate(0.0021, -179.9961, 10000).unwrap(),
                    window_start: now + 60,
                    window_end: now + 3600,
                    immediate: index % 3 == 0,
                };
                let draft = PostDraft {
                    title: format!("Transport {index:02}"),
                    direction: if index % 4 < 2 {
                        Direction::Need
                    } else {
                        Direction::Contribution
                    },
                    transport: Some(route.clone()),
                    ..Default::default()
                };
                let saved = command(
                    &engine,
                    Command::SaveDraft {
                        record: None,
                        source: None,
                        draft,
                    },
                )
                .await;
                let record = saved["record"].as_str().unwrap().to_owned();
                let doc = publish(&engine, &record, PostState::Active).await;
                assert_eq!(
                    doc["expires_at"].as_i64(),
                    Some(if route.immediate {
                        now + 900
                    } else {
                        now + 4500
                    })
                );
                assert!(!doc.to_string().contains("179.9961"));
                assert!(!doc.to_string().contains(&record));
                posts.push((record, doc));
            }
            let query = Search {
                transport: Some(Filter::default()),
                ..Default::default()
            };
            assert_eq!(all(&engine, query.clone(), now).await.len(), 52);
            for activity in [Activity::Ride, Activity::Delivery] {
                for direction in [Direction::Need, Direction::Contribution] {
                    let filter = Filter {
                        activity: Some(activity),
                        origin: Some(Area::approximate(0.0, -179.999, 10000).unwrap()),
                        destination: Some(Area::approximate(0.0, 179.999, 10000).unwrap()),
                        window_start: Some(now),
                        window_end: Some(now + 3600),
                    };
                    let rows = all(
                        &engine,
                        Search {
                            transport: Some(filter.clone()),
                            direction: Some(direction),
                            ..Default::default()
                        },
                        now,
                    )
                    .await;
                    let expected = posts
                        .iter()
                        .filter(|(_, doc)| {
                            let doc: Snippet = serde_json::from_value(doc.clone()).unwrap();
                            doc.direction == direction && filter.matches(doc.transport.as_ref())
                        })
                        .count();
                    assert_eq!(rows.len(), expected);
                }
            }
            let scheduled = Announcement {
                activity: Activity::Ride,
                origin: Area::approximate(1.0, 2.0, 1000).unwrap(),
                destination: Area::approximate(2.0, 3.0, 1000).unwrap(),
                window_start: now + 8 * 86400,
                window_end: now + 9 * 86400,
                immediate: false,
            };
            assert_eq!(
                scheduled.expires_at(now),
                now + nucleus::social::MAX_LIFETIME
            );
            let draft = command(
                &engine,
                Command::SaveDraft {
                    record: None,
                    source: None,
                    draft: PostDraft {
                        title: "Beyond seven days".into(),
                        transport: Some(scheduled),
                        ..Default::default()
                    },
                },
            )
            .await;
            let last = publish(
                &engine,
                draft["record"].as_str().unwrap(),
                PostState::Active,
            )
            .await;
            assert_eq!(
                last["expires_at"].as_i64(),
                Some(now + nucleus::social::MAX_LIFETIME)
            );
            let withdrawn = publish(&engine, &posts[1].0, PostState::Withdrawn).await;
            let rows = all(&engine, query.clone(), now).await;
            assert_eq!(rows.len(), 52);
            assert!(
                rows.iter()
                    .all(|row| row["document"]["id"] != withdrawn["id"])
            );
            clock.set_time((now + 901) * 1000).unwrap();
            let after_expiry = all(&engine, query, now + 901).await;
            assert_eq!(
                after_expiry.len(),
                52 - posts
                    .iter()
                    .filter(|(_, doc)| doc["transport"]["immediate"] == true)
                    .count()
            );
            assert!(
                after_expiry
                    .iter()
                    .all(|row| row["document"]["transport"]["immediate"] != true)
            );
            assert_eq!(
                store::sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM transfer_agreement")
                    .fetch_one(&engine.store.pool)
                    .await
                    .unwrap(),
                0
            );
            assert!(engine.location_sources().await.is_empty());
        })
        .await;
}
