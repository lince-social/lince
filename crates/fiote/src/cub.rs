use {
    crate::credential::{CredentialSource, ProviderCredential},
    serde::{Deserialize, Serialize},
    std::{
        collections::VecDeque,
        path::PathBuf,
        process::Stdio,
        sync::{
            Arc, Mutex,
            atomic::{AtomicU64, Ordering},
        },
    },
    tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        process::{Child, ChildStdin, Command},
        sync::{Mutex as AsyncMutex, broadcast, oneshot, watch},
    },
};

pub const BACKLOG_LINES: usize = 2000;
const BROADCAST_CAPACITY: usize = 512;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CubSpec {
    pub name: String,
    pub cwd: PathBuf,
    pub session_dir: Option<PathBuf>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub tools: Option<Vec<String>>,
    pub skills: Vec<PathBuf>,
    pub extensions: Vec<PathBuf>,
    pub context_files: bool,
    pub persist_session: bool,
}

impl CubSpec {
    pub fn new(name: impl Into<String>, cwd: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            cwd: cwd.into(),
            session_dir: None,
            provider: None,
            model: None,
            thinking: None,
            tools: None,
            skills: Vec::new(),
            extensions: Vec::new(),
            context_files: true,
            persist_session: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Rpc,
    Stderr,
    Supervisor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CubEvent {
    pub cub: String,
    pub seq: u64,
    pub channel: Channel,
    pub line: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CubStatus {
    pub id: String,
    pub name: String,
    pub cwd: PathBuf,
    pub credential_source: Option<CredentialSource>,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub events: u64,
}

pub struct Cub {
    pub id: String,
    pub spec: CubSpec,
    credential_source: Option<CredentialSource>,
    stdin: AsyncMutex<Option<ChildStdin>>,
    events: broadcast::Sender<Arc<CubEvent>>,
    backlog: Mutex<VecDeque<Arc<CubEvent>>>,
    seq: AtomicU64,
    exit: watch::Receiver<Option<Option<i32>>>,
    kill: Mutex<Option<oneshot::Sender<()>>>,
}

impl Cub {
    pub(crate) fn spawn(
        id: String,
        program: &PathBuf,
        spec: CubSpec,
        credential: Option<ProviderCredential>,
    ) -> Result<Arc<Self>, String> {
        let mut command = Command::new(program);
        if let Some(credential) = &credential {
            command.env(credential.variable(), credential.secret());
        }
        let mut child = command
            .current_dir(&spec.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(false)
            .spawn()
            .map_err(|error| format!("could not start `{}`: {error}", program.display()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "the Fiote process gave no stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "the Fiote process gave no stdout".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "the Fiote process gave no stderr".to_string())?;

        let (events, _) = broadcast::channel(BROADCAST_CAPACITY);
        let (exit_tx, exit_rx) = watch::channel(None);
        let (kill_tx, kill_rx) = oneshot::channel();

        let cub = Arc::new(Self {
            id,
            spec,
            credential_source: credential.as_ref().map(ProviderCredential::source),
            stdin: AsyncMutex::new(Some(stdin)),
            events,
            backlog: Mutex::new(VecDeque::new()),
            seq: AtomicU64::new(0),
            exit: exit_rx,
            kill: Mutex::new(Some(kill_tx)),
        });

        pump(Arc::clone(&cub), stdout, Channel::Rpc);
        pump(Arc::clone(&cub), stderr, Channel::Stderr);
        reap(Arc::clone(&cub), child, kill_rx, exit_tx);
        Ok(cub)
    }

    pub async fn send(&self, command: &serde_json::Value) -> Result<(), String> {
        let mut line = serde_json::to_string(command)
            .map_err(|error| format!("command is not serialisable: {error}"))?;
        line.push('\n');
        let mut guard = self.stdin.lock().await;
        let stdin = guard
            .as_mut()
            .ok_or_else(|| format!("cub `{}` is no longer accepting commands", self.id))?;
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|error| format!("cub `{}` refused the command: {error}", self.id))?;
        stdin
            .flush()
            .await
            .map_err(|error| format!("cub `{}` refused the command: {error}", self.id))
    }

    pub fn attach(&self) -> (Vec<Arc<CubEvent>>, broadcast::Receiver<Arc<CubEvent>>) {
        let receiver = self.events.subscribe();
        let backlog = self
            .backlog
            .lock()
            .expect("fiote backlog mutex")
            .iter()
            .cloned()
            .collect();
        (backlog, receiver)
    }

    pub fn status(&self) -> CubStatus {
        let exit = *self.exit.borrow();
        CubStatus {
            id: self.id.clone(),
            name: self.spec.name.clone(),
            cwd: self.spec.cwd.clone(),
            credential_source: self.credential_source,
            running: exit.is_none(),
            exit_code: exit.flatten(),
            events: self.seq.load(Ordering::Relaxed),
        }
    }

    pub fn is_running(&self) -> bool {
        self.exit.borrow().is_none()
    }

    pub fn stop(&self) {
        if let Some(sender) = self.kill.lock().expect("fiote kill mutex").take() {
            let _ = sender.send(());
        }
    }

    pub async fn wait(&self) -> Option<i32> {
        let mut exit = self.exit.clone();
        loop {
            if let Some(status) = *exit.borrow() {
                return status;
            }
            if exit.changed().await.is_err() {
                return None;
            }
        }
    }

    fn record(&self, channel: Channel, line: String) {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let event = Arc::new(CubEvent {
            cub: self.id.clone(),
            seq,
            channel,
            line,
        });
        {
            let mut backlog = self.backlog.lock().expect("fiote backlog mutex");
            backlog.push_back(Arc::clone(&event));
            while backlog.len() > BACKLOG_LINES {
                backlog.pop_front();
            }
        }
        let _ = self.events.send(event);
    }
}

fn pump<R>(cub: Arc<Cub>, source: R, channel: Channel)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(source).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            cub.record(channel, line);
        }
    });
}

fn reap(
    cub: Arc<Cub>,
    mut child: Child,
    kill: oneshot::Receiver<()>,
    exit: watch::Sender<Option<Option<i32>>>,
) {
    tokio::spawn(async move {
        let status = tokio::select! {
            waited = child.wait() => waited,
            _ = kill => {
                let _ = child.start_kill();
                child.wait().await
            }
        };
        let code = match status {
            Ok(status) => status.code(),
            Err(error) => {
                cub.record(Channel::Supervisor, format!("cub wait failed: {error}"));
                None
            }
        };
        cub.stdin.lock().await.take();
        let _ = exit.send(Some(code));
        cub.record(
            Channel::Supervisor,
            match code {
                Some(code) => format!("cub exited with code {code}"),
                None => "cub exited without a code".to_string(),
            },
        );
    });
}
