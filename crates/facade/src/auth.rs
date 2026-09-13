use std::{collections::HashMap, sync::Arc};

use axum::{
    Json,
    extract::State as Extract,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use engine::login::LoginSession;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::{Mutex, Semaphore};

use crate::State;

pub(crate) type Failure = (StatusCode, String);

#[derive(Default)]
pub(crate) struct Auth {
    sessions: Mutex<HashMap<String, BrowserSession>>,
}

#[derive(Clone)]
pub(crate) struct BrowserSession {
    pub(crate) login: LoginSession,
    pub(crate) connections: Arc<Semaphore>,
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

fn token(state: &State, headers: &HeaderMap) -> Option<String> {
    let mut matches = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|part| part.trim().split_once('='))
        .filter(|(key, _)| *key == state.security.cookie_name());
    let (_, value) = matches.next()?;
    if matches.next().is_some() || value.len() != 36 || uuid::Uuid::parse_str(value).is_err() {
        return None;
    }
    Some(value.to_string())
}

pub(crate) async fn authenticated(
    state: &State,
    headers: &HeaderMap,
) -> Result<BrowserSession, Failure> {
    let token = token(state, headers).ok_or_else(refused)?;
    let login = {
        let mut sessions = state.auth.sessions.lock().await;
        sessions.retain(|_, session| session.login.is_live());
        sessions.get(&token).cloned()
    }
    .ok_or_else(refused)?;
    login
        .login
        .require(&state.cell.engine)
        .await
        .map_err(|_| refused())?;
    Ok(login)
}

pub(crate) async fn viewer(
    state: &State,
    headers: &HeaderMap,
) -> Result<Option<store::auth::AuthUser>, Failure> {
    let session = authenticated(state, headers).await?;
    let user = store::auth::user_by_uid(&state.cell.store.pool, session.login.person_uid())
        .await
        .map_err(internal)?
        .ok_or_else(refused)?;
    Ok(Some(user))
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
    state.security.same_origin(&headers)?;
    let password = engine::private_password::PasswordInput::new(credentials.password.into_bytes())
        .map_err(|_| refused())?;
    let login = state
        .cell
        .engine
        .login_password(&credentials.username, password, None)
        .await
        .map_err(|error| {
            if error.code() == Some("login_throttled") {
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    "Please try again shortly.".into(),
                )
            } else {
                (
                    StatusCode::UNAUTHORIZED,
                    "Invalid username or password.".into(),
                )
            }
        })?;
    let mut sessions = state.auth.sessions.lock().await;
    sessions.retain(|_, session| session.login.is_live());
    if sessions.len() >= 1024 {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Please try again later.".into(),
        ));
    }
    if let Some(old) = token(&state, &headers).and_then(|token| sessions.remove(&token)) {
        old.login.revoke();
    }
    let token = uuid::Uuid::new_v4().to_string();
    sessions.insert(
        token.clone(),
        BrowserSession {
            login,
            connections: Arc::new(Semaphore::new(8)),
        },
    );
    let cookie = state
        .security
        .cookie(&token, engine::login::SESSION_TTL.as_secs());
    Ok(([(header::SET_COOKIE, cookie)], Json(json!({"ready": true}))).into_response())
}

pub(crate) async fn logout(
    Extract(state): Extract<State>,
    headers: HeaderMap,
) -> Result<Response, Failure> {
    state.security.same_origin(&headers)?;
    state
        .cell
        .engine
        .access_scope(true, async {
            if let Some(token) = token(&state, &headers)
                && let Some(session) = state.auth.sessions.lock().await.remove(&token)
            {
                session.login.revoke();
            }
            Ok(())
        })
        .await
        .map_err(internal)?;
    Ok((
        [(header::SET_COOKIE, state.security.cookie("", 0))],
        Json(json!({"ready": false})),
    )
        .into_response())
}
