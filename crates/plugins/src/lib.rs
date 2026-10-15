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
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout},
    sync::{Mutex, Semaphore},
};
use tokio_util::sync::CancellationToken;

mod packages;
pub use packages::install_current;

pub async fn serve<H, F>(mut handle: H) -> Result<(), String>
where
    H: FnMut(RpcRequest) -> F,
    F: std::future::Future<Output = Result<Value, String>>,
{
    let mut reader = BufReader::new(tokio::io::stdin());
    let mut output = tokio::io::stdout();
    loop {
        let frame = match read_frame(&mut reader).await {
            Ok(frame) => frame,
            Err(error) if error == "Plugin connection closed" => return Ok(()),
            Err(error) => return Err(error),
        };
        let request: RpcRequest =
            serde_json::from_slice(&frame).map_err(|_| "Invalid plugin request")?;
        let id = request.id;
        let result = if request.protocol != PROTOCOL {
            Err("Unsupported plugin protocol".into())
        } else if request.operation == "initialize" {
            Ok(json!({"protocol": PROTOCOL}))
        } else {
            handle(request).await
        };
        let response = match result {
            Ok(result) => RpcResponse {
                protocol: PROTOCOL.into(),
                id,
                result: Some(result),
                error: None,
            },
            Err(error) => RpcResponse {
                protocol: PROTOCOL.into(),
                id,
                result: None,
                error: Some(error),
            },
        };
        let mut bytes = serde_json::to_vec(&response).map_err(|error| error.to_string())?;
        if bytes.len() >= MAX_BYTES {
            return Err("Plugin response is too large".into());
        }
        bytes.push(b'\n');
        output
            .write_all(&bytes)
            .await
            .map_err(|_| "Plugin connection closed")?;
        output
            .flush()
            .await
            .map_err(|_| "Plugin connection closed")?;
    }
}

struct Installed {
    manifest: Manifest,
    digest: String,
    executable: PathBuf,
    state: PathBuf,
    manifest_path: PathBuf,
}

struct Snapshot {
    installed: BTreeMap<String, Arc<Installed>>,
    diagnostics: Vec<String>,
}

struct Session {
    connection: Mutex<Option<Connection>>,
    cancelled: CancellationToken,
}

pub struct Host {
    directory: PathBuf,
    snapshot: RwLock<Snapshot>,
    sessions: std::sync::Mutex<BTreeMap<String, Arc<Session>>>,
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
        let directory = directory
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let directory = directory.as_path();
        let mut host = Snapshot {
            installed: BTreeMap::new(),
            diagnostics: Vec::new(),
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
                        host.installed.insert(id, Arc::new(installed));
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
        let mut remaining = std::mem::take(&mut host.installed);
        for (id, installed) in &remaining {
            let compatibility = &installed.manifest.compatibility;
            if !compatibility.operating_systems.is_empty()
                && !compatibility
                    .operating_systems
                    .iter()
                    .any(|os| os == std::env::consts::OS)
            {
                host.diagnostics.push(format!(
                    "{id}: unsupported operating system {}",
                    std::env::consts::OS
                ));
            } else if let Some(interface) = compatibility
                .interfaces
                .iter()
                .find(|interface| interface.as_str() != PROTOCOL)
            {
                host.diagnostics
                    .push(format!("{id}: unsupported Lince interface {interface}"));
            } else {
                continue;
            }
            duplicates.insert(id.clone());
        }
        remaining.retain(|id, _| !duplicates.contains(id));
        loop {
            let ready: Vec<_> = remaining
                .iter()
                .filter(|(_, package)| {
                    package
                        .manifest
                        .compatibility
                        .dependencies
                        .iter()
                        .all(|(id, version)| {
                            host.installed
                                .get(id)
                                .is_some_and(|dependency| dependency.manifest.version >= *version)
                        })
                })
                .map(|(id, _)| id.clone())
                .collect();
            if ready.is_empty() {
                break;
            }
            for id in ready {
                host.installed
                    .insert(id.clone(), remaining.remove(&id).unwrap());
            }
        }
        for (id, package) in remaining {
            host.diagnostics.push(format!(
                "{id}: missing, incompatible or cyclic package dependencies: {:?}",
                package.manifest.compatibility.dependencies
            ));
        }
        Ok(Self {
            directory: directory.to_path_buf(),
            snapshot: RwLock::new(host),
            sessions: Default::default(),
            capacity: Arc::new(Semaphore::new(4)),
        })
    }

