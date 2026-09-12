use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use axum::{
    Json,
    extract::State as Extract,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::{Mutex, Semaphore};

use crate::State;

pub(crate) type Failure = (StatusCode, String);
const COOKIE: &str = "lince_facade";
const TTL: Duration = Duration::from_secs(8 * 60 * 60);
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

pub(crate) struct Auth {
    pub(crate) management: Mutex<()>,
    sessions: Mutex<HashMap<String, Login>>,
    pub(crate) passwords: Arc<Semaphore>,
    required: AtomicBool,
}

impl Default for Auth {
    fn default() -> Self {
        Self {
            management: Mutex::new(()),
            sessions: Mutex::new(HashMap::new()),
            passwords: Arc::new(Semaphore::new(2)),
            required: AtomicBool::new(false),
        }
    }
}

#[derive(Clone)]
struct Login {
    person: String,
    hash: String,
    expires: Instant,
}

pub(crate) fn refused() -> Failure {
    (StatusCode::UNAUTHORIZED, "Please log in.".into())
}

pub(crate) fn internal(error: impl std::fmt::Display) -> Failure {
    tracing::warn!(%error, "Facade request failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Could not complete this request.".into(),
    )
}

pub(crate) fn same_origin(headers: &HeaderMap) -> Result<(), Failure> {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<axum::http::Uri>().ok());
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    if origin.as_ref().is_some_and(|uri| {
        matches!(uri.scheme_str(), Some("http" | "https"))
            && uri.authority().map(|a| a.as_str()) == host
            && uri.path() == "/"
    }) {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            "Open Facade from its own address.".into(),
        ))
    }
}

fn token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            (key == COOKIE).then(|| value.to_string())
        })
}

pub(crate) async fn required(state: &State) -> Result<bool, Failure> {
    if !store::auth::list_users(&state.cell.store.pool)
        .await
        .map_err(internal)?
        .is_empty()
    {
        state.auth.required.store(true, Ordering::Relaxed);
    }
    Ok(state.auth.required.load(Ordering::Relaxed))
}

pub(crate) async fn viewer(
    state: &State,
    headers: &HeaderMap,
) -> Result<Option<store::auth::AuthUser>, Failure> {
    if let Some(token) = token(headers) {
        let login = {
            let mut sessions = state.auth.sessions.lock().await;
            sessions.retain(|_, login| login.expires > Instant::now());
            sessions.get(&token).cloned()
        }
        .ok_or_else(refused)?;
        let user = store::auth::user_by_uid(&state.cell.store.pool, &login.person)
            .await
            .map_err(internal)?
            .ok_or_else(refused)?;
        if user.password_hash != login.hash
            || !store::people::is_active(&state.cell.store.pool, &user.uid)
                .await
                .map_err(internal)?
        {
            return Err(refused());
        }
        return Ok(Some(user));
    }
    if required(state).await? {
        Err(refused())
    } else {
        Ok(None)
    }
}

pub(crate) async fn session(
    Extract(state): Extract<State>,
    headers: HeaderMap,
) -> Json<serde_json::Value> {
    let ready = viewer(&state, &headers).await.is_ok();
    let general = crate::settings::general(&state)
        .await
        .unwrap_or_else(|_| crate::settings::general_defaults());
    Json(json!({"ready": ready,"customtitle":general["title"],"language":general["language"]}))
}

#[derive(Deserialize)]
pub(crate) struct Credentials {
    username: String,
    password: String,
}

pub(crate) async fn login(
    Extract(state): Extract<State>,
    headers: HeaderMap,
    Json(credentials): Json<Credentials>,
) -> Result<Response, Failure> {
    same_origin(&headers)?;
    if credentials.username.len() > 256 || credentials.password.len() > 1024 {
        return Err(refused());
    }
    let permit = state
        .auth
        .passwords
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            (
                StatusCode::TOO_MANY_REQUESTS,
                "Please try again shortly.".into(),
            )
        })?;
    let user = store::auth::user_by_username(&state.cell.store.pool, credentials.username.trim())
        .await
        .map_err(internal)?;
    let hash = user
        .as_ref()
        .map_or(DUMMY_HASH, |u| u.password_hash.as_str())
        .to_string();
    let valid = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        utils::auth::verify_password(&credentials.password, &hash).unwrap_or(false)
    })
    .await
    .map_err(internal)?;
    let user = user.filter(|_| valid).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            "Invalid username or password.".into(),
        )
    })?;
    if !store::people::is_active(&state.cell.store.pool, &user.uid)
        .await
        .map_err(internal)?
    {
        return Err(refused());
    }
    let mut sessions = state.auth.sessions.lock().await;
    sessions.retain(|_, login| login.expires > Instant::now());
    if let Some(old) = token(&headers) {
        sessions.remove(&old);
    }
    if sessions.len() >= 1024 {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Please try again later.".into(),
        ));
    }
    let token = uuid::Uuid::new_v4().to_string();
    sessions.insert(
        token.clone(),
        Login {
            person: user.uid,
            hash: user.password_hash,
            expires: Instant::now() + TTL,
        },
    );
    let secure = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("https://"));
    let cookie = format!(
        "{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=28800{}",
        if secure { "; Secure" } else { "" }
    );
    Ok(([(header::SET_COOKIE, cookie)], Json(json!({"ready": true}))).into_response())
}

pub(crate) async fn logout(
    Extract(state): Extract<State>,
    headers: HeaderMap,
) -> Result<Response, Failure> {
    same_origin(&headers)?;
    if let Some(token) = token(&headers) {
        state.auth.sessions.lock().await.remove(&token);
    }
    Ok((
        [(
            header::SET_COOKIE,
            "lince_facade=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0",
        )],
        Json(json!({"ready": false})),
    )
        .into_response())
}
