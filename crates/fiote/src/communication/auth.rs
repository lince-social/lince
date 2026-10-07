use crate::config::Secret;
use openidconnect::{
    ClientId, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier,
    core::{CoreIdToken, CoreIdTokenVerifier, CoreJsonWebKeySet},
};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Mutex,
};

pub const PROVIDER: &str = "chatgpt";
pub const RESOURCE: &str = "https://api.openai.com/v1";
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub client_id: String,
    pub subject: String,
    pub email: Option<String>,
    pub host_id: String,
    pub access_token: Secret,
    pub refresh_token: Secret,
    pub id_token: Secret,
    pub scopes: Vec<String>,
    pub expires_at: u64,
}

impl Account {
    pub fn decode(secret: &Secret) -> Result<Self, String> {
        let account: Self = serde_json::from_str(&secret.0)
            .map_err(|_| "The ChatGPT account credential is invalid. Sign in from Lince again.")?;
        if account.client_id.is_empty()
            || account.client_id == "dynamic_agent_client"
            || account.subject.is_empty()
            || account.host_id.is_empty()
        {
            return Err("ChatGPT registration is incomplete. Continue with ChatGPT again.".into());
        }
        Ok(account)
    }
    pub fn encode(&self) -> Result<Secret, String> {
        serde_json::to_string(self)
            .map(Secret)
            .map_err(|_| "Cannot save the ChatGPT credential.".into())
    }
    pub fn enabled(&self) -> bool {
        self.scopes.iter().any(|s| s == "chatgpt.tokens.use.direct")
            && !self.access_token.0.is_empty()
    }
}

#[async_trait::async_trait]
pub trait CredentialStore: Send + Sync {
    async fn refresh_guard(&self) -> Result<Option<std::fs::File>, String> {
        Ok(None)
    }
    async fn read(&self) -> Result<Secret, String>;
    async fn replace(&self, previous: &Secret, next: Secret) -> Result<(), String>;
}

pub struct MemoryCredential(pub Mutex<Secret>);
#[async_trait::async_trait]
impl CredentialStore for MemoryCredential {
    async fn read(&self) -> Result<Secret, String> {
        Ok(self.0.lock().await.clone())
    }
    async fn replace(&self, previous: &Secret, next: Secret) -> Result<(), String> {
        let mut value = self.0.lock().await;
        if value.0 != previous.0 {
            return Err("The account changed during refresh. Retry the request.".into());
        }
        *value = next;
        Ok(())
    }
}

#[derive(Clone)]
pub struct Auth {
    pub(crate) http: reqwest::Client,
    issuer: String,
    authorize: String,
    token: String,
}

#[derive(Deserialize)]
struct Metadata {
    issuer: String,
    jwks_uri: String,
    revocation_endpoint: Option<String>,
}
#[derive(Deserialize)]
struct Tokens {
    access_token: Secret,
    #[serde(default)]
    refresh_token: Secret,
    #[serde(default)]
    id_token: Secret,
    expires_in: u64,
    #[serde(default)]
    scope: Option<String>,
    token_type: String,
}

pub fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|_| "Cannot initialize native provider communication.".into())
}

