use std::{net::SocketAddr, path::PathBuf, time::Duration};

use web::{HttpServeMode, serve_cell_api_only};

const ADMIN_PASSWORD: &str = "correct-horse-battery-staple";

async fn boot_server_mode() -> SocketAddr {
    let data_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("server-mode-cell");
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
            HttpServeMode::ApiOnly,
        )
        .await;
        if let Err(error) = result {
            eprintln!("server-mode Cell stopped: {error}");
        }
    });

    tokio::time::timeout(Duration::from_secs(120), addr_rx)
        .await
        .expect("the Cell should bind within 120s")
        .expect("the Cell should report its bound address")
}

#[tokio::test(flavor = "multi_thread")]
async fn server_mode_serves_no_board_but_still_answers_authenticated_clients() {
    let addr = boot_server_mode().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("build the http client");
    let url = |path: &str| format!("http://{addr}{path}");

    for path in [
        "/",
        "/favicon.ico",
        "/board/frame.js",
        "/board/editor.js",
        "/board/lynx-ui.js",
        "/board/lynx-ui.css",
        "/board/collab-editor.js",
        "/board/vendor/loro-index.js",
        "/board/vendor/d3.v7.min.js",
        "/board/vendor/mermaid.min.js",
        "/sand/record.html",
        "/sand/kanban.html",
        "/static/presentation/board/frame.js",
        "/host/static/presentation/board/frame.js",
        "/live/organ-anything/connect",
    ] {
        let response = client.get(url(path)).send().await.expect("request sent");
        assert_eq!(
            response.status(),
            reqwest::StatusCode::NOT_FOUND,
            "{path} must not exist in server mode",
        );
    }

    let index = client.get(url("/")).send().await.expect("request sent");
    let body = index.text().await.unwrap_or_default();
    assert!(
        !body.contains("<html") && !body.contains("lince-board"),
        "server mode served something that looks like the board: {body}",
    );

    let login = client
        .post(url("/api/auth/login"))
        .json(&serde_json::json!({ "username": "user", "password": "wrong" }))
        .send()
        .await
        .expect("request sent");
    assert_ne!(
        login.status(),
        reqwest::StatusCode::NOT_FOUND,
        "login must stay reachable in server mode",
    );

    let login = client
        .post(url("/api/auth/login"))
        .json(&serde_json::json!({ "username": "user", "password": ADMIN_PASSWORD }))
        .send()
        .await
        .expect("request sent");
    assert_eq!(
        login.status(),
        reqwest::StatusCode::OK,
        "the staged admin password should log in",
    );

    let socket = client
        .get(url("/host/transport/ws"))
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
        .send()
        .await
        .expect("request sent");
    assert_eq!(
        socket.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "the transport socket must exist and refuse anonymous callers",
    );
}
