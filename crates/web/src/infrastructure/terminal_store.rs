use {
    crate::infrastructure::paths,
    base64::{Engine as _, engine::general_purpose::STANDARD as BASE64},
    portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system},
    serde::Serialize,
    std::{
        collections::HashMap,
        io::{Read, Write},
        sync::{Arc, Mutex},
        thread,
    },
    tokio::sync::{RwLock, broadcast},
};

const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;
const MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Default)]
pub struct TerminalSessionStore {
    sessions: Arc<RwLock<HashMap<String, TerminalSessionHandle>>>,
}

#[derive(Clone)]
struct TerminalSessionHandle {
    id: String,
    shell: String,
    cwd: String,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    killer: Arc<Mutex<Box<dyn ChildKiller + Send + Sync>>>,
    output: Arc<RwLock<TerminalOutputState>>,
    events: broadcast::Sender<TerminalStreamEvent>,
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
    pub cols: u16,
    pub rows: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutputChunk {
    pub session: TerminalSessionSnapshot,
    pub data: String,
    pub data_base64: String,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub struct TerminalOutputBytes {
    pub session: TerminalSessionSnapshot,
    pub bytes: Vec<u8>,
    pub truncated: bool,
}

#[derive(Debug)]
pub struct TerminalResize {
    pub cols: u16,
    pub rows: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
}

#[derive(Debug, Clone)]
pub enum TerminalStreamEvent {
    Output,
    Snapshot(TerminalSessionSnapshot),
    Closed(TerminalSessionSnapshot),
}

pub struct TerminalStreamSubscription {
    pub session: TerminalSessionSnapshot,
    pub bytes: Vec<u8>,
    pub receiver: broadcast::Receiver<TerminalStreamEvent>,
}

#[derive(Debug)]
struct TerminalOutputState {
    base_cursor: usize,
    data: Vec<u8>,
    closed: bool,
    exit_code: Option<i32>,
    cols: u16,
    rows: u16,
    pixel_width: u16,
    pixel_height: u16,
}

impl Default for TerminalOutputState {
    fn default() -> Self {
        Self {
            base_cursor: 0,
            data: Vec::new(),
            closed: false,
            exit_code: None,
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

impl TerminalSessionStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn create_session(&self) -> Result<TerminalSessionSnapshot, String> {
        self.create_session_with_size(None).await
    }

    pub async fn create_session_with_size(
        &self,
        initial_resize: Option<TerminalResize>,
    ) -> Result<TerminalSessionSnapshot, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        let cwd = paths::workspace_root_dir().display().to_string();
        let size = resize_to_size(initial_resize.unwrap_or(TerminalResize {
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
            pixel_width: 0,
            pixel_height: 0,
        }));

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(size)
            .map_err(|error| format!("Nao consegui abrir a shell local: {error}"))?;

        let mut command = CommandBuilder::new(shell.as_str());
        command.arg("-i");
        command.cwd(&cwd);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");

        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| format!("Nao consegui abrir a shell local: {error}"))?;
        let killer = child.clone_killer();
        drop(pair.slave);

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| format!("Nao consegui abrir a leitura da shell local: {error}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| format!("Nao consegui abrir a escrita da shell local: {error}"))?;

        let output = Arc::new(RwLock::new(TerminalOutputState {
            cols: size.cols,
            rows: size.rows,
            pixel_width: size.pixel_width,
            pixel_height: size.pixel_height,
            ..TerminalOutputState::default()
        }));
        let (events, _) = broadcast::channel(1024);
        thread::Builder::new()
            .name(format!("terminal-reader-{id}"))
            .spawn({
                let output = output.clone();
                let events = events.clone();
                move || pump_reader(reader, output, events)
            })
            .map_err(|error| format!("Nao consegui iniciar o leitor da shell local: {error}"))?;
        thread::Builder::new()
            .name(format!("terminal-wait-{id}"))
            .spawn({
                let output = output.clone();
                let events = events.clone();
                let id = id.clone();
                let shell = shell.clone();
                let cwd = cwd.clone();
                move || wait_for_exit(&mut child, output, events, id, shell, cwd)
            })
            .map_err(|error| format!("Nao consegui monitorar a shell local: {error}"))?;

        let handle = TerminalSessionHandle {
            id: id.clone(),
            shell,
            cwd,
            writer: Arc::new(Mutex::new(writer)),
            master: Arc::new(Mutex::new(pair.master)),
            killer: Arc::new(Mutex::new(killer)),
            output,
            events,
        };

        self.sessions.write().await.insert(id, handle.clone());
        Ok(handle.snapshot().await)
    }

    pub async fn read_output(
        &self,
        id: &str,
        cursor: usize,
    ) -> Result<TerminalOutputChunk, String> {
        let chunk = self.read_output_bytes(id, cursor).await?;

        Ok(TerminalOutputChunk {
            session: chunk.session,
            data: String::from_utf8_lossy(&chunk.bytes).into_owned(),
            data_base64: BASE64.encode(&chunk.bytes),
            truncated: chunk.truncated,
        })
    }

    pub async fn read_output_bytes(
        &self,
        id: &str,
        cursor: usize,
    ) -> Result<TerminalOutputBytes, String> {
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
        let bytes = output.data.get(start..).unwrap_or_default().to_vec();

        Ok(TerminalOutputBytes {
            session: handle.snapshot_from_output(&output),
            bytes,
            truncated: cursor < output.base_cursor,
        })
    }

    pub async fn send_input_text(
        &self,
        id: &str,
        input: &str,
    ) -> Result<TerminalSessionSnapshot, String> {
        self.send_input_bytes(id, input.as_bytes()).await
    }

    pub async fn send_input_base64(
        &self,
        id: &str,
        input_base64: &str,
    ) -> Result<TerminalSessionSnapshot, String> {
        let bytes = BASE64
            .decode(input_base64)
            .map_err(|error| format!("Input base64 invalido: {error}"))?;
        self.send_input_bytes(id, &bytes).await
    }

    pub async fn resize(
        &self,
        id: &str,
        resize: TerminalResize,
    ) -> Result<TerminalSessionSnapshot, String> {
        let handle = self
            .sessions
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| "Sessao de terminal nao encontrada.".to_string())?;

        let size = resize_to_size(resize);

        {
            let master = handle
                .master
                .lock()
                .map_err(|_| "Nao consegui bloquear o terminal para resize.".to_string())?;
            master
                .resize(size)
                .map_err(|error| format!("Nao consegui redimensionar a shell local: {error}"))?;
        }

        {
            let mut output = handle.output.write().await;
            output.cols = size.cols;
            output.rows = size.rows;
            output.pixel_width = size.pixel_width;
            output.pixel_height = size.pixel_height;
        }

        let snapshot = handle.snapshot().await;
        let _ = handle
            .events
            .send(TerminalStreamEvent::Snapshot(snapshot.clone()));
        Ok(snapshot)
    }

    pub async fn terminate(&self, id: &str) -> Result<(), String> {
        let handle = self
            .sessions
            .write()
            .await
            .remove(id)
            .ok_or_else(|| "Sessao de terminal nao encontrada.".to_string())?;

        if handle.output.read().await.closed {
            return Ok(());
        }

        let mut killer = handle
            .killer
            .lock()
            .map_err(|_| "Nao consegui bloquear a shell local para encerrar.".to_string())?;
        killer
            .kill()
            .map_err(|error| format!("Nao consegui encerrar a shell local: {error}"))?;

        Ok(())
    }

    pub async fn send_input_bytes(
        &self,
        id: &str,
        input: &[u8],
    ) -> Result<TerminalSessionSnapshot, String> {
        let handle = self
            .sessions
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| "Sessao de terminal nao encontrada.".to_string())?;

        if input.is_empty() {
            return Ok(handle.snapshot().await);
        }

        {
            let mut writer = handle
                .writer
                .lock()
                .map_err(|_| "Nao consegui bloquear a shell local para escrita.".to_string())?;
            writer
                .write_all(input)
                .map_err(|error| format!("Nao consegui escrever na shell local: {error}"))?;
            writer
                .flush()
                .map_err(|error| format!("Nao consegui flush da shell local: {error}"))?;
        }

        Ok(handle.snapshot().await)
    }

    pub async fn subscribe_stream(
        &self,
        id: &str,
    ) -> Result<TerminalStreamSubscription, String> {
        let handle = self
            .sessions
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| "Sessao de terminal nao encontrada.".to_string())?;

        let receiver = handle.events.subscribe();
        let output = handle.output.read().await;
        Ok(TerminalStreamSubscription {
            session: handle.snapshot_from_output(&output),
            bytes: output.data.clone(),
            receiver,
        })
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
            cols: output.cols,
            rows: output.rows,
            pixel_width: output.pixel_width,
            pixel_height: output.pixel_height,
        }
    }
}

