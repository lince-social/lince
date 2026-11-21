//! PTY lifecycle and frame handling for interactive terminals and saved Commands.

use std::{
    collections::{HashMap, hash_map::Entry},
    env::{current_dir, var},
    fmt,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, Weak,
        atomic::{
            AtomicBool,
            Ordering::{Acquire, Release},
        },
    },
    thread,
};

use base64::{DecodeError, Engine as _, engine::general_purpose::STANDARD as BASE64};
use dirs::home_dir;
use portable_pty::{Child, ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use tokio::{sync::mpsc, task::spawn_blocking};

use crate::ServerMessage;

const MAX_SESSIONS: usize = 16;
const MAX_INPUT_BYTES: usize = 1024 * 1024;
const OUTPUT_CHUNK_BYTES: usize = 8192;

pub mod commands;

/// Interactive terminal sessions for one connection.
#[derive(Default)]
pub struct TerminalHost {
    sessions: HashMap<String, Arc<TerminalHandle>>,
}

struct TerminalHandle {
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    exited: AtomicBool,
}

struct SpawnedTerminal {
    shell: String,
    cwd: String,
    handle: Arc<TerminalHandle>,
    reader: Box<dyn Read + Send>,
    child: Box<dyn Child + Send + Sync>,
}

impl fmt::Debug for TerminalHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TerminalHost")
            .field("sessions", &self.sessions.len())
            .finish_non_exhaustive()
    }
}

impl TerminalHost {
    /// Creates a terminal host with no open sessions.
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens an interactive terminal and publishes its initial session message.
    ///
    /// # Errors
    /// Returns a diagnostic for a duplicate ID, a full host, or a failed spawn or delivery.
    pub async fn open(
        &mut self,
        id: String,
        size: PtySize,
        out_tx: mpsc::Sender<ServerMessage>,
        done_tx: mpsc::Sender<String>,
    ) -> Result<(), String> {
        let at_limit = self.sessions.len() >= MAX_SESSIONS;
        let slot = match self.sessions.entry(id) {
            Entry::Occupied(entry) => {
                return Err(format!(
                    "terminal session `{}` is already open",
                    entry.key()
                ));
            }
            Entry::Vacant(slot) => slot,
        };
        if at_limit {
            return Err(
                "Close a terminal before opening another (16 sessions per connection)".into(),
            );
        }

        let spawned = spawn_blocking(move || spawn_terminal(size))
            .await
            .map_err(|error| format!("terminal spawn task failed: {error}"))??;

        let id = slot.key();
        if out_tx
            .send(ServerMessage::TerminalOpened {
                id: id.clone(),
                shell: spawned.shell,
                cwd: spawned.cwd,
            })
            .await
            .is_err()
        {
            return Err("terminal transport closed while opening".into());
        }

        spawn_reader(id.clone(), spawned.reader, out_tx.clone())?;
        spawn_waiter(
            id.clone(),
            spawned.child,
            Arc::downgrade(&spawned.handle),
            out_tx,
            done_tx,
        )?;
        slot.insert(spawned.handle);
        Ok(())
    }

    /// Writes a Base64 frame to the selected terminal and flushes the PTY.
    ///
    /// # Errors
    /// Returns a diagnostic for malformed or oversized input, a missing session, or a failed write.
    pub async fn input(&self, id: &str, data_base64: &str) -> Result<(), String> {
        let bytes = decode_input(data_base64).map_err(|error| match error {
            InputError::EncodedLimit => "Terminal input is too large".into(),
            InputError::DecodedLimit => {
                format!("terminal input exceeds the {MAX_INPUT_BYTES}-byte frame limit")
            }
            InputError::Encoding(error) => format!("invalid terminal input base64: {error}"),
        })?;
        let handle = self.handle(id)?;
        spawn_blocking(move || {
            let mut writer = handle
                .writer
                .lock()
                .map_err(|_| "terminal writer lock is poisoned".to_string())?;
            writer
                .write_all(&bytes)
                .and_then(|()| writer.flush())
                .map_err(|error| format!("terminal input failed: {error}"))
        })
        .await
        .map_err(|error| format!("terminal input task failed: {error}"))?
    }

    /// Applies the terminal's cell and pixel dimensions.
    ///
    /// # Errors
    /// Returns a diagnostic if the session is missing or its resize fails.
    pub async fn resize(&self, id: &str, size: PtySize) -> Result<(), String> {
        let handle = self.handle(id)?;
        spawn_blocking(move || {
            handle
                .master
                .lock()
                .map_err(|_| "terminal resize lock is poisoned".to_string())?
                .resize(size)
                .map_err(|error| format!("terminal resize failed: {error}"))
        })
        .await
        .map_err(|error| format!("terminal resize task failed: {error}"))?
    }

