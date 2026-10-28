use serde::{Deserialize, Serialize};

pub const ARGUMENT_LIMIT: usize = 16 * 1024;
pub const IN_FLIGHT_LIMIT: usize = 16;
pub const EVENT_LIMIT: usize = 256;
pub const REQUESTS_PER_SECOND: u32 = 64;
pub const REQUEST_BURST: u32 = 32;
pub const RESPONSE_LIMIT: usize = 48 * 1024;
pub const CHANGE_WINDOW: u32 = 1;
pub const CHANGE_WORKSPACE: u32 = 2;
pub const CHANGE_OUTPUT: u32 = 4;
pub const CHANGE_CONTEXT: u32 = 8;
pub const CHANGE_COMMANDS: u32 = 16;
pub const CHANGE_RESYNC: u32 = 32;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub id: String,
    pub label: String,
    pub unavailable: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub context: Option<Context>,
    pub commands: Vec<Command>,
}

#[cfg_attr(feature = "ui", derive(bevy::prelude::Resource))]
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub available: bool,
    pub active: bool,
    pub readers: u16,
    pub controllers: u16,
    pub pending: u16,
    pub locked: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Mode {
    pub size: [i32; 2],
    pub refresh_mhz: i32,
    pub preferred: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Entry {
    Session { locked: bool, active: bool },
    Context { value: Option<Context> },
    Command { value: Command },
    Window { surface: u64, title: String, app_id: String, parent: Option<u64> },
    Workspace { id: u64, name: String, active: bool, output: Option<u64> },
    Output { id: u64, name: String, position: [i32; 2], size: [u32; 2], scale: f64, rotation: Option<super::mapping::QuarterTurn>, adaptive_sync_requested: bool, adaptive_sync: super::display::AdaptiveSyncStatus, enabled: bool, refresh_mhz: Option<i32>, modes: Vec<Mode> },
}

impl Entry {
    pub fn scope(&self) -> u32 {
        match self {
            Self::Context { .. } | Self::Session { .. } => CHANGE_CONTEXT,
            Self::Command { .. } => CHANGE_COMMANDS,
            Self::Window { .. } => CHANGE_WINDOW,
            Self::Workspace { .. } => CHANGE_WORKSPACE,
            Self::Output { .. } => CHANGE_OUTPUT,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Disposition {
    Applied,
    Dispatched,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Response {
    Snapshot { revision: u64, next: Option<u32>, entries: Vec<Entry> },
    Complete { outcome: Disposition },
    Failed { failure: Failure },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Item {
    Native { placement: [u8; 16] },
    Application { placement: [u8; 16], surface: u64 },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub owner: u64,
    pub canvas: String,
    pub workspace: u64,
    pub output: Option<u64>,
    pub item: Option<Item>,
}

impl Context {
    pub fn valid(&self) -> bool {
        self.owner != 0 && self.workspace != 0 && !self.canvas.is_empty() && self.canvas.len() <= 512
            && !self.canvas.chars().any(char::is_control) && self.output.is_none_or(|output| output != 0)
            && !matches!(self.item, Some(Item::Application { surface: 0, .. }))
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Arguments {
    #[default]
    None,
    Workspace { id: u64 },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Invocation {
    pub command: String,
    pub context: Context,
    #[serde(default)]
    pub arguments: Arguments,
}

impl Invocation {
    pub fn validate(&self) -> Result<(), Failure> {
        if self.command.is_empty() || self.command.len() > 128
            || !self.command.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-'))
            || !self.context.valid() || matches!(self.arguments, Arguments::Workspace { id: 0 })
        { return Err(Failure::new(Error::InvalidArguments, "Use a registered command and a current published target.")); }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Error {
    UnknownCommand,
    InvalidArguments,
    Denied,
    StaleTarget,
    Unavailable,
    Busy,
    RateLimited,
    Cancelled,
    Locked,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Failure {
    pub code: Error,
    pub message: String,
}

impl Failure {
    pub fn new(code: Error, message: impl AsRef<str>) -> Self {
        let message = message.as_ref();
        let mut end = message.len().min(4096);
        while !message.is_char_boundary(end) { end -= 1; }
        Self { code, message: message[..end].into() }
    }
}

#[cfg(test)]
mod tests;
