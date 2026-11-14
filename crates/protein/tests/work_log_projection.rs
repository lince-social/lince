use serde_json::{Value, json};
use std::sync::{
    Once,
    atomic::{AtomicU64, Ordering},
};

static OBSERVER: Once = Once::new();
static LOOKUPS: AtomicU64 = AtomicU64::new(0);
static MEASUREMENT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn observe() {
    OBSERVER.call_once(|| {
        assert!(nucleus::performance::install(|reading| {
            if let nucleus::performance::Reading::Elapsed("protein-work-log-ids-await", _) = reading
            {
                LOOKUPS.fetch_add(1, Ordering::Relaxed);
            }
        }));
    });
}

fn logs() -> Value {
    json!([
        {"note":"Missing start"},
        {"start":"2026-11-14T10:00:00Z","end":"2026-11-14T10:15:00Z","note":"café 日本語"},
        {"start":"2026-11-14T10:00:00Z","end":"2026-11-14T10:30:00Z"},
        {"start":"2026-11-14T11:00:00Z","end":"2026-11-14T11:05:00Z"},
        {"start":"2026-11-14T12:00:00Z","end":"2026-11-14T12:01:00Z"}
    ])
}

async fn fixture(store: &store::Store, count: usize, populated: bool) -> Vec<String> {
    let first = store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Plain,
            head: "Private logs 0000",
            body: &"x".repeat(4096),
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap();
    let mut ids = vec![first.uid.clone()];
    let mut transaction = store::write_tx(&store.pool).await.unwrap();
    for index in 1..count {
        let uid = nucleus::new_uid("r");
        store::records::create_with_uid_on(
            &mut transaction,
            &uid,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: &format!("Private logs {index:04}"),
                body: &"x".repeat(4096),
                quantity: store::exact::zero(),
            },
            first.organ_uid.as_ref().unwrap(),
            None,
        )
        .await
        .unwrap();
        ids.push(uid);
    }
    for (index, uid) in ids.iter().enumerate() {
        if populated || index % 3 != 0 {
            let metadata = if populated {
                json!({"logs":logs()})
            } else if index % 3 == 1 {
                json!({"logs":[]})
            } else {
                json!({"estimate_min":17})
            };
            store::sqlx::query(
                "INSERT INTO record_extension (record_uid, namespace, fds) VALUES (?, 'work', ?)",
            )
            .bind(uid)
            .bind(metadata.to_string())
            .execute(&mut *transaction)
            .await
            .unwrap();
        }
        if populated {
            let values = logs();
            for (property, value) in [
                ("work.log:z", values[3].clone()),
                ("work.log:b", values[2].clone()),
                ("work.log:null", values[0].clone()),
                ("work.log:a", values[1].clone()),
                ("work.log:scalar", json!(7)),
                ("unrelated.log:x", values[4].clone()),
            ] {
                store::sqlx::query("INSERT INTO record_property (record_uid, property, clock, peer, change_uid, value) VALUES (?, ?, 0, 'private', 'fixture', ?)")
                    .bind(uid).bind(property).bind(value.to_string()).execute(&mut *transaction).await.unwrap();
            }
        }
    }
    transaction.commit().await.unwrap();
    ids
}

fn query() -> protein::Protein {
    serde_json::from_value(json!({
        "source":"record", "where":[{"kind_eq":"plain"}],
        "fields":["head","work_logs","spent_seconds","running_since"],
        "order":[{"asc":"head"}]
    }))
    .unwrap()
}

fn expected() -> Value {
    let mut result = logs();
    for (log, id) in result.as_array_mut().unwrap().iter_mut().zip([
        "work.log:null",
        "work.log:a",
        "work.log:b",
        "work.log:z",
        "work.log:seed-4",
    ]) {
        log["id"] = json!(id);
    }
    result
}

