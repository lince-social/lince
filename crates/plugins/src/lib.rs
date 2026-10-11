use nucleus::plugins::{
    Contribution, MAX_BYTES, Manifest, PROTOCOL, Package, RpcRequest, RpcResponse,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout},
    sync::Semaphore,
};

struct Installed {
    manifest: Manifest,
    digest: String,
    executable: PathBuf,
    state: PathBuf,
    manifest_path: PathBuf,
}

pub struct Host {
    installed: BTreeMap<String, Installed>,
    pub diagnostics: Vec<String>,
    capacity: Arc<Semaphore>,
}

fn package_digest(path: &Path, manifest: &Path) -> Result<String, String> {
    let bytes = manifest_bytes(manifest)?;
    if bytes.len() > 256 * 1024 {
        return Err("Plugin manifest is too large".into());
    }
    let mut hash = Sha256::new();
    hash.update(bytes);
    let file = std::fs::File::open(path).map_err(|_| "Cannot read plugin executable")?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
        return Err("Plugin executable exceeds 64 MiB".into());
    }
    let mut file = file.take(64 * 1024 * 1024 + 1);
    let mut buffer = [0u8; 65536];
    let mut total = 0;
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        total += count;
        if total > 64 * 1024 * 1024 {
            return Err("Plugin executable exceeds 64 MiB".into());
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn manifest_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > 256 * 1024 {
        return Err("Plugin manifest must be a regular file of at most 256 KiB".into());
    }
    let file = std::fs::File::open(path).map_err(|_| "Cannot read plugin manifest")?;
    let mut bytes = Vec::new();
    file.take(256 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 256 * 1024 {
        return Err("Plugin manifest is too large".into());
    }
    Ok(bytes)
}

impl Host {
    pub async fn discover(directory: PathBuf) -> Result<Self, String> {
        tokio::task::spawn_blocking(move || Self::load(&directory))
            .await
            .map_err(|e| e.to_string())?
    }

    fn load(directory: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let mut host = Self {
            installed: BTreeMap::new(),
            diagnostics: Vec::new(),
            capacity: Arc::new(Semaphore::new(4)),
        };
        let mut paths: Vec<_> = std::fs::read_dir(directory)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .is_some_and(|name| !name.to_string_lossy().starts_with('.'))
            })
            .take(129)
            .collect();
        paths.sort();
        if paths.len() > 128 {
            return Err("At most 128 plugin packages may be installed".into());
        }
        let mut duplicates = std::collections::BTreeSet::new();
        for path in paths {
            if !path.is_dir() {
                continue;
            }
            let manifest_path = path.join("plugin.json");
            let result = (|| {
                if std::fs::metadata(&manifest_path)
                    .map_err(|_| "Missing plugin.json")?
                    .len()
                    > 256 * 1024
                {
                    return Err("Plugin manifest is too large".to_string());
                }
                let manifest: Manifest = serde_json::from_slice(&manifest_bytes(&manifest_path)?)
                    .map_err(|e| e.to_string())?;
                manifest.validate()?;
                let relative = Path::new(&manifest.executable);
                if relative
                    .components()
                    .any(|part| !matches!(part, Component::Normal(_)))
                {
                    return Err(
                        "Plugin executable must be a relative path inside its package".into(),
                    );
                }
                let canonical = path.canonicalize().map_err(|e| e.to_string())?;
                let executable = path
                    .join(relative)
                    .canonicalize()
                    .map_err(|_| "Plugin executable is missing")?;
                if !executable.starts_with(&canonical) || !executable.is_file() {
                    return Err("Plugin executable escapes its package".into());
                }
                let digest = package_digest(&executable, &manifest_path)?;
                let state = directory.join(".state").join(&manifest.id);
                Ok(Installed {
                    manifest,
                    executable,
                    digest,
                    state,
                    manifest_path,
                })
            })();
            match result {
                Ok(installed) => {
                    let id = installed.manifest.id.clone();
                    if host.installed.contains_key(&id) || duplicates.contains(&id) {
                        host.installed.remove(&id);
                        duplicates.insert(id.clone());
                        host.diagnostics.push(format!(
                            "Duplicate plugin ID {id}; all copies are unavailable"
                        ));
                    } else {
                        host.installed.insert(id, installed);
                    }
                }
                Err(error) => host.diagnostics.push(format!(
                    "{}: {error}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                )),
            }
        }
        let mut namespaces: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for installed in host.installed.values() {
            for contribution in &installed.manifest.contributions {
                if let Contribution::Schema { namespace, .. } = contribution {
                    namespaces
                        .entry(namespace.clone())
                        .or_default()
                        .push(installed.manifest.id.clone());
                }
            }
        }
        for (namespace, owners) in namespaces {
            if owners.len() > 1 {
                for owner in &owners {
                    host.installed.remove(owner);
                }
                host.diagnostics.push(format!("Conflicting schema namespace {namespace}; affected packages are unavailable: {}",owners.join(", ")));
            }
        }
        Ok(host)
    }

    pub fn packages(&self) -> Vec<Package> {
        self.installed
            .values()
            .map(|installed| Package {
                manifest: installed.manifest.clone(),
                digest: installed.digest.clone(),
                enabled: false,
            })
            .collect()
    }

    pub fn contribution(&self, qualified: &str) -> Result<(&Manifest, &Contribution), String> {
        let (id, name) = qualified
            .split_once('/')
            .ok_or("Choose a plugin contribution")?;
        let installed = self
            .installed
            .get(id)
            .ok_or("Plugin is unavailable; install it and restart Lince")?;
        let contribution = installed
            .manifest
            .contributions
            .iter()
            .find(|c| c.id() == name)
            .ok_or("Plugin contribution is unavailable")?;
        Ok((&installed.manifest, contribution))
    }

    pub async fn call(
        &self,
        plugin: &str,
        approved_digest: &str,
        contribution: &str,
        operation: &str,
        input: Value,
    ) -> Result<Value, String> {
        let installed = self.installed.get(plugin).ok_or("Plugin is unavailable")?;
        if approved_digest != installed.digest {
            return Err("Plugin changed; review and enable this version".into());
        }
        if contribution != ""
            && !installed
                .manifest
                .contributions
                .iter()
                .any(|c| c.id() == contribution)
        {
            return Err("Unknown plugin contribution".into());
        }
        let permit = self
            .capacity
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| "Plugin host is stopped")?;
        let path = installed.executable.clone();
        let manifest_path = installed.manifest_path.clone();
        let current = tokio::task::spawn_blocking(move || package_digest(&path, &manifest_path))
            .await
            .map_err(|e| e.to_string())??;
        if current != approved_digest {
            return Err("Plugin files changed; restart and review this version".into());
        }
        tokio::fs::create_dir_all(&installed.state)
            .await
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&installed.state, std::fs::Permissions::from_mode(0o700))
                .await
                .map_err(|e| e.to_string())?;
        }
        let mut command = tokio::process::Command::new(&installed.executable);
        command
            .args(&installed.manifest.arguments)
            .env_clear()
            .env("LINCE_PLUGIN_STATE", &installed.state)
            .current_dir(&installed.state)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        if let Some(root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", root);
        }
        let child = command
            .spawn()
            .map_err(|_| "Cannot start installed plugin")?;
        let mut connection = Connection::new(child)?;
        let result = tokio::time::timeout(Duration::from_secs(45), async {
            let initialized = connection
                .request(1, "", "initialize", json!({"protocol":PROTOCOL}))
                .await?;
            if initialized["protocol"] != PROTOCOL {
                return Err("Plugin uses an unsupported protocol".into());
            }
            connection.request(2, contribution, operation, input).await
        })
        .await
        .map_err(|_| "Plugin operation timed out; inspect delivery before retrying".to_string())?;
        drop(permit);
        result
    }
}

