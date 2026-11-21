// non-lean-yet
// Scope: struct Work, struct Recovered, struct Loaded, struct Write.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 151, 216, 247,
//   256.
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 71, 75, 81,
//   84, 89, 92, 154, 182, 259, 283.
// - [review: §5 Panics] Verify panic isolation boundary and preserve the failure contract. Sites
//   68.
// - [Clippy probe: clippy::as_conversions] Use From or TryFrom when it preserves the conversion
//   contract. Sites 112.
// - [Clippy probe: clippy::clone_on_ref_ptr] Make shared ownership explicit at retained clones.
//   Sites 148, 253.
// - [Clippy probe: clippy::map_err_ignore] Preserve relevant causes and context at the boundary.
//   Sites 71, 75, 81, 84, 89, 92, 154, 182, 259, 283.
// - [Clippy probe: clippy::missing_errors_doc] State the observable failure cases for exported
//   functions. Sites 130, 189, 296.
// - [rustc probe: missing_debug_implementations] type does not implement `std::fmt::Debug`;
//   consider adding `#[derive(Debug)]` or a manual implementation. Sites 25, 31, 36.
// - [rustc probe: missing_docs] Document the externally visible contracts where applicable. Sites
//   25, 26, 27, 28, 31, 32, 33, 36, 37, 38, 39, 40, 41, 42, 130, 189, 296.

