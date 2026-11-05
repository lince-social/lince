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
                .import_batch(&updates)
                .map_err(|_| invalid("Recovery history cannot be reconstructed"))?;
            document.attach();
            has_pending |= status.pending.is_some_and(|pending| !pending.is_empty());
        }
        let snapshot = document
            .export(ExportMode::Snapshot)
            .map_err(|_| invalid("Recovery document cannot be exported"))?;
        let pending = if has_pending {
            updates.insert(0, checkpoint);
            updates
        } else {
            Vec::new()
        };
        Ok((snapshot, pending))
    }))
    .map_err(|_| invalid("Recovery history was refused"))?
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
        let stored = drafts::replay(&self.store.pool, source, uid, &current.stamp).await?;
        let uid = uid.to_owned();
        let recovered = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let pending = stored
                .requests
                .into_iter()
                .map(|pending| {
                    let request: Request = serde_json::from_str(&pending.payload)?;
                    if request.id != pending.uid || request.record_uid != uid {
                        return Err(invalid("Recovery request belongs to another Record"));
                    }
                    Ok(Arc::new(request))
                })
                .collect::<Result<Vec<_>, EngineError>>()?;
            let (snapshot, pending_updates) = materialize(&stored.checkpoint, stored.updates)?;
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
        let known: HashSet<_> = write
            .expected
            .as_ref()
            .into_iter()
            .flat_map(|metadata| &metadata.pending)
            .map(String::as_str)
            .collect();
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
        let compact = !write.updates.is_empty()
            && (write.atoms >= 65_536
                || rows / 128 != rows.saturating_add(write.updates.len()) / 128
                || bytes / 65_536 != bytes.saturating_add(added_bytes) / 65_536);
        let change = drafts::Write {
            expected: write.expected.map(|metadata| metadata.stamp),
            generation: write.generation,
            checkpoint: write
                .checkpoint
                .map(|checkpoint| checkpoint.as_ref().clone()),
            pending: write
                .pending
                .iter()
                .map(|request| request.id.clone())
                .collect(),
            requests,
            updates: write.updates,
        };
        let compacted = if compact {
            let permit = self
                .record_edit_work
                .slots
                .clone()
                .acquire_owned()
                .await
                .map_err(|_| invalid("Recovery work is unavailable"))?;
            let state = if let Some(checkpoint) = &change.checkpoint {
                Some((checkpoint.clone(), Vec::new()))
            } else if let Some(expected) = &change.expected {
                let current = drafts::metadata(&self.store.pool, source, uid).await?;
                if current.as_ref().map(|metadata| &metadata.stamp) != Some(expected) {
                    return Ok(drafts::save(&self.store.pool, source, uid, &change, None).await?);
                }
                let stored = drafts::replay(&self.store.pool, source, uid, expected).await?;
                Some((stored.checkpoint, stored.updates))
            } else {
                None
            };
            if let Some((checkpoint, mut updates)) = state {
                updates.extend(change.updates.iter().cloned());
                tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    let (snapshot, pending) = materialize(&checkpoint, updates)?;
                    if !pending.is_empty() {
                        return Ok(None);
                    }
                    let encoded = B64.encode(snapshot);
                    Ok::<_, EngineError>(
                        (encoded.len() <= drafts::MAX_CHECKPOINT_BYTES).then_some(encoded),
                    )
                })
                .await
                .map_err(|_| invalid("Recovery checkpoint work did not finish"))??
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
