use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    sync::Mutex as SyncMutex,
};
use zeroize::Zeroize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalRequest {
    pub record: String,
    pub login: String,
    pub offset: u64,
    pub input: crate::config::Secret,
    pub cols: u16,
    pub rows: u16,
    pub close: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalFrame {
    pub next: u64,
    pub data_base64: String,
    pub trimmed: bool,
    pub exit: Option<u32>,
    pub error: Option<String>,
}

#[derive(Default)]
struct State {
    bytes: VecDeque<u8>,
    end: u64,
    exit: Option<u32>,
    error: Option<String>,
}

pub struct LoginTerminal {
    state: Arc<SyncMutex<State>>,
    input: std::sync::mpsc::SyncSender<Vec<u8>>,
    master: SyncMutex<Box<dyn MasterPty + Send>>,
    killer: SyncMutex<Box<dyn ChildKiller + Send + Sync>>,
}

impl Drop for LoginTerminal {
    fn drop(&mut self) {
        self.close();
    }
}

impl LoginTerminal {
    pub fn start(config: &Config, method: &AuthMethodTerminal) -> Result<Arc<Self>, String> {
        if method.args.len() > 64
            || method.env.len() > 64
            || serde_json::to_vec(method)
                .map_err(|error| error.to_string())?
                .len()
                > 32_768
        {
            return Err("The agent's terminal login command is too large.".into());
        }
        let mut validated = config.clone();
        validated.validate()?;
        let pair = native_pty_system()
            .openpty(size(80, 24)?)
            .map_err(|error| error.to_string())?;
        let mut command = CommandBuilder::new(launch::resolve(&validated)?);
        command.args(&config.args);
        command.args(&method.args);
        command.cwd(&config.directory);
        for (key, value) in launch::environment(config) {
            command.env(key, value);
        }
        for (key, value) in &method.env {
            command.env(key, value);
        }
        command.env("TERM", "xterm-256color");
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| error.to_string())?;
        let mut writer = pair
            .master
            .take_writer()
            .map_err(|error| error.to_string())?;
        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| error.to_string())?;
        drop(pair.slave);
        let killer = child.clone_killer();
        let state = Arc::new(SyncMutex::new(State::default()));
        let (input, received) = std::sync::mpsc::sync_channel::<Vec<u8>>(16);
        let terminal = Arc::new(Self {
            state: state.clone(),
            input,
            master: SyncMutex::new(pair.master),
            killer: SyncMutex::new(killer),
        });
        let output = state.clone();
        std::thread::Builder::new()
            .name("login-output".into())
            .spawn(move || {
                let mut bytes = [0; 8192];
                loop {
                    match reader.read(&mut bytes) {
                        Ok(0) | Err(_) => break,
                        Ok(count) => {
                            let mut state = output.lock().unwrap();
                            state.bytes.extend(&bytes[..count]);
                            state.end += count as u64;
                            let excess = state.bytes.len().saturating_sub(262_144);
                            state.bytes.drain(..excess);
                        }
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        let input_state = state.clone();
        std::thread::Builder::new()
            .name("login-input".into())
            .spawn(move || {
                for mut bytes in received {
                    let result = writer.write_all(&bytes).and_then(|()| writer.flush());
                    bytes.zeroize();
                    if let Err(error) = result {
                        input_state.lock().unwrap().error = Some(error.to_string());
                        break;
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        std::thread::Builder::new()
            .name("login-wait".into())
            .spawn(move || match child.wait() {
                Ok(exit) => state.lock().unwrap().exit = Some(exit.exit_code()),
                Err(error) => state.lock().unwrap().error = Some(error.to_string()),
            })
            .map_err(|error| error.to_string())?;
        Ok(terminal)
    }

    pub fn exchange(&self, request: &TerminalRequest) -> Result<TerminalFrame, String> {
        let size = size(request.cols, request.rows)?;
        if request.input.0.len() > 5464 {
            return Err("Login input exceeds 4 KiB.".into());
        }
        let mut bytes = BASE64
            .decode(&request.input.0)
            .map_err(|_| "Invalid login input.")?;
        if bytes.len() > 4096 {
            bytes.zeroize();
            return Err("Login input exceeds 4 KiB.".into());
        }
        if request.close {
            bytes.zeroize();
            self.close();
        } else {
            self.master
                .lock()
                .unwrap()
                .resize(size)
                .map_err(|error| error.to_string())?;
            if !bytes.is_empty() {
                if let Err(error) = self.input.try_send(bytes) {
                    let mut bytes = match error {
                        std::sync::mpsc::TrySendError::Full(bytes)
                        | std::sync::mpsc::TrySendError::Disconnected(bytes) => bytes,
                    };
                    bytes.zeroize();
                    return Err("Login input is busy. Try again.".into());
                }
            }
        }
        let state = self.state.lock().unwrap();
        let start = state.end - state.bytes.len() as u64;
        let offset = request.offset.clamp(start, state.end);
        let bytes: Vec<_> = state
            .bytes
            .iter()
            .skip((offset - start) as usize)
            .take(32_768)
            .copied()
            .collect();
        Ok(TerminalFrame {
            next: offset + bytes.len() as u64,
            data_base64: BASE64.encode(bytes),
            trimmed: request.offset < start,
            exit: state.exit,
            error: state.error.clone(),
        })
    }

    pub fn result(&self) -> Option<Result<(), String>> {
        let state = self.state.lock().unwrap();
        state.error.clone().map(Err).or_else(|| {
            state.exit.map(|code| {
                if code == 0 {
                    Ok(())
                } else {
                    Err(format!("Login command exited with status {code}."))
                }
            })
        })
    }

    pub fn close(&self) {
        let _ = self.killer.lock().unwrap().kill();
    }
}

fn size(cols: u16, rows: u16) -> Result<PtySize, String> {
    if !(10..=240).contains(&cols) || !(4..=100).contains(&rows) {
        return Err("Invalid login terminal size.".into());
    }
    Ok(PtySize {
        cols,
        rows,
        pixel_width: 0,
        pixel_height: 0,
    })
}
