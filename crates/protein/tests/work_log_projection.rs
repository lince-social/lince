#[cfg(target_os = "linux")]
#[path = "support/process_cpu.rs"]
mod process_cpu;

use nucleus::{
    RecordKind, new_uid,
    performance::{Reading, install},
};
use protein::{Protein, execute, execute_for};
use serde_json::{Value, from_value, json};
use std::sync::{
    Once,
    atomic::{AtomicU64, Ordering},
};
use store::{
    Store, auth,
    exact::zero,
    records::{NewRecord, create, create_with_uid_on},
    sqlx::query as sql,
    visibility::grant,
    write_tx,
};
use tokio::sync::Mutex;

static OBSERVER: Once = Once::new();
static LOOKUPS: AtomicU64 = AtomicU64::new(0);
static MEASUREMENT: Mutex<()> = Mutex::const_new(());

fn observe() {
    OBSERVER.call_once(|| {
        assert!(install(|reading| {
            if let Reading::Elapsed("protein-work-log-ids-await", _) = reading {
                LOOKUPS.fetch_add(1, Ordering::Relaxed);
            }
        }));
    });
}

fn logs() -> [Value; 5] {
    [
        json!({"note":"Missing start"}),
        json!({"start":"2026-11-14T10:00:00Z","end":"2026-11-14T10:15:00Z","note":"café 日本語"}),
        json!({"start":"2026-11-14T10:00:00Z","end":"2026-11-14T10:30:00Z"}),
        json!({"start":"2026-11-14T11:00:00Z","end":"2026-11-14T11:05:00Z"}),
        json!({"start":"2026-11-14T12:00:00Z","end":"2026-11-14T12:01:00Z"}),
    ]
}

async fn fixture(store: &Store, count: usize, populated: bool) -> Vec<String> {
    let pool = &store.pool;
    let body = "x".repeat(4096);
    let first = create(
        pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "Private logs 0000",
            body: &body,
            quantity: zero(),
        },
    )
    .await
    .expect("create first Record");
    let organ = first.organ_uid.as_deref().expect("fixture Organ");
    let mut ids = Vec::with_capacity(count);
    ids.push(first.uid);
    let mut tx = write_tx(pool).await.expect("begin fixture transaction");
    for index in 1..count {
        let uid = new_uid("r");
        create_with_uid_on(
            &mut tx,
            &uid,
            NewRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: &format!("Private logs {index:04}"),
                body: &body,
                quantity: zero(),
            },
            organ,
            None,
        )
        .await
        .expect("create fixture Record");
        ids.push(uid);
    }
    let full = populated.then(|| {
        let values = logs();
        let metadata = json!({ "logs": values }).to_string();
        let [missing, first, later, last, unrelated] = values;
        let properties = [
            ("work.log:z", last),
            ("work.log:b", later),
            ("work.log:null", missing),
            ("work.log:a", first),
            ("work.log:scalar", json!(7)),
            ("unrelated.log:x", unrelated),
        ]
        .map(|(property, value)| (property, value.to_string()));
        (metadata, properties)
    });
    for (index, uid) in ids.iter().enumerate() {
        let metadata = match (&full, index % 3) {
            (Some((meta, _)), _) => meta.as_str(),
            (None, 1) => r#"{"logs":[]}"#,
            (None, 2) => r#"{"estimate_min":17}"#,
            (None, _) => continue,
        };
        sql("INSERT INTO record_extension (record_uid, namespace, fds) VALUES (?, 'work', ?)")
            .bind(uid)
            .bind(metadata)
            .execute(&mut *tx)
            .await
            .expect("write work metadata");
        if let Some((_, properties)) = &full {
            for (property, value) in properties {
                sql("INSERT INTO record_property (record_uid, property, clock, peer, change_uid, value)
                     VALUES (?, ?, 0, 'private', 'fixture', ?)")
                    .bind(uid).bind(property).bind(value).execute(&mut *tx).await.expect("write work-log property");
            }
        }
    }
    tx.commit().await.expect("commit work-log fixtures");
    ids
}

fn query() -> Protein {
    from_value(json!({
        "source":"record", "where":[{"kind_eq":"plain"}],
        "fields":["head","work_logs","spent_seconds","running_since"],
        "order":[{"asc":"head"}]
    }))
    .expect("work-log query")
}

fn expected() -> Value {
    let mut result = logs();
    for (log, id) in result.iter_mut().zip([
        "work.log:null",
        "work.log:a",
        "work.log:b",
        "work.log:z",
        "work.log:seed-4",
    ]) {
        log["id"] = json!(id);
    }
    Value::Array(Vec::from(result))
}