fn pump_reader(
    mut reader: Box<dyn Read + Send>,
    output: Arc<RwLock<TerminalOutputState>>,
    events: broadcast::Sender<TerminalStreamEvent>,
) {
    let mut buffer = [0_u8; 4096];
    loop {
        let bytes_read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(bytes_read) => bytes_read,
            Err(error) => {
                tracing::warn!("terminal reader failed: {error}");
                break;
            }
        };

        let mut state = output.blocking_write();
        state.data.extend_from_slice(&buffer[..bytes_read]);
        trim_output(&mut state);
        let _ = events.send(TerminalStreamEvent::Output);
    }
}

fn wait_for_exit(
    child: &mut Box<dyn portable_pty::Child + Send + Sync>,
    output: Arc<RwLock<TerminalOutputState>>,
    events: broadcast::Sender<TerminalStreamEvent>,
    id: String,
    shell: String,
    cwd: String,
) {
    let result = child.wait();
    match result {
        Ok(status) => {
            let mut state = output.blocking_write();
            state.closed = true;
            state.exit_code = i32::try_from(status.exit_code()).ok();
            let snapshot = snapshot_from_output(&id, &shell, &cwd, &state);
            let _ = events.send(TerminalStreamEvent::Closed(snapshot));
        }
        Err(error) => {
            tracing::warn!("terminal wait failed: {error}");
            let mut state = output.blocking_write();
            state.closed = true;
            state.exit_code = None;
            let snapshot = snapshot_from_output(&id, &shell, &cwd, &state);
            let _ = events.send(TerminalStreamEvent::Closed(snapshot));
        }
    }
}