#[tokio::test]
async fn empty_work_logs_do_not_issue_identifier_reads() {
    let _measurement = MEASUREMENT.lock().await;
    observe();
    let store = store::Store::open_memory().await.unwrap();
    fixture(&store, 12, false).await;
    LOOKUPS.store(0, Ordering::Relaxed);
    let rows = protein::execute(&store, &query()).await.unwrap();
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
    let store = store::Store::open_memory().await.unwrap();
    let ids = fixture(&store, 513, true).await;
    LOOKUPS.store(0, Ordering::Relaxed);
    let rows = protein::execute(&store, &query()).await.unwrap();
    assert_eq!(rows.len(), ids.len());
    for (row, uid) in rows.iter().zip(&ids) {
        assert_eq!(row["uid"], *uid);
        assert_eq!(row["work_logs"], expected());
        assert_eq!(row["spent_seconds"], 3060);
        assert_eq!(row["running_since"], Value::Null);
    }
    assert_eq!(LOOKUPS.load(Ordering::Relaxed), 2);
    store::sqlx::query("UPDATE record_property SET property = 'work.log:aa' WHERE record_uid = ? AND property = 'work.log:a'")
        .bind(&ids[0]).execute(&store.pool).await.unwrap();
    let mut page = query();
    page.limit = Some(1);
    LOOKUPS.store(0, Ordering::Relaxed);
    let rows = protein::execute(&store, &page).await.unwrap();
    assert_eq!(rows.len(), 1);
    let mut changed = expected();
    changed[1]["id"] = json!("work.log:aa");
    assert_eq!(rows[0]["work_logs"], changed);
    assert_eq!(LOOKUPS.load(Ordering::Relaxed), 1);
    page.include.record_after = Some(ids[0].clone());
    let rows = protein::execute(&store, &page).await.unwrap();
    assert_eq!(rows[0]["uid"], ids[1]);
    assert_eq!(rows[0]["work_logs"], expected());
    page.fields = Some(vec!["head".into(), "spent_seconds".into()]);
    LOOKUPS.store(0, Ordering::Relaxed);
    let rows = protein::execute(&store, &page).await.unwrap();
    assert!(rows[0].get("work_logs").is_none());
    assert_eq!(rows[0]["spent_seconds"], 3060);
    assert_eq!(LOOKUPS.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn work_log_batches_follow_current_visibility_and_permission_revocation() {
    let _measurement = MEASUREMENT.lock().await;
    observe();
    let store = store::Store::open_memory().await.unwrap();
    let ids = fixture(&store, 2, true).await;
    let owner = store::auth::ensure_role(&store.pool, "work-log-owner")
        .await
        .unwrap()
        .to_string();
    for uid in &ids {
        store::visibility::grant(&store.pool, "role", Some(&owner), uid)
            .await
            .unwrap();
    }
    let role = store::auth::ensure_role(&store.pool, "work-log-reader")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&store.pool, role, permission)
        .await
        .unwrap();
    let person =
        store::auth::create_person_login(&store.pool, "Reader", "work-log-reader", "unused", role)
            .await
            .unwrap()
            .to_string();
    store::visibility::grant(&store.pool, "actor", Some(&person), &ids[0])
        .await
        .unwrap();
    LOOKUPS.store(0, Ordering::Relaxed);
    let rows = protein::execute_for(&store, &query(), Some(&person))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["uid"], ids[0]);
    assert_eq!(rows[0]["work_logs"], expected());
    assert_eq!(LOOKUPS.load(Ordering::Relaxed), 1);
    store::auth::revoke(&store.pool, role, permission)
        .await
        .unwrap();
    LOOKUPS.store(0, Ordering::Relaxed);
    assert!(
        protein::execute_for(&store, &query(), Some(&person))
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
    fn cpu() -> u64 {
        let mut time = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        assert_eq!(
            unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut time) },
            0
        );
        time.tv_sec as u64 * 1_000_000 + time.tv_nsec as u64 / 1000
    }
    let _measurement = MEASUREMENT.lock().await;
    observe();
    for populated in [false, true] {
        let directory =
            std::path::PathBuf::from(std::env::var_os("LINCE_PARK_TEST_ARTIFACTS").unwrap());
        let database = directory.join(format!("work-logs-{populated}.sqlite"));
        assert!(!database.exists());
        let store = store::Store::open(&format!("sqlite://{}", database.display()))
            .await
            .unwrap();
        fixture(&store, 300, populated).await;
        let mut query = query();
        query.fields = None;
        for _ in 0..2 {
            assert_eq!(protein::execute(&store, &query).await.unwrap().len(), 300);
        }
        LOOKUPS.store(0, Ordering::Relaxed);
        let mut samples = Vec::new();
        for _ in 0..12 {
            let before = cpu();
            let started = std::time::Instant::now();
            let rows = protein::execute(&store, &query).await.unwrap();
            samples
                .push(json!({"elapsed_us":started.elapsed().as_micros(), "cpu_us":cpu()-before}));
            assert_eq!(rows.len(), 300);
            for row in rows {
                assert_eq!(row["body"].as_str().unwrap().len(), 4096);
                assert_eq!(
                    row["work_logs"],
                    if populated { expected() } else { json!([]) }
                );
            }
        }
        println!(
            "{}",
            json!({"benchmark":"work-log-projection", "optimized":!cfg!(debug_assertions), "records":300, "populated":populated, "lookups":LOOKUPS.load(Ordering::Relaxed), "samples":samples})
        );
    }
}
