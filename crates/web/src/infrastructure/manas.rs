use {
    reqwest::{Client, Method, Response},
    serde::Deserialize,
    serde_json::Value,
};

#[derive(Clone)]
pub struct ManasGateway {
    http: Client,
}

#[derive(Debug, Deserialize)]
struct LoginResponse {
    token: String,
}

impl ManasGateway {
    pub fn new() -> Result<Self, reqwest::Error> {
        let http = Client::builder().user_agent("lince-web/0.1").build()?;
        Ok(Self { http })
    }

    pub async fn login_with_credentials(
        &self,
        base_url: &str,
        username: &str,
        password: &str,
    ) -> Result<String, String> {
        let login_url = format!("{}/api/auth/login", normalize_base_url(base_url));
        let response = self
            .http
            .post(login_url)
            .json(&serde_json::json!({
                "username": username,
                "password": password,
            }))
            .send()
            .await
            .map_err(|error| {
                tracing::warn!("manas login request failed: {error}");
                "Nao foi possivel autenticar no servidor externo.".to_string()
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::warn!("manas login rejected with {status}: {body}");
            return Err("Login ou senha invalidos no servidor externo.".into());
        }

        let payload = response.json::<LoginResponse>().await.map_err(|error| {
            tracing::warn!("manas login payload invalid: {error}");
            "Resposta invalida ao autenticar no servidor externo.".to_string()
        })?;

        Ok(payload.token)
    }

    pub async fn open_view_stream(
        &self,
        base_url: &str,
        bearer_token: &str,
        view_id: u64,
    ) -> Result<reqwest::Response, String> {
        let url = format!("{}/api/sse/view/{view_id}", normalize_base_url(base_url));
        let response = self
            .http
            .get(url)
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .bearer_auth(bearer_token)
            .send()
            .await
            .map_err(|error| {
                tracing::warn!("manas view request failed: {error}");
                "Nao foi possivel abrir o stream remoto da view.".to_string()
            })?;

        Ok(response)
    }

    pub async fn send_table_request(
        &self,
        base_url: &str,
        bearer_token: &str,
        method: Method,
        table_name: &str,
        id: Option<i64>,
        body: Option<Value>,
    ) -> Result<Response, String> {
        let mut path = format!("/api/table/{table_name}");
        if let Some(id) = id {
            path.push('/');
            path.push_str(&id.to_string());
        }

        self.send_backend_request(base_url, bearer_token, method, &path, body)
            .await
    }

    pub async fn send_backend_request(
        &self,
        base_url: &str,
        bearer_token: &str,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Response, String> {
        let url = if path.starts_with("http://") || path.starts_with("https://") {
            path.to_string()
        } else {
            format!("{}{}", normalize_base_url(base_url), path)
        };
        let mut request = self
            .http
            .request(method.clone(), url)
            .bearer_auth(bearer_token);

        if let Some(body) = body {
            request = request.json(&body);
        }

        request.send().await.map_err(|error| {
            tracing::warn!("manas backend request failed ({method} {path}): {error}");
            "Nao foi possivel falar com o servidor externo.".to_string()
        })
    }
}

fn normalize_base_url(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_string()
}
