use std::{sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::Signer;
use engine::actions::Action;
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

use crate::{Facade, State, auth, records};

async fn fixture() -> State {
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let permissions: Vec<_> = utils::auth::ALL_PERMISSIONS
        .iter()
        .map(|p| (p.subject, p.action))
        .collect();
    store::seed::seed(&engine.store.pool, &permissions)
        .await
        .unwrap();
    engine
        .act(
            Action::CreateUser {
                username: "fixture-admin".into(),
                name: "Administrator".into(),
                password: "fixture-password".into(),
                role: "admin".into(),
            },
            None,
        )
        .await
        .unwrap();
    let (_, stop) = tokio::sync::watch::channel(false);
    State {
        cell: cell::CellRuntime {
            commands: Default::default(),
            store: engine.store.clone(),
            engine,
            lanes: Arc::new(cell::LaneHub::new()),
            wire: Arc::new(tokio::sync::RwLock::new(None)),
            fiote: None,
            information: None,
        },
        auth: Arc::new(auth::Auth::default()),
        connections: Arc::new(tokio::sync::Semaphore::new(8)),
        stop,
        security: crate::security::Security::new("http://localhost", false).unwrap(),
    }
}

async fn allowed(
    state: &State,
    user: Option<&store::auth::AuthUser>,
    _selected: &str,
    action: &Action,
) -> Result<(), auth::Failure> {
    state
        .cell
        .engine
        .authorize_action(action, user.map(|user| user.uid.as_str()))
        .await
        .map_err(|error| (axum::http::StatusCode::FORBIDDEN, error.to_string()))
}

async fn record(state: &State, title: &str) -> String {
    state
        .cell
        .engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: title.into(),
                body: "Description".into(),
                quantity: -1.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

async fn user(state: &State, name: &str, write: bool) -> store::auth::AuthUser {
    let role = store::auth::ensure_role(&state.cell.store.pool, name)
        .await
        .unwrap();
    for action in if write {
        vec!["read", "update", "create"]
    } else {
        vec!["read"]
    } {
        let permission = store::auth::ensure_permission(&state.cell.store.pool, "record", action)
            .await
            .unwrap();
        store::auth::grant(&state.cell.store.pool, role, permission)
            .await
            .unwrap();
    }
    let hash = utils::auth::hash_password("facade-test-password").unwrap();
    store::auth::create_person_login(&state.cell.store.pool, name, name, &hash, role)
        .await
        .unwrap();
    store::auth::user_by_username(&state.cell.store.pool, name)
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn visibility_and_read_filters_gate_details_comments_and_writes() {
    let state = fixture().await;
    let shown = record(&state, "Shared").await;
    let hidden = record(&state, "Hidden").await;
    let guest = user(&state, "reader", false).await;
    store::visibility::grant(&state.cell.store.pool, "actor", Some(&guest.uid), &shown)
        .await
        .unwrap();
    let view = records::snapshot(&state, Some(&guest), &shown, "")
        .await
        .unwrap();
    assert_eq!(view["records"].as_array().unwrap().len(), 1);
    assert_eq!(view["record"]["head"], "Shared");
    assert_eq!(view["canedit"], false);
    assert_eq!(view["cancomment"], false);
    let edit = Action::EditRecordText {
        target: shown.clone(),
        head: Some("Changed".into()),
        body: None,
    };
    assert!(allowed(&state, Some(&guest), &shown, &edit).await.is_err());
    let hidden_view = records::snapshot(&state, Some(&guest), &hidden, "")
        .await
        .unwrap();
    assert!(hidden_view["record"]["uid"].is_null());
    let writer = user(&state, "writer", true).await;
    store::visibility::grant(&state.cell.store.pool, "actor", Some(&writer.uid), &shown)
        .await
        .unwrap();
    assert!(allowed(&state, Some(&writer), &shown, &edit).await.is_ok());
    assert!(
        allowed(
            &state,
            Some(&writer),
            &hidden,
            &Action::SetQuantity {
                target: hidden.clone(),
                value: 1.0
            }
        )
        .await
        .is_err()
    );
    assert!(
        allowed(
            &state,
            Some(&writer),
            &shown,
            &Action::DeleteRecord {
                target: shown.clone()
            }
        )
        .await
        .is_err()
    );
    let thread = state
        .cell
        .engine
        .act(
            Action::CreateThread {
                target: shown.clone(),
                head: "Private comments".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    state
        .cell
        .engine
        .act(
            Action::CreateMessage {
                thread,
                body: "Hidden comment".into(),
                author: None,
                state: nucleus::MessageState::Finished,
                parent: None,
                references: vec![],
            },
            None,
        )
        .await
        .unwrap();
    let view = records::snapshot(&state, Some(&writer), &shown, "")
        .await
        .unwrap();
    assert!(view["messages"].as_array().unwrap().is_empty());
    state
        .cell
        .engine
        .set_read_filter(&writer.uid, Some(&protein::Predicate::UidEq(hidden)))
        .await
        .unwrap();
    let view = records::snapshot(&state, Some(&writer), &shown, "")
        .await
        .unwrap();
    assert!(view["record"]["uid"].is_null());
    assert!(allowed(&state, Some(&writer), &shown, &edit).await.is_err());
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[tokio::test]
async fn configuration_templates_and_management_enforce_permissions() {
    let state = fixture().await;
    crate::settings::ensure(&state).await.unwrap();
    let facade = store::records::resolve(&state.cell.store.pool, crate::settings::SLUG)
        .await
        .unwrap()
        .unwrap();
    crate::settings::ensure(&state).await.unwrap();
    assert_eq!(
        store::records::resolve(&state.cell.store.pool, crate::settings::SLUG)
            .await
            .unwrap()
            .unwrap()
            .uid,
        facade.uid
    );
    let writer = user(&state, "writer", true).await;
    let hidden = record(&state, "Secret template").await;
    let template = crate::settings::defaults();
    store::records::set_extension(
        &state.cell.store.pool,
        &hidden,
        crate::settings::COLUMNS,
        &template,
    )
    .await
    .unwrap();
    let snapshot = crate::settings::snapshot(&state, Some(&writer))
        .await
        .unwrap();
    assert!(snapshot["templates"].as_array().unwrap().is_empty());
    store::visibility::grant(&state.cell.store.pool, "actor", Some(&writer.uid), &hidden)
        .await
        .unwrap();
    assert_eq!(
        crate::settings::snapshot(&state, Some(&writer))
            .await
            .unwrap()["templates"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let config = Action::SetExtension {
        target: facade.uid.clone(),
        namespace: crate::settings::COLUMNS.into(),
        fds: template.clone(),
    };
    assert!(
        allowed(&state, Some(&writer), &facade.uid, &config)
            .await
            .is_err()
    );
    assert!(allowed(&state, None, &facade.uid, &config).await.is_ok());
    state.cell.engine.act(config, None).await.unwrap();
    state
        .cell
        .engine
        .act(
            Action::SetExtension {
                target: facade.uid.clone(),
                namespace: crate::settings::CUSTOM.into(),
                fds: json!({"title":"My Company's Ticket View"}),
            },
            None,
        )
        .await
        .unwrap();
    crate::settings::ensure(&state).await.unwrap();
    assert_eq!(
        crate::settings::snapshot(&state, Some(&writer))
            .await
            .unwrap()["customtitle"],
        "My Company's Ticket View"
    );
    assert!(
        crate::settings::validate(
            crate::settings::COLUMNS,
            &json!({"name":"Bad","columns":[{"title":"A","quantity":0},{"title":"B","quantity":0}]})
        )
        .is_err()
    );
    assert!(
        crate::settings::validate(
            crate::settings::COLUMNS,
            &json!({"name":"Bad","columns":[]})
        )
        .is_err()
    );
    assert!(crate::settings::validate("unrelated.namespace", &template).is_err());
    assert!(
        allowed(
            &state,
            None,
            &facade.uid,
            &Action::DeleteRecord {
                target: facade.uid.clone()
            }
        )
        .await
        .is_ok()
    );
    assert!(
        allowed(
            &state,
            Some(&writer),
            "",
            &Action::CreateUser {
                username: "other".into(),
                name: "Other".into(),
                password: "password".into(),
                role: "admin".into()
            }
        )
        .await
        .is_err()
    );
    let permission = store::auth::ensure_permission(&state.cell.store.pool, "permission", "assign")
        .await
        .unwrap();
    store::auth::grant(&state.cell.store.pool, writer.role_id, permission)
        .await
        .unwrap();
    let writer = store::auth::user_by_uid(&state.cell.store.pool, &writer.uid)
        .await
        .unwrap()
        .unwrap();
    assert!(
        allowed(
            &state,
            Some(&writer),
            "",
            &Action::GrantPermission {
                role: "writer".into(),
                permission: "user:delete".into()
            }
        )
        .await
        .is_err()
    );
    assert!(
        allowed(
            &state,
            Some(&writer),
            "",
            &Action::GrantPermission {
                role: "admin".into(),
                permission: "record:read".into()
            }
        )
        .await
        .is_err()
    );
    assert!(
        allowed(
            &state,
            Some(&writer),
            "",
            &Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "New".into(),
                body: String::new(),
                quantity: 0.0
            }
        )
        .await
        .is_ok()
    );
    let card = record(&state, "Delete me").await;
    let delete = Action::DeleteRecord {
        target: card.clone(),
    };
    assert!(allowed(&state, None, "", &delete).await.is_ok());
    state.cell.engine.act(delete, None).await.unwrap();
    assert!(!records::visible(&state, None, &card).await.unwrap());
}

async fn receive(socket: &mut Socket, kind: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let message = socket.next().await.unwrap().unwrap();
            if let Message::Text(text) = message {
                let value: Value = serde_json::from_str(&text).unwrap();
                if value["type"] == kind {
                    return value;
                }
            }
        }
    })
    .await
    .unwrap()
}

async fn send(socket: &mut Socket, value: Value) {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .unwrap();
}

async fn signed_action(
    socket: &mut Socket,
    proof: &nucleus::action_intent::ActionIntentSessionProof,
    key: &ed25519_dalek::SigningKey,
    sequence: u64,
    action: Value,
) -> Value {
    let mut intent = nucleus::action_intent::SignedActionIntent {
        session_id: proof.session_id.clone(),
        session_challenge: proof.session_challenge.clone(),
        sequence,
        message_id: format!("account-{sequence}"),
        action_base64: STANDARD.encode(action.to_string()),
        signature: String::new(),
    };
    intent.signature = STANDARD.encode(key.sign(&intent.signing_bytes()).to_bytes());
    send(socket, json!({"type":"signed_act","id":intent.message_id,"session_id":intent.session_id,"session_challenge":intent.session_challenge,"sequence":sequence,"action_base64":intent.action_base64,"signature":intent.signature})).await;
    receive(socket, "action_ok").await
}

#[tokio::test]
async fn browser_account_changes_use_signed_backend_actions_without_journaling_passwords() {
    let state = fixture().await;
    let owner = store::auth::user_by_username(&state.cell.store.pool, "fixture-admin")
        .await
        .unwrap()
        .unwrap();
    store::auth::ensure_role(&state.cell.store.pool, "limited")
        .await
        .unwrap();
    let server = Facade::start(state.cell.clone(), "127.0.0.1:0")
        .await
        .unwrap();
    let origin = format!("http://{}", server.address);
    let response = reqwest::Client::new()
        .post(format!("{origin}/login"))
        .header("Origin", &origin)
        .json(&json!({"username":"fixture-admin","password":"fixture-password"}))
        .send()
        .await
        .unwrap();
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let mut request = format!("ws://{}/live", server.address)
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Origin", origin.parse().unwrap());
    request
        .headers_mut()
        .insert("Cookie", cookie.parse().unwrap());
    let (mut socket, _) = connect_async(request).await.unwrap();
    let challenge = receive(&mut socket, "session_challenge").await;
    let key = ed25519_dalek::SigningKey::from_bytes(&[17; 32]);
    let mut proof = nucleus::action_intent::ActionIntentSessionProof {
        session_id: challenge["session_id"].as_str().unwrap().into(),
        session_challenge: challenge["challenge"].as_str().unwrap().into(),
        person_uid: owner.uid,
        key_id: "account-test-key".into(),
        public_key_base64: STANDARD.encode(key.verifying_key().to_bytes()),
        signature: String::new(),
    };
    proof.signature = STANDARD.encode(key.sign(&proof.signing_bytes()).to_bytes());
    let mut message = serde_json::to_value(&proof).unwrap();
    message["type"] = json!("session_authenticate");
    message["id"] = json!("auth");
    send(&mut socket, message).await;
    receive(&mut socket, "session_authenticated").await;
    let created = signed_action(&mut socket, &proof, &key, 1, json!({"action":"create-user","username":"new-account","name":"New account","password":"initial-secret","role":"limited"})).await;
    let uid = created["created"].as_str().unwrap();
    signed_action(&mut socket, &proof, &key, 2, json!({"action":"update-user","user":uid,"username":"renamed","name":"Changed account","password":"replacement-secret"})).await;
    let target = store::auth::user_by_uid(&state.cell.store.pool, uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(target.username, "renamed");
    assert!(utils::auth::verify_password("replacement-secret", &target.password_hash).unwrap());
    let journaled: i64 =
        store::sqlx::query_scalar("SELECT COUNT(*) FROM signed_action_intent WHERE session_id = ?")
            .bind(&proof.session_id)
            .fetch_one(&state.cell.store.pool)
            .await
            .unwrap();
    assert_eq!(journaled, 0);
    signed_action(
        &mut socket,
        &proof,
        &key,
        3,
        json!({"action":"delete-user","user":uid}),
    )
    .await;
    assert!(
        store::auth::user_by_uid(&state.cell.store.pool, uid)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn public_facade_requires_its_https_origin_and_issues_a_host_only_secure_cookie() {
    let state = fixture().await;
    let public = "https://lince.mycompany.example";
    assert!(
        Facade::start_public(
            state.cell.clone(),
            "127.0.0.1:0",
            "http://lince.mycompany.example"
        )
        .await
        .is_err()
    );
    assert!(
        Facade::start_public(state.cell.clone(), "0.0.0.0:0", public)
            .await
            .is_err()
    );
    let server = Facade::start_public(state.cell.clone(), "127.0.0.1:0", public)
        .await
        .unwrap();
    let url = format!("http://{}/login", server.address);
    let client = reqwest::Client::new();
    let credentials = json!({"username":"fixture-admin", "password":"fixture-password"});
    for (host, origin, forwarded) in [
        ("wrong.example", public, "https"),
        (
            "lince.mycompany.example",
            "https://attacker.example",
            "https",
        ),
        ("lince.mycompany.example", public, "http"),
    ] {
        let response = client
            .post(&url)
            .header("Host", host)
            .header("Origin", origin)
            .header("X-Forwarded-Proto", forwarded)
            .json(&credentials)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 403);
    }
    let response = client
        .post(&url)
        .header("Host", "lince.mycompany.example")
        .header("Origin", public)
        .header("X-Forwarded-Proto", "https")
        .json(&credentials)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let cookie = response.headers()["set-cookie"].to_str().unwrap();
    assert!(cookie.starts_with("__Host-lince_facade="));
    for flag in ["Secure", "HttpOnly", "SameSite=Strict", "Path=/"] {
        assert!(cookie.contains(flag));
    }
    assert!(!cookie.contains("Domain="));
    assert_eq!(
        response.headers()["strict-transport-security"],
        "max-age=31536000"
    );
    assert_eq!(response.headers()["cache-control"], "no-store");
    let cookie = cookie.split(';').next().unwrap();
    let mut request = format!("ws://{}/live", server.address)
        .into_client_request()
        .unwrap();
    for (key, value) in [
        ("Host", "lince.mycompany.example"),
        ("Origin", "https://attacker.example"),
        ("X-Forwarded-Proto", "https"),
        ("Cookie", cookie),
    ] {
        request.headers_mut().insert(
            axum::http::HeaderName::from_bytes(key.as_bytes()).unwrap(),
            value.parse().unwrap(),
        );
    }
    assert!(connect_async(request).await.is_err());
}

#[tokio::test]
async fn facade_never_opens_anonymous_owner_access() {
    let state = fixture().await;
    assert!(
        auth::viewer(&state, &axum::http::HeaderMap::new())
            .await
            .is_err()
    );
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let mut cell = state.cell;
    cell.store = engine.store.clone();
    cell.engine = engine;
    assert!(Facade::start(cell, "127.0.0.1:0").await.is_err());
}

#[tokio::test]
async fn browser_sessions_reject_duplicate_cookies_and_close_when_credentials_change() {
    let state = fixture().await;
    let viewer = user(&state, "revoked-browser", true).await;
    let facade = Facade::start(state.cell.clone(), "127.0.0.1:0")
        .await
        .unwrap();
    let origin = format!("http://{}", facade.address);
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{origin}/login"))
        .header("Origin", &origin)
        .json(&json!({"username":viewer.username,"password":"facade-test-password"}))
        .send()
        .await
        .unwrap();
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let session = client
        .get(format!("{origin}/session"))
        .header("Cookie", format!("{cookie}; {cookie}"))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(session["ready"], false);
    let mut request = format!("ws://{}/live", facade.address)
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Origin", origin.parse().unwrap());
    request
        .headers_mut()
        .insert("Cookie", cookie.parse().unwrap());
    let (mut socket, _) = connect_async(request).await.unwrap();
    receive(&mut socket, "session_challenge").await;
    state
        .cell
        .engine
        .act(
            Action::UpdateUser {
                user: viewer.uid,
                username: viewer.username,
                name: "Changed".into(),
                password: "changed-password".into(),
            },
            None,
        )
        .await
        .unwrap();
    receive(&mut socket, "logout").await;
    let session = client
        .get(format!("{origin}/session"))
        .header("Cookie", cookie)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(session["ready"], false);
}

#[tokio::test]
async fn browser_login_signed_edit_live_updates_and_logout() {
    let state = fixture().await;
    let target = record(&state, "Before").await;
    let writer = user(&state, "writer", true).await;
    store::visibility::grant(&state.cell.store.pool, "actor", Some(&writer.uid), &target)
        .await
        .unwrap();
    let facade = Facade::start(state.cell.clone(), "127.0.0.1:0")
        .await
        .unwrap();
    let origin = format!("http://{}", facade.address);
    let client = reqwest::Client::new();
    let credentials = json!({"username":"writer", "password":"facade-test-password"});
    assert_eq!(
        client
            .post(format!("{origin}/login"))
            .header("Origin", "https://elsewhere.invalid")
            .json(&credentials)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .post(format!("{origin}/login"))
            .header("Origin", &origin)
            .json(&json!({"username":"writer", "password":"wrong"}))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let response = client
        .post(format!("{origin}/login"))
        .header("Origin", &origin)
        .json(&credentials)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let mut request = format!("ws://{}/live", facade.address)
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Origin", origin.parse().unwrap());
    assert!(connect_async(request.clone()).await.is_err());
    request
        .headers_mut()
        .insert("Cookie", cookie.parse().unwrap());
    let (mut socket, _) = connect_async(request).await.unwrap();
    let challenge = receive(&mut socket, "session_challenge").await;
    let key = ed25519_dalek::SigningKey::from_bytes(&[7; 32]);
    let mut proof = nucleus::action_intent::ActionIntentSessionProof {
        session_id: challenge["session_id"].as_str().unwrap().into(),
        session_challenge: challenge["challenge"].as_str().unwrap().into(),
        person_uid: writer.uid.clone(),
        key_id: "facade-test-key".into(),
        public_key_base64: STANDARD.encode(key.verifying_key().to_bytes()),
        signature: String::new(),
    };
    proof.signature = STANDARD.encode(key.sign(&proof.signing_bytes()).to_bytes());
    let mut message = serde_json::to_value(&proof).unwrap();
    message["type"] = json!("session_authenticate");
    message["id"] = json!("auth");
    send(&mut socket, message).await;
    receive(&mut socket, "session_authenticated").await;
    send(&mut socket, json!({"type":"select", "uid":target})).await;
    let view = receive(&mut socket, "signals").await;
    assert_eq!(view["signals"]["record"]["head"], "Before");
    send(&mut socket, json!({"type":"act", "id":"unsigned", "action":{"action":"edit-record-text", "target":target,"head":"Forged"}})).await;
    receive(&mut socket, "error").await;
    let mut intent = nucleus::action_intent::SignedActionIntent {
        session_id: proof.session_id,
        session_challenge: proof.session_challenge,
        sequence: 2,
        message_id: "edit".into(),
        action_base64: STANDARD.encode(
            json!({"action":"edit-record-text", "target":target,"head":"After"}).to_string(),
        ),
        signature: String::new(),
    };
    intent.signature = STANDARD.encode(key.sign(&intent.signing_bytes()).to_bytes());
    let message = json!({"type":"signed_act", "id":"edit", "session_id":intent.session_id, "session_challenge":intent.session_challenge, "sequence":2, "action_base64":intent.action_base64, "signature":intent.signature});
    let mut denied_intent = intent.clone();
    denied_intent.message_id = "denied".into();
    denied_intent.sequence = 1;
    denied_intent.action_base64 =
        STANDARD.encode(json!({"action":"delete-record", "target":target}).to_string());
    denied_intent.signature = STANDARD.encode(key.sign(&denied_intent.signing_bytes()).to_bytes());
    send(&mut socket, json!({"type":"signed_act", "id":"denied", "session_id":denied_intent.session_id, "session_challenge":denied_intent.session_challenge, "sequence":1, "action_base64":denied_intent.action_base64, "signature":denied_intent.signature})).await;
    assert_eq!(receive(&mut socket, "error").await["code"], "forbidden");
    send(&mut socket, message.clone()).await;
    receive(&mut socket, "action_ok").await;
    let view = receive(&mut socket, "signals").await;
    assert_eq!(view["signals"]["record"]["head"], "After");
    send(&mut socket, message).await;
    receive(&mut socket, "error").await;
    send(&mut socket, json!({"type":"select","uid":""})).await;
    receive(&mut socket, "signals").await;
    let mut move_intent = intent.clone();
    move_intent.message_id = "move".into();
    move_intent.sequence = 3;
    move_intent.action_base64 =
        STANDARD.encode(json!({"action":"set-quantity","target":target,"value":1.0}).to_string());
    move_intent.signature = STANDARD.encode(key.sign(&move_intent.signing_bytes()).to_bytes());
    send(&mut socket, json!({"type":"signed_act","id":"move","session_id":move_intent.session_id,"session_challenge":move_intent.session_challenge,"sequence":3,"action_base64":move_intent.action_base64,"signature":move_intent.signature})).await;
    receive(&mut socket, "action_ok").await;
    let board = receive(&mut socket, "signals").await;
    assert_eq!(board["signals"]["records"][0]["quantity"], 1.0);
    send(&mut socket, json!({"type":"select","uid":target})).await;
    let view = receive(&mut socket, "signals").await;
    assert_eq!(view["signals"]["record"]["quantity"], 1.0);
    client
        .post(format!("{origin}/logout"))
        .header("Origin", &origin)
        .header("Cookie", &cookie)
        .json(&json!({}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    receive(&mut socket, "logout").await;
    let closed = tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap();
    assert!(matches!(closed, Some(Ok(Message::Close(_))) | None));
    assert_eq!(
        client
            .get(format!("{origin}/session"))
            .header("Cookie", cookie)
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap()["ready"],
        false
    );
}

#[tokio::test]
async fn backend_comment_access_does_not_depend_on_browser_selection() {
    let state = fixture().await;
    let target = record(&state, "Task").await;
    let create = Action::CreateThread {
        target: target.clone(),
        head: "General".into(),
    };
    allowed(&state, None, &target, &create).await.unwrap();
    let thread = state
        .cell
        .engine
        .act(create, None)
        .await
        .unwrap()
        .created
        .unwrap();
    let comment = Action::CreateMessage {
        thread: thread.clone(),
        body: "A comment".into(),
        author: None,
        state: nucleus::MessageState::Finished,
        parent: None,
        references: vec![],
    };
    allowed(&state, None, &target, &comment).await.unwrap();
    state.cell.engine.act(comment.clone(), None).await.unwrap();
    let view = records::snapshot(&state, None, &target, "").await.unwrap();
    assert_eq!(view["messages"][0]["body"], "A comment");
    assert_eq!(view["messages"][0]["thread_uid"], thread);
    let other = record(&state, "Another task").await;
    assert!(allowed(&state, None, &other, &comment).await.is_ok());
    let view = records::snapshot(&state, None, "", "Another")
        .await
        .unwrap();
    assert_eq!(view["records"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn general_settings_use_shared_record_access_and_keep_public_login_labels() {
    let state = fixture().await;
    crate::settings::ensure(&state).await.unwrap();
    assert_eq!(
        crate::settings::general(&state).await.unwrap(),
        json!({"title":"Facade","language":"pt-BR"})
    );
    let facade = store::records::resolve(&state.cell.store.pool, crate::settings::SLUG)
        .await
        .unwrap()
        .unwrap();
    let mut editor = user(&state, "settings-editor", true).await;
    editor.permissions.push("configuration:update".into());
    store::visibility::grant(
        &state.cell.store.pool,
        "actor",
        Some(&editor.uid),
        &facade.uid,
    )
    .await
    .unwrap();
    let owner = user(&state, "settings-owner", true).await;
    let role = store::auth::role_by_name(&state.cell.store.pool, "admin")
        .await
        .unwrap()
        .unwrap();
    store::auth::set_user_role(&state.cell.store.pool, &owner.uid, role)
        .await
        .unwrap();
    let owner = store::auth::user_by_username(&state.cell.store.pool, "settings-owner")
        .await
        .unwrap()
        .unwrap();
    let action = Action::SetExtension {
        target: facade.uid.clone(),
        namespace: crate::settings::CUSTOM.into(),
        fds: json!({"title":"Equipe <Azul>","language":"en"}),
    };
    assert!(allowed(&state, None, "", &action).await.is_ok());
    assert!(allowed(&state, Some(&editor), "", &action).await.is_ok());
    assert_eq!(
        crate::settings::snapshot(&state, Some(&editor))
            .await
            .unwrap()["canadmin"],
        true
    );
    assert!(allowed(&state, Some(&owner), "", &action).await.is_ok());
    assert_eq!(
        crate::settings::snapshot(&state, Some(&owner))
            .await
            .unwrap()["canadmin"],
        true
    );
    state.cell.engine.act(action, None).await.unwrap();
    crate::settings::ensure(&state).await.unwrap();
    let snapshot = crate::settings::snapshot(&state, Some(&editor))
        .await
        .unwrap();
    assert_eq!(snapshot["customtitle"], "Equipe <Azul>");
    assert_eq!(snapshot["language"], "en");
    let server = Facade::start(state.cell.clone(), "127.0.0.1:0")
        .await
        .unwrap();
    let public: Value = reqwest::get(format!("http://{}/session", server.address))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(public["customtitle"], "Equipe <Azul>");
    assert_eq!(public["language"], "en");
    assert_eq!(public["ready"], false);
    for value in [
        json!({"title":"App","language":"fr"}),
        json!({"title":" ","language":"pt-BR"}),
        json!({"title":"App","language":"en","admin":true}),
        json!({"title":"App"}),
        json!({"title":"x".repeat(201),"language":"en"}),
    ] {
        assert!(crate::settings::validate(crate::settings::CUSTOM, &value).is_err());
    }
    let portuguese = json!({"title":"Equipe","language":"pt-BR"});
    assert!(crate::settings::validate(crate::settings::CUSTOM, &portuguese).is_ok());
    state
        .cell
        .engine
        .act(
            Action::SetExtension {
                target: facade.uid,
                namespace: crate::settings::CUSTOM.into(),
                fds: portuguese.clone(),
            },
            None,
        )
        .await
        .unwrap();
    crate::settings::ensure(&state).await.unwrap();
    assert_eq!(crate::settings::general(&state).await.unwrap(), portuguese);
}

#[tokio::test]
async fn role_allow_and_block_rules_gate_reads_moves_and_role_changes() {
    use protein::{Predicate, read_rules::ReadRules};
    let state = fixture().await;
    let editor = user(&state, "project-editor", true).await;
    let reader = user(&state, "project-reader", false).await;
    let project = store::concepts::ensure(&state.cell.store.pool, "project-a")
        .await
        .unwrap();
    let done = store::concepts::ensure(&state.cell.store.pool, "done")
        .await
        .unwrap();
    let good = record(&state, "Allowed project task").await;
    let blocked = record(&state, "Unfinished project task").await;
    let outside = record(&state, "Other project task").await;
    for (target, tag) in [
        (&good, &project),
        (&good, &done),
        (&blocked, &project),
        (&outside, &done),
    ] {
        state
            .cell
            .engine
            .act(
                Action::AssertRecord {
                    subject: target.clone(),
                    predicate: tag.clone(),
                    object: None,
                    quantity: None,
                    unit: None,
                },
                None,
            )
            .await
            .unwrap();
    }
    let private = record(&state, "Explicitly restricted task").await;
    for tag in [&project, &done] {
        state
            .cell
            .engine
            .act(
                Action::AssertRecord {
                    subject: private.clone(),
                    predicate: tag.clone(),
                    object: None,
                    quantity: None,
                    unit: None,
                },
                None,
            )
            .await
            .unwrap();
    }
    store::visibility::grant(
        &state.cell.store.pool,
        "actor",
        Some(&nucleus::new_uid("r")),
        &private,
    )
    .await
    .unwrap();
    let rules = ReadRules {
        allow: Predicate::All(vec![Predicate::ConceptIn(project.clone())]),
        block: Predicate::Any(vec![Predicate::Not(Box::new(Predicate::ConceptIn(
            done.clone(),
        )))]),
    };
    let set = Action::SetRoleReadRules {
        role: editor.role.clone(),
        rules: rules.clone(),
        expected_revision: 0,
    };
    assert!(allowed(&state, Some(&editor), "", &set).await.is_err());
    assert!(
        state
            .cell
            .engine
            .act(set.clone(), Some(editor.uid.clone()))
            .await
            .is_err()
    );
    state.cell.engine.act(set.clone(), None).await.unwrap();
    assert!(state.cell.engine.act(set, None).await.is_err());
    let snapshot = records::snapshot(&state, Some(&editor), &blocked, "")
        .await
        .unwrap();
    assert!(snapshot["record"]["uid"].is_null());
    assert_eq!(snapshot["records"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["records"][0]["uid"], good);
    for target in [&blocked, &outside, &private] {
        let action = Action::SetQuantity {
            target: target.clone(),
            value: 1.0,
        };
        assert!(allowed(&state, Some(&editor), "", &action).await.is_err());
        assert!(
            state
                .cell
                .engine
                .act(action, Some(editor.uid.clone()))
                .await
                .is_err()
        );
    }
    let move_allowed = Action::SetQuantity {
        target: good.clone(),
        value: 1.0,
    };
    assert!(
        allowed(&state, Some(&editor), "", &move_allowed)
            .await
            .is_ok()
    );
    assert!(
        allowed(&state, Some(&reader), "", &move_allowed)
            .await
            .is_err()
    );
    state
        .cell
        .engine
        .act(move_allowed, Some(editor.uid.clone()))
        .await
        .unwrap();
    assert_eq!(
        records::snapshot(&state, Some(&editor), &good, "")
            .await
            .unwrap()["record"]["quantity"],
        1.0
    );
    store::auth::set_user_role(&state.cell.store.pool, &reader.uid, editor.role_id)
        .await
        .unwrap();
    let reader = store::auth::user_by_uid(&state.cell.store.pool, &reader.uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        records::snapshot(&state, Some(&reader), "", "")
            .await
            .unwrap()["records"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    state
        .cell
        .engine
        .act(
            Action::SetRoleReadRules {
                role: editor.role.clone(),
                rules: ReadRules {
                    allow: Predicate::Any(vec![
                        Predicate::ConceptIn(project),
                        Predicate::ConceptIn(done),
                    ]),
                    block: Predicate::Any(vec![]),
                },
                expected_revision: 1,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        records::snapshot(&state, Some(&editor), "", "")
            .await
            .unwrap()["records"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    state
        .cell
        .engine
        .set_read_filter(&editor.uid, Some(&Predicate::UidEq(good)))
        .await
        .unwrap();
    assert_eq!(
        records::snapshot(&state, Some(&editor), "", "")
            .await
            .unwrap()["records"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let stored = store::role_policies::get(&state.cell.store.pool, editor.role_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.revision, 2);
    let policy: protein::authority::RolePolicy =
        serde_json::from_value(stored.policy.unwrap()).unwrap();
    ReadRules::from_predicate(policy.read).validate().unwrap();
}

#[tokio::test]
async fn tagged_creation_is_visible_immediately_and_invalid_drafts_leave_no_record() {
    use protein::{Predicate, read_rules::ReadRules};
    let state = fixture().await;
    let editor = user(&state, "draft-editor", true).await;
    let project = store::concepts::ensure(&state.cell.store.pool, "project-a")
        .await
        .unwrap();
    let done = store::concepts::ensure(&state.cell.store.pool, "done")
        .await
        .unwrap();
    state
        .cell
        .engine
        .act(
            Action::SetRoleReadRules {
                role: editor.role.clone(),
                expected_revision: 0,
                rules: ReadRules {
                    allow: Predicate::All(vec![
                        Predicate::ConceptIn(project.clone()),
                        Predicate::ConceptIn(done.clone()),
                    ]),
                    block: Predicate::Any(vec![]),
                },
            },
            None,
        )
        .await
        .unwrap();
    let make = |tags| Action::CreateRecordWithTags {
        head: "New task".into(),
        body: "Its description".into(),
        quantity: -2.0,
        tags,
    };
    assert!(
        state
            .cell
            .engine
            .act(make(vec![project.clone()]), Some(editor.uid.clone()))
            .await
            .is_err()
    );
    assert!(
        records::read(
            &state,
            None,
            vec![Predicate::TextContains("New task".into())],
            10
        )
        .await
        .unwrap()
        .is_empty()
    );
    let action = make(vec![project, done]);
    allowed(&state, Some(&editor), "", &action).await.unwrap();
    let created = state
        .cell
        .engine
        .act(action, Some(editor.uid.clone()))
        .await
        .unwrap()
        .created
        .unwrap();
    let full = records::snapshot(&state, Some(&editor), &created, "")
        .await
        .unwrap();
    assert_eq!(full["record"]["head"], "New task");
    assert_eq!(full["record"]["body"], "Its description");
    assert_eq!(full["record"]["quantity"], -2.0);
    assert_eq!(full["assertions"].as_array().unwrap().len(), 2);
    assert_eq!(full["canedit"], true);
    let thread_action = Action::CreateThread {
        target: created.clone(),
        head: "Discussion".into(),
    };
    allowed(&state, Some(&editor), &created, &thread_action)
        .await
        .unwrap();
    let thread = state
        .cell
        .engine
        .act(thread_action, Some(editor.uid.clone()))
        .await
        .unwrap()
        .created
        .unwrap();
    let comment = Action::CreateMessage {
        thread,
        body: "Visible comment".into(),
        author: None,
        state: nucleus::MessageState::Finished,
        parent: None,
        references: vec![],
    };
    allowed(&state, Some(&editor), &created, &comment)
        .await
        .unwrap();
    state
        .cell
        .engine
        .act(comment, Some(editor.uid.clone()))
        .await
        .unwrap();
    let full = records::snapshot(&state, Some(&editor), &created, "")
        .await
        .unwrap();
    assert_eq!(full["messages"].as_array().unwrap().len(), 1);
    assert_eq!(full["messages"][0]["body"], "Visible comment");
}
