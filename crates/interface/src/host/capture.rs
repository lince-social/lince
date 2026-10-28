use super::{OutputId, SurfaceId};
use std::{collections::{BTreeMap, BTreeSet}, sync::{Arc, RwLock, RwLockReadGuard}};

pub const MAX_CONTRIBUTORS: usize = 262_144;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target { Output(OutputId), Window(SurfaceId) }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub id: u64,
    pub connection: u64,
    pub target: Target,
    pub label: String,
    pub cursor: bool,
    pub paused: bool,
}

#[cfg_attr(feature = "ui", derive(bevy::prelude::Resource))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub available: bool,
    pub paused: bool,
    pub sessions: Vec<Session>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Policy {
    pub windows: BTreeMap<SurfaceId, bool>,
    pub excluded: BTreeSet<[u8; 16]>,
    pub private_entities: BTreeSet<u64>,
    pub unresolved_images: bool,
}

#[cfg_attr(feature = "ui", derive(bevy::prelude::Resource))]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Demand {
    pub outputs: BTreeSet<OutputId>,
    pub windows: BTreeSet<SurfaceId>,
}

impl Demand {
    pub fn is_empty(&self) -> bool { self.outputs.is_empty() && self.windows.is_empty() }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mapping {
    pub output: OutputId,
    pub bounds: super::Rectangle,
    pub usable: Option<super::Rectangle>,
    pub physical_size: [u32; 2],
    pub raster: super::raster::RasterMapping,
}

impl From<&super::OutputViewport> for Mapping {
    fn from(output: &super::OutputViewport) -> Self {
        Self { output: output.id, bounds: output.bounds, usable: output.usable, physical_size: output.physical_size, raster: output.raster }
    }
}

struct State {
    revision: u64,
    enforced: bool,
    policy: Option<Policy>,
}

#[cfg_attr(feature = "ui", derive(bevy::prelude::Resource))]
#[derive(Clone)]
pub struct PolicyGate(Arc<RwLock<State>>);

impl Default for PolicyGate {
    fn default() -> Self {
        Self(Arc::new(RwLock::new(State { revision: 1, enforced: false, policy: None })))
    }
}

pub struct PolicyRead<'a>(RwLockReadGuard<'a, State>);

impl PolicyGate {
    pub fn invalidate(&self) {
        if let Ok(mut state) = self.0.write() {
            state.revision = state.revision.checked_add(1).expect("capture security revision");
            state.enforced = true;
            state.policy = None;
        }
    }

    pub fn publish(&self, policy: Policy) {
        if let Ok(mut state) = self.0.write()
            && (!state.enforced || state.policy.as_ref() != Some(&policy))
        {
            state.revision = state.revision.checked_add(1).expect("capture security revision");
            state.enforced = true;
            state.policy = Some(policy);
        }
    }

    pub fn read(&self) -> Option<PolicyRead<'_>> {
        self.0.read().ok().map(PolicyRead)
    }
}

impl PolicyRead<'_> {
    pub fn revision(&self) -> u64 { self.0.revision }

    pub fn knows(&self, surface: SurfaceId) -> bool {
        !self.0.enforced || self.0.policy.as_ref().is_some_and(|policy| policy.windows.contains_key(&surface))
    }

    pub fn policy(&self) -> Option<&Policy> { self.0.policy.as_ref() }

    pub fn requires_filter(&self) -> bool {
        self.0.policy.as_ref().is_some_and(|policy| policy.unresolved_images || !policy.excluded.is_empty() || !policy.private_entities.is_empty() || policy.windows.values().any(|excluded| *excluded))
    }

    pub fn permits(&self, target: Target) -> bool {
        if !self.0.enforced { return true; }
        let Some(policy) = &self.0.policy else { return false; };
        match target {
            Target::Window(surface) => policy.windows.get(&surface) == Some(&false),
            Target::Output(_) => true,
        }
    }

    pub fn matches(&self, target: Target, revision: u64) -> bool {
        self.revision() == revision && self.permits(target)
    }
}

#[cfg(test)]
mod tests;