    /// Removes the session and requests termination of its shell.
    ///
    /// # Errors
    /// Returns a diagnostic if the session is missing or termination fails.
    pub async fn close(&mut self, id: &str) -> Result<(), String> {
        let handle = self
            .sessions
            .remove(id)
            .ok_or_else(|| format!("terminal session `{id}` was not found"))?;
        terminate(handle).await
    }

    /// Drains all sessions and attempts to terminate their shells.
    pub async fn shutdown(&mut self) {
        let handles = self.sessions.drain().map(|(_, handle)| handle);
        for handle in handles {
            let _ = terminate(handle).await;
        }
    }

    /// Removes a session after its exit monitor reports completion.
    pub fn forget(&mut self, id: &str) {
        self.sessions.remove(id);
    }

    fn handle(&self, id: &str) -> Result<Arc<TerminalHandle>, String> {
        self.sessions
            .get(id)
            .map(Arc::clone)
            .ok_or_else(|| format!("terminal session `{id}` was not found"))
    }
}

enum InputError {
    EncodedLimit,
    DecodedLimit,
    Encoding(DecodeError),
}

fn decode_input(data: &str) -> Result<Vec<u8>, InputError> {
    if data.len() > MAX_INPUT_BYTES.div_ceil(3) * 4 {
        return Err(InputError::EncodedLimit);
    }
    let bytes = BASE64.decode(data).map_err(InputError::Encoding)?;
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(InputError::DecodedLimit);
    }
    Ok(bytes)
}

impl Drop for TerminalHandle {
    fn drop(&mut self) {
        if !self.exited.load(Acquire)
            && let Ok(killer) = self.killer.get_mut()
        {
            let _ = killer.kill();
        }
    }
}

/// Builds PTY dimensions with at least one row and column.
pub fn pty_size(cols: u16, rows: u16, pixel_width: u16, pixel_height: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width,
        pixel_height,
    }
}

fn spawn_terminal(size: PtySize) -> Result<SpawnedTerminal, String> {
    let shell = configured_shell();
    let working_directory = terminal_working_directory()?;
    let cwd = working_directory.display().to_string();
    let command = terminal_command(&shell, &working_directory);
    spawn_command(size, command, shell, cwd)
}

fn spawn_command(
    size: PtySize,
    command: CommandBuilder,
    shell: String,
    cwd: String,
) -> Result<SpawnedTerminal, String> {
    let pair = native_pty_system()
        .openpty(size)
        .map_err(|error| format!("could not open terminal PTY: {error}"))?;

    let child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| format!("could not start terminal shell: {error}"))?;
    let killer = child.clone_killer();
    drop(pair.slave);

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("could not open terminal output: {error}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| format!("could not open terminal input: {error}"))?;
    let handle = Arc::new(TerminalHandle {
        writer: Mutex::new(writer),
        master: Mutex::new(pair.master),
        killer: Mutex::new(killer),
        exited: AtomicBool::new(false),
    });

    Ok(SpawnedTerminal {
        shell,
        cwd,
        handle,
        reader,
        child,
    })
}

fn terminal_command(shell: &str, working_directory: &Path) -> CommandBuilder {
    let mut command = CommandBuilder::new(shell);
    #[cfg(not(windows))]
    command.arg("-i");
    command.cwd(working_directory);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    for variable in ["PS1", "PS2", "PS3", "PS4"] {
        command.env_remove(variable);
    }
    command
}

fn terminal_working_directory() -> Result<PathBuf, String> {
    if let Some(home) = home_dir().filter(|path| path.is_dir()) {
        return Ok(home);
    }

    current_dir().map_err(|error| format!("could not resolve terminal working directory: {error}"))
}

fn configured_shell() -> String {
    var("SHELL")
        .or_else(|_| var("COMSPEC"))
        .unwrap_or_else(|_| {
            if cfg!(windows) {
                "cmd.exe".into()
            } else {
                "/bin/sh".into()
            }
        })
}

