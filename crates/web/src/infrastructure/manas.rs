use {reqwest::Client, serde::Deserialize, serde_json::Value};

const DEFAULT_LINCE_API_BASE_URL: &str = "http://127.0.0.1:6174/consult";

#[derive(Clone)]
pub struct ManasGateway {
    http: Client,
    api_base_url: String,
}

#[derive(Debug, Deserialize)]
struct LoginResponse {
    token: String,
}

impl ManasGateway {
    pub fn new() -> Result<Self, reqwest::Error> {
        let http = Client::builder().user_agent("lince-web/0.1").build()?;
        let api_base_url = std::env::var("LINCE_API_BASE_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_LINCE_API_BASE_URL.to_string());

        Ok(Self {
            http,
            api_base_url: api_base_url.trim_end_matches('/').to_string(),
        })
    }

    pub async fn login_with_credentials(
        &self,
        username: &str,
        password: &str,
    ) -> Result<String, String> {
        let login_url = format!("{}/api/auth/login", self.api_base_url);
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
        bearer_token: &str,
        view_id: u64,
    ) -> Result<reqwest::Response, String> {
        let url = format!("{}/api/sse/view/{view_id}", self.api_base_url);
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

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::warn!("manas view stream rejected with {status}: {body}");
            return Err(format!(
                "Stream remoto recusou a conexao com status {status}."
            ));
        }

        Ok(response)
    }

    pub async fn execute_sql(
        &self,
        bearer_token: &str,
        sql: &str,
    ) -> Result<(reqwest::StatusCode, Value), String> {
        let sql_url = format!("{}/api/sql", self.api_base_url);
        let response = self
            .http
            .post(sql_url)
            .bearer_auth(bearer_token)
            .json(&serde_json::json!({ "sql": sql }))
            .send()
            .await
            .map_err(|error| {
                tracing::warn!("manas sql request failed: {error}");
                "Nao foi possivel executar o SQL remoto.".to_string()
            })?;

        let status = response.status();
        let body = response.text().await.map_err(|error| {
            tracing::warn!("manas sql response read failed: {error}");
            "Nao foi possivel ler a resposta do SQL remoto.".to_string()
        })?;

        let body_json = serde_json::from_str::<Value>(&body).map_err(|error| {
            tracing::warn!("manas sql invalid json: {error}; body={body}");
            "O endpoint SQL remoto retornou JSON invalido.".to_string()
        })?;

        Ok((status, body_json))
    }
}
