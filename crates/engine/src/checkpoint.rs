use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeDelta, Utc};
use nucleus::{Cause, CauseKind, Fact, NewFact};

use crate::Engine;
use crate::append::append_one;
use crate::error::EngineError;

#[derive(Debug, Default)]
pub struct CompactionReport {
    pub archived: usize,
    pub archive_file: Option<PathBuf>,
    pub anchor: Option<Fact>,
}

impl Engine {
    pub async fn checkpoint_all(&self, now: DateTime<Utc>) -> Result<Vec<Fact>, EngineError> {
        let signer = self.signer.lock().await.clone();
        let mut out = Vec::new();
        for (uid, level) in store::records::all_levels(&self.store.pool).await? {
            let last = store::facts::for_record(&self.store.pool, &uid, 1).await?;
            match last.first() {
                None => continue,
                Some(f) if f.cause.kind == CauseKind::Checkpoint => continue,
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
                continue;
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