fn spawn_reader(
    id: String,
    mut reader: Box<dyn Read + Send>,
    out_tx: mpsc::Sender<ServerMessage>,
) -> Result<(), String> {
    thread::Builder::new()
        .name(format!("terminal-reader-{id}"))
        .spawn(move || {
            let mut buffer = [0; OUTPUT_CHUNK_BYTES];
            loop {
                let chunk = reader.read(&mut buffer).and_then(|count| {
                    buffer
                        .get(..count)
                        .ok_or_else(|| io::Error::other("invalid byte count"))
                });
                match chunk {
                    Ok([]) => return,
                    Ok(bytes) => {
                        if out_tx
                            .blocking_send(ServerMessage::TerminalData {
                                id: id.clone(),
                                data_base64: BASE64.encode(bytes),
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(error) => {
                        let _ = out_tx.blocking_send(ServerMessage::Error {
                            id: id.clone(),
                            message: format!("terminal output failed: {error}"),
                            code: None,
                        });
                        return;
                    }
                }
            }
        })
        .map(|_| ())
        .map_err(|error| format!("could not start terminal reader: {error}"))
}

fn spawn_waiter(
    id: String,
    mut child: Box<dyn Child + Send + Sync>,
    handle: Weak<TerminalHandle>,
    out_tx: mpsc::Sender<ServerMessage>,
    done_tx: mpsc::Sender<String>,
) -> Result<(), String> {
    thread::Builder::new()
        .name(format!("terminal-wait-{id}"))
        .spawn(move || {
            let exit_code = child.wait().ok().map(|status| status.exit_code());
            if let Some(handle) = handle.upgrade() {
                handle.exited.store(true, Release);
            }
            let _ = out_tx.blocking_send(ServerMessage::TerminalExit {
                id: id.clone(),
                exit_code,
            });
            let _ = done_tx.blocking_send(id);
        })
        .map(|_| ())
        .map_err(|error| format!("could not start terminal exit monitor: {error}"))
}

async fn terminate(handle: Arc<TerminalHandle>) -> Result<(), String> {
    if handle.exited.load(Acquire) {
        return Ok(());
    }
    spawn_blocking(move || {
        handle
            .killer
            .lock()
            .map_err(|_| "terminal process lock is poisoned".to_string())?
            .kill()
            .map_err(|error| format!("could not terminate terminal shell: {error}"))
    })
    .await
    .map_err(|error| format!("terminal close task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use {
        super::{
            BASE64, InputError, MAX_INPUT_BYTES, TerminalHost, decode_input, pty_size,
            terminal_command,
        },
        crate::ServerMessage,
        base64::Engine as _,
        tokio::{sync::mpsc, time::Duration},
    };

    #[test]
    fn terminal_command_does_not_inherit_foreign_prompt_syntax() {
        let command = terminal_command("test-shell", std::path::Path::new("."));

        for variable in ["PS1", "PS2", "PS3", "PS4"] {
            assert!(command.get_env(variable).is_none());
        }
    }

    #[test]
    fn input_checks_the_encoded_limit_before_decoding() {
        let mut input = vec![0; MAX_INPUT_BYTES];
        let max = BASE64.encode(&input);
        assert!(matches!(decode_input(&max), Ok(bytes) if bytes.len() == MAX_INPUT_BYTES));
        input.push(0);
        let decoded_overflow = BASE64.encode(&input);
        assert!(matches!(
            decode_input(&decoded_overflow),
            Err(InputError::DecodedLimit)
        ));
        input.extend([0, 0]);
        assert!(matches!(
            decode_input(&BASE64.encode(input)),
            Err(InputError::EncodedLimit)
        ));
        assert!(matches!(decode_input("!"), Err(InputError::Encoding(_))));
    }

    #[tokio::test]
    async fn pty_round_trips_over_terminal_messages() {
        let (out_tx, mut out_rx) = mpsc::channel(64);
        let (done_tx, _done_rx) = mpsc::channel(4);
        let mut host = TerminalHost::new();
        host.open("test".into(), pty_size(80, 24, 0, 0), out_tx, done_tx)
            .await
            .expect("PTY should open");

        let opened = out_rx.recv().await;
        let Some(ServerMessage::TerminalOpened { id, cwd, .. }) = opened else {
            panic!("PTY should announce its open session");
        };
        assert_eq!(id, "test");
        assert_eq!(
            cwd,
            super::terminal_working_directory()
                .expect("terminal working directory")
                .display()
                .to_string()
        );
        host.resize("test", pty_size(104, 37, 0, 0))
            .await
            .expect("PTY should resize");
        host.input(
            "test",
            &BASE64.encode(b"printf '__lince_%s__\\n' terminal; stty size\r"),
        )
        .await
        .expect("input should reach the shell");

        let output = tokio::time::timeout(Duration::from_secs(5), async {
            let mut output = Vec::new();
            loop {
                let text = String::from_utf8_lossy(&output);
                if text.contains("__lince_terminal__") && text.contains("37 104") {
                    return output;
                }
                match out_rx.recv().await {
                    Some(ServerMessage::TerminalData { data_base64, .. }) => {
                        output.extend(BASE64.decode(data_base64).expect("valid output base64"));
                    }
                    Some(ServerMessage::Error { message, .. }) => {
                        panic!("terminal stream failed: {message}");
                    }
                    Some(_) => {}
                    None => panic!("terminal stream ended before marker"),
                }
            }
        })
        .await
        .expect("terminal output timed out");

        assert!(String::from_utf8_lossy(&output).contains("__lince_terminal__"));
        host.close("test").await.expect("PTY should close");
    }
}