fn trim_output(state: &mut TerminalOutputState) {
    if state.data.len() <= MAX_OUTPUT_BYTES {
        return;
    }

    let drop_bytes = state.data.len() - MAX_OUTPUT_BYTES;
    state.data.drain(..drop_bytes);
    state.base_cursor += drop_bytes;
}

fn resize_to_size(resize: TerminalResize) -> PtySize {
    PtySize {
        rows: resize.rows.max(1),
        cols: resize.cols.max(1),
        pixel_width: resize.pixel_width,
        pixel_height: resize.pixel_height,
    }
}

fn snapshot_from_output(
    id: &str,
    shell: &str,
    cwd: &str,
    output: &TerminalOutputState,
) -> TerminalSessionSnapshot {
    TerminalSessionSnapshot {
        id: id.to_string(),
        shell: shell.to_string(),
        cwd: cwd.to_string(),
        closed: output.closed,
        exit_code: output.exit_code,
        base_cursor: output.base_cursor,
        next_cursor: output.base_cursor + output.data.len(),
        cols: output.cols,
        rows: output.rows,
        pixel_width: output.pixel_width,
        pixel_height: output.pixel_height,
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalSessionStore;
    use tokio::time::{Duration, sleep};

    #[tokio::test]
    async fn terminal_session_round_trips_output() {
        let store = TerminalSessionStore::new();
        let session = store
            .create_session()
            .await
            .expect("session should start");
        let session_id = session.id.clone();

        let mut cursor = 0;
        let mut combined = String::new();
        let mut command_sent = false;
        for _ in 0..60 {
            let chunk = store
                .read_output(&session_id, cursor)
                .await
                .expect("output should read");
            cursor = chunk.session.next_cursor;
            combined.push_str(&chunk.data);

            reply_to_cursor_queries(&store, &session_id, &chunk.data).await;

            if !command_sent && combined.contains("\u{1b}[6n") {
                store
                    .send_input_text(&session_id, "printf '__codex_terminal_store__\\n'\r")
                    .await
                    .expect("input should send");
                command_sent = true;
            }

            if combined.contains("__codex_terminal_store__") {
                break;
            }
            sleep(Duration::from_millis(80)).await;
        }

        assert!(
            combined.contains("__codex_terminal_store__"),
            "terminal output did not contain round-trip marker: {combined:?}"
        );

        store
            .terminate(&session_id)
            .await
            .expect("session should terminate");
    }

    async fn reply_to_cursor_queries(
        store: &TerminalSessionStore,
        session_id: &str,
        output: &str,
    ) {
        for _ in output.match_indices("\u{1b}[6n") {
            store
                .send_input_text(session_id, "\u{1b}[1;1R")
                .await
                .expect("cursor query reply should send");
        }

        for _ in output.match_indices("\u{1b}[?6n") {
            store
                .send_input_text(session_id, "\u{1b}[?1;1R")
                .await
                .expect("private cursor query reply should send");
        }
    }
}
