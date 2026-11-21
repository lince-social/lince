//! Saved Command runs, terminal workers, and retained history.

mod changes;
mod journal;
pub use changes::{Changes, Source};
#[cfg(test)]
mod tests;

use super::{BASE64, InputError, TerminalHandle, decode_input, pty_size, spawn_command};
use crate::command::{Event, Request, Response, Run};
use base64::Engine as _;
use portable_pty::CommandBuilder;
use std::{
    collections::HashMap,
    fmt,
    fs::File,
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::Ordering::{Acquire, Release},
        mpsc::{SyncSender, sync_channel},
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_ACTIVE_RUNS: usize = 16;
const MAX_SCRIPT_BYTES: usize = 65_536;
const INPUT_QUEUE_DEPTH: usize = 8;
const OUTPUT_CHUNK_BYTES: usize = 8192;
const OUTPUT_LIMIT: usize = 256 * 1024 * 1024;
// The terminal backend can give this raw status at PTY EOF.
const CLOSED_PTY_ERROR: i32 = 5;

/// Runs saved Commands and retains their output independently of open views.
#[derive(Clone)]
pub struct CommandHost(Arc<Host>);

impl fmt::Debug for CommandHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandHost").finish_non_exhaustive()
    }
}

struct Host {
    root: PathBuf,
    runs: Mutex<HashMap<String, Active>>,
    changes: Arc<changes::Registry>,
}

struct Active {
    command: String,
    handle: Arc<TerminalHandle>,
    journal: Arc<Mutex<File>>,
    pid: Option<u32>,
    stopped: Option<String>,
    input: SyncSender<Vec<u8>>,
}

impl Default for CommandHost {
    fn default() -> Self {
        Self::new(utils::config::lince_data_dir().unwrap_or_default())
    }
}

