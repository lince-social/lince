use super::{OutputId, Rectangle, SurfaceId};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Caret { Application { surface: SurfaceId, rectangle: Rectangle }, Layer { output: OutputId, rectangle: Rectangle } }

pub const MINIMUM: f64 = 1.0;
pub const MAXIMUM: f64 = 4.0;
pub const STEP: f64 = 0.25;

pub fn valid(factor: f64) -> bool { factor.is_finite() && (MINIMUM..=MAXIMUM).contains(&factor) }

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Adjustment { Increase, Decrease, Reset }

impl Adjustment {
    pub fn factor(self, current: f64) -> f64 {
        match self { Self::Increase => (current + STEP).min(MAXIMUM), Self::Decrease => (current - STEP).max(MINIMUM), Self::Reset => MINIMUM }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Head {
    pub output: OutputId,
    pub name: String,
    pub requested: f64,
    pub effective: Option<f64>,
}

#[cfg_attr(feature = "ui", derive(bevy::prelude::Resource))]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub available: bool,
    pub revision: u64,
    pub heads: Vec<Head>,
    pub last_request: u64,
    pub response: Option<Response>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub id: u64,
    pub revision: u64,
    pub output: OutputId,
    pub factor: f64,
    pub context: super::control::Context,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reload {
    pub id: u64,
    pub context: super::control::Context,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome { Saved { warning: Option<String> }, Loaded, Failed(String) }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub request: u64,
    pub outcome: Outcome,
}

impl Snapshot {
    pub fn head(&self, output: OutputId) -> Option<&Head> { self.heads.iter().find(|head| head.output == output) }
}