use std::{
    collections::HashSet,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use loro::{ExportMode, LoroDoc};
use store::record_edit_drafts::{self as drafts, Metadata, PendingRequest};

use crate::{Engine, EngineError, record_change::Request};

pub(crate) struct Work {
    slots: Arc<tokio::sync::Semaphore>,
}

impl Default for Work {
    fn default() -> Self {
        Self {
            slots: Arc::new(tokio::sync::Semaphore::new(2)),
        }
    }
}

pub struct Recovered {
    pub snapshot: Vec<u8>,
    pub pending_updates: Vec<Vec<u8>>,
    pub pending: Vec<Arc<Request>>,
}

pub struct Loaded {
    pub metadata: Option<Metadata>,
    pub recovered: Option<Recovered>,
}

pub struct Write {
    pub expected: Option<Metadata>,
    pub generation: String,
    pub checkpoint: Option<Arc<String>>,
    pub pending: Vec<Arc<Request>>,
    pub updates: Vec<Vec<u8>>,
    pub atoms: usize,
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::Consequence(message.into())
}

fn materialize(
    checkpoint: &str,
    mut updates: Vec<Vec<u8>>,
) -> Result<(Vec<u8>, Vec<Vec<u8>>), EngineError> {
    let (snapshot, pending_checkpoint) = reconstruct(checkpoint, &updates)?;
    let pending = if let Some(checkpoint) = pending_checkpoint {
        updates.insert(0, checkpoint);
        updates
    } else {
        Vec::new()
    };
    Ok((snapshot, pending))
}

fn reconstruct(
    checkpoint: &str,
    updates: &[Vec<u8>],
) -> Result<(Vec<u8>, Option<Vec<u8>>), EngineError> {
    let _reconstruction = crate::performance::Span::new("record-recovery-reconstruction");
    catch_unwind(AssertUnwindSafe(|| {
        let checkpoint = B64
            .decode(checkpoint)
            .map_err(|_| invalid("Recovery checkpoint is not valid base64"))?;
        let document = LoroDoc::new();
        let status = document
            .import(&checkpoint)
            .map_err(|_| invalid("Recovery history cannot be reconstructed"))?;
        let mut has_pending = status.pending.is_some_and(|pending| !pending.is_empty());
        if !updates.is_empty() {
            document.detach();
            let status = document
                .import_batch(updates)
                .map_err(|_| invalid("Recovery history cannot be reconstructed"))?;
            document
                .try_attach()
                .map_err(|_| invalid("Recovery history cannot be reattached"))?;
            has_pending |= status.pending.is_some_and(|pending| !pending.is_empty());
        }
        let snapshot = document
            .export(ExportMode::Snapshot)
            .map_err(|_| invalid("Recovery document cannot be exported"))?;
        Ok((snapshot, has_pending.then_some(checkpoint)))
    }))
    .map_err(|_| invalid("Recovery history was refused"))?
}

struct Compaction {
    checkpoint: Option<String>,
    original: Option<String>,
    updates: Vec<Vec<u8>>,
}

fn compact(
    history: drafts::History,
    supplied_checkpoint: bool,
    mut added: Vec<Vec<u8>>,
) -> Result<Compaction, EngineError> {
    let _compaction = crate::performance::Span::new("record-recovery-compaction");
    let drafts::History {
        checkpoint,
        mut updates,
    } = history;
    let previous = updates.len();
    crate::performance::add("record-recovery-compaction-inputs", added.len() as u64);
    updates.append(&mut added);
    let (snapshot, pending) = reconstruct(&checkpoint, &updates)?;
    let encoded = if pending.is_some() {
        None
    } else {
        let encoded = B64.encode(snapshot);
        (encoded.len() <= drafts::MAX_CHECKPOINT_BYTES).then_some(encoded)
    };
    added.extend(updates.drain(previous..));
    Ok(Compaction {
        checkpoint: encoded,
        original: supplied_checkpoint.then_some(checkpoint),
        updates: added,
    })
}

impl Engine {
    pub async fn load_record_edit_draft(
        &self,
        source: &str,
        uid: &str,
    ) -> Result<Loaded, EngineError> {
        let metadata = drafts::metadata(&self.store.pool, source, uid).await?;
        let Some(current) = &metadata else {
            return Ok(Loaded {
                metadata: None,
                recovered: None,
            });
        };
        if current.pending.is_empty() {
            return Ok(Loaded {
                metadata,
                recovered: None,
            });
        }
        let permit = self
            .record_edit_work
            .slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| invalid("Recovery work is unavailable"))?;
        let drafts::Replay {
            checkpoint,
            updates,
            requests,
            ..
        } = drafts::replay(&self.store.pool, source, uid, &current.stamp).await?;
        let uid = uid.to_owned();
        let recovered = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let pending = requests
                .into_iter()
                .map(|pending| {
                    let request: Request = serde_json::from_str(&pending.payload)?;
                    if request.id != pending.uid || request.record_uid != uid {
                        return Err(invalid("Recovery request belongs to another Record"));
                    }
                    Ok(Arc::new(request))
                })
                .collect::<Result<Vec<_>, EngineError>>()?;
            let (snapshot, pending_updates) = materialize(&checkpoint, updates)?;
            Ok::<_, EngineError>(Recovered {
                snapshot,
                pending_updates,
                pending,
            })
        })
        .await
        .map_err(|_| invalid("Recovery work did not finish"))??;
        Ok(Loaded {
            metadata,
            recovered: Some(recovered),
        })
    }

    pub async fn save_record_edit_draft(
        &self,
        source: &str,
        uid: &str,
        write: Write,
    ) -> Result<Metadata, EngineError> {
        if write.pending.len() > drafts::MAX_REQUESTS || write.updates.len() > 4096 {
            return Err(invalid("Recovery journal batch is too large"));
        }
        let known: HashSet<_> = if write.pending.is_empty() {
            HashSet::new()
        } else {
            write
                .expected
                .as_ref()
                .into_iter()
                .flat_map(|metadata| &metadata.pending)
                .map(String::as_str)
                .collect()
        };
        let mut requests = Vec::new();
        for request in &write.pending {
            if request.record_uid != uid {
                return Err(invalid("Recovery request belongs to another Record"));
            }
            if !known.contains(request.id.as_str()) {
                requests.push(PendingRequest {
                    uid: request.id.clone(),
                    payload: serde_json::to_string(request.as_ref())?,
                });
            }
        }
        drop(known);
        let rows = write
            .expected
            .as_ref()
            .map_or(0, |metadata| metadata.update_count);
        let bytes = write
            .expected
            .as_ref()
            .map_or(0, |metadata| metadata.update_bytes);
        let added_bytes = write.updates.iter().try_fold(0_usize, |total, update| {
            total
                .checked_add(update.len())
                .filter(|total| *total <= drafts::MAX_BYTES)
                .ok_or_else(|| invalid("Recovery updates exceed their storage limit"))
        })?;
        let needs_compaction = !write.updates.is_empty()
            && (write.atoms >= 65_536
                || rows / 128 != rows.saturating_add(write.updates.len()) / 128
                || bytes / 65_536 != bytes.saturating_add(added_bytes) / 65_536);
        let mut change = drafts::Write {
            expected: write.expected.map(|metadata| metadata.stamp),
            generation: write.generation,
            checkpoint: write.checkpoint.map(Arc::unwrap_or_clone),
            pending: write
                .pending
                .iter()
                .map(|request| request.id.clone())
                .collect(),
            requests,
            updates: write.updates,
        };
        let compacted = if needs_compaction {
            let permit = self
                .record_edit_work
                .slots
                .clone()
                .acquire_owned()
                .await
                .map_err(|_| invalid("Recovery work is unavailable"))?;
            let supplied_checkpoint = change.checkpoint.is_some();
            let state = if let Some(checkpoint) = change.checkpoint.take() {
                Some(drafts::History {
                    checkpoint,
                    updates: Vec::new(),
                })
            } else if let Some(expected) = &change.expected {
                let Some(history) =
                    drafts::history_if_current(&self.store.pool, source, uid, expected).await?
                else {
                    return Ok(drafts::save(&self.store.pool, source, uid, &change, None).await?);
                };
                Some(history)
            } else {
                None
            };
            if let Some(history) = state {
                let updates = std::mem::take(&mut change.updates);
                let compacted = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    compact(history, supplied_checkpoint, updates)
                })
                .await
                .map_err(|_| invalid("Recovery checkpoint work did not finish"))??;
                change.checkpoint = compacted.original;
                change.updates = compacted.updates;
                compacted.checkpoint
            } else {
                None
            }
        } else {
            None
        };
        Ok(drafts::save(&self.store.pool, source, uid, &change, compacted.as_deref()).await?)
    }

    pub async fn retire_record_edit_draft(
        &self,
        source: &str,
        uid: &str,
        expected: &drafts::Stamp,
    ) -> Result<bool, EngineError> {
        Ok(drafts::retire(&self.store.pool, source, uid, expected).await?)
    }
}
