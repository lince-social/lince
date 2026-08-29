//! The recent-changes diff, over the wire.
//!
//! `store` proves the log fills and ages out and `engine` proves a losing edit
//! is recorded with its winner. This is the one that proves a PERSON can reach
//! it — a log nobody can see is exactly the silent merge the whole thing was
//! built to end, just moved one layer down.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use nucleus::RecordKind;
use store::Store;
use store::records::NewRecord;
use web::{HttpServeMode, serve_cell_api_only};

const ADMIN_PASSWORD: &str = "correct-horse-battery-staple";

async fn boot() -> SocketAddr {
    let data_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("record-changes-cell");
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).expect("create the test data dir");
    utils::config::set_lince_data_dir_override(data_dir).expect("set the data dir override");
    unsafe { std::env::set_var("LINCE_DISCOVERY_INTERNET", "0") };

    let staged = utils::desktop_setup::DesktopInstallSetup {
        initial_admin_password: Some(ADMIN_PASSWORD.to_string()),
        ..Default::default()
    };

    let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let result = serve_cell_api_only(
            Some("127.0.0.1:0".to_string()),
            "test-jwt-secret-that-is-long-enough-to-be-accepted-by-the-bootstrap".to_string(),
            true,
            Some(staged),
            Some(addr_tx),
            HttpServeMode::FullUi,
        )
        .await;
        if let Err(error) = result {
            eprintln!("test Cell stopped: {error}");
        }
    });

    tokio::time::timeout(Duration::from_secs(120), addr_rx)
        .await
        .expect("the Cell should bind within 120s")
        .expect("the Cell should report its bound address")
}

async fn store() -> Store {
    Store::open(&web::default_lince_db_url())
        .await
        .expect("open the Cell's store")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_person_can_read_what_recently_happened_to_a_record() {
    let addr = boot().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("build the http client");

    let login: serde_json::Value = client
        .post(format!("http://{addr}/api/auth/login"))
        .json(&serde_json::json!({ "username": "user", "password": ADMIN_PASSWORD }))
        .send()
        .await
        .expect("login sent")
        .json()
        .await
        .expect("login json");
    let token = login
        .get("token")
        .and_then(|t| t.as_str())
        .expect("a session token")
        .to_string();

    let store = store().await;
    let uid = store::records::create(
        &store.pool,
        NewRecord {
            slug: Some("watched"),
            kind: RecordKind::Plain,
            head: "watched",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid;
    store::records::set_slug(&store.pool, &uid, Some("watched-again"))
        .await
        .expect("edit");

    let body: serde_json::Value = client
        .get(format!("http://{addr}/host/records/{uid}/changes"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("request sent")
        .json()
        .await
        .expect("changes json");

    let changes = body
        .get("changes")
        .and_then(|c| c.as_array())
        .expect("a changes array");
    assert!(!changes.is_empty(), "the edits are visible to a person");

    let slug_change = changes
        .iter()
        .find(|c| c.get("field").and_then(|f| f.as_str()) == Some("slug"))
        .expect("the slug edit is reported");
    assert_eq!(
        slug_change.get("cause").and_then(|c| c.as_str()),
        Some("local"),
        "nothing raced, so it reads as an ordinary local edit"
    );
    assert_eq!(
        slug_change.get("mine").and_then(|m| m.as_bool()),
        Some(false),
        "an unraced local edit displaced nobody, so it is not flagged as a loss"
    );
    assert!(
        body.get("retention_days")
            .and_then(|d| d.as_i64())
            .is_some(),
        "the surface is told how long this window is, so it never reads as a \
         full history"
    );

    // The endpoint is behind the same door as everything else on /host.
    let refused = client
        .get(format!("http://{addr}/host/records/{uid}/changes"))
        .send()
        .await
        .expect("request sent");
    assert!(
        !refused.status().is_success(),
        "what changed on a Record is not public"
    );
}