pub(crate) async fn json(response: reqwest::Response) -> Result<serde_json::Value, String> {
    use futures::StreamExt;
    let status = response.status();
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "The provider connection was interrupted.")?;
        if bytes.len() + chunk.len() > 1024 * 1024 {
            return Err("The provider response exceeds 1 MiB.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| format!("The provider returned invalid JSON (HTTP {status})."))?;
    if !status.is_success() {
        return Err(super::native::http_error(status, &value));
    }
    Ok(value)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl Auth {
    pub fn production() -> Result<Self, String> {
        Self::at("https://auth.openai.com")
    }
    pub fn at(issuer: &str) -> Result<Self, String> {
        let mut settings = crate::config::Settings {
            endpoint: issuer.into(),
            ..Default::default()
        };
        settings.validate()?;
        let issuer = settings.endpoint.trim_end_matches('/').to_string();
        Ok(Self {
            http: http_client()?,
            authorize: format!("{issuer}/api/accounts/authorize"),
            token: format!("{issuer}/api/accounts/oauth/token"),
            issuer,
        })
    }
    async fn metadata(&self) -> Result<Metadata, String> {
        let response = self
            .http
            .get(format!("{}/.well-known/openid-configuration", self.issuer))
            .send()
            .await
            .map_err(|_| "Cannot load OpenAI identity metadata.")?;
        let metadata: Metadata = serde_json::from_value(json(response).await?)
            .map_err(|_| "Invalid OpenAI identity metadata.")?;
        if metadata.issuer != self.issuer {
            return Err("The identity issuer does not match.".into());
        }
        for endpoint in
            std::iter::once(&metadata.jwks_uri).chain(metadata.revocation_endpoint.iter())
        {
            let url =
                url::Url::parse(endpoint).map_err(|_| "Invalid identity metadata endpoint.")?;
            let issuer = url::Url::parse(&self.issuer).map_err(|_| "Invalid identity issuer.")?;
            if url.origin() != issuer.origin()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err("Identity metadata points outside the trusted issuer.".into());
            }
        }
        Ok(metadata)
    }
    async fn identity(
        &self,
        token: &Secret,
        client: &str,
        nonce: Option<&str>,
    ) -> Result<(String, Option<String>), String> {
        let metadata = self.metadata().await?;
        let response = self
            .http
            .get(metadata.jwks_uri)
            .send()
            .await
            .map_err(|_| "Cannot load identity signature keys.")?;
        let keys: CoreJsonWebKeySet = serde_json::from_value(json(response).await?)
            .map_err(|_| "Invalid identity signature keys.")?;
        let token: CoreIdToken = token.0.parse().map_err(|_| "Invalid identity token.")?;
        let verifier = CoreIdTokenVerifier::new_public_client(
            ClientId::new(client.into()),
            IssuerUrl::new(self.issuer.clone()).map_err(|_| "Invalid identity issuer.")?,
            keys,
        );
        let claims = if let Some(nonce) = nonce {
            token.claims(&verifier, &Nonce::new(nonce.into()))
        } else {
            token.claims(&verifier, |_: Option<&Nonce>| Ok(()))
        }.map_err(|_| "The identity token failed signature, issuer, audience, expiry, or nonce validation.")?;
        Ok((
            claims.subject().as_str().into(),
            claims.email().map(|email| email.as_str().into()),
        ))
    }
    pub async fn begin(&self, host: String, existing: Option<Account>) -> Result<Login, String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| "Cannot open the local sign-in callback. Check local socket access.")?;
        let redirect = format!(
            "http://127.0.0.1:{}/auth/callback",
            listener
                .local_addr()
                .map_err(|_| "Cannot read callback port.")?
                .port()
        );
        let state = uuid::Uuid::new_v4().to_string();
        let nonce = uuid::Uuid::new_v4().to_string();
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let mut url = url::Url::parse(&self.authorize).map_err(|_| "Invalid authorization URL.")?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair(
                    "client_id",
                    existing
                        .as_ref()
                        .map(|a| a.client_id.as_str())
                        .unwrap_or("dynamic_agent_client"),
                )
                .append_pair("ext_agent_host_id", &host)
                .append_pair("response_type", "code")
                .append_pair("redirect_uri", &redirect)
                .append_pair("scope", SCOPES)
                .append_pair("resource", RESOURCE)
                .append_pair("state", &state)
                .append_pair("nonce", &nonce)
                .append_pair("code_challenge_method", "S256")
                .append_pair("code_challenge", challenge.as_str());
            if let Some(account) = &existing {
                if account.host_id != host {
                    return Err("This account belongs to another Lince installation. Register it here again.".into());
                }
                if !account.id_token.0.is_empty() {
                    query.append_pair("id_token_hint", &account.id_token.0);
                }
                if let Some(email) = &account.email {
                    query.append_pair("login_hint", email);
                }
            } else {
                query.append_pair("agent_name_hint", "Lince");
            }
        }
        Ok(Login {
            url: url.to_string(),
            listener,
            auth: self.clone(),
            host,
            existing,
            redirect,
            state,
            nonce,
            verifier,
        })
    }
    pub async fn refresh(&self, account: &Account) -> Result<Account, String> {
        if account.refresh_token.0.is_empty() {
            return Err("This ChatGPT session is signed out. Continue with ChatGPT again.".into());
        }
        let response = self
            .http
            .post(&self.token)
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", &account.client_id),
                ("refresh_token", &account.refresh_token.0),
                ("resource", RESOURCE),
            ])
            .send()
            .await
            .map_err(|_| "ChatGPT token renewal could not connect. Retry later.")?;
        let tokens: Tokens = serde_json::from_value(json(response).await?)
            .map_err(|_| "Invalid renewed credential.")?;
        if !tokens.token_type.eq_ignore_ascii_case("bearer") || tokens.access_token.0.is_empty() {
            return Err("Invalid renewed token type.".into());
        }
        let mut next = account.clone();
        if !tokens.id_token.0.is_empty() {
            let (subject, email) = self
                .identity(&tokens.id_token, &account.client_id, None)
                .await?;
            if subject != account.subject {
                return Err("Renewal changed account identity; sign in again.".into());
            }
            next.id_token = tokens.id_token;
            next.email = email;
        }
        next.access_token = tokens.access_token;
        if !tokens.refresh_token.0.is_empty() {
            next.refresh_token = tokens.refresh_token;
        }
        if let Some(scope) = tokens.scope {
            next.scopes = scope.split_whitespace().map(String::from).collect();
        }
        next.expires_at = now().saturating_add(tokens.expires_in);
        if !next.enabled() {
            return Err(
                "ChatGPT plan usage permission is missing. Continue with ChatGPT again.".into(),
            );
        }
        Ok(next)
    }
    pub async fn revoke(&self, account: &Account) -> Result<(), String> {
        if account.refresh_token.0.is_empty() {
            return Ok(());
        }
        let endpoint = self
            .metadata()
            .await?
            .revocation_endpoint
            .ok_or("The issuer did not advertise token revocation.")?;
        let response = self
            .http
            .post(endpoint)
            .form(&[
                ("token", account.refresh_token.0.as_str()),
                ("token_type_hint", "refresh_token"),
                ("client_id", account.client_id.as_str()),
            ])
            .send()
            .await
            .map_err(|_| "Remote revocation could not connect.")?;
        if !response.status().is_success() {
            return Err("Remote revocation was not confirmed.".into());
        }
        Ok(())
    }
}

