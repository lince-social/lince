//! A deactivated Person cannot log in, and cannot keep a session they had.
//!
//! The store tests prove the flag; `engine/tests/person_standing.rs` proves it
//! travels. This is the one that proves it DOES ANYTHING — deactivation whose
//! only evidence is a row is a checkbox, not a decision.
//!
//! Two properties, and the second is the one that is easy to miss:
//!
//! 1. **The refusal says exactly what a wrong password says.** "This account is
//!    deactivated" is username enumeration with a helpful tone — it confirms
//!    the name is real to anyone who guessed it. The person refused already
//!    knows why, from whoever deactivated them.
//! 2. **An OPEN session ends.** Blocking only new logins leaves whoever was
//!    already signed in acting indefinitely, and the session that matters most
//!    is precisely the one running when you decided to end it.
//!
//! Run against a real bound server rather than the handler, because both
//! properties are about what reaches the wire.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use store::Store;
use web::{HttpServeMode, serve_cell_api_only};

const ADMIN_PASSWORD: &str = "correct-horse-battery-staple";
const MARIA_PASSWORD: &str = "she-picked-this-herself";

/// One Cell per test binary: the data-dir override is a process-global
/// `OnceCell`, same constraint `server_mode.rs` works under.
async fn boot() -> SocketAddr {
    let data_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("person-standing-cell");
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).expect("create the test data dir");
    utils::config::set_lince_data_dir_override(data_dir).expect("set the data dir override");
    // A test Cell must reach nothing: no node addresses in public DNS, no
    // directory record on public pkarr relays.
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

/// The same database the running Cell holds. Standing is read from the store on
/// every request, so writing it from here is exactly what an admin panel on
/// another Cell would have done — and it keeps this test about the HTTP
/// behaviour rather than about the action plumbing, which has its own suite.
async fn store() -> Store {
    Store::open(&web::default_lince_db_url())
        .await
        .expect("open the Cell's store")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deactivated_person_is_refused_in_the_same_words_and_loses_their_session() {
    let addr = boot().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("build the http client");
    let url = |path: &str| format!("http://{addr}{path}");
    let login = |username: &'static str, password: &'static str| {
        let client = client.clone();
        let url = url("/api/auth/login");
        async move {
            client
                .post(url)
                .json(&serde_json::json!({ "username": username, "password": password }))
                .send()
                .await
                .expect("request sent")
        }
    };

    let store = store().await;
    let role = store::auth::ensure_role(&store.pool, "staff")
        .await
        .expect("role");
    let maria = store::auth::create_person_login(
        &store.pool,
        "Maria",
        "maria",
        &utils::auth::hash_password(MARIA_PASSWORD).expect("hash"),
        role,
    )
    .await
    .expect("maria has a login");

    // --- she works to begin with, or nothing below means anything ---------
    let ok = login("maria", MARIA_PASSWORD).await;
    assert_eq!(
        ok.status(),
        reqwest::StatusCode::OK,
        "precondition: maria can log in before anyone turns her off"
    );
    let token = ok
        .json::<serde_json::Value>()
        .await
        .expect("login body")
        .get("token")
        .and_then(|token| token.as_str())
        .expect("a token")
        .to_string();

    let session = client
        .get(url("/host/board/state"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("request sent");
    assert_eq!(
        session.status(),
        reqwest::StatusCode::OK,
        "precondition: her token works while she is active"
    );

    // What a wrong password looks like, recorded BEFORE deactivating so the
    // comparison below is against a real response and not a guessed string.
    let wrong = login("maria", "not-her-password").await;
    assert_eq!(wrong.status(), reqwest::StatusCode::UNAUTHORIZED);
    let wrong_body = wrong.text().await.unwrap_or_default();

    // --- she stops using Lince --------------------------------------------
    store::people::deactivate(
        &store.pool,
        &maria,
        "2026-08-15T12:00:00Z",
        Some("moved out"),
    )
    .await
    .expect("deactivate");

    let refused = login("maria", MARIA_PASSWORD).await;
    assert_eq!(
        refused.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "her password must stop working"
    );
    let refused_body = refused.text().await.unwrap_or_default();
    assert_eq!(
        refused_body, wrong_body,
        "the refusal must be word for word what a wrong password says — anything \
         else tells a guesser that `maria` is a real account here"
    );
    assert!(
        !refused_body.to_lowercase().contains("deactiv"),
        "and it must not name the reason: {refused_body}"
    );

    // --- and the session she already had is over --------------------------
    let stale = client
        .get(url("/host/board/state"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("request sent");
    assert_eq!(
        stale.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "a token held from before must not outlive the decision"
    );

    // --- the page must not render her either -------------------------------
    //
    // The SSR bootstrap resolves a viewer best-effort and never errors, so it
    // is the quiet one: a chrome showing her name and role while every request
    // behind it 401s is worse than a logged-out page, because it looks like
    // Lince is broken rather than like she was turned off.
    let page = client
        .get(url("/"))
        .header("Cookie", format!("lince_auth={token}"))
        .send()
        .await
        .expect("request sent");
    assert_eq!(page.status(), reqwest::StatusCode::OK, "the board renders");
    let page = page.text().await.unwrap_or_default();
    assert!(
        !page.contains("\"username\":\"maria\""),
        "the bootstrap must carry no viewer for a deactivated Person"
    );

    // --- the socket is the door that matters -------------------------------
    //
    // `/host/board/state` is not where a signed-in person does things: the
    // board and every sand act over this WebSocket, which resolves its subject
    // ONCE at the upgrade and keeps it for the life of the connection. Without
    // a per-frame re-read, a deactivated person with an open tab keeps working
    // until they reload — which is the opposite of what deactivating means.
    store::people::reactivate(&store.pool, &maria)
        .await
        .expect("reactivate");
    let token = login("maria", MARIA_PASSWORD)
        .await
        .json::<serde_json::Value>()
        .await
        .expect("login body")
        .get("token")
        .and_then(|token| token.as_str())
        .expect("a token")
        .to_string();

    let request = tokio_tungstenite::tungstenite::http::Request::builder()
        .uri(format!("ws://{addr}/host/transport/ws"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Host", addr.to_string())
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        )
        .body(())
        .expect("build the upgrade request");
    let (mut socket, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("the socket opens while she is active");

    store::people::deactivate(&store.pool, &maria, "2026-08-15T13:00:00Z", None)
        .await
        .expect("deactivate mid-session");

    use futures::{SinkExt, StreamExt};
    socket
        .send(tokio_tungstenite::tungstenite::Message::Text(
            serde_json::json!({ "type": "unsubscribe", "id": "anything" })
                .to_string()
                .into(),
        ))
        .await
        .expect("send a frame");

    // The connection ends rather than each frame erroring: there is nothing
    // left for it to do, and a board that reconnects lands on the login screen.
    let closed = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(message) = socket.next().await {
            match message {
                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) | Err(_) => return true,
                Ok(_) => continue,
            }
        }
        true
    })
    .await
    .expect("the socket must not simply hang");
    assert!(closed, "an open board tab must not outlive the decision");

    // --- people come back --------------------------------------------------
    store::people::reactivate(&store.pool, &maria)
        .await
        .expect("reactivate");
    let back = login("maria", MARIA_PASSWORD).await;
    assert_eq!(
        back.status(),
        reqwest::StatusCode::OK,
        "reactivating must restore exactly what was there"
    );
}