#[tokio::test]
async fn empty_work_logs_do_not_issue_identifier_reads() {
    let _measurement = MEASUREMENT.lock().await;
    observe();
    let store = Store::open_memory().await.unwrap();
    fixture(&store, 12, false).await;
    LOOKUPS.store(0, Ordering::Relaxed);
    let rows = execute(&store, &query()).await.unwrap();
    assert_eq!(rows.len(), 12);
    for row in rows {
        assert_eq!(row["work_logs"], json!([]));
        assert_eq!(row["spent_seconds"], 0);
        assert_eq!(row["running_since"], Value::Null);
    }
    assert_eq!(LOOKUPS.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn populated_logs_keep_order_ids_fallbacks_edits_and_selected_row_bounds() {
    let _measurement = MEASUREMENT.lock().await;
    observe();
    let store = Store::open_memory().await.unwrap();
    let ids = fixture(&store, 513, true).await;
    LOOKUPS.store(0, Ordering::Relaxed);
    let rows = execute(&store, &query()).await.unwrap();
    assert_eq!(rows.len(), ids.len());
    let expected = expected();
    for (row, uid) in rows.iter().zip(&ids) {
        assert_eq!(row["uid"], *uid);
        assert_eq!(row["work_logs"], expected);
        assert_eq!(row["spent_seconds"], 3060);
        assert_eq!(row["running_since"], Value::Null);
    }
    assert_eq!(LOOKUPS.load(Ordering::Relaxed), 2);
    sql("UPDATE record_property SET property = 'work.log:aa' WHERE record_uid = ? AND property = 'work.log:a'")
        .bind(&ids[0]).execute(&store.pool).await.unwrap();
    let mut page = query();
    page.limit = Some(1);
    LOOKUPS.store(0, Ordering::Relaxed);
    let rows = execute(&store, &page).await.unwrap();
    assert_eq!(rows.len(), 1);
    let mut changed = expected.clone();
    changed[1]["id"] = json!("work.log:aa");
    assert_eq!(rows[0]["work_logs"], changed);
    assert_eq!(LOOKUPS.load(Ordering::Relaxed), 1);
    page.include.record_after = Some(ids[0].clone());
    let rows = execute(&store, &page).await.unwrap();
    assert_eq!(rows[0]["uid"], ids[1]);
    assert_eq!(rows[0]["work_logs"], expected);
    page.fields = Some(vec!["head".into(), "spent_seconds".into()]);
    LOOKUPS.store(0, Ordering::Relaxed);
    let rows = execute(&store, &page).await.unwrap();
    assert!(rows[0].get("work_logs").is_none());
    assert_eq!(rows[0]["spent_seconds"], 3060);
    assert_eq!(LOOKUPS.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn work_log_batches_follow_current_visibility_and_permission_revocation() {
    let _measurement = MEASUREMENT.lock().await;
    observe();
    let store = Store::open_memory().await.unwrap();
    let ids = fixture(&store, 2, true).await;
    let owner = auth::ensure_role(&store.pool, "work-log-owner")
        .await
        .unwrap()
        .to_string();
    for uid in &ids {
        grant(&store.pool, "role", Some(&owner), uid).await.unwrap();
    }
    let role = auth::ensure_role(&store.pool, "work-log-reader")
        .await
        .unwrap();
    let permission = auth::ensure_permission(&store.pool, "record", "read")
        .await
        .unwrap();
    auth::grant(&store.pool, role, permission).await.unwrap();
    let person =
        auth::create_person_login(&store.pool, "Reader", "work-log-reader", "unused", role)
            .await
            .unwrap()
            .to_string();
    grant(&store.pool, "actor", Some(&person), &ids[0])
        .await
        .unwrap();
    LOOKUPS.store(0, Ordering::Relaxed);
    let rows = execute_for(&store, &query(), Some(&person)).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["uid"], ids[0]);
    assert_eq!(rows[0]["work_logs"], expected());
    assert_eq!(LOOKUPS.load(Ordering::Relaxed), 1);
    auth::revoke(&store.pool, role, permission).await.unwrap();
    LOOKUPS.store(0, Ordering::Relaxed);
    assert!(
        execute_for(&store, &query(), Some(&person))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(LOOKUPS.load(Ordering::Relaxed), 0);
}

#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "private file-backed work-log projection benchmark; no concurrent compilation"]
async fn measure_work_log_projection() {
    use process_cpu::process_cpu_us;
    let _measurement = MEASUREMENT.lock().await;
    observe();
    for populated in [false, true] {
        let directory =
            std::path::PathBuf::from(std::env::var_os("LINCE_PARK_TEST_ARTIFACTS").unwrap());
        let database = directory.join(format!("work-logs-{populated}.sqlite"));
        assert!(!database.exists());
        let store = Store::open(&format!("sqlite://{}", database.display()))
            .await
            .unwrap();
        fixture(&store, 300, populated).await;
        let mut query = query();
        query.fields = None;
        for _ in 0..2 {
            assert_eq!(execute(&store, &query).await.unwrap().len(), 300);
        }
        LOOKUPS.store(0, Ordering::Relaxed);
        let expected = if populated { expected() } else { json!([]) };
        let mut samples = Vec::with_capacity(12);
        for _ in 0..12 {
            let before = process_cpu_us().expect("process CPU clock");
            let started = std::time::Instant::now();
            let rows = execute(&store, &query).await.unwrap();
            samples
                .push(json!({"elapsed_us":started.elapsed().as_micros(), "cpu_us":process_cpu_us().expect("process CPU clock")-before}));
            assert_eq!(rows.len(), 300);
            for row in rows {
                assert_eq!(row["body"].as_str().unwrap().len(), 4096);
                assert_eq!(row["work_logs"], expected);
            }
        }
        println!(
            "{}",
            json!({"benchmark":"work-log-projection", "optimized":!cfg!(debug_assertions), "records":300, "populated":populated, "lookups":LOOKUPS.load(Ordering::Relaxed), "samples":samples})
        );
    }
}
