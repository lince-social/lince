//! Sync (Ontology §11): the unit of sync is the op, not the row. Every local
//! write became a field-level op in the `sync_op` log; this module moves op
//! BATCHES between Organs (reactive deltas through the bounded outbox,
//! catch-up through per-contact checkpoints) and applies incoming ops with
//! per-field LWW, idempotent by `(actor_organ, hlc)`. Facts keep their signed,
//! hash-chained semantics and simply ride the log as kind `fact`.

use chrono::Utc;
use nucleus::{Cause, CauseKind, Fact, NewFact};
use serde::{Deserialize, Serialize};
use store::sync_ops::{self, OpKind, OpRow};

use crate::Engine;
use crate::error::EngineError;

/// One op on the wire: the log row minus its local seq, plus the full signed
/// Fact for kind=`fact` (hydrated at serve time — the log stores no copy).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireOp {
    pub tbl: String,
    pub uid: String,
    pub field: String,
    pub kind: String,
    pub value: Option<String>,
    pub hlc: i64,
    pub actor_organ: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact: Option<Fact>,
}

/// A batch of ops from one Organ's log — the ONLY sync payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpBatch {
    pub from_organ: String,
    pub ops: Vec<WireOp>,
}

impl Engine {
    /// Turn log rows into wire ops, hydrating facts from the read model.
    pub async fn hydrate_ops(&self, rows: Vec<OpRow>) -> Result<Vec<WireOp>, EngineError> {
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let fact = if row.kind == "fact" {
                store::facts::get(&self.store.pool, &row.uid).await?
            } else {
                None
            };
            let mut value = row.value;
            // An individual replica may be the only thing two Organs share,
            // so its assertion predicates cannot depend on the general feed
            // arriving later. Carry the predicate's canonical name as
            // hydration metadata; the persisted op identity/value stays
            // unchanged.
            if row.tbl == "record_assertion" && row.kind == "set" {
                if let Some(mut assertion) = value
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                {
                    if let Some(predicate_uid) = assertion
                        .get("predicate_uid")
                        .and_then(|item| item.as_str())
                    {
                        if let Some(name) =
                            store::concepts::canonical_name(&self.store.pool, predicate_uid).await?
                        {
                            assertion["predicate_name"] = serde_json::Value::String(name);
                            value = Some(assertion.to_string());
                        }
                    }
                }
            }
            out.push(WireOp {
                tbl: row.tbl,
                uid: row.uid,
                field: row.field,
                kind: row.kind,
                value,
                hlc: row.hlc,
                actor_organ: row.actor_organ,
                fact,
            });
        }
        Ok(out)
    }

    /// Serve the catch-up feed: ops past a checkpoint, hydrated, plus the
    /// current head seq. One indexed rowid-range query — an empty answer means
    /// converged.
    pub async fn ops_after(
        &self,
        after: i64,
        limit: i64,
    ) -> Result<(Vec<WireOp>, i64), EngineError> {
        let rows = sync_ops::after(&self.store.pool, after, limit).await?;
        let head = rows
            .last()
            .map(|row| row.seq)
            .unwrap_or(sync_ops::max_seq(&self.store.pool).await?.max(after));
        Ok((self.hydrate_ops(rows).await?, head))
    }

    /// Import ops that arrived on an individual-replica GRANT channel.
    ///
    /// `root` comes from the CHANNEL, never from the payload — the peer is
    /// told which conversation they are pushing into by the request framing,
    /// and that framing is checked against the local grant table here. A root
    /// read out of an op would let a grantee name any root they liked.
    ///
    /// Enforcement point three of three, and the load-bearing one: a bug here
    /// lets a contact write to Records that were never shared with them.
    pub async fn import_grant_batch(
        &self,
        root: &str,
        batch: &OpBatch,
    ) -> Result<usize, EngineError> {
        if !store::replica::is_accepted(&self.store.pool, root, &batch.from_organ).await? {
            return Err(EngineError::Consequence(format!(
                "no accepted grant on {root} for organ {}",
                batch.from_organ
            )));
        }
        // Immutability is enforced on ARRIVAL, not merely applied. Three cases:
        // a Record that already exists must already belong to THIS root;
        // one that belongs to a different root, or to the general feed, is an
        // attempt to re-scope an existing uid through a channel that does not
        // govern it; and one that does not exist yet is created inside the
        // root by the stamp below.
        for op in &batch.ops {
            let existing = match op.tbl.as_str() {
                "record" => store::replica::root_of(&self.store.pool, &op.uid).await?,
                _ => store::replica::root_for_op(&self.store.pool, &op.tbl, &op.uid).await?,
            };
            let known_row = store::records::get(&self.store.pool, &op.uid)
                .await?
                .is_some();
            if (known_row || existing.is_some()) && existing.as_deref() != Some(root) {
                store::organs::quarantine(
                    &self.store.pool,
                    &batch.from_organ,
                    "grant channel targeted a record outside its root",
                    &serde_json::to_string(op).unwrap_or_default(),
                )
                .await?;
                return Err(EngineError::Consequence(
                    "grant channel targeted a record outside its root".into(),
                ));
            }
        }
        self.import_ops(batch, Some(root)).await
    }

    /// Apply a batch of remote ops from the GENERAL feed. Returns how many
    /// were newly applied. Idempotent by op identity `(actor_organ, hlc)`;
    /// rejected rows land in quarantine and the rest of the batch still
    /// applies; batches from blocked organs are rejected wholesale.
    pub async fn import_op_batch(&self, batch: &OpBatch) -> Result<usize, EngineError> {
        self.import_ops(batch, None).await
    }

    async fn import_ops(
        &self,
        batch: &OpBatch,
        replica_root: Option<&str>,
    ) -> Result<usize, EngineError> {
        if let Some(contact) = store::organs::contact(&self.store.pool, &batch.from_organ).await? {
            if contact.trust == "blocked" {
                return Err(EngineError::Consequence(format!(
                    "organ {} is blocked",
                    batch.from_organ
                )));
            }
        }
        let pool = &self.store.pool;
        let from = Some(batch.from_organ.as_str());
        let mut applied = 0usize;
        let mut touched: Vec<String> = Vec::new();
        for op in &batch.ops {
            // The general feed may not touch an individually-replicated
            // Record. Without this, a contact with ordinary `sync_out` could
            // write into a conversation shared with someone else entirely by
            // pushing a plain op batch at its uid — the same re-scoping attack
            // the grant channel guards against, arriving by the other door.
            if replica_root.is_none() {
                if store::replica::root_for_op(pool, &op.tbl, &op.uid)
                    .await?
                    .is_some()
                {
                    store::organs::quarantine(
                        pool,
                        &batch.from_organ,
                        "general feed targeted an individually-replicated record",
                        &serde_json::to_string(op).unwrap_or_default(),
                    )
                    .await?;
                    continue;
                }
            }
            let Some(kind) = OpKind::parse(&op.kind) else {
                store::organs::quarantine(
                    pool,
                    &batch.from_organ,
                    "unknown op kind",
                    &serde_json::to_string(op).unwrap_or_default(),
                )
                .await?;
                continue;
            };
            match (op.tbl.as_str(), kind) {
                ("fact", OpKind::Fact) => {
                    if self.import_fact_op(op, &batch.from_organ).await? {
                        applied += 1;
                        if let Some(fact) = &op.fact {
                            touched.push(fact.record_uid.clone());
                        }
                    }
                }
                ("record", OpKind::Set) => {
                    // LWW against the stored stamp BEFORE appending this op.
                    let prior =
                        sync_ops::latest_hlc_for_field(pool, "record", &op.uid, &op.field).await?;
                    let tomb = sync_ops::latest_hlc_for_field(pool, "record", &op.uid, "").await?;
                    if sync_ops::append(
                        pool,
                        &op.tbl,
                        &op.uid,
                        &op.field,
                        kind,
                        op.value.as_deref(),
                        op.hlc,
                        &op.actor_organ,
                        from,
                        replica_root,
                    )
                    .await?
                    .is_none()
                    {
                        continue; // duplicate identity
                    }
                    nucleus::hlc::observe(op.hlc);
                    if op.hlc <= prior.unwrap_or(i64::MIN) {
                        continue; // older than the stored value: log only
                    }
                    if let Some(tomb) = tomb {
                        if op.hlc <= tomb {
                            continue; // deleted stays deleted; late sets lose
                        }
                    }
                    // Once collab history exists, the record-doc owns text:
                    // a late create-era head/body set op is log-only. The
                    // op's undelete power still applies — it won its HLC race.
                    if (op.field == "head" || op.field == "body")
                        && store::record_docs::has_crdt_history(pool, &op.uid).await?
                    {
                        if tomb.is_some() {
                            store::sync_apply::undelete_record(pool, &op.uid).await?;
                            applied += 1;
                            touched.push(op.uid.clone());
                        }
                        continue;
                    }
                    let value: serde_json::Value = op
                        .value
                        .as_deref()
                        .and_then(|raw| serde_json::from_str(raw).ok())
                        .unwrap_or(serde_json::Value::Null);
                    store::sync_apply::ensure_record_stub(
                        pool,
                        &op.uid,
                        if op.field == "kind" {
                            value.as_str().unwrap_or("plain")
                        } else {
                            "plain"
                        },
                        &op.actor_organ,
                        replica_root,
                        Some(op.hlc),
                    )
                    .await?;
                    store::sync_apply::set_record_field(
                        pool,
                        &op.uid,
                        &op.field,
                        &value,
                        tomb.is_some(), // newer set undeletes
                    )
                    .await?;
                    applied += 1;
                    touched.push(op.uid.clone());
                }
                ("record", OpKind::Tombstone) => {
                    let prior = sync_ops::latest_hlc_for_field(pool, "record", &op.uid, "").await?;
                    let latest_set =
                        sync_ops::latest_set_hlc_for_row(pool, "record", &op.uid).await?;
                    if sync_ops::append(
                        pool,
                        &op.tbl,
                        &op.uid,
                        &op.field,
                        kind,
                        None,
                        op.hlc,
                        &op.actor_organ,
                        from,
                        replica_root,
                    )
                    .await?
                    .is_none()
                    {
                        continue;
                    }
                    nucleus::hlc::observe(op.hlc);
                    if op.hlc <= prior.unwrap_or(i64::MIN) {
                        continue;
                    }
                    // Alive iff the latest set beats the latest tombstone —
                    // "undelete is a newer write". A concurrent newer edit
                    // keeps the record alive; otherwise the tombstone lands.
                    if op.hlc > latest_set.unwrap_or(i64::MIN) {
                        store::sync_apply::tombstone_record(pool, &op.uid).await?;
                        applied += 1;
                        touched.push(op.uid.clone());
                    }
                }
                ("record_extension", OpKind::Set | OpKind::Tombstone) => {
                    let prior = sync_ops::latest_hlc_for_field(
                        pool,
                        "record_extension",
                        &op.uid,
                        &op.field,
                    )
                    .await?;
                    if sync_ops::append(
                        pool,
                        &op.tbl,
                        &op.uid,
                        &op.field,
                        kind,
                        op.value.as_deref(),
                        op.hlc,
                        &op.actor_organ,
                        from,
                        replica_root,
                    )
                    .await?
                    .is_none()
                    {
                        continue;
                    }
                    nucleus::hlc::observe(op.hlc);
                    if op.hlc <= prior.unwrap_or(i64::MIN) {
                        continue;
                    }
                    store::sync_apply::ensure_record_stub(
                        pool,
                        &op.uid,
                        "plain",
                        &op.actor_organ,
                        replica_root,
                        Some(op.hlc),
                    )
                    .await?;
                    // Field is "{namespace}.{key}" — keys have no dots,
                    // namespaces may. No dot at all = whole-value namespace.
                    match op.field.rsplit_once('.') {
                        Some((namespace, key)) => match kind {
                            OpKind::Set => {
                                let value = op
                                    .value
                                    .as_deref()
                                    .and_then(|raw| serde_json::from_str(raw).ok())
                                    .unwrap_or(serde_json::Value::Null);
                                store::sync_apply::set_extension_key(
                                    pool, &op.uid, namespace, key, value,
                                )
                                .await?;
                            }
                            _ => {
                                store::sync_apply::tombstone_extension_key(
                                    pool, &op.uid, namespace, key,
                                )
                                .await?;
                            }
                        },
                        None => {
                            let value = op
                                .value
                                .as_deref()
                                .and_then(|raw| serde_json::from_str(raw).ok())
                                .unwrap_or(serde_json::Value::Null);
                            store::sync_apply::set_extension_whole(
                                pool, &op.uid, &op.field, &value,
                            )
                            .await?;
                        }
                    }
                    applied += 1;
                    touched.push(op.uid.clone());
                }
                ("record_assertion", OpKind::Set | OpKind::Tombstone) => {
                    let prior =
                        sync_ops::latest_hlc_for_field(pool, "record_assertion", &op.uid, "")
                            .await?;
                    if sync_ops::append(
                        pool,
                        &op.tbl,
                        &op.uid,
                        &op.field,
                        kind,
                        op.value.as_deref(),
                        op.hlc,
                        &op.actor_organ,
                        from,
                        replica_root,
                    )
                    .await?
                    .is_none()
                    {
                        continue;
                    }
                    nucleus::hlc::observe(op.hlc);
                    if op.hlc <= prior.unwrap_or(i64::MIN) {
                        continue; // later HLC wins per assertion uid
                    }
                    match kind {
                        OpKind::Set => {
                            let Some(value) = op.value.as_deref().and_then(|raw| {
                                serde_json::from_str::<serde_json::Value>(raw).ok()
                            }) else {
                                continue;
                            };
                            if let Some(subject) = value.get("subject_uid").and_then(|v| v.as_str())
                            {
                                store::sync_apply::ensure_record_stub(
                                    pool,
                                    subject,
                                    "plain",
                                    &op.actor_organ,
                                    replica_root,
                                    Some(op.hlc),
                                )
                                .await?;
                                touched.push(subject.to_string());
                            }
                            store::sync_apply::upsert_assertion(pool, &op.uid, &value).await?;
                        }
                        _ => {
                            store::sync_apply::retract_assertion(pool, &op.uid).await?;
                        }
                    }
                    applied += 1;
                }
                ("concept", OpKind::Set | OpKind::Tombstone) => {
                    let prior =
                        sync_ops::latest_hlc_for_field(pool, "concept", &op.uid, &op.field).await?;
                    if sync_ops::append(
                        pool,
                        &op.tbl,
                        &op.uid,
                        &op.field,
                        kind,
                        op.value.as_deref(),
                        op.hlc,
                        &op.actor_organ,
                        from,
                        replica_root,
                    )
                    .await?
                    .is_none()
                    {
                        continue;
                    }
                    nucleus::hlc::observe(op.hlc);
                    if op.hlc <= prior.unwrap_or(i64::MIN) {
                        continue;
                    }
                    match kind {
                        OpKind::Set => {
                            let name = op
                                .value
                                .as_deref()
                                .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                                .and_then(|v| v.as_str().map(str::to_string))
                                .unwrap_or_default();
                            if !name.is_empty() {
                                store::sync_apply::upsert_concept(
                                    pool,
                                    &op.uid,
                                    &name,
                                    &op.actor_organ,
                                )
                                .await?;
                            }
                        }
                        _ => {
                            store::sync_apply::delete_concept(pool, &op.uid).await?;
                        }
                    }
                    applied += 1;
                }
                ("record", OpKind::Crdt) => {
                    let Some(value) = op.value.as_deref() else {
                        store::organs::quarantine(
                            pool,
                            &batch.from_organ,
                            "crdt op without a payload",
                            &serde_json::to_string(op).unwrap_or_default(),
                        )
                        .await?;
                        continue;
                    };
                    if sync_ops::append(
                        pool,
                        &op.tbl,
                        &op.uid,
                        &op.field,
                        kind,
                        Some(value),
                        op.hlc,
                        &op.actor_organ,
                        from,
                        replica_root,
                    )
                    .await?
                    .is_none()
                    {
                        continue; // duplicate identity
                    }
                    nucleus::hlc::observe(op.hlc);
                    match store::sync_apply::record_deleted(pool, &op.uid).await? {
                        // Tombstone freeze: a deleted record's doc takes no
                        // more updates — the op stays in the log for relay.
                        Some(true) => continue,
                        Some(false) => {}
                        None => {
                            store::sync_apply::ensure_record_stub(
                                pool,
                                &op.uid,
                                "plain",
                                &op.actor_organ,
                                replica_root,
                                Some(op.hlc),
                            )
                            .await?;
                        }
                    }
                    match self.apply_remote_crdt(&op.uid, value).await? {
                        Ok(()) => {
                            applied += 1;
                            touched.push(op.uid.clone());
                        }
                        Err(reason) => {
                            store::organs::quarantine(
                                pool,
                                &batch.from_organ,
                                &reason,
                                &serde_json::to_string(op).unwrap_or_default(),
                            )
                            .await?;
                        }
                    }
                }
                _ => {
                    store::organs::quarantine(
                        pool,
                        &batch.from_organ,
                        "op kind does not fit its table",
                        &serde_json::to_string(op).unwrap_or_default(),
                    )
                    .await?;
                }
            }
        }
        // One refresh signal per touched record, not per op: a zero-delta
        // Sync fact wakes Protein subscribers (and stays out of the op log —
        // facts::insert skips CauseKind::Sync).
        touched.sort();
        touched.dedup();
        for record_uid in touched {
            if store::records::get(pool, &record_uid).await?.is_some() {
                let _ = self
                    .append(
                        NewFact {
                            uid: None,
                            record_uid,
                            delta: nucleus::fact::zero_delta(),
                            at: None,
                            actor_uid: None,
                            cause: Cause {
                                kind: CauseKind::Sync,
                                uid: Some(batch.from_organ.clone()),
                            },
                            payload: Some("{\"sync\":true}".to_string()),
                        },
                        Utc::now(),
                    )
                    .await;
            }
        }
        Ok(applied)
    }

    /// Import one fact op: two-layer tamper model (XI) — the chain step guards
    /// content→hash, the signature guards hash→author. Re-seals for the local
    /// chain but keeps the ORIGIN signature. Returns whether the fact is new.
    async fn import_fact_op(&self, op: &WireOp, from_organ: &str) -> Result<bool, EngineError> {
        let pool = &self.store.pool;
        let Some(fact) = &op.fact else {
            store::organs::quarantine(
                pool,
                from_organ,
                "fact op without its fact",
                &serde_json::to_string(op).unwrap_or_default(),
            )
            .await?;
            return Ok(false);
        };
        if !nucleus::fact::verify_chain_step(fact) {
            store::organs::quarantine(
                pool,
                from_organ,
                "chain step does not verify",
                &serde_json::to_string(fact).unwrap_or_default(),
            )
            .await?;
            return Ok(false);
        }
        if fact.signature.is_some()
            && !crate::trust::verify_fact(&self.store, fact)
                .await
                .unwrap_or(false)
        {
            store::organs::quarantine(
                pool,
                from_organ,
                "signature does not verify",
                &serde_json::to_string(fact).unwrap_or_default(),
            )
            .await?;
            return Ok(false);
        }
        // The op joins the log under its ORIGIN identity (relay); dedupe by
        // identity first, then by fact uid.
        if sync_ops::append(
            pool,
            "fact",
            &op.uid,
            "",
            OpKind::Fact,
            None,
            op.hlc,
            &op.actor_organ,
            Some(from_organ),
            // Facts do not participate in individual replica today; the
            // scope is `record` and `record_assertion` rows, which is what a
            // conversation is made of.
            None,
        )
        .await?
        .is_none()
        {
            return Ok(false);
        }
        nucleus::hlc::observe(op.hlc);
        store::sync_apply::ensure_record_stub(
            pool,
            &fact.record_uid,
            "plain",
            &op.actor_organ,
            None,
            Some(op.hlc),
        )
        .await?;
        let imported = NewFact {
            uid: Some(fact.uid.clone()), // idempotent by uid
            record_uid: fact.record_uid.clone(),
            delta: fact.delta,
            at: Some(fact.at),
            actor_uid: fact.actor_uid.clone(), // origin author survives
            cause: Cause {
                kind: CauseKind::Sync,
                uid: Some(from_organ.to_string()),
            },
            payload: fact.payload.clone(),
        };
        let mut news = imported;
        let signer = self.signer.lock().await.clone();
        let mut tx = self.store.pool.begin().await?;
        if store::facts::exists(&mut tx, news.uid.as_ref().unwrap()).await? {
            tx.rollback().await?;
            return Ok(false);
        }
        news.actor_uid.get_or_insert_with(|| {
            signer
                .as_ref()
                .map(|s| s.actor_uid.clone())
                .unwrap_or_default()
        });
        let prev = store::facts::last_hash(&mut tx).await?;
        let mut sealed = nucleus::fact::seal(news, &prev, Utc::now());
        sealed.signature = fact.signature.clone(); // origin authorship
        store::facts::insert(&mut tx, &sealed).await?;
        store::records::bump_quantity(
            &mut tx,
            &sealed.record_uid,
            sealed.delta,
            &Utc::now().to_rfc3339(),
        )
        .await?;
        tx.commit().await?;
        let _ = self.bus.send(sealed);
        Ok(true)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Introduction {
    pub organ_uid: String,
    pub slug: Option<String>,
    pub head: String,
    pub base_url: String,
    /// `(key_id, public_key_b64)` pairs.
    pub keys: Vec<(String, String)>,
}

impl Engine {
    /// Export my own introduction.
    pub async fn introduction(&self) -> Result<Introduction, EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("no local organ".into()))?;
        let keys = crate::trust::keys_of(&self.store, &organ.uid).await?;
        Ok(Introduction {
            organ_uid: organ.uid,
            slug: organ.slug,
            head: organ.head,
            base_url: organ.base_url,
            keys,
        })
    }

    /// Adopt a peer's introduction: register the contact under THEIR organ uid
    /// and store their public keys so their signed facts verify.
    pub async fn adopt_introduction(
        &self,
        intro: &Introduction,
        proximity: u32,
    ) -> Result<String, EngineError> {
        let uid = store::organs::add_contact(
            &self.store.pool,
            &intro.organ_uid,
            intro.slug.as_deref(),
            &intro.head,
            &intro.base_url,
            proximity,
        )
        .await?;
        for (key_id, public_key) in &intro.keys {
            crate::trust::adopt_key(&self.store, &intro.organ_uid, key_id, public_key).await?;
        }
        Ok(uid)
    }
}

