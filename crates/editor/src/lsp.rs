// non-lean-yet
// Scope: mod protocol, mod session, enum Command, struct Completion.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::missing_panics_doc] State the panic conditions for exported functions. Sites
//   112.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 12, 47.
// - [Clippy: clippy::expect_used] Check the stated invariant and handle reachable failures. Sites
//   113.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 85, 106.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 12, 34, 40, 47, 78.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 1, 12,
//   13, 14, 15, 16, 17, 18, 20, 21, 22, 23, 24, 25, 27, 28, 29, 34, 35, 36, 40, 41, 42, 43, 44, 47,
//   48, 49, 51, 52, 53, 54, 55, 57, 58, 59, 60, 61, 62, 64, 65, 66, 67, 68, 69, 71, 72, 73, 75, 78,
//   85, 106, 112.

pub mod protocol;
mod session;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use ropey::Rope;
use tokio::sync::mpsc;

use crate::{Edit, Result};

pub enum Command {
    Sync {
        path: PathBuf,
        language: String,
        text: Rope,
        identity: u64,
        revision: u64,
    },
    Close(PathBuf),
    Saved(PathBuf),
    Complete {
        path: PathBuf,
        position: usize,
        token: u64,
    },
    Format {
        path: PathBuf,
        token: u64,
    },
}

#[derive(Clone)]
pub struct Completion {
    pub label: String,
    pub edits: Vec<Edit>,
}

#[derive(Clone)]
pub struct Diagnostic {
    pub position: usize,
    pub line: usize,
    pub severity: u64,
    pub message: String,
}

pub enum Event {
    Ready {
        formatting: bool,
    },
    Diagnostics {
        path: PathBuf,
        identity: u64,
        revision: u64,
        items: Vec<Diagnostic>,
    },
    Completions {
        token: u64,
        path: PathBuf,
        identity: u64,
        revision: u64,
        items: Vec<Completion>,
    },
    Formatted {
        token: u64,
        path: PathBuf,
        identity: u64,
        revision: u64,
        edits: Vec<Edit>,
    },
    RequestError {
        token: u64,
        message: String,
    },
    Failed(String),
}

pub struct Client {
    commands: mpsc::Sender<Command>,
    events: Mutex<mpsc::Receiver<Event>>,
    task: tokio::task::JoinHandle<()>,
}

impl Client {
    pub fn start(
        command: Vec<String>,
        root: PathBuf,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self> {
        let (commands, requests) = mpsc::channel(16);
        let (events, replies) = mpsc::channel(32);
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        let task = crate::tooling::spawn(async move {
            if let Err(error) = session::run(command, root, requests, &events, &wake).await {
                let _ = events.try_send(Event::Failed(error));
                wake();
            }
        })?;
        Ok(Self {
            commands,
            events: Mutex::new(replies),
            task,
        })
    }

    pub fn send(&self, command: Command) -> Result<()> {
        self.commands
            .try_send(command)
            .map_err(|error| format!("Language server is busy or stopped: {error}"))
    }

    pub fn drain(&self) -> Vec<Event> {
        let mut receiver = self.events.lock().expect("language events");
        let mut events = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            events.push(event);
        }
        events
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.task.abort();
    }
}
