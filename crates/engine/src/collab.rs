use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use loro::{ExportMode, LoroDoc, VersionVector};
use protein::authority::Property;

use crate::collab_guard::{AcceptedDoc, GuardError, Limits, PreparedDelta};
use crate::error::EngineError;

const MAX_OPEN_DOCS: usize = 64;
const COMPACT_OPS: i64 = 100;

struct OpenDoc {
    doc: Arc<AcceptedDoc>,
    last_used: u64,
}

#[derive(Default)]
pub struct DocRegistry {
    docs: HashMap<String, OpenDoc>,
    tick: u64,
}

pub fn limits() -> Limits {
    Limits {
        peers: 4096,
        delta_atoms: 1_048_576,
        ..Limits::default()
    }
}

fn refused(error: impl std::fmt::Display) -> EngineError {
    EngineError::Consequence(format!("Collaborative edit refused: {error}"))
}

pub fn encode_version(version: &VersionVector) -> String {
    B64.encode(version.encode())
}

pub fn decode_version(value: &str) -> Result<VersionVector, EngineError> {
    if value.len() > 128 * 1024 {
        return Err(refused("document version is too large"));
    }
    VersionVector::decode(&B64.decode(value).map_err(refused)?).map_err(refused)
}

impl crate::Engine {
    fn cache_doc(&self, uid: &str, doc: Arc<AcceptedDoc>) {
        let mut registry = self.collab_docs.lock().expect("document registry");
        registry.tick = registry.tick.wrapping_add(1);
        let last_used = registry.tick;
        registry.docs.insert(uid.into(), OpenDoc { doc, last_used });
        if registry.docs.len() > MAX_OPEN_DOCS {
            let oldest = registry
                .docs
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(uid, _)| uid.clone());
            if let Some(oldest) = oldest {
                registry.docs.remove(&oldest);
            }
        }
    }

    async fn accepted_doc(&self, uid: &str) -> Result<Arc<AcceptedDoc>, EngineError> {
        {
            let mut registry = self.collab_docs.lock().map_err(refused)?;
            registry.tick = registry.tick.wrapping_add(1);
            let tick = registry.tick;
            if let Some(entry) = registry.docs.get_mut(uid) {
                entry.last_used = tick;
                return Ok(entry.doc.clone());
            }
        }
        let stored = store::record_docs::get(&self.store.pool, uid).await?;
        let through = stored.as_ref().map_or(0, |row| row.through_seq);
        let tail = store::record_docs::doc_tail(&self.store.pool, uid, through).await?;
        let mut accepted = if let Some(stored) = stored {
            AcceptedDoc::from_trusted_snapshot(&stored.snapshot, &limits()).map_err(refused)?
        } else if tail.is_empty() {
            let record = store::records::get(&self.store.pool, uid)
                .await?
                .ok_or_else(|| EngineError::UnknownRecord(uid.into()))?;
            let doc = LoroDoc::new();
            doc.set_peer_id(seed_peer_id(uid, &record.head, &record.body))
                .map_err(refused)?;
            for (key, text) in [("head", record.head), ("body", record.body)] {
                doc.get_text(key).insert(0, &text).map_err(refused)?;
            }
            doc.commit();
            let snapshot = doc.export(ExportMode::Snapshot).map_err(refused)?;
            let accepted =
                AcceptedDoc::from_trusted_snapshot(&snapshot, &limits()).map_err(refused)?;
            store::record_docs::put(&self.store.pool, uid, accepted.snapshot(), 0).await?;
            accepted
        } else {
            AcceptedDoc::empty(&limits()).map_err(refused)?
        };
        for (_, encoded) in tail {
            let bytes = B64.decode(&encoded).map_err(refused)?;
            accepted = accepted
                .prepare_replica(&bytes, &limits())
                .map_err(refused)?
                .into_accepted_after_commit();
        }
        let doc = Arc::new(accepted);
        self.cache_doc(uid, doc.clone());
        Ok(doc)
    }

    pub async fn may_read_record(
        &self,
        subject: Option<&str>,
        uid: &str,
    ) -> Result<bool, EngineError> {
        let Some(subject) = subject else {
            return Ok(true);
        };
        let query = protein::Protein {
            source: protein::Source::Record,
            filter: vec![protein::Predicate::UidEq(uid.into())],
            fields: Some(vec!["uid".into()]),
            include: Default::default(),
            aggregate: None,
            order: vec![],
            limit: Some(1),
        };
        Ok(!protein::execute_for(&self.store, &query, Some(subject))
            .await?
            .is_empty())
    }

    pub async fn record_text_permissions(
        &self,
        actor: Option<&str>,
        uid: &str,
    ) -> Result<BTreeSet<Property>, EngineError> {
        self.record_property_permissions(
            actor,
            uid,
            BTreeSet::from([Property::Head, Property::Body]),
        )
        .await
    }

    pub(crate) async fn record_property_permissions(
        &self,
        actor: Option<&str>,
        uid: &str,
        requested: BTreeSet<Property>,
    ) -> Result<BTreeSet<Property>, EngineError> {
        self.authorize_action(
            &crate::actions::Action::EditRecordText {
                target: uid.into(),
                head: None,
                body: None,
            },
            actor,
        )
        .await?;
        self.reject_direct_transfer_record_mutation(uid).await?;
        if !self.may_read_record(actor, uid).await? {
            return Err(EngineError::Forbidden("Record is no longer visible".into()));
        }
        let record = store::records::get(&self.store.pool, uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(uid.into()))?;
        if record.kind == "message" {
            if let Some(metadata) =
                store::records::get_extension(&self.store.pool, uid, "lince.message").await?
            {
                if metadata["state"] == "writing"
                    && actor.is_some_and(|actor| metadata["operator"].as_str() != Some(actor))
                {
                    return Err(EngineError::Forbidden(
                        "Only the message's writer may change it while it is being written".into(),
                    ));
                }
            }
        }
        let both = requested;
        let Some(actor) = actor else {
            return Ok(both);
        };
        let Some(person) = store::auth::person_access(&self.store.pool, actor).await? else {
            return Ok(both);
        };
        let Some(role) = person.role_id else {
            return Ok(both);
        };
        let Some(policy) = store::role_policies::get(&self.store.pool, role)
            .await?
            .and_then(|row| row.policy)
        else {
            return Ok(both);
        };
        let policy: protein::authority::RolePolicy =
            serde_json::from_value(policy).map_err(EngineError::Json)?;
        let mut allowed = BTreeSet::new();
        for grant in policy.grants {
            if grant.operation != protein::authority::Operation::Update {
                continue;
            }
            let query = protein::Protein {
                source: protein::Source::Record,
                filter: vec![protein::Predicate::UidEq(uid.into()), grant.selector],
                fields: Some(vec!["uid".into()]),
                include: Default::default(),
                aggregate: None,
                order: vec![],
                limit: Some(1),
            };
            if !protein::execute_for(&self.store, &query, Some(actor))
                .await?
                .is_empty()
            {
                allowed.extend(grant.properties.intersection(&both).cloned());
            }
        }
        Ok(allowed)
    }

    pub async fn doc_text(&self, uid: &str) -> Result<(String, String), EngineError> {
        let doc = self.accepted_doc(uid).await?;
        Ok((doc.head().into(), doc.body().into()))
    }

    pub async fn collab_snapshot(&self, uid: &str) -> Result<String, EngineError> {
        Ok(self.collab_state(uid).await?.0)
    }

    pub async fn collab_state(&self, uid: &str) -> Result<(String, String), EngineError> {
        let _serial = self.import_lock.lock().await;
        let doc = self.accepted_doc(uid).await?;
        Ok((B64.encode(doc.snapshot()), encode_version(&doc.version())))
    }

    pub async fn collab_since(
        &self,
        uid: &str,
        version: &str,
    ) -> Result<Option<(String, String)>, EngineError> {
        let version = decode_version(version)?;
        let _serial = self.import_lock.lock().await;
        let doc = self.accepted_doc(uid).await?;
        if doc.version() == version {
            return Ok(None);
        }
        Ok(Some((
            B64.encode(doc.export_updates(&version).map_err(refused)?),
            encode_version(&doc.version()),
        )))
    }

    pub async fn collab_version(&self, uid: &str) -> Result<String, EngineError> {
        Ok(encode_version(&self.accepted_doc(uid).await?.version()))
    }

    pub(crate) async fn text_update_is_saved(
        &self,
        uid: &str,
        update: &str,
    ) -> Result<bool, EngineError> {
        if update.len() > limits().snapshot_bytes * 2 {
            return Ok(false);
        }
        let raw = match B64.decode(update) {
            Ok(raw) => raw,
            Err(_) => return Ok(false),
        };
        let doc = self.accepted_doc(uid).await?;
        Ok(doc
            .prepare_replica(&raw, &limits())
            .is_ok_and(|prepared| prepared.is_duplicate()))
    }

    async fn persist_text(
        &self,
        uid: &str,
        prepared: PreparedDelta,
        publish: bool,
        actor: Option<&str>,
        incoming: Option<(&crate::sync::WireOp, Option<&str>)>,
        receipt: Option<(&str, &str)>,
    ) -> Result<Option<nucleus::Fact>, EngineError> {
        if prepared.is_duplicate() && incoming.is_none() && receipt.is_none() {
            return Ok(None);
        }
        let publish = publish && !prepared.is_duplicate();
        let candidate = &prepared;
        let has_history: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sync_op WHERE tbl = 'record' AND uid = ? AND kind IN ('crdt', 'snapshot'))").bind(uid).fetch_one(&self.store.pool).await?;
        let beginning = VersionVector::default();
        let delta = candidate
            .export_updates(if has_history {
                prepared.base_version()
            } else {
                &beginning
            })
            .map_err(refused)?;
        let relay = if let Some((op, _)) = incoming {
            let local = store::cells::local(&self.store.pool).await?;
            let record = store::records::get(&self.store.pool, uid).await?;
            !prepared.is_duplicate()
                && local.is_some_and(|local| {
                    op.organ_uid != local.organ_uid
                        && record.and_then(|record| record.organ_uid).as_deref()
                            == Some(local.organ_uid.as_str())
                })
        } else {
            false
        };
        let signer = self.signer.lock().await.clone();
        let now = chrono::Utc::now();
        let mut tx = store::write_tx(&self.store.pool).await?;
        if let Some((id, payload)) = receipt {
            let original: Option<String> = store::sqlx::query_scalar(
                "SELECT payload FROM record_change_receipt WHERE actor = ? AND change_uid = ?",
            )
            .bind(actor.unwrap_or(""))
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some(original) = original {
                if original != payload {
                    return Err(refused("Change identity was already used for another edit"));
                }
                return Ok(None);
            }
            let result = serde_json::json!({"change_id":id,"state":"saved"}).to_string();
            store::sqlx::query("INSERT INTO record_change_receipt (actor, change_uid, record_uid, payload, result) VALUES (?, ?, ?, ?, ?)")
                .bind(actor.unwrap_or("")).bind(id).bind(uid).bind(payload).bind(result).execute(&mut *tx).await?;
        }
        if let Some((op, root)) = incoming {
            store::sqlx::query("INSERT OR IGNORE INTO sync_op (tbl, uid, field, kind, value, hlc, actor_cell, organ_uid, replica_root) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
                .bind(&op.tbl).bind(&op.uid).bind(&op.field).bind(&op.kind).bind(&op.value)
                .bind(op.hlc).bind(&op.actor_cell).bind(&op.organ_uid).bind(root).execute(&mut *tx).await?;
        }
        store::sqlx::query("UPDATE record SET head = ?, body = ?, updated_at = ? WHERE uid = ? AND deleted_at IS NULL")
            .bind(candidate.head()).bind(candidate.body()).bind(now.to_rfc3339()).bind(uid)
            .execute(&mut *tx).await?
            .rows_affected().checked_sub(1).ok_or_else(|| EngineError::UnknownRecord(uid.into()))?;
        if publish || relay {
            store::sync_ops::log_local_tx(
                &mut tx,
                "record",
                uid,
                "",
                store::sync_ops::OpKind::Crdt,
                Some(B64.encode(&delta)),
            )
            .await?;
        }
        let through: i64 = store::sqlx::query_scalar(
            "SELECT COALESCE(through_seq, 0) FROM record_doc WHERE record_uid = ?",
        )
        .bind(uid)
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or(0);
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM sync_op WHERE tbl = 'record' AND uid = ? AND kind = 'crdt' AND seq > ?")
            .bind(uid).bind(through).fetch_one(&mut *tx).await?;
        if count >= COMPACT_OPS {
            let seq: i64 = store::sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM sync_op")
                .fetch_one(&mut *tx)
                .await?;
            store::record_docs::put_on(&mut tx, uid, candidate.snapshot(), seq).await?;
        }
        let fact = if publish {
            crate::append::append_one_in_transaction(
                &mut tx,
                nucleus::NewFact {
                    uid: None,
                    record_uid: uid.into(),
                    delta: nucleus::fact::zero_delta(),
                    at: None,
                    actor_uid: actor.map(str::to_owned),
                    cause: nucleus::Cause::user_edit(),
                    payload: Some("{\"collab\":true}".into()),
                },
                now,
                signer.as_ref(),
            )
            .await?
        } else {
            None
        };
        tx.commit().await?;
        self.cache_doc(uid, Arc::new(prepared.into_accepted_after_commit()));
        Ok(fact)
    }

    pub async fn write_record_text(
        &self,
        uid: &str,
        head: Option<&str>,
        body: Option<&str>,
    ) -> Result<(), EngineError> {
        self.write_record_text_as(uid, head, body, None).await?;
        Ok(())
    }

    pub async fn write_record_text_as(
        &self,
        uid: &str,
        head: Option<&str>,
        body: Option<&str>,
        actor: Option<&str>,
    ) -> Result<Vec<nucleus::Fact>, EngineError> {
        let permissions = self.record_text_permissions(actor, uid).await?;
        if (head.is_some() && !permissions.contains(&Property::Head))
            || (body.is_some() && !permissions.contains(&Property::Body))
        {
            return Err(EngineError::Forbidden(
                "This text property is not writable".into(),
            ));
        }
        let _serial = self.import_lock.lock().await;
        let doc = self.accepted_doc(uid).await?;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| refused("local Cell is unavailable"))?;
        let peer = seed_peer_id(&cell.uid, "author", uid);
        let prepared = doc
            .prepare_text(peer, head, body, &limits())
            .map_err(refused)?;
        let fact = self
            .persist_text(uid, prepared, true, actor, None, None)
            .await?;
        drop(_serial);
        if let Some(fact) = &fact {
            self.observe_committed_fact(fact.clone(), chrono::Utc::now())
                .await?;
        }
        Ok(fact.into_iter().collect())
    }

    pub async fn apply_client_crdt_update(
        &self,
        uid: &str,
        update: &str,
    ) -> Result<(), EngineError> {
        self.apply_client_crdt_update_as(uid, update, None).await
    }

    pub async fn apply_client_crdt_update_as(
        &self,
        uid: &str,
        update: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        self.apply_text_change_as(uid, update, actor, None)
            .await
            .map(|_| ())
    }

    pub(crate) async fn apply_text_change_as(
        &self,
        uid: &str,
        update: &str,
        actor: Option<&str>,
        receipt: Option<(&str, &str)>,
    ) -> Result<Vec<nucleus::Fact>, EngineError> {
        self.access_scope(true, async {
            let permissions = self.record_text_permissions(actor, uid).await?;
            if update.len() > limits().delta_bytes * 2 {
                return Err(refused("edit is too large"));
            }
            let raw = B64.decode(update).map_err(refused)?;
            let _serial = self.import_lock.lock().await;
            let doc = self.accepted_doc(uid).await?;
            let prepared = doc
                .prepare_delta(&raw, &permissions, &limits())
                .map_err(refused)?;
            let fact = self
                .persist_text(uid, prepared, true, actor, None, receipt)
                .await?;
            drop(_serial);
            if let Some(fact) = fact {
                self.observe_committed_fact(fact, chrono::Utc::now()).await
            } else {
                Ok(Vec::new())
            }
        })
        .await
    }

    pub(crate) async fn apply_remote_crdt(
        &self,
        uid: &str,
        update: &str,
        _was_snapshot: bool,
    ) -> Result<Result<(), String>, EngineError> {
        if update.len() > limits().snapshot_bytes * 2 {
            return Ok(Err("document is too large".into()));
        }
        let raw = match B64.decode(update) {
            Ok(raw) => raw,
            Err(error) => return Ok(Err(error.to_string())),
        };
        let doc = self.accepted_doc(uid).await?;
        let prepared = match doc.prepare_replica(&raw, &limits()) {
            Ok(prepared) => prepared,
            Err(error) => return Ok(Err(error.to_string())),
        };
        self.persist_text(uid, prepared, false, None, None, None)
            .await?;
        Ok(Ok(()))
    }

    pub(crate) async fn import_text_op(
        &self,
        op: &crate::sync::WireOp,
        root: Option<&str>,
    ) -> Result<bool, EngineError> {
        let update = op
            .value
            .as_deref()
            .ok_or_else(|| refused("missing text operations"))?;
        if update.len() > limits().snapshot_bytes * 2 {
            return Err(refused("document is too large"));
        }
        let doc = self.accepted_doc(&op.uid).await?;
        let prepared = doc
            .prepare_replica(&B64.decode(update).map_err(refused)?, &limits())
            .map_err(refused)?;
        let changed = !prepared.is_duplicate();
        self.persist_text(&op.uid, prepared, false, None, Some((op, root)), None)
            .await?;
        Ok(changed)
    }

    pub async fn compact_doc(&self, uid: &str) -> Result<bool, EngineError> {
        if store::sync_apply::record_deleted(&self.store.pool, uid).await? != Some(false) {
            return Ok(false);
        }
        let doc = self.accepted_doc(uid).await?;
        let seq = store::sync_ops::max_seq(&self.store.pool).await?;
        store::record_docs::put(&self.store.pool, uid, doc.snapshot(), seq).await?;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| refused("Local Cell is unavailable"))?;
        store::sync_ops::append(
            &self.store.pool,
            "record",
            uid,
            "",
            store::sync_ops::OpKind::Snapshot,
            Some(&B64.encode(doc.snapshot())),
            nucleus::hlc::next(),
            &cell.uid,
            &cell.organ_uid,
            None,
            None,
        )
        .await?;
        Ok(true)
    }

    pub async fn compact_stale_docs(&self) -> Result<usize, EngineError> {
        let _serial = self.import_lock.lock().await;
        let stale =
            store::record_docs::records_needing_compaction(&self.store.pool, COMPACT_OPS).await?;
        let mut count = 0;
        for uid in stale {
            count += usize::from(self.compact_doc(&uid).await?);
        }
        Ok(count)
    }

    pub fn close_record_doc(&self, uid: &str) {
        if let Ok(mut registry) = self.collab_docs.lock() {
            registry.docs.remove(uid);
        }
    }
}

fn seed_peer_id(uid: &str, head: &str, body: &str) -> u64 {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for value in [uid, head, body] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    let digest = hasher.finalize();
    u64::from_le_bytes(digest[..8].try_into().expect("peer identity")) | 1
}

impl From<GuardError> for EngineError {
    fn from(error: GuardError) -> Self {
        refused(error)
    }
}
