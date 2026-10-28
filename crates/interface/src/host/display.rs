use super::{OutputId, OutputViewport};
use std::{collections::BTreeMap, time::Instant};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ColorStatus {
    pub hdr10: bool,
    pub prepared: Option<super::color::OutputColor>,
    pub effective: Option<super::color::OutputColor>,
    pub peak_reported: bool,
    pub revision: u64,
    pub pending: bool,
    pub reason: Option<String>,
}

pub fn color_confirmation_ready(heads: &[OutputHead], status: &BTreeMap<OutputId, ColorStatus>) -> bool {
    heads.iter().filter(|head| head.current.is_some() && head.color == super::color::ColorMode::Hdr10).all(|head| {
        status.get(&head.id).is_some_and(|status| !status.pending
            && status.prepared.is_some_and(|prepared| prepared.mode == head.color)
            && status.effective == status.prepared)
    })
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AdaptiveSyncSupport {
    #[default]
    Unknown,
    Unsupported,
    Supported,
    RequiresModeset,
}

impl AdaptiveSyncSupport {
    pub fn available(self) -> bool { matches!(self, Self::Supported | Self::RequiresModeset) }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AdaptiveSyncStatus {
    pub support: AdaptiveSyncSupport,
    pub prepared: Option<bool>,
    pub effective: Option<bool>,
    pub pending: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputMode {
    pub size: [i32; 2],
    pub refresh_mhz: i32,
    pub preferred: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutputHead {
    pub id: OutputId,
    pub name: String,
    pub modes: Vec<OutputMode>,
    pub current: Option<usize>,
    pub position: [i32; 2],
    pub scale: f64,
    pub rotation: super::mapping::QuarterTurn,
    pub adaptive_sync: bool,
    pub color: super::color::ColorMode,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutputSettings {
    pub mode: OutputMode,
    pub position: [i32; 2],
    pub scale: f64,
    pub rotation: super::mapping::QuarterTurn,
    pub adaptive_sync: bool,
    pub color: super::color::ColorMode,
}

impl OutputSettings {
    pub fn logical_size(&self) -> [i32; 2] {
        let mut size = self.mode.size;
        if self.rotation.swaps_axes() { size.swap(0, 1); }
        size.map(|size| (f64::from(size) / self.scale).round() as i32)
    }
}

pub type DisplayConfiguration = BTreeMap<OutputId, Option<OutputSettings>>;

#[derive(Clone, Debug, PartialEq)]
pub enum DisplayAction {
    Test {
        revision: u64,
        heads: DisplayConfiguration,
    },
    Preview {
        revision: u64,
        heads: DisplayConfiguration,
    },
    Keep {
        token: u64,
    },
    Revert {
        token: u64,
    },
    Cancel {
        request: u64,
    },
    Identify {
        output: OutputId,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct DisplayRequest {
    pub id: u64,
    pub action: DisplayAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisplayOutcome {
    Pending,
    Tested,
    Previewed,
    Kept,
    Reverted,
    Identified,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayResponse {
    pub request: u64,
    pub outcome: DisplayOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayPreview {
    pub token: u64,
    pub deadline: Instant,
    pub reverting: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayIdentification {
    pub output: OutputId,
    pub deadline: Instant,
}

#[cfg_attr(feature = "ui", derive(bevy::prelude::Resource))]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplaySnapshot {
    pub revision: u64,
    pub heads: Vec<OutputHead>,
    pub adaptive_sync: BTreeMap<OutputId, AdaptiveSyncStatus>,
    pub color: BTreeMap<OutputId, ColorStatus>,
    pub outputs: Vec<OutputViewport>,
    pub virtual_bounds: Option<[u32; 2]>,
    pub preview: Option<DisplayPreview>,
    pub identification: Option<DisplayIdentification>,
    pub response: Option<DisplayResponse>,
    pub last_request: u64,
}

impl OutputHead {
    pub fn settings(&self) -> Option<OutputSettings> {
        Some(OutputSettings {
            mode: self.modes.get(self.current?)?.clone(),
            position: self.position,
            scale: self.scale,
            rotation: self.rotation,
            adaptive_sync: self.adaptive_sync,
            color: self.color,
        })
    }
}

impl DisplaySnapshot {
    pub fn configuration(&self) -> DisplayConfiguration {
        self.heads
            .iter()
            .map(|head| (head.id, head.settings()))
            .collect()
    }
}
