//! Renderer-independent protocol for bounded interface checks.
// non-lean-yet

use std::time::Duration;

mod cancellation;
mod runner;

pub use cancellation::Cancellation;
pub use runner::Runner;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InterfaceSelection {
    #[default]
    All,
    Protein,
    Edit,
    Input,
}

impl InterfaceSelection {
    pub const ENV: &str = "LINCE_INTERFACE_SELECTION";

    pub const fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Protein => "protein",
            Self::Edit => "edit",
            Self::Input => "input",
        }
    }
}

impl std::str::FromStr for InterfaceSelection {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "all" => Ok(Self::All),
            "protein" => Ok(Self::Protein),
            "edit" => Ok(Self::Edit),
            "input" => Ok(Self::Input),
            _ => Err(format!(
                "Unknown interface selection {value:?}. Use all, protein, edit or input."
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub timeout: Duration,
    pub max_polls: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(2),
            max_polls: 8,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector {
    pub name: String,
    pub role: Option<String>,
}

impl Selector {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            role: None,
        }
    }

    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.role = Some(role.into());
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Primary,
    Fallback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dispatch {
    Accepted,
    Rejected(String),
    Unavailable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    Rejected,
    Unavailable,
    Timeout,
    PollLimit,
    Cancelled,
    BackendError(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Snapshot,
    Dispatch { route: Route, result: Dispatch },
    Condition { matched: bool },
    Pump,
    Finished(Outcome),
}

#[derive(Clone, Debug, Default)]
pub struct Metrics {
    pub events: usize,
    pub pumps: usize,
    pub polls: usize,
    pub snapshots: usize,
    pub dispatches: usize,
    pub elapsed: Duration,
}

#[derive(Clone, Debug)]
pub struct Report {
    pub step: String,
    pub selector: Selector,
    pub outcome: Outcome,
    pub metrics: Metrics,
    pub events: Vec<Event>,
    pub diagnostics: String,
}

impl Report {
    pub fn is_success(&self) -> bool {
        self.outcome == Outcome::Passed
    }
}

#[derive(Clone, Debug)]
pub struct Step<A, C> {
    pub name: String,
    pub selector: Selector,
    pub action: Option<A>,
    pub condition: C,
    pub fallback: bool,
}

impl<A, C> Step<A, C> {
    pub fn action(name: impl Into<String>, selector: Selector, action: A, condition: C) -> Self {
        Self {
            name: name.into(),
            selector,
            action: Some(action),
            condition,
            fallback: false,
        }
    }

    pub fn observe(name: impl Into<String>, selector: Selector, condition: C) -> Self {
        Self {
            name: name.into(),
            selector,
            action: None,
            condition,
            fallback: false,
        }
    }

    pub fn with_fallback(mut self) -> Self {
        self.fallback = true;
        self
    }
}

pub trait Backend {
    type Snapshot;
    type Action;
    type Condition;

    fn snapshot(&mut self, remaining: Duration) -> Result<Self::Snapshot, String>;
    fn dispatch(
        &mut self,
        snapshot: &Self::Snapshot,
        selector: &Selector,
        action: &Self::Action,
        route: Route,
        remaining: Duration,
    ) -> Dispatch;
    fn condition(
        &self,
        snapshot: &Self::Snapshot,
        condition: &Self::Condition,
    ) -> Result<bool, String>;
    fn pump(&mut self, remaining: Duration) -> Result<(), String>;

    fn diagnostics(&self) -> String {
        String::new()
    }

    fn finish(&mut self) {}
}
