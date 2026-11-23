use std::{collections::BTreeSet, sync::Arc};

use serde::{Deserialize, Serialize};

use super::{
    Descriptor, MAX_PLACEMENTS, Mutation, Persistence, Receipt, Request, Response, Snapshot,
    request_identifier, room, validate_registry,
};
use crate::{
    json::{encoded_len, sha256},
    performance::{self, Span},
};

mod mutation;
mod query;
mod undo;

#[cfg(test)]
mod tests;

const MAX_UNDO_RECEIPTS: usize = 16;
const MAX_UNDO_BYTES: usize = 16 * 1024 * 1024;
const MAX_RECOVERY_BYTES: usize = 33 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedReceipt {
    id: String,
    hash: [u8; 32],
    receipt: Receipt,
    before: Arc<Snapshot>,
    after: Arc<Snapshot>,
    #[serde(skip)]
    encoded_bytes: Option<usize>,
}

impl SavedReceipt {
    fn encoded_bytes(&mut self) -> Result<usize, String> {
        if let Some(bytes) = self.encoded_bytes {
            performance::add("canvas-history-size-reuses", 1);
            return Ok(bytes);
        }
        let bytes = encoded_len(&(&self.before, &self.after)).map_err(|error| error.to_string())?;
        self.encoded_bytes = Some(bytes);
        Ok(bytes)
    }

    fn replace_after(&mut self, snapshot: Arc<Snapshot>) {
        self.after = snapshot;
        self.encoded_bytes = None;
    }
}

/// Current canvas, component registry, and bounded history of mutation receipts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    /// Current complete snapshot shared with readers and retained receipts.
    pub snapshot: Arc<Snapshot>,
    next_workspace_id: u64,
    registry: Arc<Vec<Descriptor>>,
    receipts: Vec<SavedReceipt>,
}

impl State {
    /// Creates state from a complete snapshot and a compatible component registry.
    ///
    /// # Errors
    ///
    /// Returns an error for an incomplete or invalid snapshot, an invalid registry, or exhausted workspace IDs.
    pub fn new(
        mut snapshot: Snapshot,
        registry: impl Into<Arc<Vec<Descriptor>>>,
    ) -> Result<Self, String> {
        let registry = registry.into();
        if snapshot.next_offset.is_some() {
            return Err(
                "Initialize canvas state from a complete snapshot, not an inspection page.".into(),
            );
        }
        snapshot.validate_full()?;
        room::project(&mut snapshot.placements)?;
        validate_registry(&registry)?;
        validate_components(&snapshot, &registry)?;
        let next_workspace_id = next_workspace_after(&snapshot)?;
        Ok(Self {
            next_workspace_id,
            snapshot: Arc::new(snapshot),
            registry,
            receipts: Vec::new(),
        })
    }

    /// Adopts a normalized snapshot at the current revision and updates its retained receipt.
    ///
    /// # Errors
    ///
    /// Returns an error if the normalized snapshot or its components fail validation.
    pub fn normalize_applied_snapshot(&mut self, mut snapshot: Snapshot) -> Result<(), String> {
        self.prepare_snapshot(&mut snapshot)?;
        self.snapshot = Arc::new(snapshot);
        if let Some(receipt) = self.receipts.last_mut()
            && receipt.receipt.revision == self.snapshot.revision
        {
            receipt.replace_after(Arc::clone(&self.snapshot));
        }
        Ok(())
    }

    /// Replaces the registry after checking every existing component against it.
    ///
    /// # Errors
    ///
    /// Returns an error if the registry or a component in the current snapshot fails validation.
    pub fn refresh_registry(
        &mut self,
        registry: impl Into<Arc<Vec<Descriptor>>>,
    ) -> Result<(), String> {
        let registry = registry.into();
        validate_registry(&registry)?;
        validate_components(&self.snapshot, &registry)?;
        self.registry = registry;
        Ok(())
    }

