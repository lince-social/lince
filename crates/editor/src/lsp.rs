pub mod protocol;
mod session;

use crate::{Edit, Result};
use ropey::Rope;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc;

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