struct Connection {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Connection {
    fn new(mut child: Child) -> Result<Self, String> {
        let input = child.stdin.take().ok_or("Plugin input is unavailable")?;
        let output = BufReader::new(child.stdout.take().ok_or("Plugin output is unavailable")?);
        Ok(Self {
            child,
            input,
            output,
        })
    }

    async fn request(
        &mut self,
        id: u64,
        contribution: &str,
        operation: &str,
        input: Value,
    ) -> Result<Value, String> {
        let request = RpcRequest {
            protocol: PROTOCOL.into(),
            id,
            contribution: contribution.into(),
            operation: operation.into(),
            input,
        };
        let mut bytes = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
        if bytes.len() >= MAX_BYTES {
            return Err("Plugin request is too large".into());
        }
        bytes.push(b'\n');
        self.input
            .write_all(&bytes)
            .await
            .map_err(|_| "Plugin connection closed")?;
        self.input
            .flush()
            .await
            .map_err(|_| "Plugin connection closed")?;
        let bytes = read_frame(&mut self.output).await?;
        let response: RpcResponse =
            serde_json::from_slice(&bytes).map_err(|_| "Invalid plugin response")?;
        if response.id != id
            || response.protocol != PROTOCOL
            || (response.result.is_some() == response.error.is_some())
        {
            return Err("Plugin response does not match its request".into());
        }
        match response.error {
            Some(error) => Err(error
                .chars()
                .filter(|c| !c.is_control())
                .take(1024)
                .collect()),
            None => response.result.ok_or("Missing plugin result".into()),
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

pub async fn read_frame(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    loop {
        let available = reader
            .fill_buf()
            .await
            .map_err(|_| "Plugin connection closed")?;
        if available.is_empty() {
            return Err("Plugin connection closed".into());
        }
        let end = available.iter().position(|b| *b == b'\n').map(|n| n + 1);
        let count = end.unwrap_or(available.len());
        if bytes.len() + count > MAX_BYTES {
            return Err("Plugin response exceeds its size limit".into());
        }
        bytes.extend_from_slice(&available[..count]);
        reader.consume(count);
        if end.is_some() {
            return Ok(bytes);
        }
    }
}
