mod support;

use engine::{Engine, actions::Action};
use nucleus::vocabulary::SavedLocationCommand;
use serde_json::{Value, json};
use std::{collections::BTreeSet, time::Instant};

async fn fixture(count: usize) -> (Engine, Vec<String>) {
    let engine = support::engine().await;
    let organ = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
    let mut records = Vec::new();
    for index in 0..count {
        let uid = nucleus::new_uid("r");
        store::records::create_with_uid_on(
            &mut tx,
            &uid,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: &format!("Map point {index:05}"),
                body: "",
                quantity: store::exact::zero(),
            },
            &organ,
            None,
        )
        .await
        .unwrap();
        let place = nucleus::new_uid("pl");
        let longitude = 179.0 + ((index * 48271 + 17) % 20000) as f64 / 10000.0;
        store::sqlx::query("INSERT INTO place (uid,lat,lon) VALUES (?,?,?)")
            .bind(&place)
            .bind(((index * 37) % 100) as f64 / 100.0)
            .bind(if longitude > 180.0 {
                longitude - 360.0
            } else {
                longitude
            })
            .execute(&mut *tx)
            .await
            .unwrap();
        store::records::set_place_on(&mut tx, &uid, Some(&place))
            .await
            .unwrap();
        records.push(uid);
    }
    tx.commit().await.unwrap();
    records.sort();
    (engine, records)
}

async fn page(
    engine: &Engine,
    records: &[String],
    after: Option<String>,
    generation: &str,
    bounds: [f64; 4],
    actor: Option<String>,
) -> Result<Value, engine::EngineError> {
    engine
        .act(
            Action::SavedLocation {
                request: SavedLocationCommand::Page {
                    query: None,
                    records: records.into(),
                    after,
                    generation: generation.into(),
                    bounds,
                },
            },
            actor,
        )
        .await
        .map(|result| result.data.unwrap())
}

#[test]
fn geometry_pages_cover_three_hundred_once_and_bind_continuations_to_viewport_and_viewer() {
    support::run_async_test("map-geometry-pages", || async {
        let (engine, records) = fixture(300).await;
        let world = [-180.0, -90.0, 180.0, 90.0];
        let mut seen = BTreeSet::new();
        let mut after = None;
        loop {
            let result = page(&engine, &records, after, "first", world, None)
                .await
                .unwrap();
            let features = result["features"].as_array().unwrap();
            assert!(features.len() <= 128);
            for feature in features {
                assert!(seen.insert(feature["record"].as_str().unwrap().to_owned()));
                assert!(feature["location"].is_object());
            }
            after = result["next_after"].as_str().map(str::to_owned);
            if after.is_none() {
                break;
            }
        }
        assert_eq!(seen, records.iter().cloned().collect());
        let first = page(&engine, &records, None, "first", world, None)
            .await
            .unwrap();
        let cursor = first["next_after"].as_str().unwrap().to_owned();
        assert!(
            page(
                &engine,
                &records,
                Some(cursor.clone()),
                "changed",
                world,
                None
            )
            .await
            .is_err()
        );
        assert!(
            page(
                &engine,
                &records,
                Some(cursor.clone()),
                "first",
                [179.5, -1.0, -179.5, 1.0],
                None
            )
            .await
            .is_err()
        );
        let mut different = records.clone();
        different.pop();
        assert!(
            page(&engine, &different, Some(cursor), "first", world, None)
                .await
                .is_err()
        );
        let dateline = page(
            &engine,
            &records,
            None,
            "dateline",
            [179.5, -1.0, -179.5, 1.0],
            None,
        )
        .await
        .unwrap();
        assert!(
            dateline["features"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|feature| feature["location"]["longitude"].as_f64())
                .all(|longitude| longitude >= 179.5 || longitude <= -179.5)
        );
        let role = store::auth::ensure_role(&engine.store.pool, "Map reader")
            .await
            .unwrap();
        let read = store::auth::ensure_permission(&engine.store.pool, "record", "read")
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, read)
            .await
            .unwrap();
        let person = store::auth::create_person_login(
            &engine.store.pool,
            "Map reader",
            "map.reader",
            "hash",
            role,
        )
        .await
        .unwrap();
        let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
        for uid in &records {
            for data in [
                nucleus::visibility::Data::Record,
                nucleus::visibility::Data::Place,
            ] {
                store::data_visibility::save_on(
                    &mut tx,
                    uid,
                    data,
                    None,
                    &nucleus::visibility::Policy {
                        include: vec![nucleus::visibility::Condition {
                            persons: vec![person.clone()],
                            ..Default::default()
                        }],
                        exclude: vec![],
                    },
                    0,
                )
                .await
                .unwrap();
            }
        }
        tx.commit().await.unwrap();
        let expected = page(
            &engine,
            &records[..128],
            None,
            "privacy",
            world,
            Some(person.clone()),
        )
        .await
        .unwrap();
        let hidden = support::plain(&engine, "hidden.map", 0.0).await;
        let place = store::places::create(&engine.store.pool, 89.0, 0.0, None)
            .await
            .unwrap();
        store::places::set_record_place(&engine.store.pool, &hidden, &place)
            .await
            .unwrap();
        let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
        store::data_visibility::save_on(
            &mut tx,
            &hidden,
            nucleus::visibility::Data::Record,
            None,
            &Default::default(),
            0,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let same = page(
            &engine,
            &records[..128],
            None,
            "privacy",
            world,
            Some(person.clone()),
        )
        .await
        .unwrap();
        assert_eq!(same["features"], expected["features"]);
        let denied = page(
            &engine,
            &[hidden.clone()],
            None,
            "hidden",
            world,
            Some(person.clone()),
        )
        .await
        .unwrap();
        assert!(denied["features"].as_array().unwrap().is_empty());
        assert!(
            page(
                &engine,
                &records,
                Some(first["next_after"].as_str().unwrap().into()),
                "first",
                world,
                Some(person)
            )
            .await
            .is_err()
        );
    });
}

