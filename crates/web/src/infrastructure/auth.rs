use {
    serde::Serialize,
    std::{
        collections::HashMap,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    },
    tokio::sync::RwLock,
};

const SESSION_COOKIE_NAME: &str = "lince_session";

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteServerSessionState {
    Connected,
    LoggedOut,
    Expired,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServerSessionSnapshot {
    pub server_id: String,
    pub session_state: RemoteServerSessionState,
    pub username_hint: String,
    pub connected_at_unix: Option<u64>,
    pub last_error: String,
}

#[derive(Clone, Debug)]
pub struct RemoteServerSession {
    pub session_state: RemoteServerSessionState,
    pub username_hint: String,
    pub bearer_token: String,
    pub connected_at_unix: Option<u64>,
    pub last_error: String,
}

#[derive(Clone, Debug, Default)]
pub struct SessionRecord {
    pub server_sessions: HashMap<String, RemoteServerSession>,
}

#[derive(Clone, Default)]
pub struct AppAuth {
    sessions: Arc<RwLock<HashMap<String, SessionRecord>>>,
}

impl AppAuth {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn ensure_session(&self, token: Option<&str>) -> (String, bool) {
        let token = token
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        {
            let sessions = self.sessions.read().await;
            if let Some(token) = token.as_deref()
                && sessions.contains_key(token)
            {
                return (token.to_string(), false);
            }
        }

        let token = uuid::Uuid::new_v4().simple().to_string();
        self.sessions
            .write()
            .await
            .insert(token.clone(), SessionRecord::default());
        (token, true)
    }

    pub async fn session(&self, token: Option<&str>) -> Option<SessionRecord> {
        let token = token?.trim();
        if token.is_empty() {
            return None;
        }

        self.sessions.read().await.get(token).cloned()
    }

    pub async fn set_server_session(
        &self,
        token: &str,
        server_id: impl Into<String>,
        username_hint: impl Into<String>,
        bearer_token: impl Into<String>,
    ) -> Result<(), String> {
        let token = token.trim();
        if token.is_empty() {
            return Err("Sessao local ausente.".into());
        }

        let mut sessions = self.sessions.write().await;
        let Some(session) = sessions.get_mut(token) else {
            return Err("Sessao local ausente.".into());
        };
        session.server_sessions.insert(
            server_id.into(),
            RemoteServerSession {
                session_state: RemoteServerSessionState::Connected,
                username_hint: username_hint.into(),
                bearer_token: bearer_token.into(),
                connected_at_unix: Some(current_unix_timestamp()),
                last_error: String::new(),
            },
        );
        Ok(())
    }

    pub async fn clear_server_session(&self, token: Option<&str>, server_id: &str) {
        let Some(token) = token.map(str::trim).filter(|value| !value.is_empty()) else {
            return;
        };

        let mut sessions = self.sessions.write().await;
        let Some(session) = sessions.get_mut(token) else {
            return;
        };
        if let Some(server_session) = session.server_sessions.get_mut(server_id) {
            server_session.session_state = RemoteServerSessionState::LoggedOut;
            server_session.bearer_token.clear();
            server_session.connected_at_unix = None;
            server_session.last_error.clear();
        }
    }

    pub async fn expire_server_session(
        &self,
        token: Option<&str>,
        server_id: &str,
        message: impl Into<String>,
    ) {
        let Some(token) = token.map(str::trim).filter(|value| !value.is_empty()) else {
            return;
        };

        let mut sessions = self.sessions.write().await;
        let Some(session) = sessions.get_mut(token) else {
            return;
        };
        if let Some(server_session) = session.server_sessions.get_mut(server_id) {
            server_session.session_state = RemoteServerSessionState::Expired;
            server_session.bearer_token.clear();
            server_session.connected_at_unix = None;
            server_session.last_error = message.into();
        }
    }

    pub async fn server_session(
        &self,
        token: Option<&str>,
        server_id: &str,
    ) -> Option<RemoteServerSession> {
        self.session(token)
            .await
            .and_then(|record| record.server_sessions.get(server_id).cloned())
            .filter(|session| {
                session.session_state == RemoteServerSessionState::Connected
                    && !session.bearer_token.trim().is_empty()
            })
    }

    pub async fn remote_server_snapshots(
        &self,
        token: Option<&str>,
    ) -> HashMap<String, RemoteServerSessionSnapshot> {
        self.session(token)
            .await
            .map(|record| {
                record
                    .server_sessions
                    .into_iter()
                    .map(|(server_id, session)| {
                        (
                            server_id.clone(),
                            RemoteServerSessionSnapshot {
                                server_id,
                                session_state: session.session_state,
                                username_hint: session.username_hint,
                                connected_at_unix: session.connected_at_unix,
                                last_error: session.last_error,
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

pub fn session_cookie_name() -> &'static str {
    SESSION_COOKIE_NAME
}

pub fn session_cookie_header(token: &str) -> String {
    format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax",
        session_cookie_name(),
        token
    )
}

pub fn parse_cookie_header(header_value: Option<&str>, cookie_name: &str) -> Option<String> {
    let header_value = header_value?;

    header_value.split(';').find_map(|entry| {
        let (name, value) = entry.trim().split_once('=')?;
        if name == cookie_name {
            Some(value.to_string())
        } else {
            None
        }
    })
}

fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
