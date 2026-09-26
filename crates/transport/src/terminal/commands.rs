mod journal;
#[cfg(test)]
mod tests;

use super::*;
use crate::command::{Event, Request, Response, Run};
use std::{
    fs::File,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
pub struct CommandHost(Arc<Host>);

struct Host {
    root: PathBuf,
    runs: Mutex<HashMap<String, Active>>,
}

struct Active {
    run: Run,
    handle: Arc<TerminalHandle>,
    journal: Arc<Mutex<File>>,
    pid: Option<u32>,
    stopped: Option<String>,
    input: std::sync::mpsc::SyncSender<Vec<u8>>,
}

impl Default for CommandHost {
    fn default() -> Self {
        Self::new(utils::config::lince_data_dir().unwrap_or_default())
    }
}

impl CommandHost {
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
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    pub fn new(root: PathBuf) -> Self {
        Self(Arc::new(Host {
            root,
            runs: Mutex::new(HashMap::new()),
        }))
    }

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
            } => self.start(command, script, cwd),
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
                    .is_some_and(|active| active.run.command == command);
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
                if data_base64.len() > MAX_INPUT_BYTES.div_ceil(3) * 4 {
                    return Err("Terminal input is too large".into());
                }
                let bytes = BASE64
                    .decode(data_base64)
                    .map_err(|error| error.to_string())?;
                if bytes.len() > MAX_INPUT_BYTES {
                    return Err("Terminal input is too large".into());
                }
                let runs = self.0.runs.lock().map_err(|error| error.to_string())?;
                let active = runs
                    .get(&run)
                    .filter(|active| active.run.command == command)
                    .ok_or("The command has finished")?;
                active
                    .input
                    .try_send(bytes)
                    .map_err(|_| "Command input is busy or closed".to_string())?;
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
                    .filter(|active| active.run.command == command)
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
            .filter(|active| active.run.command == command)
            .ok_or("The command has finished")?;
        Ok((active.handle.clone(), active.journal.clone()))
    }

    fn start(&self, command: String, script: String, cwd: String) -> Result<Response, String> {
        if script.trim().is_empty() || script.len() > 65536 || script.contains('\0') {
            return Err("Enter a Bash script of at most 65536 bytes".into());
        }
        let cwd = working_directory(&cwd)?;
        let mut runs = self.0.runs.lock().map_err(|error| error.to_string())?;
        if runs.len() >= 16 || runs.values().any(|active| active.run.command == command) {
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
        let mut command = CommandBuilder::new("bash");
        command.args(["--noprofile", "--norc", "-c", &run.script]);
        command.cwd(&cwd);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.env_remove("BASH_ENV");
        command.env_remove("ENV");
        let spawned = match spawn_command(
            pty_size(80, 24, 720, 432),
            command,
            "bash".into(),
            run.cwd.clone(),
        ) {
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
                return Err(error);
            }
        };
        let pid = spawned.child.process_id();
        let journal = Arc::new(Mutex::new(file));
        let (input, incoming) = std::sync::mpsc::sync_channel::<Vec<u8>>(8);
        let writer = spawned.handle.clone();
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
                error.to_string()
            })?;
        runs.insert(
            run.id.clone(),
            Active {
                run: run.clone(),
                handle: spawned.handle.clone(),
                journal: journal.clone(),
                pid,
                stopped: None,
                input,
            },
        );
        let host = Arc::downgrade(&self.0);
        let id = run.id.clone();
        let command_uid = run.command.clone();
        if let Err(error) = thread::Builder::new()
            .name(format!("command-{id}"))
            .spawn(move || {
                let error = collect(spawned.reader, &journal, &spawned.handle, pid);
                let mut child = spawned.child;
                let result = child.wait();
                spawned
                    .handle
                    .exited
                    .store(true, std::sync::atomic::Ordering::Release);
                let exit_code = result.as_ref().ok().map(|status| status.exit_code());
                let mut error = error.or_else(|| result.err().map(|error| error.to_string()));
                if let Some(host) = host.upgrade() {
                    if let Ok(mut runs) = host.runs.lock() {
                        if let Some(active) = runs.get(&id) {
                            if let Some(stopped) = &active.stopped {
                                error = Some(stopped.clone());
                            }
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
                    let host = CommandHost(host);
                    let _ = host.prune(&command_uid);
                }
            })
        {
            if let Some(active) = runs.remove(&run.id) {
                stop(&active.handle, active.pid);
            }
            return Err(format!("Cannot monitor command: {error}"));
        }
        Ok(Response::Started { run })
    }

    fn history(&self, command: &str) -> Result<Vec<Run>, String> {
        let directory = journal::directory(&self.0.root, command)?;
        let active = self.0.runs.lock().map_err(|error| error.to_string())?;
        let mut runs = Vec::new();
        for entry in std::fs::read_dir(directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            if entry
                .path()
                .extension()
                .is_none_or(|extension| extension != "jsonl")
            {
                continue;
            }
            let mut run = journal::summary(&entry.path())?;
            if run.command != command {
                return Err("Run belongs to a different Command".into());
            }
            if run.finished_ms.is_none() && !active.contains_key(&run.id) {
                run.finished_ms = Some(run.started_ms);
                run.error = Some("Interrupted · no saved exit status".into());
            }
            runs.push(run);
        }
        runs.sort_by(|a, b| {
            b.started_ms
                .cmp(&a.started_ms)
                .then_with(|| b.id.cmp(&a.id))
        });
        Ok(runs)
    }

    fn prune(&self, command: &str) -> Result<(), String> {
        let runs = self.history(command)?;
        let _active = self.0.runs.lock().map_err(|error| error.to_string())?;
        for run in runs
            .into_iter()
            .filter(|run| run.finished_ms.is_some())
            .skip(10)
        {
            match std::fs::remove_file(journal::path(&self.0.root, command, &run.id)?) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        Ok(())
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
) -> Option<String> {
    let mut bytes = [0; 8192];
    let mut total = 0;
    loop {
        let count = match reader.read(&mut bytes) {
            Ok(0) => return None,
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if error.raw_os_error() == Some(5) => return None,
            Err(error) => {
                stop(handle, pid);
                return Some(error.to_string());
            }
        };
        total += count as u64;
        let result = journal
            .lock()
            .map_err(|error| error.to_string())
            .and_then(|mut file| {
                journal::append(
                    &mut file,
                    &Event::Output {
                        data_base64: BASE64.encode(&bytes[..count]),
                    },
                )
            });
        if let Err(error) = result {
            stop(handle, pid);
            return Some(error);
        }
        if total >= journal::OUTPUT_LIMIT {
            stop(handle, pid);
            return Some("Stopped at the 256 MiB output limit".into());
        }
    }
}

fn stop(handle: &TerminalHandle, pid: Option<u32>) {
    if handle.exited.load(std::sync::atomic::Ordering::Acquire) {
        return;
    }
    #[cfg(unix)]
    if let Some(pid) = pid.filter(|pid| *pid > 1) {
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    let _ = pid;
    if let Ok(mut killer) = handle.killer.lock() {
        let _ = killer.kill();
    }
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
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
