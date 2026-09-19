use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerProfile {
    pub id: String,
    pub username: String,
    pub name: String,
    pub role: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProfile {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub requires_auth: bool,
    pub authenticated: bool,
    pub session_state: Option<String>,
    pub username_hint: String,
    pub connected_at_unix: Option<u64>,
    pub last_error: String,
    pub local: bool,
}