    /// Adopts a validated snapshot and advances the revision only if its content changed.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid snapshot data or exhausted revision or workspace counters.
    pub fn synchronize(&mut self, mut snapshot: Snapshot) -> Result<(), String> {
        self.prepare_snapshot(&mut snapshot)?;
        if snapshot != *self.snapshot {
            snapshot.revision = snapshot
                .revision
                .checked_add(1)
                .ok_or("Canvas revision overflow.")?;
            self.next_workspace_id = self.next_workspace_id.max(next_workspace_after(&snapshot)?);
            self.snapshot = Arc::new(snapshot);
        }
        Ok(())
    }

    /// Advances workspace allocation past an identifier from another snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if the supplied identifier has no successor.
    pub fn reserve_workspace_id(&mut self, id: u64) -> Result<(), String> {
        self.next_workspace_id = self
            .next_workspace_id
            .max(id.checked_add(1).ok_or("Workspace identity overflow.")?);
        Ok(())
    }

    /// Returns the next workspace identifier this state will allocate.
    pub fn next_workspace_id(&self) -> u64 {
        self.next_workspace_id
    }

    /// Checks recovery data, including the registry, counters, and retained history.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid data, inconsistent history, or exceeded recovery limits.
    pub fn validate(&self) -> Result<(), String> {
        self.snapshot.validate_full()?;
        if self
            .snapshot
            .workspaces
            .iter()
            .any(|w| w.id >= self.next_workspace_id)
        {
            return Err("The workspace identity counter is invalid.".into());
        }
        validate_registry(&self.registry)?;
        validate_components(&self.snapshot, &self.registry)?;
        if encoded_len(self).map_err(|error| error.to_string())? > MAX_RECOVERY_BYTES {
            return Err("Canvas recovery exceeds 33 MiB.".into());
        }
        if self.receipts.len() > MAX_UNDO_RECEIPTS {
            return Err("Canvas recovery has too many undo receipts.".into());
        }
        let mut ids = BTreeSet::new();
        for receipt in &self.receipts {
            request_identifier(&receipt.id)?;
            if receipt.receipt.request_id != receipt.id
                || !ids.insert(&receipt.id)
                || receipt.receipt.revision > self.snapshot.revision
            {
                return Err("Invalid canvas recovery receipt.".into());
            }
            receipt.before.validate_full()?;
            receipt.after.validate_full()?;
        }
        Ok(())
    }

    /// Handles an inspection, export, receipt lookup, or revision-checked mutation.
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails, an identifier is unavailable, or the request
    /// conflicts with retained history or the current revision.
    pub fn handle(&mut self, request: &Request) -> Result<Response, String> {
        request.validate()?;
        match request {
            Request::Scene { .. } => {
                Err("Open a published document in the native scene inspector.".into())
            }
            Request::InspectElement { placement, .. } => self.inspect_element(placement),
            Request::Export { placement, name } => self.export(placement, name),
            Request::Registry => Ok(Response::Registry {
                components: self.registry.as_ref().clone(),
            }),
            Request::Inspect {
                workspace,
                selected_only,
                offset,
                limit,
            } => self.inspect(*workspace, *selected_only, *offset, *limit),
            Request::Receipt { request_id } => {
                let saved = self.retained_receipt(request_id).ok_or(
                    "This request has no retained receipt. Inspect actual canvas state before retrying.",
                )?;
                Ok(Response::Receipt {
                    receipt: saved.receipt.clone(),
                })
            }
            Request::Mutate {
                request_id,
                expected_revision,
                mutation,
            } => {
                let receipt = self.mutate(request, request_id, *expected_revision, mutation)?;
                Ok(Response::Receipt { receipt })
            }
        }
    }

