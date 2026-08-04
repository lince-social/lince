//! The Loro adapter (Ontology §11 "Collab", layer 2): one record-doc per
//! record holding its collaborative text (`head`, `body`). ALL Loro calls in
//! the workspace live here — sands, sync, and store handle opaque blobs, so
//! replacing the CRDT engine (yrs is the named fallback) is a contained
//! change.
//!
//! Doc lifecycle: docs are lazy — loaded on first edit or remote update
//! (stored snapshot + crdt-op tail, never a history replay), kept in an LRU
//! of open docs, dropped when idle. A record nobody edits costs zero Loro
//! memory.
//!
//! Wire shape: each `crdt` op's value is the base64 of the doc's full update
//! tail since the last stored snapshot (cumulative). The bounded outbox
//! replaces an older queued op per (contact, tbl, uid, field); a cumulative
//! tail makes that replacement lossless — the newer op is a superset — and
//! compaction bounds tail growth.

use std::collections::HashMap;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use loro::{ExportMode, LoroDoc, UpdateOptions, VersionVector};

use crate::error::EngineError;

/// LRU capacity for open docs.
const MAX_OPEN_DOCS: usize = 64;
/// Compaction thresholds: refresh the stored snapshot once a record has this
/// many crdt ops past it, or once its cumulative tail blob grows this large.
const COMPACT_OPS: i64 = 100;
const COMPACT_BYTES: usize = 256 * 1024;

struct OpenDoc {
    doc: LoroDoc,
    /// The version the stored snapshot covers — tails export from here.
    snapshot_vv: VersionVector,
    last_used: u64,
}

/// In-memory registry of open docs. Sync (`std`) mutex: guards only pure
/// in-memory Loro work, never held across an await.
#[derive(Default)]
pub struct DocRegistry {
    docs: HashMap<String, OpenDoc>,
    tick: u64,
}

/// What a doc load needs from the store, fetched before taking the lock.
struct LoadedState {
    snapshot: Option<Vec<u8>>,
    tail: Vec<(i64, String)>,
}

/// The result of one doc mutation, exported under the lock.
pub struct TextWrite {
    pub head: String,
    pub body: String,
    /// base64 cumulative tail since the stored snapshot.
    pub tail_b64: String,
}

impl crate::Engine {
    async fn load_state(&self, record_uid: &str) -> Result<LoadedState, EngineError> {
        let doc_row = store::record_docs::get(&self.store.pool, record_uid).await?;
        let through = doc_row.as_ref().map(|d| d.through_seq).unwrap_or(0);
        let tail = store::record_docs::crdt_tail(&self.store.pool, record_uid, through).await?;
        Ok(LoadedState {
            snapshot: doc_row.map(|d| d.snapshot),
            tail,
        })
    }

