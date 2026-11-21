// non-lean-yet
// Scope: enum Request, struct Run, enum Event, enum Response.
// BeautifulRust (baseline lines):
// - [Clippy probe: clippy::exhaustive_enums] Review whether public API evolution requires
//   non_exhaustive. Sites 5, 63, 83.
// - [rustc probe: missing_docs] Document the externally visible contracts where applicable. Sites
//   5, 6, 7, 8, 9, 11, 12, 14, 15, 16, 17, 19, 20, 21, 22, 24, 25, 26, 27, 28, 30, 31, 32, 37, 50,
//   51, 52, 53, 54, 55, 56, 57, 58, 63, 64, 65, 67, 68, 70, 71, 72, 74, 75, 76, 77, 83, 84, 85, 87,
//   88, 90, 91, 92, 93, 95.

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
