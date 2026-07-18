use {
    crate::ServerMessage,
    base64::{Engine as _, engine::general_purpose::STANDARD as BASE64},
    portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system},
    std::{
        collections::HashMap,
        io::{Read, Write},
        path::PathBuf,
        sync::{Arc, Mutex},
        thread,
    },
    tokio::sync::mpsc,
};

const MAX_INPUT_BYTES: usize = 1024 * 1024;

pub(crate) struct TerminalHost {
    sessions: HashMap<String, Arc<TerminalHandle>>,
}

struct TerminalHandle {
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
}

struct SpawnedTerminal {
    shell: String,
    cwd: String,
    handle: Arc<TerminalHandle>,
    reader: Box<dyn Read + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

impl TerminalHost {
    pub(crate) fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub(crate) async fn open(
        &mut self,
        id: String,
        size: PtySize,
        out_tx: mpsc::Sender<ServerMessage>,
        done_tx: mpsc::Sender<String>,
    ) -> Result<(), String> {
        if self.sessions.contains_key(&id) {
            return Err(format!("terminal session `{id}` is already open"));
        }

        let spawned = tokio::task::spawn_blocking(move || spawn_terminal(size))
            .await
            .map_err(|error| format!("terminal spawn task failed: {error}"))??;

        out_tx
            .send(ServerMessage::TerminalOpened {
                id: id.clone(),
                shell: spawned.shell,
                cwd: spawned.cwd,
            })
            .await
            .map_err(|_| "terminal transport closed while opening".to_string())?;

        spawn_reader(id.clone(), spawned.reader, out_tx.clone())?;
        spawn_waiter(id.clone(), spawned.child, out_tx, done_tx)?;
        self.sessions.insert(id, spawned.handle);
        Ok(())
    }

    pub(crate) async fn input(&self, id: &str, data_base64: &str) -> Result<(), String> {
        let bytes = BASE64
            .decode(data_base64)
            .map_err(|error| format!("invalid terminal input base64: {error}"))?;
        if bytes.len() > MAX_INPUT_BYTES {
            return Err(format!(
                "terminal input exceeds the {MAX_INPUT_BYTES}-byte frame limit"
            ));
        }
        let handle = self.handle(id)?;
        tokio::task::spawn_blocking(move || {
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

    pub(crate) async fn resize(&self, id: &str, size: PtySize) -> Result<(), String> {
        let handle = self.handle(id)?;
        tokio::task::spawn_blocking(move || {
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

    pub(crate) async fn close(&mut self, id: &str) -> Result<(), String> {
        let handle = self
            .sessions
            .remove(id)
            .ok_or_else(|| format!("terminal session `{id}` was not found"))?;
        terminate(handle).await
    }

    pub(crate) async fn shutdown(&mut self) {
        let handles = self.sessions.drain().map(|(_, handle)| handle);
        for handle in handles {
            let _ = terminate(handle).await;
        }
    }

    pub(crate) fn forget(&mut self, id: &str) {
        self.sessions.remove(id);
    }

    fn handle(&self, id: &str) -> Result<Arc<TerminalHandle>, String> {
        self.sessions
            .get(id)
            .cloned()
            .ok_or_else(|| format!("terminal session `{id}` was not found"))
    }
}

pub(crate) fn pty_size(cols: u16, rows: u16, pixel_width: u16, pixel_height: u16) -> PtySize {
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
    let pair = native_pty_system()
        .openpty(size)
        .map_err(|error| format!("could not open terminal PTY: {error}"))?;

    let command = terminal_command(&shell, &working_directory);

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
    });

    Ok(SpawnedTerminal {
        shell,
        cwd,
        handle,
        reader,
        child,
    })
}

fn terminal_command(shell: &str, working_directory: &std::path::Path) -> CommandBuilder {
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
    if let Some(home) = dirs::home_dir().filter(|path| path.is_dir()) {
        return Ok(home);
    }

    std::env::current_dir()
        .map_err(|error| format!("could not resolve terminal working directory: {error}"))
}

fn configured_shell() -> String {
    std::env::var("SHELL")
        .or_else(|_| std::env::var("COMSPEC"))
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
            let mut buffer = [0_u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => return,
                    Ok(read) => {
                        if out_tx
                            .blocking_send(ServerMessage::TerminalData {
                                id: id.clone(),
                                data_base64: BASE64.encode(&buffer[..read]),
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
    mut child: Box<dyn portable_pty::Child + Send + Sync>,
    out_tx: mpsc::Sender<ServerMessage>,
    done_tx: mpsc::Sender<String>,
) -> Result<(), String> {
    thread::Builder::new()
        .name(format!("terminal-wait-{id}"))
        .spawn(move || {
            let exit_code = child.wait().ok().map(|status| status.exit_code());
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
    tokio::task::spawn_blocking(move || {
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
        super::{BASE64, TerminalHost, pty_size, terminal_command},
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
                .unwrap()
                .display()
                .to_string()
        );
        host.input("test", &BASE64.encode(b"printf '__lince_terminal__\\n'\r"))
            .await
            .expect("input should reach the shell");

        let output = tokio::time::timeout(Duration::from_secs(5), async {
            let mut output = Vec::new();
            while !String::from_utf8_lossy(&output).contains("__lince_terminal__") {
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
            output
        })
        .await
        .expect("terminal output timed out");

        assert!(String::from_utf8_lossy(&output).contains("__lince_terminal__"));
        host.close("test").await.expect("PTY should close");
    }
}