    /// Get-or-load the doc under the registry lock, run `work` on it, evict
    /// LRU overflow, and return the work's output. `seed` (head, body) is
    /// inserted into a doc that has no history anywhere — the writer-side
    /// initialization from materialized columns.
    fn with_doc<T>(
        &self,
        record_uid: &str,
        loaded: LoadedState,
        seed: Option<(String, String)>,
        work: impl FnOnce(&LoroDoc, &VersionVector) -> Result<T, EngineError>,
    ) -> Result<T, EngineError> {
        let mut registry = self
            .collab_docs
            .lock()
            .map_err(|_| EngineError::Consequence("collab registry poisoned".into()))?;
        registry.tick += 1;
        let tick = registry.tick;
        if !registry.docs.contains_key(record_uid) {
            let doc = LoroDoc::new();
            let mut had_history = false;
            if let Some(snapshot) = &loaded.snapshot {
                doc.import(snapshot)
                    .map_err(|e| EngineError::Consequence(format!("doc snapshot import: {e}")))?;
                had_history = true;
            }
            let snapshot_vv = doc.oplog_vv();
            for (_seq, value) in &loaded.tail {
                if let Ok(bytes) = B64.decode(value) {
                    // Tails are cumulative; Loro dedupes by version vector,
                    // so over-importing is harmless.
                    let _ = doc.import(&bytes);
                    had_history = true;
                }
            }
            if !had_history {
                if let Some((head, body)) = seed {
                    // DETERMINISTIC seed: two Cells first-collab-editing the
                    // same replicated record seed identical text — with the
                    // same peer id and insert order the seed ops get identical
                    // Loro IDs, so the two seeds dedupe instead of doubling
                    // the text. Diverged materialized text (stub records)
                    // hashes to different peers and merges as two inserts —
                    // visible, never corrupt.
                    let _ = doc.set_peer_id(seed_peer_id(record_uid, &head, &body));
                    if !head.is_empty() {
                        let _ = doc.get_text("head").update(&head, UpdateOptions::default());
                    }
                    if !body.is_empty() {
                        let _ = doc.get_text("body").update(&body, UpdateOptions::default());
                    }
                    doc.commit();
                    // The USER edit that follows must come from a unique peer.
                    let _ = doc.set_peer_id(random_peer_id());
                }
            }
            registry.docs.insert(
                record_uid.to_string(),
                OpenDoc {
                    doc,
                    snapshot_vv,
                    last_used: tick,
                },
            );
        }
        let entry = registry.docs.get_mut(record_uid).expect("just ensured");
        entry.last_used = tick;
        let out = work(&entry.doc, &entry.snapshot_vv);
        if registry.docs.len() > MAX_OPEN_DOCS {
            let evict = registry
                .docs
                .iter()
                .min_by_key(|(_, open)| open.last_used)
                .map(|(uid, _)| uid.clone());
            if let Some(uid) = evict {
                registry.docs.remove(&uid);
            }
        }
        out
    }

    /// A user text write: apply to the record-doc, log ONE cumulative `crdt`
    /// op, materialize head/body back to SQLite. The whole reason two Organs
    /// editing the same text converge instead of overwriting each other.
    pub async fn write_record_text(
        &self,
        uid: &str,
        head: Option<&str>,
        body: Option<&str>,
    ) -> Result<(), EngineError> {
        let record = store::records::get(&self.store.pool, uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(uid.to_string()))?;
        let loaded = self.load_state(uid).await?;
        let head_owned = head.map(str::to_string);
        let body_owned = body.map(str::to_string);
        let write = self.with_doc(
            uid,
            loaded,
            Some((record.head.clone(), record.body.clone())),
            move |doc, snapshot_vv| {
                if let Some(new) = &head_owned {
                    doc.get_text("head")
                        .update(new, UpdateOptions::default())
                        .map_err(|e| EngineError::Consequence(format!("text update: {e}")))?;
                }
                if let Some(new) = &body_owned {
                    doc.get_text("body")
                        .update(new, UpdateOptions::default())
                        .map_err(|e| EngineError::Consequence(format!("text update: {e}")))?;
                }
                let tail = doc
                    .export(ExportMode::updates(snapshot_vv))
                    .map_err(|e| EngineError::Consequence(format!("tail export: {e}")))?;
                Ok(TextWrite {
                    head: doc.get_text("head").to_string(),
                    body: doc.get_text("body").to_string(),
                    tail_b64: B64.encode(tail),
                })
            },
        )?;
        if let Some(local) = store::organs::local(&self.store.pool).await? {
            store::sync_ops::append(
                &self.store.pool,
                "record",
                uid,
                "",
                store::sync_ops::OpKind::Crdt,
                Some(&write.tail_b64),
                nucleus::hlc::next(),
                &local.uid,
                None,
                // A local collab write inherits whatever root the Record was
                // born in, resolved from the row rather than assumed: a
                // co-written Record inside a conversation must not leak onto
                // the general feed through its crdt ops.
                store::replica::root_of(&self.store.pool, uid)
                    .await?
                    .as_deref(),
            )
            .await?;
        }
        store::records::set_text(&self.store.pool, uid, Some(&write.head), Some(&write.body))
            .await?;
        self.maybe_compact(uid, write.tail_b64.len()).await?;
        Ok(())
    }

