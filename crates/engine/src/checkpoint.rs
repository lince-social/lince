//! Checkpoint facts (blueprint II.2): periodic level snapshots so old facts can
//! later be archived/compacted. Checkpoints are bookkeeping — they bypass the
//! Karma cascade on purpose (a snapshot is not a change).

use chrono::{DateTime, Utc};
use nucleus::{Cause, CauseKind, Fact, NewFact};

use crate::append::append_one;
use crate::error::EngineError;
use crate::Engine;

impl Engine {
    /// Write a `delta=0, payload={"level": q}` checkpoint fact for every record
    /// whose latest fact is not already a checkpoint. Returns the checkpoints.
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
                    delta: 0.0,
                    at: None,
                    actor_uid: None,
                    cause: Cause { kind: CauseKind::Checkpoint, uid: None },
                    payload: Some(serde_json::json!({ "level": level }).to_string()),
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
}
