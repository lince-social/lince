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
        let tail = store::record_docs::doc_tail(&self.store.pool, record_uid, through).await?;
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
        // Same lock the import path takes. A local write logs a crdt op and may
        // compact, and compaction records "everything up to this seq is folded
        // into the snapshot" — which must not be decided while a peer's op is
        // mid-import, or `through_seq` can move past a blob that never reached
        // the doc. Silent text loss, and the hardest kind to trace back.
        let _op = self.import_lock.lock().await;
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
        // The CELL, not the Organ. This call site is the one the Ontology names
        // as the example of the confusion the split resolves: it passed the
        // Organ uid as the op actor, which collides two Cells on
        // `UNIQUE(actor_cell, hlc)`.
        if let Some(local) = store::cells::local(&self.store.pool).await? {
            store::sync_ops::append(
                &self.store.pool,
                "record",
                uid,
                "",
                store::sync_ops::OpKind::Crdt,
                Some(&write.tail_b64),
                nucleus::hlc::next(),
                &local.uid,
                &local.organ_uid,
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

    /// The record-doc's current text, rebuilt from the stored snapshot and the
    /// logged tail — NOT read from the materialized columns.
    ///
    /// The distinction is the whole point: this is what the doc says, so
    /// comparing it against `record.head`/`record.body` is how you find out
    /// whether the log can still reconstruct the text it is supposed to own.
    /// No seeding, deliberately — a doc with no history answers empty rather
    /// than inventing content from the columns it is meant to be checked
    /// against.
    pub async fn doc_text(&self, uid: &str) -> Result<(String, String), EngineError> {
        let loaded = self.load_state(uid).await?;
        self.with_doc(uid, loaded, None, |doc, _| {
            Ok((
                doc.get_text("head").to_string(),
                doc.get_text("body").to_string(),
            ))
        })
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
        // See `write_record_text` — same reason.
        let _op = self.import_lock.lock().await;
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
        // The CELL, not the Organ — see the sibling call above.
        let local = store::cells::local(&self.store.pool).await?;
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
                &local.organ_uid,
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

    /// Apply one remote `crdt` or `snapshot` blob to the record-doc and
    /// materialize. Returns Err(reason) suitable for quarantine on a malformed
    /// payload.
    ///
    /// `was_snapshot` suppresses the compaction check, and that is not an
    /// optimization. A snapshot blob is large, so it clears the byte threshold
    /// on arrival; compacting in response would emit OUR snapshot op, which the
    /// peer imports, which clears their threshold, which emits theirs — two
    /// Cells volleying whole documents at each other forever. There is also
    /// nothing to compact: we just adopted someone else's compaction.
    pub(crate) async fn apply_remote_crdt(
        &self,
        uid: &str,
        value_b64: &str,
        was_snapshot: bool,
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
                if !was_snapshot {
                    self.maybe_compact(uid, value_b64.len()).await?;
                }
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
        self.compact_doc(uid).await?;
        Ok(())
    }

    /// Compact one record-doc: store the snapshot, reset the tail baseline,
    /// and LOG the snapshot as an op.
    ///
    /// The op is the point. Without it the snapshot lives only in `record_doc`,
    /// which is local and never travels, so a rebuild from the log alone cannot
    /// reconstruct text and no `crdt` op can ever be pruned — each is only
    /// cumulative since a base the log does not contain. With it, all three of
    /// those resolve at once (Ontology §11, decision 2).
    ///
    /// The snapshot op's identity is this Cell's, minted like any other write.
    /// That was the objection to snapshots-in-the-log ("it needs a synthesized
    /// op identity, and that identity IS the unique index") and it dissolves on
    /// contact: the compacting Cell is a real actor asserting a real fact about
    /// a doc it holds. Nothing is invented, so nothing collides — and it passes
    /// the import gate that requires an op's Cell to be in its Organ's roster.
    ///
    /// Returns whether a snapshot was written.
    pub async fn compact_doc(&self, uid: &str) -> Result<bool, EngineError> {
        // BEFORE the export: an op appended concurrently then lands ABOVE
        // `through_seq` and is replayed on the next load. The reverse would
        // mark it folded when it is not, and lose it.
        //
        // WHOSE WINDOW IS THIS: the compaction decision is guarded by the op
        // lock, taken by every entry point rather than here. This function does
        // NOT take it, and must not: it is
        // reached from the import path
        // (`import_ops` → `materialise` → `apply_remote_crdt` → `maybe_compact`),
        // which already holds it, so locking here would deadlock on any
        // imported crdt op that trips the compaction threshold. Every entry
        // point holds it instead: `write_record_text`, `apply_client_crdt_update`,
        // `import_ops` and `compact_stale_docs`.
        // A deleted record's doc takes no more updates — that is the tombstone
        // freeze, and a snapshot is an update like any other. Compacting one
        // would log a `snapshot` op above its tombstone, which is the exact
        // shape the kind-aware supersede rule exists to survive; there is no
        // reason to manufacture it locally as well.
        if store::sync_apply::record_deleted(&self.store.pool, uid).await? != Some(false) {
            return Ok(false);
        }
        let max_seq = store::sync_ops::max_seq(&self.store.pool).await?;
        // Load rather than requiring the doc to be open. Requiring it is what
        // made an abandoned doc never compact: `maybe_compact` returned early
        // when the doc had been evicted, on the reasoning that the next write
        // compacts — true for a live doc, false for one edited heavily and then
        // left alone, whose tail then grew forever.
        let loaded = self.load_state(uid).await?;
        let snapshot = self.with_doc(uid, loaded, None, |doc, _| {
            doc.export(ExportMode::Snapshot)
                .map_err(|e| EngineError::Consequence(format!("snapshot export: {e}")))
        })?;
        // An empty doc has no history worth asserting, and logging one would
        // make a snapshot op that a peer could import over real text.
        if snapshot.is_empty() {
            return Ok(false);
        }
        {
            let mut registry = self
                .collab_docs
                .lock()
                .map_err(|_| EngineError::Consequence("collab registry poisoned".into()))?;
            if let Some(open) = registry.docs.get_mut(uid) {
                // Reset the tail baseline to the compacted version.
                open.snapshot_vv = open.doc.oplog_vv();
            }
        }
        store::record_docs::put(&self.store.pool, uid, &snapshot, max_seq).await?;
        let Some(local) = store::cells::local(&self.store.pool).await? else {
            return Ok(false);
        };
        store::sync_ops::append(
            &self.store.pool,
            "record",
            uid,
            "",
            store::sync_ops::OpKind::Snapshot,
            Some(&B64.encode(&snapshot)),
            nucleus::hlc::next(),
            &local.uid,
            &local.organ_uid,
            None,
            store::replica::root_of(&self.store.pool, uid)
                .await?
                .as_deref(),
        )
        .await?;
        Ok(true)
    }

    /// Compact every doc whose tail has grown past the threshold, whether or
    /// not anyone still has it open. Run from the sync loop's idle moment,
    /// alongside pruning — a doc that is never written again never compacts on
    /// its own, and until it does, none of its history can be pruned.
    pub async fn compact_stale_docs(&self) -> Result<usize, EngineError> {
        // Taken HERE and not in `compact_doc`, deliberately. `compact_doc` is
        // also reached from the import path
        // (`import_ops` → `materialise` → `apply_remote_crdt` → `maybe_compact`),
        // which already holds this lock — locking again there would deadlock on
        // any imported crdt op that happens to trip the compaction threshold.
        // The sweep's only caller is `sync_once`, which holds nothing.
        let _op = self.import_lock.lock().await;
        let stale =
            store::record_docs::records_needing_compaction(&self.store.pool, COMPACT_OPS).await?;
        let mut compacted = 0;
        for uid in stale {
            if self.compact_doc(&uid).await? {
                compacted += 1;
            }
        }
        Ok(compacted)
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