    /// Whether `subject` may READ this Record — the gate on joining its live
    /// collab doc (Ontology §11 "Collab").
    ///
    /// Before this existed, any authenticated session could join ANY record's
    /// doc by uid and receive its full snapshot, which made collab a way
    /// AROUND §12 visibility rather than a consumer of it.
    ///
    /// Deliberately the same rule Protein applies: `None` is the local Cell and
    /// sees everything; a remote subject sees only what `visible_targets`
    /// says. Default deny, which means an individually-replicated conversation
    /// — with no visibility rule of its own — is invisible to every remote
    /// subject without needing a special case.
    ///
    /// `read` + `write` IS what enables CRDT editing. Collab is not a separate
    /// privilege; it is what having those permissions means.
    pub async fn may_read_record(
        &self,
        subject: Option<&str>,
        record_uid: &str,
    ) -> Result<bool, EngineError> {
        let Some(subject) = subject else {
            return Ok(true);
        };
        Ok(
            store::visibility::visible_targets(&self.store.pool, subject)
                .await?
                .contains(record_uid),
        )
    }

    /// The record-doc's full snapshot, base64 — what a joining collab client
    /// imports into its fresh browser-side doc. Seeds the doc from the
    /// materialized head/body when no history exists anywhere (same
    /// deterministic seed as `write_record_text`, so the seed ops later ride
    /// the cumulative tail like any other edit).
    pub async fn collab_snapshot(&self, uid: &str) -> Result<String, EngineError> {
        let record = store::records::get(&self.store.pool, uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(uid.to_string()))?;
        let loaded = self.load_state(uid).await?;
        self.with_doc(
            uid,
            loaded,
            Some((record.head.clone(), record.body.clone())),
            |doc, _| {
                let snapshot = doc
                    .export(ExportMode::Snapshot)
                    .map_err(|e| EngineError::Consequence(format!("snapshot export: {e}")))?;
                Ok(B64.encode(snapshot))
            },
        )
    }

    /// Merge one CLIENT-side Loro update (a browser editing over the one
    /// WebSocket) into the record-doc. Same shape as `write_record_text` —
    /// logs ONE cumulative `crdt` op for peer sync, materializes head/body —
    /// but the text change arrives as CRDT bytes instead of replacement
    /// strings, so concurrent typists merge instead of clobbering. Commits a
    /// zero-delta refresh fact (kept out of the op log by `facts::insert`'s
    /// Sync skip) so Protein subscribers and collab sessions wake up.
    pub async fn apply_client_crdt_update(
        &self,
        uid: &str,
        update_b64: &str,
    ) -> Result<(), EngineError> {
        let update = B64
            .decode(update_b64)
            .map_err(|_| EngineError::Consequence("collab update is not valid base64".into()))?;
        let record = store::records::get(&self.store.pool, uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(uid.to_string()))?;
        let loaded = self.load_state(uid).await?;
        let write = self.with_doc(
            uid,
            loaded,
            Some((record.head.clone(), record.body.clone())),
            move |doc, snapshot_vv| {
                doc.import(&update)
                    .map_err(|e| EngineError::Consequence(format!("collab import: {e}")))?;
                let tail = doc
                    .export(ExportMode::updates(snapshot_vv))
                    .map_err(|e| EngineError::Consequence(format!("tail export: {e}")))?;
                Ok(TextWrite {
                    head: doc.get_text("head").to_string(),
                    body: doc.get_text("body").to_string(),
                    tail_b64: B64.encode(tail),
                })
            },
        )?;
        let local = store::organs::local(&self.store.pool).await?;
        if let Some(local) = &local {
            store::sync_ops::append(
                &self.store.pool,
                "record",
                uid,
                "",
                store::sync_ops::OpKind::Crdt,
                Some(&write.tail_b64),
                nucleus::hlc::next(),
                &local.uid,
                None,
                // A local collab write inherits whatever root the Record was
                // born in, resolved from the row rather than assumed: a
                // co-written Record inside a conversation must not leak onto
                // the general feed through its crdt ops.
                store::replica::root_of(&self.store.pool, uid)
                    .await?
                    .as_deref(),
            )
            .await?;
        }
        store::records::set_text(&self.store.pool, uid, Some(&write.head), Some(&write.body))
            .await?;
        self.maybe_compact(uid, write.tail_b64.len()).await?;
        // The refresh fact is the fan-out signal: sessions push CollabChange,
        // Protein re-renders, and the fact-bus wake drains the outbox to peers.
        let _ = self
            .append(
                nucleus::fact::NewFact {
                    uid: None,
                    record_uid: uid.to_string(),
                    delta: nucleus::fact::zero_delta(),
                    at: None,
                    actor_uid: None,
                    cause: nucleus::fact::Cause {
                        kind: nucleus::fact::CauseKind::Sync,
                        uid: local.map(|organ| organ.uid),
                    },
                    payload: Some("{\"collab\":true}".to_string()),
                },
                chrono::Utc::now(),
            )
            .await;
        Ok(())
    }

