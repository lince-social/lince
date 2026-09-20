use crate::{
    adapters::Descriptor,
    config::{Secret, Settings},
    provider::{Message, Provider, Reply, ToolDefinition},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout},
    sync::Mutex,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Driver {
    pub executable: PathBuf,
    #[serde(default)]
    pub arguments: Vec<String>,
}

pub struct Connection {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    _directory: tempfile::TempDir,
    next: u64,
}

impl Driver {
    pub async fn connect(&self) -> Result<Connection, String> {
        if !self.executable.is_absolute() || !self.executable.is_file() || self.arguments.len() > 64
        {
            return Err(
                "Provider adapters require an installed executable with an absolute path.".into(),
            );
        }
        let directory = if cfg!(target_os = "linux") {
            tempfile::tempdir_in("/dev/shm")
        } else {
            tempfile::tempdir()
        }
        .map_err(|_| "Cannot create a private provider connection directory.")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        let mut command = tokio::process::Command::new(&self.executable);
        command
            .args(&self.arguments)
            .env_clear()
            .env("LINCE_PROVIDER_STATE", directory.path())
            .current_dir(directory.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        if let Some(path) = std::env::var_os("PATH") {
            command.env("PATH", path);
        }
        #[cfg(windows)]
        if let Some(root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", root);
        }
        let mut child = command
            .spawn()
            .map_err(|_| "Cannot start the installed provider adapter.")?;
        let input = child.stdin.take().ok_or("Provider input is unavailable.")?;
        let output = BufReader::new(
            child
                .stdout
                .take()
                .ok_or("Provider output is unavailable.")?,
        );
        let mut connection = Connection {
            child,
            input,
            output,
            _directory: directory,
            next: 0,
        };
        let result = connection
            .request("initialize", json!({"protocol":"lince.provider.v1"}))
            .await?;
        if result["protocol"] != "lince.provider.v1" {
            return Err("The provider adapter uses an unsupported protocol.".into());
        }
        Ok(connection)
    }

    pub async fn discover(&self) -> Result<Vec<Descriptor>, String> {
        let mut connection = self.connect().await?;
        let descriptors = connection.request("providers/list", json!({})).await?;
        let result: Vec<Descriptor> =
            serde_json::from_value(descriptors).map_err(|_| "Invalid provider metadata.")?;
        if result.len() > 64 {
            return Err("Too many providers from one adapter.".into());
        }
        Ok(result)
    }
}

impl Connection {
    async fn read(&mut self) -> Result<Value, String> {
        let mut line = Vec::new();
        loop {
            let available = self
                .output
                .fill_buf()
                .await
                .map_err(|_| "Provider connection closed.")?;
            if available.is_empty() {
                return Err("Provider connection closed.".into());
            }
            let end = available.iter().position(|b| *b == b'\n').map(|n| n + 1);
            let count = end.unwrap_or(available.len());
            if line.len() + count > 4 * 1024 * 1024 {
                return Err("Provider response is too large.".into());
            }
            line.extend_from_slice(&available[..count]);
            self.output.consume(count);
            if end.is_some() {
                break;
            }
        }
        serde_json::from_slice(&line).map_err(|_| "Invalid provider response.".into())
    }

    pub async fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next += 1;
        let id = self.next;
        let mut bytes =
            serde_json::to_vec(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
                .map_err(|_| "Invalid provider request.")?;
        bytes.push(b'\n');
        tokio::time::timeout(Duration::from_secs(120), async {
            self.input.write_all(&bytes).await.map_err(|_| "Provider connection closed.")?;
            self.input.flush().await.map_err(|_| "Provider connection closed.")?;
            let result = self.read().await?;
            if result["id"] != id || result.get("error").is_some() || result.get("result").is_none() { return Err(format!("Provider adapter rejected {method}. Check its configuration and account access.")); }
            Ok(result["result"].clone())
        }).await.map_err(|_| "The provider adapter timed out.".to_string())?
    }

    pub async fn login(&mut self, settings: &Settings) -> Result<String, String> {
        let result = self
            .request("login/start", json!({"settings":settings}))
            .await?;
        let url = result["url"]
            .as_str()
            .ok_or("The provider did not return a browser login address.")?;
        let parsed = url::Url::parse(url).map_err(|_| "Invalid provider login URL.")?;
        if parsed.scheme() != "https"
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(
                "The provider login address must use HTTPS without embedded credentials.".into(),
            );
        }
        Ok(url.into())
    }

    pub async fn finish_login(mut self) -> Result<Secret, String> {
        tokio::time::timeout(Duration::from_secs(300), async {
            loop {
                let result = self.request("login/poll", json!({})).await?;
                match result["state"].as_str() {
                    Some("pending") => tokio::time::sleep(Duration::from_secs(1)).await,
                    Some("complete") => {
                        let credential: Secret =
                            serde_json::from_value(result["credential"].clone())
                                .map_err(|_| "The provider did not return a credential.")?;
                        if credential.0.is_empty() || credential.0.len() > 65_536 {
                            return Err("Invalid provider credential.".into());
                        }
                        return Ok(credential);
                    }
                    _ => {
                        return Err(
                            "Browser login failed or was cancelled. Use /login to retry.".into(),
                        );
                    }
                }
            }
        })
        .await
        .map_err(|_| "Browser login expired. Use /login to retry.".to_string())?
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

pub struct DriverProvider {
    driver: Driver,
    settings: Settings,
    credential: Mutex<Secret>,
}

impl DriverProvider {
    pub fn new(driver: Driver, settings: Settings, credential: Secret) -> Self {
        Self {
            driver,
            settings,
            credential: Mutex::new(credential),
        }
    }
    pub async fn credential(&self) -> Secret {
        self.credential.lock().await.clone()
    }
}

#[async_trait::async_trait]
impl Provider for DriverProvider {
    async fn complete(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<Reply, String> {
        let mut credential = self.credential.lock().await;
        let mut connection = self.driver.connect().await?;
        let result = connection.request("provider/complete", json!({"settings":self.settings,"credential":*credential,"system":system,"messages":messages,"tools":tools})).await?;
        if let Some(refreshed) = result.get("credential") {
            let refreshed: Secret = serde_json::from_value(refreshed.clone())
                .map_err(|_| "Invalid refreshed credential.")?;
            if refreshed.0.is_empty() || refreshed.0.len() > 65_536 {
                return Err("Invalid refreshed credential.".into());
            }
            *credential = refreshed;
        }
        if result.get("error").is_some() {
            return Err("The provider request did not complete. Check account access and availability.".into());
        }
        serde_json::from_value(result["reply"].clone())
            .map_err(|_| "Invalid provider completion.".into())
    }
}
