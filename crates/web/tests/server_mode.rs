//! `lince --server` must hand nobody a board.
//!
//! The point of server mode is a box that holds data and answers authenticated
//! clients, but that a stranger cannot open in a browser and drop sands onto.
//! Two ways to get that wrong, both of which look fine from a casual `curl /`:
//!
//!   1. Remove `/` and stop there. The board is a handful of JS files plus the
//!      static tree; `/sand/{*path}` and `/static/{*path}` still serve every
//!      asset, and `/static` was registered in BOTH arms of an
//!      `if static_dir.exists()`, so skipping one arm leaves the other live.
//!   2. Remove the UI but leave auth off. `authenticate_headers` is a no-op
//!      when `local_auth_required` is false, so `/host/transport/ws` would
//!      still be an unauthenticated way to act on this store — strictly worse
//!      than doing nothing, because it looks hardened.
//!
//! So this asserts both halves: the UI surface is gone, and the surface that
//! remains demands a token. The distinction that matters throughout is 404
//! (route does not exist) versus 401 (route exists, refused) — a test that
//! only checked "not 200" would pass on a server whose every route 401s.

use std::{net::SocketAddr, path::PathBuf, time::Duration};

use web::{HttpServeMode, serve_cell_api_only};

const ADMIN_PASSWORD: &str = "correct-horse-battery-staple";

/// Boot a real server-mode Cell on an ephemeral port and return its address.
///
/// The data-dir override is a process-global `OnceCell`, so this test binary
/// gets exactly one Cell — hence one `#[tokio::test]` making every assertion
/// rather than one test per route.
async fn boot_server_mode() -> SocketAddr {
    let data_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("server-mode-cell");
    // A stale store from a previous run would already have an admin, which
    // would skip the bootstrap path this test relies on.
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).expect("create the test data dir");
    utils::config::set_lince_data_dir_override(data_dir).expect("set the data dir override");
    // A test Cell must reach NOTHING. Without this the endpoint binds for the
    // internet — publishing node addresses to public DNS and this throwaway
    // Organ's directory record to public pkarr relays — on every run.
    unsafe { std::env::set_var("LINCE_DISCOVERY_INTERNET", "0") };

    // Without a staged password the bootstrap would refuse to start at all
    // (no admin, no TTY, and server mode makes an admin mandatory) — which is
    // itself the intended behaviour, just not what this test is measuring.
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

    // --- the board must be gone, entirely ---------------------------------
    //
    // Not "gated" — absent. Each of these is on its own a complete way to
    // bootstrap a working board against this server's store.
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
        // Flat, as extracted into `<data-dir>/web/sand` at boot — a nested
        // path 404s in BOTH modes and would make this assertion vacuous.
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

    // The index specifically must not be board HTML by any other status
    // either — a 200 here is the whole failure this file exists to catch.
    let index = client.get(url("/")).send().await.expect("request sent");
    let body = index.text().await.unwrap_or_default();
    assert!(
        !body.contains("<html") && !body.contains("lince-board"),
        "server mode served something that looks like the board: {body}",
    );

    // --- what remains must exist, and must demand a token -----------------
    //
    // 404 here would mean server mode broke the very thing it exists for: a
    // logged-in client reaching this Cell.
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

    // The real credential works, which proves `--initial-admin-password-file`
    // actually provisions a usable account rather than just a row.
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

    // The live-mode socket: present, and refusing anonymous callers. The
    // upgrade headers are required because `WebSocketUpgrade` is extracted
    // before the handler runs — without them axum rejects the request on
    // shape and we would never reach the auth check we are testing.
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