    /// Apply one remote `crdt` update to the record-doc and materialize.
    /// Returns Err(reason) suitable for quarantine on a malformed payload.
    pub(crate) async fn apply_remote_crdt(
        &self,
        uid: &str,
        value_b64: &str,
    ) -> Result<Result<(), String>, EngineError> {
        let Ok(update) = B64.decode(value_b64) else {
            return Ok(Err("crdt op is not valid base64".into()));
        };
        let loaded = self.load_state(uid).await?;
        let applied = self.with_doc(uid, loaded, None, move |doc, _| {
            Ok(match doc.import(&update) {
                Ok(_) => Ok(TextPair {
                    head: doc.get_text("head").to_string(),
                    body: doc.get_text("body").to_string(),
                }),
                Err(e) => Err(format!("loro import: {e}")),
            })
        })?;
        match applied {
            Ok(text) => {
                store::sync_apply::set_record_text_raw(
                    &self.store.pool,
                    uid,
                    &text.head,
                    &text.body,
                )
                .await?;
                self.maybe_compact(uid, value_b64.len()).await?;
                Ok(Ok(()))
            }
            Err(reason) => Ok(Err(reason)),
        }
    }

    /// Refresh the stored snapshot when the tail passed a threshold; the next
    /// load is snapshot + short tail, and the exported tail baseline resets.
    async fn maybe_compact(&self, uid: &str, last_tail_len: usize) -> Result<(), EngineError> {
        let through = store::record_docs::get(&self.store.pool, uid)
            .await?
            .map(|d| d.through_seq)
            .unwrap_or(0);
        let ops = store::record_docs::crdt_ops_since(&self.store.pool, uid, through).await?;
        if ops < COMPACT_OPS && last_tail_len < COMPACT_BYTES {
            return Ok(());
        }
        let max_seq = store::sync_ops::max_seq(&self.store.pool).await?;
        // Export from the ALREADY-OPEN doc only: snapshotting a freshly
        // created empty doc would erase real history. If the doc was evicted
        // between the write and here, skip — the next write compacts.
        let snapshot = {
            let mut registry = self
                .collab_docs
                .lock()
                .map_err(|_| EngineError::Consequence("collab registry poisoned".into()))?;
            let Some(open) = registry.docs.get_mut(uid) else {
                return Ok(());
            };
            let snapshot = open
                .doc
                .export(ExportMode::Snapshot)
                .map_err(|e| EngineError::Consequence(format!("snapshot export: {e}")))?;
            // Reset the tail baseline to the compacted version.
            open.snapshot_vv = open.doc.oplog_vv();
            snapshot
        };
        store::record_docs::put(&self.store.pool, uid, &snapshot, max_seq).await?;
        Ok(())
    }

    /// Drop a doc from the open registry (tests / tombstone cleanup).
    pub fn close_record_doc(&self, uid: &str) {
        if let Ok(mut registry) = self.collab_docs.lock() {
            registry.docs.remove(uid);
        }
    }
}

struct TextPair {
    head: String,
    body: String,
}

/// Stable across processes (sha2, not `DefaultHasher`): both Cells must
/// derive the SAME seed peer from the same (uid, text).
fn seed_peer_id(uid: &str, head: &str, body: &str) -> u64 {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(uid.as_bytes());
    hasher.update([0]);
    hasher.update(head.as_bytes());
    hasher.update([0]);
    hasher.update(body.as_bytes());
    let digest = hasher.finalize();
    u64::from_le_bytes(digest[..8].try_into().expect("8 bytes")) | 1
}

fn random_peer_id() -> u64 {
    let uuid = uuid::Uuid::new_v4();
    u64::from_le_bytes(uuid.as_bytes()[..8].try_into().expect("8 bytes")) | 1
}
