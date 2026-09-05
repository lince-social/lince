use std::collections::HashMap;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use loro::{ExportMode, LoroDoc, UpdateOptions, VersionVector};

use crate::error::EngineError;

const MAX_OPEN_DOCS: usize = 64;
const COMPACT_OPS: i64 = 100;
const COMPACT_BYTES: usize = 256 * 1024;

struct OpenDoc {
    doc: LoroDoc,
    snapshot_vv: VersionVector,
    last_used: u64,
}

#[derive(Default)]
pub struct DocRegistry {
    docs: HashMap<String, OpenDoc>,
    tick: u64,
}

struct LoadedState {
    snapshot: Option<Vec<u8>>,
    tail: Vec<(i64, String)>,
}

pub struct TextWrite {
    pub head: String,
    pub body: String,
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
                    let _ = doc.import(&bytes);
                    had_history = true;
                }
            }
            if !had_history {
                if let Some((head, body)) = seed {
                    let _ = doc.set_peer_id(seed_peer_id(record_uid, &head, &body));
                    if !head.is_empty() {
                        let _ = doc.get_text("head").update(&head, UpdateOptions::default());
                    }
                    if !body.is_empty() {
                        let _ = doc.get_text("body").update(&body, UpdateOptions::default());
                    }
                    doc.commit();
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

    pub async fn write_record_text(
        &self,
        uid: &str,
        head: Option<&str>,
        body: Option<&str>,
    ) -> Result<(), EngineError> {
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

    pub async fn doc_text(&self, uid: &str) -> Result<(String, String), EngineError> {
        let loaded = self.load_state(uid).await?;
        self.with_doc(uid, loaded, None, |doc, _| {
            Ok((
                doc.get_text("head").to_string(),
                doc.get_text("body").to_string(),
            ))
        })
    }

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

    pub async fn apply_client_crdt_update(
        &self,
        uid: &str,
        update_b64: &str,
    ) -> Result<(), EngineError> {
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
                store::replica::root_of(&self.store.pool, uid)
                    .await?
                    .as_deref(),
            )
            .await?;
        }
        store::records::set_text(&self.store.pool, uid, Some(&write.head), Some(&write.body))
            .await?;
        self.maybe_compact(uid, write.tail_b64.len()).await?;
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

    pub async fn compact_doc(&self, uid: &str) -> Result<bool, EngineError> {
        if store::sync_apply::record_deleted(&self.store.pool, uid).await? != Some(false) {
            return Ok(false);
        }
        let max_seq = store::sync_ops::max_seq(&self.store.pool).await?;
        let loaded = self.load_state(uid).await?;
        let snapshot = self.with_doc(uid, loaded, None, |doc, _| {
            doc.export(ExportMode::Snapshot)
                .map_err(|e| EngineError::Consequence(format!("snapshot export: {e}")))
        })?;
        if snapshot.is_empty() {
            return Ok(false);
        }
        {
            let mut registry = self
                .collab_docs
                .lock()
                .map_err(|_| EngineError::Consequence("collab registry poisoned".into()))?;
            if let Some(open) = registry.docs.get_mut(uid) {
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

    pub async fn compact_stale_docs(&self) -> Result<usize, EngineError> {
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
