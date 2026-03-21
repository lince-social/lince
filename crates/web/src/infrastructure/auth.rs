use {
    std::{collections::HashMap, sync::Arc},
    tokio::sync::RwLock,
};

const SESSION_COOKIE_NAME: &str = "lince_session";

#[derive(Clone, Debug)]
pub struct SessionRecord {
    pub username_hint: String,
    pub manas_token: Option<String>,
}

#[derive(Clone, Default)]
pub struct AppAuth {
    sessions: Arc<RwLock<HashMap<String, SessionRecord>>>,
}

impl AppAuth {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn default_username_hint(&self) -> &str {
        ""
    }

    pub async fn create_authenticated_session(
        &self,
        username_hint: impl Into<String>,
        manas_token: String,
    ) -> String {
        self.create_session(SessionRecord {
            username_hint: username_hint.into(),
            manas_token: Some(manas_token),
        })
        .await
    }

    pub async fn create_guest_session(&self) -> String {
        self.create_session(SessionRecord {
            username_hint: String::new(),
            manas_token: None,
        })
        .await
    }

    pub async fn is_authenticated(&self, token: Option<&str>) -> bool {
        let Some(token) = token.filter(|value| !value.is_empty()) else {
            return false;
        };

        self.sessions.read().await.contains_key(token)
    }

    pub async fn session(&self, token: Option<&str>) -> Option<SessionRecord> {
        let token = token?.trim();
        if token.is_empty() {
            return None;
        }

        self.sessions.read().await.get(token).cloned()
    }

    pub async fn destroy_session(&self, token: Option<&str>) {
        let Some(token) = token.filter(|value| !value.is_empty()) else {
            return;
        };

        self.sessions.write().await.remove(token);
    }

    async fn create_session(&self, record: SessionRecord) -> String {
        let token = uuid::Uuid::new_v4().simple().to_string();
        self.sessions.write().await.insert(token.clone(), record);
        token
    }
}

pub fn session_cookie_name() -> &'static str {
    SESSION_COOKIE_NAME
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