pub struct Login {
    pub url: String,
    listener: TcpListener,
    auth: Auth,
    host: String,
    existing: Option<Account>,
    redirect: String,
    state: String,
    nonce: String,
    verifier: PkceCodeVerifier,
}

impl Login {
    fn callback(&self, target: &str) -> Result<(String, String), String> {
        let url = url::Url::parse(&format!("http://127.0.0.1{target}"))
            .map_err(|_| "Invalid login callback.")?;
        if url.path() != "/auth/callback" {
            return Err("Unexpected login callback path.".into());
        }
        let mut query = std::collections::HashMap::new();
        for (key, value) in url.query_pairs() {
            if query.insert(key.into_owned(), value.into_owned()).is_some() {
                return Err("Duplicate login callback field.".into());
            }
        }
        if query.get("state") != Some(&self.state) {
            return Err("Sign-in state did not match this attempt.".into());
        }
        if query.contains_key("error") {
            return Err("ChatGPT sign-in was declined or unavailable.".into());
        }
        let client = match (&self.existing, query.get("client_id")) {
            (Some(account), None) => account.client_id.clone(),
            (Some(account), Some(id)) if id == &account.client_id => id.clone(),
            (None, Some(id))
                if !id.is_empty() && id != "dynamic_agent_client" && id.len() <= 256 =>
            {
                id.clone()
            }
            _ => return Err("The issued registration ID is missing or changed.".into()),
        };
        let code = query
            .remove("code")
            .filter(|code| !code.is_empty())
            .ok_or("Sign-in did not return an authorization code.")?;
        Ok((client, code))
    }
    pub async fn finish(self) -> Result<Secret, String> {
        tokio::time::timeout(Duration::from_secs(600), self.finish_inner())
            .await
            .map_err(|_| "ChatGPT browser sign-in timed out. Try again.")?
    }
    async fn finish_inner(self) -> Result<Secret, String> {
        let (client, code) = loop {
            let (mut socket, peer) = self
                .listener
                .accept()
                .await
                .map_err(|_| "Local login callback stopped.")?;
            if !peer.ip().is_loopback() {
                continue;
            }
            let mut bytes = Vec::new();
            let request = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let mut chunk = [0; 1024];
                    let count = socket
                        .read(&mut chunk)
                        .await
                        .map_err(|_| "Cannot read login callback.")?;
                    if count == 0 {
                        return Err("Incomplete login callback.");
                    }
                    bytes.extend_from_slice(&chunk[..count]);
                    if bytes.len() > 16384 {
                        return Err("Login callback exceeds its size limit.");
                    }
                    if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                        return Ok(());
                    }
                }
            })
            .await;
            if !matches!(request, Ok(Ok(()))) {
                continue;
            }
            let request = String::from_utf8(bytes).map_err(|_| "Invalid callback encoding.")?;
            let target = request
                .lines()
                .next()
                .and_then(|line| line.strip_prefix("GET "))
                .and_then(|s| s.split_whitespace().next())
                .unwrap_or("");
            if !target.starts_with("/auth/callback?") {
                let _ = socket
                    .write_all(
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await;
                continue;
            }
            let valid_state = url::Url::parse(&format!("http://127.0.0.1{target}"))
                .ok()
                .is_some_and(|url| {
                    let states: Vec<_> = url
                        .query_pairs()
                        .filter(|(key, _)| key == "state")
                        .map(|(_, value)| value.into_owned())
                        .collect();
                    states.len() == 1 && states[0] == self.state
                });
            if !valid_state {
                let _ = socket
                    .write_all(
                        b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await;
                continue;
            }
            let result = self.callback(target);
            let body = if result.is_ok() {
                "Lince received the callback. Return to Lince to see the sign-in result."
            } else {
                "Lince rejected this sign-in callback. Return to Lince and retry."
            };
            let _ = socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await;
            break result?;
        };
        let response = self
            .auth
            .http
            .post(&self.auth.token)
            .form(&[
                ("grant_type", "authorization_code"),
                ("client_id", &client),
                ("code", &code),
                ("code_verifier", self.verifier.secret()),
                ("redirect_uri", &self.redirect),
                ("resource", RESOURCE),
            ])
            .send()
            .await
            .map_err(|_| "Cannot exchange the ChatGPT authorization code. Start a new sign-in.")?;
        let tokens: Tokens = serde_json::from_value(json(response).await?)
            .map_err(|_| "The token endpoint returned invalid credentials.")?;
        if !tokens.token_type.eq_ignore_ascii_case("bearer") || tokens.access_token.0.is_empty() {
            return Err("Invalid authorization token type.".into());
        }
        let (subject, email) = self
            .auth
            .identity(&tokens.id_token, &client, Some(&self.nonce))
            .await?;
        if self.existing.as_ref().is_some_and(|a| a.subject != subject) {
            return Err("Sign-in returned another account. Add it separately instead.".into());
        }
        let account = Account {
            client_id: client,
            subject,
            email,
            host_id: self.host,
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            id_token: tokens.id_token,
            scopes: tokens
                .scope
                .unwrap_or_default()
                .split_whitespace()
                .map(String::from)
                .collect(),
            expires_at: now().saturating_add(tokens.expires_in),
        };
        account.encode()
    }
}