    pub fn diagnostics(&self) -> Vec<String> {
        self.snapshot.read().unwrap().diagnostics.clone()
    }

    pub async fn reload(&self) -> Result<(), String> {
        let fresh = Self::discover(self.directory.clone()).await?;
        self.stop_all();
        *self.snapshot.write().unwrap() = fresh.snapshot.into_inner().unwrap();
        Ok(())
    }

    pub fn stop(&self, plugin: &str) {
        if let Some(session) = self.sessions.lock().unwrap().remove(plugin) {
            session.cancelled.cancel();
        }
    }

    pub fn stop_all(&self) {
        for (_, session) in std::mem::take(&mut *self.sessions.lock().unwrap()) {
            session.cancelled.cancel();
        }
    }

    pub fn packages(&self) -> Vec<Package> {
        self.snapshot
            .read()
            .unwrap()
            .installed
            .values()
            .map(|installed| Package {
                manifest: installed.manifest.clone(),
                digest: installed.digest.clone(),
                enabled: false,
            })
            .collect()
    }

    pub fn contribution(&self, qualified: &str) -> Result<(Manifest, Contribution), String> {
        let (id, name) = qualified
            .split_once('/')
            .ok_or("Choose a plugin contribution")?;
        let snapshot = self.snapshot.read().unwrap();
        let installed = snapshot
            .installed
            .get(id)
            .ok_or("Plugin is unavailable; install it and reload packages")?;
        let contribution = installed
            .manifest
            .contributions
            .iter()
            .find(|c| c.id() == name)
            .ok_or("Plugin contribution is unavailable")?;
        Ok((installed.manifest.clone(), contribution.clone()))
    }

    pub async fn call(
        &self,
        plugin: &str,
        approved_digest: &str,
        contribution: &str,
        operation: &str,
        input: Value,
    ) -> Result<Value, String> {
        let installed = self
            .snapshot
            .read()
            .unwrap()
            .installed
            .get(plugin)
            .cloned()
            .ok_or("Plugin is unavailable")?;
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
        let session = self
            .sessions
            .lock()
            .unwrap()
            .entry(plugin.into())
            .or_insert_with(|| {
                Arc::new(Session {
                    connection: Mutex::new(None),
                    cancelled: CancellationToken::new(),
                })
            })
            .clone();
        let _permit = self
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
            self.stop(plugin);
            return Err("Plugin files changed; restart and review this version".into());
        }
        let mut connection = session.connection.lock().await;
        if session.cancelled.is_cancelled() {
            return Err("Plugin connection stopped".into());
        }
        let result = tokio::select! {
            _ = session.cancelled.cancelled() => Err("Plugin connection stopped; inspect delivery before retrying".into()),
            result = tokio::time::timeout(Duration::from_secs(45), async {
        if connection.is_none() {
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
        let mut opened = Connection::new(child)?;
            let initialized = opened
                .request(1, "", "initialize", json!({"protocol":PROTOCOL}))
                .await?;
            if initialized["protocol"] != PROTOCOL {
                return Err("Plugin uses an unsupported protocol".into());
            }
            *connection = Some(opened);
        }
            connection.as_mut().unwrap().request(2, contribution, operation, input).await
        })
        => result.unwrap_or_else(|_| Err("Plugin operation timed out; inspect delivery before retrying".into())),
        };
        if result.is_err()
            && (session.cancelled.is_cancelled()
                || connection
                    .as_ref()
                    .is_none_or(|connection| !connection.healthy))
        {
            *connection = None;
        }
        result
    }
}

struct Connection {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    healthy: bool,
}

impl Connection {
    fn new(mut child: Child) -> Result<Self, String> {
        let input = child.stdin.take().ok_or("Plugin input is unavailable")?;
        let output = BufReader::new(child.stdout.take().ok_or("Plugin output is unavailable")?);
        Ok(Self {
            child,
            input,
            output,
            healthy: true,
        })
    }

    async fn request(
        &mut self,
        id: u64,
        contribution: &str,
        operation: &str,
        input: Value,
    ) -> Result<Value, String> {
        self.healthy = false;
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
        self.healthy = true;
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
