//! Wire messages for saved command runs and their output journals.

use serde::{Deserialize, Serialize};

/// A request scoped to a saved Command Record.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum Request {
    /// Starts a run using the supplied script and working directory.
    Run {
        command: String,
        script: String,
        cwd: String,
    },
    /// Lists the Command's retained runs.
    History { command: String },
    /// Reads events from an absolute byte offset in the run's journal.
    Read {
        command: String,
        run: String,
        offset: u64,
    },
    /// Sends Base64-encoded input bytes to a running process.
    Input {
        command: String,
        run: String,
        data_base64: String,
    },
    /// Changes the running terminal's dimensions in character cells.
    Resize {
        command: String,
        run: String,
        cols: u16,
        rows: u16,
    },
    /// Requests shutdown of a running process.
    Stop { command: String, run: String },
}

impl Request {
    /// Returns the Command Record identifier shared by all operations.
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

/// The script, working directory, and saved outcome of a single run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Run {
    /// Owning Command Record.
    pub command: String,
    /// Run identifier used for subsequent requests.
    pub id: String,
    /// Script captured at launch.
    pub script: String,
    /// Resolved working directory.
    pub cwd: String,
    /// Start timestamp in Unix milliseconds.
    pub started_ms: u64,
    /// Finish timestamp in Unix milliseconds, when available.
    pub finished_ms: Option<u64>,
    /// Process exit code, when available.
    pub exit_code: Option<u32>,
    /// Saved diagnostic for an unsuccessful or interrupted run.
    pub error: Option<String>,
}

/// An event stored in the run's line-delimited journal.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// Records the initial run metadata.
    Started { run: Run },
    /// Carries Base64-encoded terminal output bytes.
    Output { data_base64: String },
    /// Records a terminal size for output playback.
    Resize { cols: u16, rows: u16 },
    /// Records the end timestamp and any available outcome.
    Finished {
        finished_ms: u64,
        exit_code: Option<u32>,
        error: Option<String>,
    },
}

/// A result produced by the command host.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Response {
    /// Confirms a newly started run.
    Started { run: Run },
    /// Lists retained runs in newest-first order.
    History { runs: Vec<Run> },
    /// A page of events and its continuation position.
    Output {
        events: Vec<Event>,
        /// Absolute byte offset for the next read.
        next_offset: u64,
        /// Whether the host has reached the end of a finished run.
        complete: bool,
    },
    /// Acknowledges a request with no response payload.
    Ok,
}
