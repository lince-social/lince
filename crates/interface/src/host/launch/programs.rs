use super::Description;
use crate::host::input::{InputRequest, InputResponse};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Program {
    Terminal,
    Launcher,
    Lock,
    Startup,
}

impl Program {
    pub const ALL: [Self; 4] = [Self::Terminal, Self::Launcher, Self::Lock, Self::Startup];

    pub fn key(self) -> &'static str {
        match self { Self::Terminal => "terminal", Self::Launcher => "launcher", Self::Lock => "lock", Self::Startup => "ready" }
    }

    pub fn label(self) -> &'static str {
        match self { Self::Terminal => "Terminal", Self::Launcher => "Application launcher", Self::Lock => "Screen locker", Self::Startup => "Session startup" }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Programs {
    pub terminal: Option<Description>,
    pub launcher: Option<Description>,
    pub lock: Option<Description>,
    pub ready: Option<Description>,
}

impl Programs {
    pub fn get(&self, program: Program) -> Option<&Description> {
        match program { Program::Terminal => &self.terminal, Program::Launcher => &self.launcher,
            Program::Lock => &self.lock, Program::Startup => &self.ready }.as_ref()
    }

    pub fn set(&mut self, program: Program, value: Option<Description>) {
        *match program { Program::Terminal => &mut self.terminal, Program::Launcher => &mut self.launcher,
            Program::Lock => &mut self.lock, Program::Startup => &mut self.ready } = value;
    }

    pub fn validate(&self) -> Result<(), String> {
        for program in Program::ALL {
            if let Some(value) = self.get(program)
                && (!value.valid() || value.arguments.len() > 64 || value.arguments.iter().any(|argument| argument.len() > 4096)
                    || value.directory.as_ref().is_some_and(|directory| !directory.is_absolute() || directory.as_os_str().len() > 4096 || directory.to_string_lossy().contains('\0'))) {
                return Err(format!("{} needs an executable name, up to 64 literal arguments, and an optional absolute working directory.", program.label()));
            }
        }
        Ok(())
    }
}

pub type Request = InputRequest<Programs>;

#[cfg_attr(feature = "ui", derive(bevy::prelude::Resource))]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProgramsSnapshot {
    pub available: bool,
    pub revision: u64,
    pub settings: Programs,
    pub last_request: u64,
    pub response: Option<InputResponse>,
}