impl CommandHost {
    /// Stops active runs and waits briefly for their output workers to finish.
    pub async fn shutdown(&self) {
        if let Ok(mut runs) = self.0.runs.lock() {
            for active in runs.values_mut() {
                active.stopped = Some("Stopped when Lince closed".into());
                stop(&active.handle, active.pid);
            }
        }
        for _ in 0..40 {
            if self.0.runs.lock().is_ok_and(|runs| runs.is_empty()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Creates a host whose journals are stored under the supplied data directory.
    pub fn new(root: PathBuf) -> Self {
        Self(Arc::new(Host {
            root,
            runs: Mutex::new(HashMap::new()),
            changes: Arc::new(changes::Registry::default()),
        }))
    }

    /// Observes Command history or a single run's output.
    ///
    /// # Errors
    /// Returns an error for an invalid identity or an unavailable registry.
    pub fn observe(&self, command: &str, run: Option<&str>) -> Result<Changes, String> {
        if !nucleus::valid_uid(command, "r")
            || run.is_some_and(|run| !nucleus::valid_uid(run, "run"))
        {
            return Err("Invalid Command or run identity".into());
        }
        self.0.changes.observe(command, run)
    }

    /// Returns the source identity for comparing subscriptions.
    pub fn source(&self) -> Source {
        self.0.changes.source()
    }

    /// Validates and handles a request, resolving new runs against current Command Records.
    ///
    /// # Errors
    /// Returns a diagnostic for invalid requests, unavailable Commands or runs,
    /// or failed storage, worker, or terminal operations.
    pub async fn request(
        &self,
        engine: &engine::Engine,
        request: Request,
    ) -> Result<Response, String> {
        let command = request.command();
        if !nucleus::valid_uid(command, "r") {
            return Err("Invalid Command identity".into());
        }
        if matches!(request, Request::Run { .. }) {
            let record = store::records::get(&engine.store.pool, command)
                .await
                .map_err(|error| error.to_string())?
                .ok_or("Command Record was not found")?;
            if record.kind != "command" {
                return Err("Choose a Command Record".into());
            }
        }
        let host = self.clone();
        tokio::task::spawn_blocking(move || host.handle(request))
            .await
            .map_err(|error| error.to_string())?
    }

    fn handle(&self, request: Request) -> Result<Response, String> {
        if self.0.root.as_os_str().is_empty() {
            return Err("The Lince data directory is unavailable".into());
        }
        match request {
            Request::Run {
                command,
                script,
                cwd,
            } => self.start(command, script, &cwd),
            Request::History { command } => {
                self.prune(&command)?;
                Ok(Response::History {
                    runs: self.history(&command)?,
                })
            }
            Request::Read {
                command,
                run,
                offset,
            } => {
                let path = journal::path(&self.0.root, &command, &run)?;
                let (events, next_offset) = journal::read(&path, offset)?;
                let active = self
                    .0
                    .runs
                    .lock()
                    .map_err(|error| error.to_string())?
                    .get(&run)
                    .is_some_and(|active| active.command == command);
                let complete = !active
                    && (events.is_empty()
                        || next_offset
                            == std::fs::metadata(path)
                                .map_err(|error| error.to_string())?
                                .len());
                Ok(Response::Output {
                    events,
                    next_offset,
                    complete,
                })
            }
            Request::Input {
                command,
                run,
                data_base64,
            } => {
                let bytes = decode_input(&data_base64).map_err(|error| match error {
                    InputError::EncodedLimit | InputError::DecodedLimit => {
                        "Terminal input is too large".into()
                    }
                    InputError::Encoding(error) => error.to_string(),
                })?;
                let runs = self.0.runs.lock().map_err(|error| error.to_string())?;
                let active = runs
                    .get(&run)
                    .filter(|active| active.command == command)
                    .ok_or("The command has finished")?;
                if active.input.try_send(bytes).is_err() {
                    return Err("Command input is busy or closed".into());
                }
                Ok(Response::Ok)
            }
            Request::Resize {
                command,
                run,
                cols,
                rows,
            } => {
                if !(10..=240).contains(&cols) || !(4..=100).contains(&rows) {
                    return Err("Invalid terminal size".into());
                }
                let (handle, journal) = self.active(&command, &run)?;
                let mut file = journal.lock().map_err(|error| error.to_string())?;
                journal::append(&mut file, &Event::Resize { cols, rows })?;
                drop(file);
                self.0.changes.notify(&command, &run, false);
                handle
                    .master
                    .lock()
                    .map_err(|error| error.to_string())?
                    .resize(pty_size(cols, rows, cols * 9, rows * 18))
                    .map_err(|error| error.to_string())?;
                Ok(Response::Ok)
            }
            Request::Stop { command, run } => {
                let mut runs = self.0.runs.lock().map_err(|error| error.to_string())?;
                let active = runs
                    .get_mut(&run)
                    .filter(|active| active.command == command)
                    .ok_or("The command has finished")?;
                active.stopped = Some("Stopped by user".into());
                stop(&active.handle, active.pid);
                Ok(Response::Ok)
            }
        }
    }

    fn active(
        &self,
        command: &str,
        run: &str,
    ) -> Result<(Arc<TerminalHandle>, Arc<Mutex<File>>), String> {
        let runs = self.0.runs.lock().map_err(|error| error.to_string())?;
        let active = runs
            .get(run)
            .filter(|active| active.command == command)
            .ok_or("The command has finished")?;
        Ok((Arc::clone(&active.handle), Arc::clone(&active.journal)))
    }

    fn start(&self, command: String, script: String, cwd: &str) -> Result<Response, String> {
        if script.trim().is_empty() || script.len() > MAX_SCRIPT_BYTES || script.contains('\0') {
            return Err("Enter a shell script of at most 65536 bytes".into());
        }
        let cwd = working_directory(cwd)?;
        let mut runs = self.0.runs.lock().map_err(|error| error.to_string())?;
        if runs.len() >= MAX_ACTIVE_RUNS || runs.values().any(|active| active.command == command) {
            return Err(
                "A run is already active for this Command, or 16 commands are running".into(),
            );
        }
        let run = Run {
            command,
            id: nucleus::new_uid("run"),
            script,
            cwd: cwd.display().to_string(),
            started_ms: now(),
            finished_ms: None,
            exit_code: None,
            error: None,
        };
        let path = journal::path(&self.0.root, &run.command, &run.id)?;
        let mut file = journal::create(&path, &run)?;
        let shell = CommandBuilder::new_default_prog().get_shell();
        let command = shell_command(&shell, &run.script, &cwd);
        let spawned =
            match spawn_command(pty_size(80, 24, 720, 432), command, shell, run.cwd.clone()) {
                Ok(spawned) => spawned,
                Err(error) => {
                    journal::append(
                        &mut file,
                        &Event::Finished {
                            finished_ms: now(),
                            exit_code: None,
                            error: Some(error.clone()),
                        },
                    )?;
                    self.0.changes.notify(&run.command, &run.id, true);
                    return Err(error);
                }
            };
        let pid = spawned.child.process_id();
        let journal = Arc::new(Mutex::new(file));
        let (input, incoming) = sync_channel::<Vec<u8>>(INPUT_QUEUE_DEPTH);
        let writer = Arc::clone(&spawned.handle);
        thread::Builder::new()
            .name(format!("command-input-{}", run.id))
            .spawn(move || {
                while let Ok(bytes) = incoming.recv() {
                    let Ok(mut output) = writer.writer.lock() else {
                        break;
                    };
                    if output
                        .write_all(&bytes)
                        .and_then(|()| output.flush())
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|error| {
                stop(&spawned.handle, pid);
                self.0.changes.notify(&run.command, &run.id, true);
                error.to_string()
            })?;
        runs.insert(
            run.id.clone(),
            Active {
                command: run.command.clone(),
                handle: Arc::clone(&spawned.handle),
                journal: Arc::clone(&journal),
                pid,
                stopped: None,
                input,
            },
        );
        let host = Arc::downgrade(&self.0);
        let id = run.id.clone();
        let command_uid = run.command.clone();
        let changes = Arc::clone(&self.0.changes);
        if let Err(error) = thread::Builder::new()
            .name(format!("command-{id}"))
            .spawn(move || {
                let error = collect(
                    spawned.reader,
                    &journal,
                    &spawned.handle,
                    pid,
                    &changes,
                    &command_uid,
                    &id,
                );
                let mut child = spawned.child;
                let result = child.wait();
                spawned.handle.exited.store(true, Release);
                let exit_code = result.as_ref().ok().map(|status| status.exit_code());
                let mut error = error.or_else(|| result.err().map(|error| error.to_string()));
                if let Some(host) = host.upgrade() {
                    if let Ok(mut runs) = host.runs.lock() {
                        if let Some(active) = runs.get_mut(&id) {
                            error = active.stopped.take().or(error);
                            if let Ok(mut file) = journal.lock() {
                                let _ = journal::append(
                                    &mut file,
                                    &Event::Finished {
                                        finished_ms: now(),
                                        exit_code,
                                        error,
                                    },
                                );
                                let _ = file.sync_data();
                            }
                        }
                        runs.remove(&id);
                    }
                    changes.notify(&command_uid, &id, true);
                    let host = CommandHost(host);
                    let _ = host.prune(&command_uid);
                }
            })
        {
            if let Some(active) = runs.remove(&run.id) {
                stop(&active.handle, active.pid);
            }
            self.0.changes.notify(&run.command, &run.id, true);
            return Err(format!("Cannot monitor command: {error}"));
        }
        drop(runs);
        self.0.changes.notify(&run.command, &run.id, true);
        Ok(Response::Started { run })
    }

    fn history(&self, command: &str) -> Result<Vec<Run>, String> {
        let directory = journal::directory(&self.0.root, command)?;
        let active = self.0.runs.lock().map_err(|error| error.to_string())?;
        journal::history(&directory, command, |id| active.contains_key(id))
    }

    fn prune(&self, command: &str) -> Result<(), String> {
        let runs = self.history(command)?;
        let _active = self.0.runs.lock().map_err(|error| error.to_string())?;
        journal::prune_history(&self.0.root, command, &runs)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        if let Ok(runs) = self.runs.get_mut() {
            for active in runs.values() {
                stop(&active.handle, active.pid);
            }
        }
    }
}

fn collect(
    mut reader: Box<dyn Read + Send>,
    journal: &Mutex<File>,
    handle: &TerminalHandle,
    pid: Option<u32>,
    changes: &changes::Registry,
    command: &str,
    run: &str,
) -> Option<String> {
    let mut bytes = [0; OUTPUT_CHUNK_BYTES];
    let mut total = 0;
    loop {
        let count = match reader.read(&mut bytes) {
            Ok(0) => return None,
            Ok(count) => count,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) if error.raw_os_error() == Some(CLOSED_PTY_ERROR) => return None,
            Err(error) => {
                stop(handle, pid);
                return Some(error.to_string());
            }
        };
        let Some(read) = bytes.get(..count) else {
            stop(handle, pid);
            return Some("Terminal reader returned an invalid byte count".into());
        };
        total += read.len();
        let result = journal
            .lock()
            .map_err(|error| error.to_string())
            .and_then(|mut file| {
                journal::append(
                    &mut file,
                    &Event::Output {
                        data_base64: BASE64.encode(read),
                    },
                )
            });
        if let Err(error) = result {
            stop(handle, pid);
            return Some(error);
        }
        changes.notify(command, run, false);
        if total >= OUTPUT_LIMIT {
            stop(handle, pid);
            return Some("Stopped at the 256 MiB output limit".into());
        }
    }
}

fn stop(handle: &TerminalHandle, pid: Option<u32>) {
    if handle.exited.load(Acquire) {
        return;
    }
    #[cfg(unix)]
    if let Some(pid) = pid.and_then(|pid| libc::pid_t::try_from(pid).ok())
        && pid > 1
    {
        // SAFETY: negating this positive child PID selects its process group.
        // kill takes only scalar arguments and does not access caller memory.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    let _ = pid;
    if let Ok(mut killer) = handle.killer.lock() {
        let _ = killer.kill();
    }
}

fn shell_command(shell: &str, script: &str, cwd: &Path) -> CommandBuilder {
    let mut command = CommandBuilder::new(shell);
    #[cfg(unix)]
    command.args(["-l", "-i", "-c", script]);
    #[cfg(windows)]
    command.args(["/C", script]);
    command.cwd(cwd);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command
}

fn working_directory(value: &str) -> Result<PathBuf, String> {
    let path = if value.trim().is_empty() || value == "~" {
        dirs::home_dir().ok_or("The home directory is unavailable")?
    } else if let Some(relative) = value.strip_prefix("~/") {
        dirs::home_dir()
            .ok_or("The home directory is unavailable")?
            .join(relative)
    } else {
        Path::new(value).to_path_buf()
    };
    if !path.is_absolute() || !path.is_dir() {
        return Err("Choose an existing absolute directory, or ~ for home".into());
    }
    path.canonicalize().map_err(|error| error.to_string())
}

fn now() -> u64 {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
}