/// One open promise as it travels to peers' discovery caches (blueprint XV →
/// X). Proximity is NOT here on purpose: the receiver stamps it from its own
/// contact row — offer-ordering never exposes local proximity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenPromiseExport {
    pub promise_uid: String,
    pub concept: Option<String>,
    pub unit: Option<String>,
    pub delta: f64,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
    pub confidence: f64,
}

impl Engine {
    /// Drain the bounded outbox through a sender (the HTTP boundary in
    /// production, an in-memory wire in tests): one op batch per contact,
    /// hydrated from the log by seq. On success the delivered rows are
    /// deleted (seq-guarded — an op replaced while in flight stays queued);
    /// on failure attempts bump and everything stays queued. Returns batches
    /// delivered.
    pub async fn drain_outbox<F, Fut>(&self, mut send: F) -> Result<usize, EngineError>
    where
        F: FnMut(store::organs::Contact, Option<String>, OpBatch) -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        let pool = &self.store.pool;
        let Some(from_organ) = store::organs::local(pool).await?.map(|o| o.uid) else {
            return Ok(0);
        };
        let due = sync_ops::outbox_due(pool).await?;
        let mut sent = 0usize;
        let mut index = 0usize;
        while index < due.len() {
            let contact_uid = due[index].contact_organ.clone();
            let mut rows = Vec::new();
            while index < due.len() && due[index].contact_organ == contact_uid {
                rows.push(due[index].clone());
                index += 1;
            }
            let Some(contact) = store::organs::contact(pool, &contact_uid).await? else {
                sync_ops::outbox_clear_contact(pool, &contact_uid).await?;
                continue;
            };
            if contact.trust == "blocked" {
                sync_ops::outbox_clear_contact(pool, &contact_uid).await?;
                continue;
            }
            let mut log_rows = Vec::new();
            let mut kept = Vec::new();
            for row in rows {
                match sync_ops::get_by_seq(pool, row.seq).await? {
                    // `sync_out` controls the broad contact feed. An accepted
                    // individual grant is its own, narrower permission and
                    // must keep flowing even while that broad switch is off.
                    Some(op) if !contact.sync_out && op.replica_root.is_none() => {
                        sync_ops::outbox_delete(pool, &row).await?;
                    }
                    Some(op) => {
                        log_rows.push(op);
                        kept.push(row);
                    }
                    // Pruned from the log while queued: nothing to send.
                    None => sync_ops::outbox_delete(pool, &row).await?,
                }
            }
            if kept.is_empty() {
                continue;
            }
            log_rows.sort_by_key(|op| op.seq);
            // Split by root BEFORE sending. General-feed ops and each
            // conversation's ops leave on different channels, so mixing them
            // into one batch would either leak a private Record onto the
            // general feed or force the receiver to trust a root read out of
            // the payload. One batch per (contact, root).
            let mut roots: Vec<Option<String>> = Vec::new();
            for op in &log_rows {
                if !roots.contains(&op.replica_root) {
                    roots.push(op.replica_root.clone());
                }
            }
            let mut all_ok = true;
            for root in roots {
                let slice: Vec<_> = log_rows
                    .iter()
                    .filter(|op| op.replica_root == root)
                    .cloned()
                    .collect();
                let batch = OpBatch {
                    from_organ: from_organ.clone(),
                    ops: self.hydrate_ops(slice).await?,
                };
                if send(contact.clone(), root, batch).await.is_err() {
                    all_ok = false;
                    break;
                }
            }
            match if all_ok { Ok(()) } else { Err(String::new()) } {
                Ok(()) => {
                    // The peer accepted every batch, so everything we intended
                    // to send them at or below this seq is now on their side.
                    // That is the retention floor: pruning removes ops nobody
                    // is still owed, and an op this contact was never going to
                    // be sent (not visible to them, or superseded in the
                    // bounded outbox) is not owed either.
                    if let Some(high) = kept.iter().map(|row| row.seq).max() {
                        store::organs::advance_peer_acked_seq(pool, &contact_uid, high).await?;
                    }
                    for row in &kept {
                        sync_ops::outbox_delete(pool, row).await?;
                    }
                    sent += 1;
                }
                Err(_) => {
                    sync_ops::outbox_bump_attempts(pool, &contact_uid).await?;
                }
            }
        }
        Ok(sent)
    }

    /// Drop op-log entries every synced contact has already received.
    ///
    /// An explicit operation, never a background loop. A contact that falls
    /// behind the pruned floor recovers by re-bootstrapping from a serve-time
    /// snapshot, and that path is not built yet — so until it is, this trades
    /// recoverability for disk and a human should be the one making the trade.
    /// Call with `dry_run` first: the report is computed from the same
    /// predicate the delete uses, so it cannot disagree with the real thing.
    pub async fn prune_op_log(
        &self,
        dry_run: bool,
    ) -> Result<sync_ops::PruneReport, EngineError> {
        Ok(sync_ops::prune(&self.store.pool, dry_run).await?)
    }

    /// The open promises `subject` may see — what peers pull into their
    /// discovery caches. Only proposer-owned OPEN promises travel, and only
    /// through the visibility gate (their target record must be visible). The
    /// unfilled role is the counterparty; `party_uid` is the proposer.
    pub async fn open_promise_export(
        &self,
        subject: &str,
    ) -> Result<Vec<OpenPromiseExport>, EngineError> {
        let visible = store::visibility::visible_targets(&self.store.pool, subject).await?;
        let mut out = Vec::new();
        for p in store::misc::list_promises(&self.store.pool).await? {
            if p.state != nucleus::PromiseState::Open || p.party_uid.is_none() {
                continue;
            }
            let Some(record_uid) = &p.record_uid else {
                continue;
            };
            if !visible.contains(record_uid) {
                continue;
            }
            let record = store::records::get(&self.store.pool, record_uid).await?;
            out.push(OpenPromiseExport {
                promise_uid: p.uid.clone(),
                concept: record
                    .as_ref()
                    .and_then(|r| r.identity_predicate_uid.clone()),
                unit: record.as_ref().and_then(|r| r.unit_uid.clone()),
                delta: p.delta,
                window_start: None,
                window_end: p.window_end.clone(),
                confidence: 0.5, // strangers start at the prior; XI history refines
            });
        }
        Ok(out)
    }

    /// Refresh the discovery cache with a contact's open promises (the pull
    /// side of the feed). Proximity is stamped from OUR contact row; blocked
    /// organs are rejected here too.
    pub async fn refresh_discovery(
        &self,
        organ_uid: &str,
        fetched: Vec<OpenPromiseExport>,
    ) -> Result<usize, EngineError> {
        let contact = store::organs::contact(&self.store.pool, organ_uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(organ_uid.into()))?;
        if contact.trust == "blocked" {
            return Err(EngineError::Consequence(format!(
                "organ {organ_uid} is blocked"
            )));
        }
        let mut stored = 0;
        for open in fetched {
            store::senses::upsert_remote_open(
                &self.store.pool,
                &store::senses::RemoteOpenRow {
                    promise_uid: open.promise_uid,
                    organ: organ_uid.to_string(),
                    proximity: contact.proximity,
                    concept: open.concept,
                    unit: open.unit,
                    delta: open.delta,
                    window_start: open.window_start,
                    window_end: open.window_end,
                    confidence: open.confidence,
                },
            )
            .await?;
            stored += 1;
        }
        Ok(stored)
    }
}
