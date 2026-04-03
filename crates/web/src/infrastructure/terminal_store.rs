use {
    crate::infrastructure::paths,
    serde::Serialize,
    std::{collections::HashMap, process::Stdio, sync::Arc},
    tokio::{
        io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
        process::{Child, ChildStdin, Command},
        sync::{Mutex, RwLock},
    },
};

const MAX_OUTPUT_BYTES: usize = 256 * 1024;

#[cfg(not(target_os = "macos"))]
fn shell_command(shell: &str) -> String {
    format!("exec {} -i", sh_single_quote(shell))
}

#[cfg(not(target_os = "macos"))]
fn sh_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[derive(Clone, Default)]
pub struct TerminalSessionStore {
    sessions: Arc<RwLock<HashMap<String, TerminalSessionHandle>>>,
}

#[derive(Clone)]
struct TerminalSessionHandle {
    id: String,
    shell: String,
    cwd: String,
    stdin: Arc<Mutex<ChildStdin>>,
    child: Arc<Mutex<Child>>,
    output: Arc<RwLock<TerminalOutputState>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSessionSnapshot {
    pub id: String,
    pub shell: String,
    pub cwd: String,
    pub closed: bool,
    pub exit_code: Option<i32>,
    pub base_cursor: usize,
    pub next_cursor: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutputChunk {
    pub session: TerminalSessionSnapshot,
    pub data: String,
    pub truncated: bool,
}

#[derive(Debug, Default)]
struct TerminalOutputState {
    base_cursor: usize,
    data: String,
    closed: bool,
    exit_code: Option<i32>,
}

impl TerminalSessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn create_session(&self) -> Result<TerminalSessionSnapshot, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        let cwd = paths::workspace_root_dir().display().to_string();

        let mut command = Command::new("/usr/bin/script");
        command.arg("-q");

        #[cfg(target_os = "linux")]
        {
            command
                .arg("-c")
                .arg(shell_command(&shell))
                .env("SHELL", "/bin/sh");
        }

        #[cfg(target_os = "macos")]
        {
            command.arg("/dev/null").arg(shell.as_str()).arg("-i");
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            command
                .arg("-c")
                .arg(shell_command(&shell))
                .env("SHELL", "/bin/sh");
        }

        #[cfg(target_os = "linux")]
        command.arg("/dev/null");

        let mut child = command
            .current_dir(&cwd)
            .env("TERM", "xterm-256color")
            .env("COLORTERM", "truecolor")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Nao consegui abrir a shell local: {error}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "A shell local nao expôs stdin.".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "A shell local nao expôs stdout.".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "A shell local nao expôs stderr.".to_string())?;

        let output = Arc::new(RwLock::new(TerminalOutputState::default()));
        let child = Arc::new(Mutex::new(child));
        let stdin = Arc::new(Mutex::new(stdin));

        tokio::spawn(pump_reader(stdout, output.clone()));
        tokio::spawn(pump_reader(stderr, output.clone()));
        tokio::spawn(wait_for_exit(child.clone(), output.clone()));

        let handle = TerminalSessionHandle {
            id: id.clone(),
            shell,
            cwd,
            stdin,
            child,
            output,
        };

        self.sessions.write().await.insert(id, handle.clone());
        Ok(handle.snapshot().await)
    }

    pub async fn read_output(
        &self,
        id: &str,
        cursor: usize,
    ) -> Result<TerminalOutputChunk, String> {
        let handle = self
            .sessions
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| "Sessao de terminal nao encontrada.".to_string())?;

        let output = handle.output.read().await;
        let effective_cursor = cursor.max(output.base_cursor);
        let start = effective_cursor.saturating_sub(output.base_cursor);
        let data = output.data.get(start..).unwrap_or_default().to_string();

        Ok(TerminalOutputChunk {
            session: handle.snapshot_from_output(&output),
            data,
            truncated: cursor < output.base_cursor,
        })
    }

    pub async fn send_input(&self, id: &str, input: &str) -> Result<TerminalSessionSnapshot, String> {
        let handle = self
            .sessions
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| "Sessao de terminal nao encontrada.".to_string())?;

        let mut stdin = handle.stdin.lock().await;
        stdin
            .write_all(input.as_bytes())
            .await
            .map_err(|error| format!("Nao consegui escrever na shell local: {error}"))?;
        stdin
            .flush()
            .await
            .map_err(|error| format!("Nao consegui flush da shell local: {error}"))?;

        Ok(handle.snapshot().await)
    }

    pub async fn terminate(&self, id: &str) -> Result<(), String> {
        let handle = self
            .sessions
            .write()
            .await
            .remove(id)
            .ok_or_else(|| "Sessao de terminal nao encontrada.".to_string())?;

        let mut child = handle.child.lock().await;
        child
            .kill()
            .await
            .map_err(|error| format!("Nao consegui encerrar a shell local: {error}"))?;

        Ok(())
    }
}

impl TerminalSessionHandle {
    async fn snapshot(&self) -> TerminalSessionSnapshot {
        let output = self.output.read().await;
        self.snapshot_from_output(&output)
    }

    fn snapshot_from_output(&self, output: &TerminalOutputState) -> TerminalSessionSnapshot {
        TerminalSessionSnapshot {
            id: self.id.clone(),
            shell: self.shell.clone(),
            cwd: self.cwd.clone(),
            closed: output.closed,
            exit_code: output.exit_code,
            base_cursor: output.base_cursor,
            next_cursor: output.base_cursor + output.data.len(),
        }
    }
}

async fn pump_reader<R>(mut reader: R, output: Arc<RwLock<TerminalOutputState>>)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let mut buffer = [0_u8; 4096];
    loop {
        let bytes_read = match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(bytes_read) => bytes_read,
            Err(error) => {
                tracing::warn!("terminal reader failed: {error}");
                break;
            }
        };

        let chunk = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
        let mut state = output.write().await;
        state.data.push_str(&chunk);
        trim_output(&mut state);
    }
}

async fn wait_for_exit(child: Arc<Mutex<Child>>, output: Arc<RwLock<TerminalOutputState>>) {
    let result = child.lock().await.wait().await;
    match result {
        Ok(status) => {
            let mut state = output.write().await;
            state.closed = true;
            state.exit_code = status.code();
        }
        Err(error) => {
            tracing::warn!("terminal wait failed: {error}");
            let mut state = output.write().await;
            state.closed = true;
            state.exit_code = None;
        }
    }
}

fn trim_output(state: &mut TerminalOutputState) {
    if state.data.len() <= MAX_OUTPUT_BYTES {
        return;
    }

    let drop_bytes = state.data.len() - MAX_OUTPUT_BYTES;
    let cut_index = next_char_boundary(&state.data, drop_bytes);
    state.data.drain(..cut_index);
    state.base_cursor += cut_index;
}

fn next_char_boundary(input: &str, index: usize) -> usize {
    if input.is_char_boundary(index) {
        return index;
    }

    let mut next = index;
    while next < input.len() && !input.is_char_boundary(next) {
        next += 1;
    }
    next
}