#[test]
fn fixed_ten_thousand_record_source_has_bounded_pages_and_measured_one_and_four_view_latencies() {
    support::run_async_test("map-geometry-10000", || async {
        let (engine, records) = fixture(10_000).await;
        let bounds = [-180.0, -90.0, 180.0, 90.0];
        let mut after = None;
        let mut seen = BTreeSet::new();
        for _ in 0..16 {
            let result = page(&engine, &records, after, "long", bounds, None)
                .await
                .unwrap();
            for feature in result["features"].as_array().unwrap() {
                assert!(seen.insert(feature["record"].as_str().unwrap().to_owned()));
            }
            after = Some(result["next_after"].as_str().unwrap().into());
        }
        assert_eq!(seen.len(), 2048);
        assert!(after.is_some());
        while let Some(cursor) = after {
            let result = page(&engine, &records, Some(cursor), "long", bounds, None)
                .await
                .unwrap();
            assert!(result["features"].as_array().unwrap().len() <= 128);
            for feature in result["features"].as_array().unwrap() {
                assert!(seen.insert(feature["record"].as_str().unwrap().to_owned()));
            }
            after = result["next_after"].as_str().map(str::to_owned);
        }
        assert_eq!(seen.len(), 10_000);
        let mut measurements = Vec::new();
        for views in [1, 4] {
            for _ in 0..10 {
                page(&engine, &records, None, "warm", bounds, None)
                    .await
                    .unwrap();
            }
            let mut samples = Vec::new();
            for _ in 0..100 {
                let queries = (0..views).map(|_| async {
                    let started = Instant::now();
                    let result = page(&engine, &records, None, "measure", bounds, None)
                        .await
                        .unwrap();
                    assert!(result["features"].as_array().unwrap().len() <= 128);
                    started.elapsed().as_micros() as u64
                });
                samples.extend(futures::future::join_all(queries).await);
            }
            samples.sort();
            let p95 = samples[(samples.len() * 95).div_ceil(100) - 1];
            measurements.push(json!({"views":views,"samples":samples.len(),"p95_micros":p95}));
        }
        let artifact = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/transfer-validation/map-query-measurements.json");
        std::fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        let cpu=std::fs::read_to_string("/proc/cpuinfo").ok().and_then(|text|text.lines().find(|line|line.starts_with("model name")).map(str::to_owned));
        let memory=std::fs::read_to_string("/proc/self/status").ok().map(|text|text.lines().filter(|line|line.starts_with("VmHWM:")||line.starts_with("VmRSS:")).map(str::to_owned).collect::<Vec<_>>());
        std::fs::write(&artifact, serde_json::to_vec_pretty(&json!({"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"debug_assertions":cfg!(debug_assertions),"reference_cpu":cpu,"process_memory":memory,"memory_scope":"Process high-water RSS and current RSS, not live map allocation or GPU memory","features":10_000,"measurements":measurements})).unwrap()).unwrap();
        assert!(
            measurements
                .iter()
                .all(|value| value["p95_micros"].as_u64().unwrap() <= 250_000),
            "{}",
            artifact.display()
        );
    });
}