    fn mutate(
        &mut self,
        request: &Request,
        request_id: &str,
        expected_revision: u64,
        mutation: &Mutation,
    ) -> Result<Receipt, String> {
        let _mutation = Span::new("canvas-mutation");
        let hash = sha256(request).map_err(|error| error.to_string())?;
        if let Some(saved) = self.retained_receipt(request_id) {
            if saved.hash != hash {
                return Err("This request ID already belongs to another canvas operation.".into());
            }
            return Ok(saved.receipt.clone());
        }
        if expected_revision != self.snapshot.revision {
            return Err(format!(
                "Canvas changed: expected revision {expected_revision}, current {}. Inspect before editing again.",
                self.snapshot.revision
            ));
        }
        let before = Arc::clone(&self.snapshot);
        let mut next = Arc::clone(&before);
        let (mut affected_placements, workspace) = self.apply(&mut next, mutation)?;
        let affected_count = match mutation {
            Mutation::Undo { receipt } | Mutation::UndoChanges { receipt } => {
                self.retained_receipt(receipt)
                    .ok_or("The undo receipt is no longer retained.")?
                    .receipt
                    .affected_count
            }
            _ => affected_placements.len(),
        };
        affected_placements.truncate(MAX_PLACEMENTS);
        let snapshot = Arc::make_mut(&mut next);
        snapshot.revision = snapshot
            .revision
            .checked_add(1)
            .ok_or("Canvas revision overflow.")?;
        snapshot.validate_full()?;
        room::project(&mut snapshot.placements)?;

        let request_id = request_id.to_owned();
        let receipt = Receipt {
            request_id: request_id.clone(),
            revision: next.revision,
            persistence: Persistence::Applied,
            affected_placements,
            affected_count,
            workspace,
        };
        if matches!(mutation, Mutation::CreateWorkspace { .. }) {
            self.next_workspace_id = self
                .next_workspace_id
                .checked_add(1)
                .ok_or("Workspace identity overflow.")?;
        }
        self.snapshot = next;
        self.receipts.push(SavedReceipt {
            id: request_id,
            hash,
            receipt: receipt.clone(),
            before,
            after: Arc::clone(&self.snapshot),
            encoded_bytes: None,
        });
        self.prune_receipts()?;
        Ok(receipt)
    }

    fn prune_receipts(&mut self) -> Result<(), String> {
        let sizes = self
            .receipts
            .iter_mut()
            .map(SavedReceipt::encoded_bytes)
            .collect::<Result<Vec<_>, _>>()?;
        let mut bytes = sizes.iter().sum::<usize>();
        let mut remaining = sizes.len();

        for size in sizes {
            if remaining <= 1 || (remaining <= MAX_UNDO_RECEIPTS && bytes <= MAX_UNDO_BYTES) {
                break;
            }
            bytes -= size;
            remaining -= 1;
        }
        if remaining < self.receipts.len() {
            drop(self.receipts.drain(..self.receipts.len() - remaining));
        }
        Ok(())
    }

    fn retained_receipt(&self, id: &str) -> Option<&SavedReceipt> {
        self.receipts.iter().find(|receipt| receipt.id == id)
    }

    fn prepare_snapshot(&self, snapshot: &mut Snapshot) -> Result<(), String> {
        snapshot.revision = self.snapshot.revision;
        snapshot.next_offset = None;
        snapshot.validate_full()?;
        room::project(&mut snapshot.placements)?;
        validate_components(snapshot, &self.registry)
    }

    /// Marks every retained receipt through the given revision as durable.
    pub fn mark_saved_through(&mut self, revision: u64) {
        for receipt in &mut self.receipts {
            if receipt.receipt.revision <= revision {
                receipt.receipt.persistence = Persistence::Saved;
            }
        }
    }

    /// Marks one retained receipt as durable.
    ///
    /// # Errors
    ///
    /// Returns an error if the request has no retained receipt.
    pub fn mark_saved(&mut self, request_id: &str) -> Result<(), String> {
        let receipt = self
            .receipts
            .iter_mut()
            .find(|r| r.id == request_id)
            .ok_or("This canvas receipt is no longer retained.")?;
        receipt.receipt.persistence = Persistence::Saved;
        Ok(())
    }
}

fn next_workspace_after(snapshot: &Snapshot) -> Result<u64, String> {
    snapshot
        .workspaces
        .iter()
        .map(|workspace| workspace.id)
        .max()
        .unwrap_or_default()
        .checked_add(1)
        .ok_or_else(|| "Workspace identity overflow.".into())
}

fn validate_components(snapshot: &Snapshot, registry: &[Descriptor]) -> Result<(), String> {
    snapshot
        .placements
        .iter()
        .try_for_each(|placement| placement.component.validate_snapshot(registry))
}
