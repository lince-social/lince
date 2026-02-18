use injection::cross_cutting::InjectedServices;
use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
    process::Stdio,
    sync::{Arc, OnceLock},
    time::Instant,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command as TokioCommand,
    sync::{Mutex, broadcast, mpsc},
};
use utils::logging::{LogEntry, log};

const MAX_OUTPUT_CHARS: usize = 200_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandOutputStream {
    Stdout,
    Stderr,
    Stdin,
    System,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandOrigin {
    Consequence(u32),
    Operation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandSessionStatus {
    Running,
    Finished(Option<i32>),
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct CommandSessionSnapshot {
    pub session_id: u64,
    pub command_id: Option<u32>,
    pub command: String,
    pub origin: CommandOrigin,
    pub status: CommandSessionStatus,
    pub output: String,
    pub started_at: Instant,
    pub finished_at: Option<Instant>,
}

#[derive(Clone, Debug)]
pub enum CommandBufferEvent {
    Started(u64),
    Output(u64),
    Status(u64),
}

pub struct CommandBufferSubscriber {
    receiver: broadcast::Receiver<CommandBufferEvent>,
}

impl CommandBufferSubscriber {
    pub async fn recv(&mut self) -> Option<CommandBufferEvent> {
        loop {
            match self.receiver.recv().await {
                Ok(event) => return Some(event),
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

struct CommandSessionState {
    snapshot: CommandSessionSnapshot,
    stdin_tx: Option<mpsc::Sender<String>>,
}

struct CommandBufferState {
    next_session_id: u64,
    sessions: HashMap<u64, CommandSessionState>,
    ordered_ids: Vec<u64>,
}

struct CommandBufferHub {
    state: Arc<Mutex<CommandBufferState>>,
    event_tx: broadcast::Sender<CommandBufferEvent>,
}

static COMMAND_BUFFER: OnceLock<CommandBufferHub> = OnceLock::new();

fn command_buffer() -> &'static CommandBufferHub {
    COMMAND_BUFFER.get_or_init(|| {
        let (event_tx, _) = broadcast::channel(1024);
        CommandBufferHub {
            state: Arc::new(Mutex::new(CommandBufferState {
                next_session_id: 1,
                sessions: HashMap::new(),
                ordered_ids: Vec::new(),
            })),
            event_tx,
        }
    })
}

fn trim_output(text: &mut String) {
    if text.len() <= MAX_OUTPUT_CHARS {
        return;
    }
    let remove_up_to = text.len() - MAX_OUTPUT_CHARS;
    let remove_idx = text
        .char_indices()
        .find_map(|(idx, _)| (idx >= remove_up_to).then_some(idx))
        .unwrap_or(0);
    text.drain(..remove_idx);
}

async fn register_session(
    command_id: Option<u32>,
    command: String,
    origin: CommandOrigin,
    stdin_tx: mpsc::Sender<String>,
) -> u64 {
    let hub = command_buffer();
    let mut state = hub.state.lock().await;
    let session_id = state.next_session_id;
    state.next_session_id += 1;

    state.ordered_ids.push(session_id);
    state.sessions.insert(
        session_id,
        CommandSessionState {
            snapshot: CommandSessionSnapshot {
                session_id,
                command_id,
                command,
                origin,
                status: CommandSessionStatus::Running,
                output: String::new(),
                started_at: Instant::now(),
                finished_at: None,
            },
            stdin_tx: Some(stdin_tx),
        },
    );

    let _ = hub.event_tx.send(CommandBufferEvent::Started(session_id));
    session_id
}

async fn append_session_output(
    session_id: u64,
    stream: CommandOutputStream,
    chunk: String,
) -> Result<(), Error> {
    if chunk.is_empty() {
        return Ok(());
    }
    let hub = command_buffer();
    let mut state = hub.state.lock().await;
    let Some(session) = state.sessions.get_mut(&session_id) else {
        return Ok(());
    };
    let prefix = match stream {
        CommandOutputStream::Stdout => "",
        CommandOutputStream::Stderr => "[stderr] ",
        CommandOutputStream::Stdin => "[stdin] ",
        CommandOutputStream::System => "[system] ",
    };
    if prefix.is_empty() {
        session.snapshot.output.push_str(&chunk);
    } else {
        session.snapshot.output.push_str(prefix);
        session.snapshot.output.push_str(&chunk);
    }
    trim_output(&mut session.snapshot.output);
    let _ = hub.event_tx.send(CommandBufferEvent::Output(session_id));
    Ok(())
}

async fn finish_session(session_id: u64, status: Option<i32>) {
    let hub = command_buffer();
    let mut state = hub.state.lock().await;
    if let Some(session) = state.sessions.get_mut(&session_id) {
        session.snapshot.status = CommandSessionStatus::Finished(status);
        session.snapshot.finished_at = Some(Instant::now());
        session.stdin_tx = None;
        let _ = hub.event_tx.send(CommandBufferEvent::Status(session_id));
    }
}

async fn fail_session(session_id: u64, message: String) {
    let hub = command_buffer();
    let mut state = hub.state.lock().await;
    if let Some(session) = state.sessions.get_mut(&session_id) {
        session.snapshot.status = CommandSessionStatus::Failed(message);
        session.snapshot.finished_at = Some(Instant::now());
        session.stdin_tx = None;
        let _ = hub.event_tx.send(CommandBufferEvent::Status(session_id));
    }
}

async fn stream_reader<T>(session_id: u64, stream: CommandOutputStream, mut reader: T)
where
    T: AsyncRead + Unpin,
{
    let mut bytes = vec![0_u8; 2048];
    loop {
        match reader.read(&mut bytes).await {
            Ok(0) => break,
            Ok(read) => {
                let chunk = String::from_utf8_lossy(&bytes[..read]).to_string();
                let _ = append_session_output(session_id, stream.clone(), chunk).await;
            }
            Err(e) => {
                let _ = append_session_output(
                    session_id,
                    CommandOutputStream::System,
                    format!("stream read error: {e}\n"),
                )
                .await;
                break;
            }
        }
    }
}

pub fn command_buffer_subscribe() -> CommandBufferSubscriber {
    CommandBufferSubscriber {
        receiver: command_buffer().event_tx.subscribe(),
    }
}

pub async fn command_buffer_snapshot() -> Vec<CommandSessionSnapshot> {
    let hub = command_buffer();
    let state = hub.state.lock().await;
    state
        .ordered_ids
        .iter()
        .rev()
        .filter_map(|session_id| state.sessions.get(session_id))
        .map(|session| session.snapshot.clone())
        .collect()
}

pub async fn command_buffer_send_input(session_id: u64, input: String) -> Result<(), Error> {
    append_session_output(session_id, CommandOutputStream::Stdin, input.clone()).await?;
    let hub = command_buffer();
    let tx = {
        let state = hub.state.lock().await;
        state
            .sessions
            .get(&session_id)
            .and_then(|session| session.stdin_tx.clone())
    };

    let Some(tx) = tx else {
        return Err(Error::other(format!(
            "Command session {session_id} is not accepting input"
        )));
    };

    tx.send(input)
        .await
        .map_err(|_| Error::other(format!("Failed to send input to session {session_id}")))?;
    Ok(())
}

pub async fn spawn_command_buffer_session_by_id(
    services: InjectedServices,
    command_id: u32,
    origin: CommandOrigin,
) -> Result<u64, Error> {
    let command = services.repository.command.get_by_id(command_id).await?;
    let Some(command) = command else {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("Command with id {command_id} not found"),
        ));
    };
    spawn_command_buffer_session(Some(command.id), command.command, origin).await
}

pub async fn spawn_command_buffer_session(
    command_id: Option<u32>,
    command: String,
    origin: CommandOrigin,
) -> Result<u64, Error> {
    let mut child = TokioCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(Error::other)?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::other("Failed to capture command stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| Error::other("Failed to capture command stderr"))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| Error::other("Failed to capture command stdin"))?;

    let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(128);
    let session_id = register_session(command_id, command.clone(), origin, stdin_tx).await;

    tokio::spawn(async move {
        let mut stdin = stdin;
        while let Some(input) = stdin_rx.recv().await {
            if stdin.write_all(input.as_bytes()).await.is_err() {
                break;
            }
            let _ = stdin.flush().await;
        }
    });

    tokio::spawn(stream_reader(
        session_id,
        CommandOutputStream::Stdout,
        stdout,
    ));
    tokio::spawn(stream_reader(
        session_id,
        CommandOutputStream::Stderr,
        stderr,
    ));

    tokio::spawn(async move {
        match child.wait().await {
            Ok(status) => {
                finish_session(session_id, status.code()).await;
            }
            Err(e) => {
                fail_session(session_id, e.to_string()).await;
            }
        }
    });

    Ok(session_id)
}

pub async fn karma_execute_command(services: InjectedServices, id: u32) -> Option<i64> {
    let res = services.repository.command.get_by_id(id).await;
    match res {
        Err(e) => {
            log(LogEntry::Error(
                e.kind(),
                format!("Error when getting command with id: {}. Error: {}", id, e),
            ));
            None
        }
        Ok(opt) => match opt {
            None => None,
            Some(command) => service_karma_execute_command(command.command).await,
        },
    }
}

pub async fn service_karma_execute_command(command: String) -> Option<i64> {
    let output = TokioCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await;

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            println!("stdout:\n{}", stdout);
            println!("stderr:\n{}", stderr);

            if !output.status.success() {
                log(LogEntry::Error(
                    ErrorKind::Other,
                    format!(
                        "Command '{}' failed with status: {}. Stderr: {}",
                        command, output.status, stderr
                    ),
                ));
                return None;
            }
            Some(0)
        }
        Err(e) => {
            log(LogEntry::Error(
                e.kind(),
                format!("Failed to execute command '{}': {}", command, e),
            ));
            None
        }
    }
}
