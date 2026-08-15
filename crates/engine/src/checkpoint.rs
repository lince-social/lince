//! Checkpoint facts (blueprint II.2): periodic level snapshots so old facts can
//! later be archived/compacted. Checkpoints are bookkeeping — they bypass the
//! Karma cascade on purpose (a snapshot is not a change).
//!
//! Compaction folds a record's pre-checkpoint history into the checkpoint:
//! facts older than BOTH the kind's retention horizon and the record's last
//! checkpoint are appended to a cold JSONL archive file, deleted from the hot
//! table, and anchored by a zero-delta fact carrying the archive's SHA-256.
//! Per-step chain verification (`verify_chain_step`) survives: each remaining
//! fact still recomputes its own hash from its recorded `prev_hash`; the
//! archived span stays verifiable inside the archive file, and the anchor makes
//! the file's content tamper-evident from inside the Ledger.

use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeDelta, Utc};
use nucleus::{Cause, CauseKind, Fact, NewFact};

use crate::Engine;
use crate::append::append_one;
use crate::error::EngineError;

/// What one `compact` run did.
#[derive(Debug, Default)]
pub struct CompactionReport {
    /// Facts moved to the cold archive (and deleted from the hot table).
    pub archived: usize,
    /// The archive file written, when anything was archived.
    pub archive_file: Option<PathBuf>,
    /// The anchor fact recording the archive's hash, when anything was archived.
    pub anchor: Option<Fact>,
}

impl Engine {
    /// Write a `delta=0, payload={"level": q}` checkpoint fact for every record
    /// whose latest fact is not already a checkpoint. Returns the checkpoints.
    ///
    /// The level is written as canonical decimal TEXT with its scale, not as a
    /// JSON number. After compaction this payload *is* the authoritative level
    /// of the record — every Fact behind it is gone — so a JSON float here
    /// would reintroduce the rounding the exact Ledger exists to remove, on a
    /// path with no REAL column anywhere in it.
    pub async fn checkpoint_all(&self, now: DateTime<Utc>) -> Result<Vec<Fact>, EngineError> {
        let signer = self.signer.lock().await.clone();
        let mut out = Vec::new();
        for (uid, level) in store::records::all_levels(&self.store.pool).await? {
            let last = store::facts::for_record(&self.store.pool, &uid, 1).await?;
            match last.first() {
                None => continue, // no history: nothing to anchor
                Some(f) if f.cause.kind == CauseKind::Checkpoint => continue, // already anchored
                Some(_) => {}
            }
            if let Some(fact) = append_one(
                &self.store,
                NewFact {
                    uid: None,
                    record_uid: uid,
                    delta: nucleus::fact::zero_delta(),
                    at: None,
                    actor_uid: None,
                    cause: Cause {
                        kind: CauseKind::Checkpoint,
                        uid: None,
                    },
                    payload: Some(
                        serde_json::json!({
                            "level": level.canonical(),
                            "level_scale": level.scale(),
                        })
                        .to_string(),
                    ),
                },
                now,
                signer.as_ref(),
            )
            .await?
            {
                out.push(fact);
            }
        }
        Ok(out)
    }

    /// Compact the Ledger (blueprint II.2). For every record whose kind has a
    /// retention policy: facts older than both the horizon and the record's
    /// last checkpoint are written to `<archive_dir>/facts-<timestamp>.jsonl`
    /// (one file per run), deleted from the hot table, and anchored by a
    /// zero-delta checkpoint fact on the local organ record whose payload
    /// carries `{"archive", "archive_hash", "archived"}`. Idempotent: a second
    /// run with the same clock archives nothing.
    pub async fn compact(
        &self,
        now: DateTime<Utc>,
        archive_dir: &Path,
    ) -> Result<CompactionReport, EngineError> {
        let policies: std::collections::HashMap<String, i64> =
            store::facts::retention_policies(&self.store.pool)
                .await?
                .into_iter()
                .collect();
        if policies.is_empty() {
            return Ok(CompactionReport::default());
        }

        let mut archived: Vec<Fact> = Vec::new();
        for record in store::records::list_all(&self.store.pool).await? {
            let Some(horizon) = policies.get(&record.kind) else {
                continue;
            };
            let Some((checkpoint_rowid, _)) =
                store::facts::last_checkpoint(&self.store.pool, &record.uid).await?
            else {
                continue; // nothing folds without a checkpoint to fold into
            };
            let cutoff = now - TimeDelta::seconds(*horizon);
            archived.extend(
                store::facts::archivable_before(
                    &self.store.pool,
                    &record.uid,
                    checkpoint_rowid,
                    cutoff,
                )
                .await?,
            );
        }
        if archived.is_empty() {
            return Ok(CompactionReport::default());
        }

        // Cold file first: never delete what is not durably archived.
        std::fs::create_dir_all(archive_dir).map_err(EngineError::Io)?;
        let file_name = format!("facts-{}.jsonl", now.format("%Y%m%dT%H%M%S%3fZ"));
        let path = archive_dir.join(&file_name);
        let mut buffer = Vec::new();
        for fact in &archived {
            serde_json::to_writer(&mut buffer, fact).map_err(EngineError::Json)?;
            buffer.push(b'\n');
        }
        let mut file = std::fs::File::create(&path).map_err(EngineError::Io)?;
        file.write_all(&buffer).map_err(EngineError::Io)?;
        file.sync_all().map_err(EngineError::Io)?;
        let archive_hash = nucleus::fact::sha256_hex(&buffer);

        let uids: Vec<String> = archived.iter().map(|f| f.uid.clone()).collect();
        store::facts::delete_by_uids(&self.store.pool, &uids).await?;

        // Anchor the archive from inside the Ledger. Carrier: the CELL Record.
        // Compaction is a Cell-level event — what got archived depends on this
        // machine's retention, not on the identity — and the old comment said
        // exactly that while naming the Organ, which is the confusion the
        // Organ/Cell split exists to end. Anchoring on the Organ would make
        // two Cells' independent compactions look like one Organ's history.
        let carrier = match store::cells::local(&self.store.pool).await? {
            Some(cell) => cell.uid,
            None => archived[0].record_uid.clone(),
        };
        let signer = self.signer.lock().await.clone();
        let anchor = append_one(
            &self.store,
            NewFact {
                uid: None,
                record_uid: carrier,
                delta: nucleus::fact::zero_delta(),
                at: None,
                actor_uid: None,
                cause: Cause {
                    kind: CauseKind::Checkpoint,
                    uid: None,
                },
                payload: Some(
                    serde_json::json!({
                        "archive": file_name,
                        "archive_hash": archive_hash,
                        "archived": archived.len(),
                    })
                    .to_string(),
                ),
            },
            now,
            signer.as_ref(),
        )
        .await?;

        Ok(CompactionReport {
            archived: archived.len(),
            archive_file: Some(path),
            anchor,
        })
    }
}
