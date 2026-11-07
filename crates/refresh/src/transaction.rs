use std::{collections::{BTreeMap, BTreeSet}, io, path::PathBuf};
use serde::{Deserialize, Serialize};
use crate::{authority::ProcessIdentity, checkpoint::{CheckpointScope, Envelope, ExecutableIdentity, invalid}};

pub const REQUIRED_OWNERS: &[&str] = &["protocol", "graphics", "seat", "input", "interface", "terminals", "cell", "transport", "store"];

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage { Prepared, Quiescing, Checkpointed, Replacing, Restoring, Validating, Active, Recovering, Recovered, Failed }

impl Stage {
    pub fn terminal(self) -> bool { matches!(self, Self::Active | Self::Recovered | Self::Failed) }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub protocol: u32,
    pub owners: BTreeMap<String, u32>,
    pub unhandled_resources: u64,
    pub provisional_transport: bool,
    pub recovery: bool,
    pub qualification_sha256: Option<String>,
}

fn digest(value: &str) -> bool { value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) }
fn identifier(value: &str) -> bool { !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control) }

impl Capabilities {
    pub fn full_session(&self) -> bool {
        self.protocol == 1 && self.unhandled_resources == 0 && self.provisional_transport && self.recovery
            && self.qualification_sha256.as_ref().is_some_and(|value| digest(value))
            && self.owners.len() <= 64 && self.owners.iter().all(|(name, schema)| identifier(name) && *schema > 0)
            && REQUIRED_OWNERS.iter().all(|name| self.owners.contains_key(*name))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerState {
    pub schema: u32,
    pub logical_sha256: String,
    pub resources: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    pub clients: u64,
    pub surfaces: u64,
    pub mapped_windows: u64,
    pub native_items: u64,
    pub ptys: u64,
    pub children: u64,
    pub operations: u64,
    pub outputs: BTreeSet<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub id: String,
    pub owner: ProcessIdentity,
    pub image: ExecutableIdentity,
    pub generation: u64,
    pub capabilities: Capabilities,
    pub inventory: Inventory,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub transaction: String,
    pub session: String,
    pub expected_generation: u64,
    pub candidate_path: PathBuf,
    pub candidate: ExecutableIdentity,
    pub artifact_run: u64,
    pub qualification_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Presentation {
    pub output: u64,
    pub scene_epoch: u64,
    pub presented_epoch: u64,
    pub fence_completed: bool,
    pub pixels_verified: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperationalProof {
    pub checkpoint_sha256: String,
    pub owners: BTreeMap<String, OwnerState>,
    pub inventory: Inventory,
    pub presentations: Vec<Presentation>,
    pub protocol_probe: bool,
    pub native_probe: bool,
    pub child_identities_preserved: bool,
    pub provisional_io_unread: bool,
    pub locks_enforced: bool,
    pub input_state_restored: bool,
    pub irreversible_effects_started: bool,
}

impl OperationalProof {
    fn validate(&self, expected_digest: &str, owners: &BTreeMap<String, OwnerState>, inventory: &Inventory) -> io::Result<()> {
        if self.checkpoint_sha256 != expected_digest || self.owners != *owners || self.inventory != *inventory
            || !self.protocol_probe || !self.native_probe || !self.child_identities_preserved || !self.provisional_io_unread
            || !self.locks_enforced || !self.input_state_restored || self.irreversible_effects_started || self.presentations.len() > 64 {
            return Err(invalid("Operational handoff proof is incomplete or differs from its checkpoint"));
        }
        let mut outputs = BTreeSet::new();
        for presentation in &self.presentations {
            if !outputs.insert(presentation.output) || !presentation.fence_completed || !presentation.pixels_verified
                || presentation.presented_epoch < presentation.scene_epoch { return Err(invalid("Output presentation has not been verified")); }
        }
        if outputs != inventory.outputs { return Err(invalid("Operational proof does not cover every output")); }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Failure {
    pub code: String,
    pub stage: Stage,
    pub recovery_required: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub schema: u32,
    pub transaction: String,
    pub session: String,
    pub stage: Stage,
    pub source: ExecutableIdentity,
    pub candidate: ExecutableIdentity,
    pub requested_generation: u64,
    pub active_generation: Option<u64>,
    pub active_image: Option<ExecutableIdentity>,
    pub session_preserved: Option<bool>,
    pub swap_complete: bool,
    pub checkpoint_sha256: Option<String>,
    pub validation: Option<OperationalProof>,
    pub failure: Option<Failure>,
    pub sequence: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub request: Request,
    pub receipt: Receipt,
    pub source_generation: u64,
    pub source_owner: ProcessIdentity,
    pub owners: BTreeMap<String, OwnerState>,
    pub inventory: Inventory,
    pub recovery_attempts: u32,
}

pub struct Machine {
    session: Session,
    records: BTreeMap<String, Record>,
    current: Option<String>,
    sequence: u64,
}

impl Machine {
    pub fn new(session: Session) -> io::Result<Self> {
        session.image.validate()?;
        if !digest(&session.id) || session.owner.uid != rustix::process::geteuid().as_raw() || session.inventory.outputs.len() > 64 {
            return Err(invalid("Invalid refresh session registration"));
        }
        Ok(Self { session, records: BTreeMap::new(), current: None, sequence: 0 })
    }

    pub fn session(&self) -> &Session { &self.session }
    pub fn current(&self) -> Option<&Record> { self.current.as_ref().and_then(|id| self.records.get(id)) }
    pub fn record(&self, id: &str) -> Option<&Record> { self.records.get(id) }
    pub fn records(&self) -> impl Iterator<Item = &Record> { self.records.values() }

    pub fn request(&mut self, request: Request) -> io::Result<(Receipt, bool)> {
        request.candidate.validate()?;
        if !digest(&request.transaction) || request.session != self.session.id || !request.candidate_path.is_absolute()
            || request.candidate_path.as_os_str().len() > 4096 || request.candidate.revision.is_none()
            || request.artifact_run == 0 || !digest(&request.qualification_sha256) {
            return Err(invalid("Invalid immutable refresh request"));
        }
        if let Some(previous) = self.records.get(&request.transaction) {
            if previous.request == request { return Ok((previous.receipt.clone(), false)); }
            return Err(invalid("Refresh transaction identifier was reused for a different request"));
        }
        if request.expected_generation != self.session.generation { return Err(invalid("Refresh request targets a stale active generation")); }
        if !self.session.capabilities.full_session() { return Err(io::Error::new(io::ErrorKind::Unsupported, "The active session has no qualified complete handoff")); }
        if self.current().is_some_and(|record| !record.receipt.stage.terminal()) { return Err(io::Error::new(io::ErrorKind::WouldBlock, "Another refresh transaction owns the session")); }
        if request.candidate.target != self.session.image.target { return Err(invalid("Refresh candidate has a different target")); }
        let generation = self.session.generation.checked_add(1).ok_or_else(|| invalid("Refresh generation overflow"))?;
        self.sequence = self.sequence.checked_add(1).ok_or_else(|| invalid("Receipt sequence overflow"))?;
        let receipt = Receipt {
            schema: 1, transaction: request.transaction.clone(), session: self.session.id.clone(), stage: Stage::Prepared,
            source: self.session.image.clone(), candidate: request.candidate.clone(), requested_generation: generation,
            active_generation: Some(self.session.generation), active_image: Some(self.session.image.clone()), session_preserved: None,
            swap_complete: false, checkpoint_sha256: None, validation: None, failure: None, sequence: self.sequence,
        };
        let record = Record { request, receipt: receipt.clone(), source_generation: self.session.generation, source_owner: self.session.owner.clone(),
            owners: BTreeMap::new(), inventory: self.session.inventory.clone(), recovery_attempts: 0 };
        self.current = Some(record.request.transaction.clone());
        self.records.insert(record.request.transaction.clone(), record);
        self.prune();
        Ok((receipt, true))
    }

    fn prune(&mut self) {
        while self.records.len() > 64 {
            let oldest = self.records.iter().filter(|(id, record)| Some(*id) != self.current.as_ref() && record.receipt.stage.terminal())
                .min_by_key(|(_, record)| record.receipt.sequence).map(|(id, _)| id.clone());
            if let Some(oldest) = oldest { self.records.remove(&oldest); } else { break; }
        }
    }

    fn transition(&mut self, id: &str, expected: &[Stage], next: Stage) -> io::Result<&mut Record> {
        if self.current.as_deref() != Some(id) { return Err(invalid("Refresh transaction does not own this session")); }
        let record = self.records.get_mut(id).ok_or_else(|| invalid("Unknown refresh transaction"))?;
        if record.receipt.stage == next { return Ok(record); }
        if !expected.contains(&record.receipt.stage) { return Err(invalid("Invalid refresh lifecycle transition")); }
        self.sequence = self.sequence.checked_add(1).ok_or_else(|| invalid("Receipt sequence overflow"))?;
        record.receipt.stage = next;
        record.receipt.sequence = self.sequence;
        Ok(record)
    }

    pub fn quiescing(&mut self, id: &str) -> io::Result<Receipt> {
        Ok(self.transition(id, &[Stage::Prepared], Stage::Quiescing)?.receipt.clone())
    }

    pub fn checkpointed(&mut self, id: &str, envelope: &Envelope, owners: BTreeMap<String, OwnerState>, inventory: Inventory) -> io::Result<Receipt> {
        envelope.validate()?;
        let record = self.records.get(id).ok_or_else(|| invalid("Unknown refresh transaction"))?;
        if envelope.scope != CheckpointScope::FullSession || envelope.transaction != id || envelope.source != record.receipt.source
            || envelope.candidate != record.request.candidate || envelope.source_generation != record.source_generation
            || envelope.candidate_generation != record.receipt.requested_generation || inventory.outputs.len() > 64
            || owners.len() > 64 || REQUIRED_OWNERS.iter().any(|name| !owners.contains_key(*name))
            || owners.iter().any(|(name, owner)| !identifier(name) || owner.schema == 0 || !digest(&owner.logical_sha256)) {
            return Err(invalid("Full session checkpoint differs from its request or omits an owner"));
        }
        let record = self.transition(id, &[Stage::Quiescing], Stage::Checkpointed)?;
        if let Some(previous) = &record.receipt.checkpoint_sha256 && (previous != &envelope.payload_sha256 || record.owners != owners || record.inventory != inventory) {
            return Err(invalid("Checkpointed transaction was replayed with different state"));
        }
        record.receipt.checkpoint_sha256 = Some(envelope.payload_sha256.clone());
        record.owners = owners; record.inventory = inventory;
        Ok(record.receipt.clone())
    }

    pub fn replacing(&mut self, id: &str) -> io::Result<Receipt> {
        Ok(self.transition(id, &[Stage::Checkpointed], Stage::Replacing)?.receipt.clone())
    }

    pub fn restoring(&mut self, id: &str, owner: &ProcessIdentity, observed: &ExecutableIdentity) -> io::Result<Receipt> {
        let record = self.records.get(id).ok_or_else(|| invalid("Unknown refresh transaction"))?;
        if *owner != record.source_owner || *observed != record.receipt.candidate { return Err(invalid("Restoring process differs from the requested generation")); }
        Ok(self.transition(id, &[Stage::Replacing], Stage::Restoring)?.receipt.clone())
    }

    pub fn validating(&mut self, id: &str, proof: OperationalProof) -> io::Result<Receipt> {
        let record = self.records.get(id).ok_or_else(|| invalid("Unknown refresh transaction"))?;
        proof.validate(record.receipt.checkpoint_sha256.as_deref().ok_or_else(|| invalid("Missing refresh checkpoint"))?, &record.owners, &record.inventory)?;
        let record = self.transition(id, &[Stage::Restoring], Stage::Validating)?;
        if record.receipt.validation.as_ref().is_some_and(|previous| previous != &proof) { return Err(invalid("Readiness proof was replayed with different state")); }
        record.receipt.validation = Some(proof);
        Ok(record.receipt.clone())
    }

    pub fn activate(&mut self, id: &str, owner: &ProcessIdentity, observed: &ExecutableIdentity) -> io::Result<Receipt> {
        let record = self.records.get(id).ok_or_else(|| invalid("Unknown refresh transaction"))?;
        if *owner != record.source_owner || *observed != record.receipt.candidate || record.receipt.validation.is_none() {
            return Err(invalid("Activation has no verified candidate process and operational proof"));
        }
        let record = self.transition(id, &[Stage::Validating], Stage::Active)?;
        record.receipt.active_generation = Some(record.receipt.requested_generation);
        record.receipt.active_image = Some(observed.clone());
        record.receipt.session_preserved = Some(true);
        record.receipt.swap_complete = true;
        let receipt = record.receipt.clone();
        let inventory = record.inventory.clone();
        self.session.generation = receipt.requested_generation;
        self.session.image = observed.clone();
        self.session.owner = owner.clone();
        self.session.inventory = inventory;
        Ok(receipt)
    }

    pub fn recover(&mut self, id: &str, code: &str) -> io::Result<Receipt> {
        if !identifier(code) { return Err(invalid("Invalid refresh failure code")); }
        let previous = self.records.get(id).ok_or_else(|| invalid("Unknown refresh transaction"))?.receipt.stage;
        if previous.terminal() { return Err(invalid("A completed refresh transaction cannot enter recovery")); }
        let record = self.transition(id, &[Stage::Prepared, Stage::Quiescing, Stage::Checkpointed, Stage::Replacing, Stage::Restoring, Stage::Validating, Stage::Recovering], Stage::Recovering)?;
        if record.recovery_attempts >= 2 { return Err(invalid("Refresh recovery attempt limit reached")); }
        record.recovery_attempts += 1;
        record.receipt.swap_complete = false;
        record.receipt.failure = Some(Failure { code: code.into(), stage: previous, recovery_required: true });
        Ok(record.receipt.clone())
    }

    pub fn recovered(&mut self, id: &str, owner: &ProcessIdentity, observed: &ExecutableIdentity, proof: OperationalProof) -> io::Result<Receipt> {
        let record = self.records.get(id).ok_or_else(|| invalid("Unknown refresh transaction"))?;
        if *owner != record.source_owner || *observed != record.receipt.source { return Err(invalid("Recovery did not resume the saved source generation")); }
        proof.validate(record.receipt.checkpoint_sha256.as_deref().ok_or_else(|| invalid("Missing recovery checkpoint"))?, &record.owners, &record.inventory)?;
        let record = self.transition(id, &[Stage::Recovering], Stage::Recovered)?;
        record.receipt.active_generation = Some(record.source_generation);
        record.receipt.active_image = Some(observed.clone());
        record.receipt.session_preserved = Some(true);
        record.receipt.swap_complete = false;
        record.receipt.validation = Some(proof);
        if let Some(failure) = &mut record.receipt.failure { failure.recovery_required = false; }
        Ok(record.receipt.clone())
    }

    pub fn rejected_before_replace(&mut self, id: &str, code: &str, owner: &ProcessIdentity, observed: &ExecutableIdentity) -> io::Result<Receipt> {
        if !identifier(code) { return Err(invalid("Invalid refresh rejection code")); }
        let previous = self.records.get(id).ok_or_else(|| invalid("Unknown refresh transaction"))?.receipt.stage;
        let record = self.records.get(id).ok_or_else(|| invalid("Unknown refresh transaction"))?;
        if *owner != record.source_owner || *observed != record.receipt.source { return Err(invalid("Rejected refresh did not retain the source process")); }
        let record = self.transition(id, &[Stage::Prepared, Stage::Quiescing, Stage::Checkpointed], Stage::Failed)?;
        record.receipt.active_generation = Some(record.source_generation);
        record.receipt.active_image = Some(observed.clone());
        record.receipt.swap_complete = false;
        record.receipt.failure = Some(Failure { code: code.into(), stage: previous, recovery_required: false });
        Ok(record.receipt.clone())
    }

    pub fn lost_owner(&mut self, id: &str, code: &str) -> io::Result<Receipt> {
        if !identifier(code) { return Err(invalid("Invalid refresh failure code")); }
        let previous = self.records.get(id).ok_or_else(|| invalid("Unknown refresh transaction"))?.receipt.stage;
        let record = self.transition(id, &[Stage::Prepared, Stage::Quiescing, Stage::Checkpointed, Stage::Replacing, Stage::Restoring, Stage::Validating, Stage::Recovering], Stage::Failed)?;
        record.receipt.active_generation = None; record.receipt.active_image = None; record.receipt.session_preserved = None; record.receipt.swap_complete = false;
        record.receipt.failure = Some(Failure { code: code.into(), stage: previous, recovery_required: true });
        Ok(record.receipt.clone())
    }
}
