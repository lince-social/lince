// non-lean-yet
// Scope: fn provider, fn http_error, fn has_native_login, fn login.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 32.
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 19, 22.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 10, 72, 88, 96, 117.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 10, 31,
//   68, 72, 88, 96, 117, 125.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites account_ready:
//   String (88), login: String (72), logout: String (96), provider: String (10), session: String
//   (117).

use std::sync::Arc;

use tokio::sync::Mutex;

use super::{
    auth::{CredentialStore, MemoryCredential, Session},
    chatgpt::ChatGpt,
    provider::{GenaiProvider, Provider},
};
use crate::config::{Secret, Settings};

pub fn provider(
    settings: &Settings,
    key: &Secret,
    store: Option<Arc<dyn CredentialStore>>,
    refresh_lock: Option<Arc<Mutex<()>>>,
) -> Result<Arc<dyn Provider>, String> {
    if settings.provider.0 == super::auth::PROVIDER {
        super::auth::Account::decode(key)?;
        Ok(Arc::new(ChatGpt::new(
            settings.clone(),
            Session {
                auth: super::auth::Auth::production()?,
                store: store.unwrap_or_else(|| Arc::new(MemoryCredential(Mutex::new(key.clone())))),
                lock: refresh_lock.unwrap_or_default(),
            },
        )?))
    } else {
        Ok(Arc::new(GenaiProvider::new(settings, key)?))
    }
}

pub fn http_error(status: reqwest::StatusCode, value: &serde_json::Value) -> String {
    let code = value["error"]["code"]
        .as_str()
        .or_else(|| value["error"].as_str())
        .unwrap_or("unknown_error");
    let code: String = code
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(100)
        .collect();
    let detail = match code.as_str() {
        "subscription_sharing_usage_limit_exceeded" => {
            "The ChatGPT plan usage limit was reached. Wait for the account's usage window to reset."
        }
        "subscription_sharing_usage_unavailable" | "subscription_sharing_user_not_eligible" => {
            "ChatGPT plan usage is unavailable for this account or workspace. Check its plan usage settings."
        }
        "invalid_grant" | "invalid_token" => {
            "The ChatGPT session expired or was revoked. Continue with ChatGPT again."
        }
        _ if status == reqwest::StatusCode::UNAUTHORIZED => {
            "Authentication failed. Reconnect this account."
        }
        _ if status == reqwest::StatusCode::FORBIDDEN => {
            "The account or workspace does not authorize this request."
        }
        _ if status == reqwest::StatusCode::TOO_MANY_REQUESTS => {
            "The provider is rate limited. Retry later."
        }
        _ if status.is_server_error() => "The provider is temporarily unavailable. Retry later.",
        _ => {
            "The provider rejected the request. Check the model and supported connection settings."
        }
    };
    format!("{detail} (HTTP {}, {code})", status.as_u16())
}

pub fn has_native_login(settings: &Settings) -> bool {
    settings.provider.0 == super::auth::PROVIDER
}

pub async fn login(
    settings: &mut Settings,
    directory: &std::path::Path,
    previous: Option<&Secret>,
) -> Result<super::auth::Login, String> {
    if !has_native_login(settings) {
        return Err("This provider does not supply native browser sign-in.".into());
    }
    let host = super::auth::host_id(directory)?;
    if settings.account.is_empty() {
        settings.account = format!("{host}:{}", uuid::Uuid::new_v4());
    }
    let existing = previous.map(super::auth::Account::decode).transpose()?;
    super::auth::Auth::production()?.begin(host, existing).await
}

pub fn account_ready(settings: &Settings, key: &Secret) -> Result<bool, String> {
    if has_native_login(settings) {
        Ok(super::auth::Account::decode(key)?.enabled())
    } else {
        Ok(true)
    }
}

pub async fn logout(settings: &Settings, key: &Secret) -> Result<(Secret, String), String> {
    if !has_native_login(settings) {
        return Err("Use the connection's credential editor for API keys.".into());
    }
    let mut account = super::auth::Account::decode(key)?;
    let revoked = super::auth::Auth::production()?.revoke(&account).await;
    account.access_token = Secret::default();
    account.refresh_token = Secret::default();
    account.id_token = Secret::default();
    account.scopes.clear();
    account.expires_at = 0;
    Ok((
        account.encode()?,
        if revoked.is_ok() {
            "Signed out and refresh token revoked. Registration retained for this account.".into()
        } else {
            "Signed out locally. Remote revocation was not confirmed; disconnect Lince in account settings if needed.".into()
        },
    ))
}

pub fn session(store: Arc<dyn CredentialStore>, lock: Arc<Mutex<()>>) -> Result<Session, String> {
    Ok(Session {
        auth: super::auth::Auth::production()?,
        store,
        lock,
    })
}

pub fn account_label(settings: &Settings, key: &Secret) -> Option<String> {
    if !has_native_login(settings) {
        return None;
    }
    super::auth::Account::decode(key)
        .ok()
        .and_then(|account| account.email)
        .map(|email| format!("ChatGPT · {email}"))
}
