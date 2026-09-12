use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use store::Store;
use web::{HttpServeMode, serve_cell_api_only};

const ADMIN_PASSWORD: &str = "correct-horse-battery-staple";
const MARIA_PASSWORD: &str = "she-picked-this-herself";

async fn boot() -> SocketAddr {
    let data_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("person-standing-cell");
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

    let wrong = login("maria", "not-her-password").await;
    assert_eq!(wrong.status(), reqwest::StatusCode::UNAUTHORIZED);
    let wrong_body = wrong.text().await.unwrap_or_default();

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
