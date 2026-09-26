use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum Request {
    Run {
        command: String,
        script: String,
        cwd: String,
    },
    History {
        command: String,
    },
    Read {
        command: String,
        run: String,
        offset: u64,
    },
    Input {
        command: String,
        run: String,
        data_base64: String,
    },
    Resize {
        command: String,
        run: String,
        cols: u16,
        rows: u16,
    },
    Stop {
        command: String,
        run: String,
    },
}

impl Request {
    pub fn command(&self) -> &str {
        match self {
            Self::Run { command, .. }
            | Self::History { command }
            | Self::Read { command, .. }
            | Self::Input { command, .. }
            | Self::Resize { command, .. }
            | Self::Stop { command, .. } => command,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub command: String,
    pub id: String,
    pub script: String,
    pub cwd: String,
    pub started_ms: u64,
    pub finished_ms: Option<u64>,
    pub exit_code: Option<u32>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Started {
        run: Run,
    },
    Output {
        data_base64: String,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    Finished {
        finished_ms: u64,
        exit_code: Option<u32>,
        error: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Response {
    Started {
        run: Run,
    },
    History {
        runs: Vec<Run>,
    },
    Output {
        events: Vec<Event>,
        next_offset: u64,
        complete: bool,
    },
    Ok,
}