pub fn host_id(directory: &Path) -> Result<String, String> {
    let path = directory.join("native-host-id");
    match std::fs::read_to_string(&path) {
        Ok(value)
            if value.starts_with("urn:uuid:")
                && uuid::Uuid::parse_str(value.trim_start_matches("urn:uuid:")).is_ok() =>
        {
            return Ok(value);
        }
        Ok(_) => return Err("The native host identifier is damaged.".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("Cannot read the native host identifier.".into()),
    }
    let value = format!("urn:uuid:{}", uuid::Uuid::new_v4());
    let mut file = tempfile::NamedTempFile::new_in(directory)
        .map_err(|_| "Cannot save the native host identifier.")?;
    use std::io::Write;
    file.write_all(value.as_bytes())
        .and_then(|_| file.as_file().sync_all())
        .map_err(|_| "Cannot save the native host identifier.")?;
    match file.persist_noclobber(&path) {
        Ok(_) => Ok(value),
        Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => host_id(directory),
        Err(_) => Err("Cannot save the native host identifier.".into()),
    }
}

#[derive(Clone)]
pub struct Session {
    pub auth: Auth,
    pub store: Arc<dyn CredentialStore>,
    pub lock: Arc<Mutex<()>>,
}
impl Session {
    pub async fn token(&self) -> Result<Secret, String> {
        let _guard = self.lock.lock().await;
        let _process_guard = self.store.refresh_guard().await?;
        let previous = self.store.read().await?;
        let mut account = Account::decode(&previous)?;
        if !account.enabled() {
            return Err("Continue with ChatGPT to enable this account's plan usage.".into());
        }
        if account.expires_at <= now().saturating_add(60) {
            account = self.auth.refresh(&account).await?;
            self.store.replace(&previous, account.encode()?).await?;
        }
        Ok(account.access_token.clone())
    }
}

#[cfg(test)]
mod tests;
