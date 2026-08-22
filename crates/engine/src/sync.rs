//! Sync (Ontology §11): the unit of sync is the op, not the row. Every local
//! write became a field-level op in the `sync_op` log; this module moves op
//! BATCHES between Organs (reactive deltas through the bounded outbox,
//! catch-up through per-contact checkpoints) and applies incoming ops with
//! per-field LWW, idempotent by `(actor_cell, hlc)`. Facts keep their signed,
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
    /// The DEVICE that wrote this op. Half of the op identity
    /// `(actor_cell, hlc)`, and the half that makes it unique.
    pub actor_cell: String,
    /// The Organ this op is attributed to — the published identity. Carried
    /// separately because it is what `record.organ_uid` is stamped from, and
    /// deriving it from `actor_cell` would make one contact with three
    /// devices look like three different people.
    pub organ_uid: String,
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
                actor_cell: row.actor_cell,
                organ_uid: row.organ_uid,
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
        // Only what THIS Organ authored. Relaying is off, and serving a third
        // Organ's ops here would both leak them and be rejected on arrival.
        let Some(local) = store::organs::local(&self.store.pool).await? else {
            return Ok((Vec::new(), after));
        };
        let rows = sync_ops::after(&self.store.pool, &local.uid, after, limit).await?;
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
    /// were newly applied. Idempotent by op identity `(actor_cell, hlc)`;
    /// rejected rows land in quarantine and the rest of the batch still
    /// applies; batches from blocked organs are rejected wholesale.
    pub async fn import_op_batch(&self, batch: &OpBatch) -> Result<usize, EngineError> {
        self.import_ops(batch, None).await
    }

    /// Apply a batch that arrived through a CARRIER rather than over a
    /// connection (Ontology C4, `crate::seal`).
    ///
    /// Every check in `inadmissible` rests on one thing: the authenticated
    /// connection says who the sender is, so a field the sender filled in can
    /// be compared against something they did not choose. Out of a mailbox
    /// there is no connection, and `import_op_batch` on its own would be
    /// comparing `op.organ_uid` against `batch.from_organ` — two fields inside
    /// the same payload, both written by the same sender, which is no check at
    /// all.
    ///
    /// What replaces the connection is the bundle signature. It names a CELL,
    /// and the signed roster says which Organ owns that Cell, so the tie is
    /// re-established outside the payload before any op is looked at. This is
    /// the OTHER half of "the anchor moves rather than disappearing"; without
    /// it the seal authenticates a sender nothing then consults.
    pub async fn import_mailed_batch(
        &self,
        opened: &crate::seal::OpenedBundle,
    ) -> Result<usize, EngineError> {
        match store::roster::organ_holding_cell(&self.store.pool, &opened.from_cell).await? {
            Some(holder) if holder == opened.batch.from_organ => {}
            Some(_) => {
                return Err(EngineError::Consequence(
                    "mailed batch claims an Organ that does not own the Cell that signed it".into(),
                ));
            }
            // Unlike the connection path, an unknown Cell REFUSES here instead
            // of being admitted. The reasoning that forced admission there —
            // a refusal is quarantined, the ring is bounded, nothing replays
            // it, so refusing would strand an Organ that legitimately has no
            // roster yet — does not apply: mail is only sealed to a roster we
            // already hold, so a signer we cannot place is not an Organ we are
            // starting a relationship with. It is a stranger using a carrier.
            None => {
                return Err(EngineError::Consequence(
                    "mailed batch was signed by a Cell no roster we hold names".into(),
                ));
            }
        }
        // The channel the sender sealed on, not one guessed from the ops. A
        // conversation batch that arrived as general feed would be quarantined
        // op by op ("general feed targeted an individually-replicated
        // record"), which is the correct refusal for a connection that used
        // the wrong verb and a silent black hole for mail. `import_grant_batch`
        // does its own authorization against our accepted grants, so believing
        // the root out of the payload grants nothing.
        match &opened.root {
            Some(root) => self.import_grant_batch(root, &opened.batch).await,
            None => self.import_ops(&opened.batch, None).await,
        }
    }

    /// Whether an op may be believed at all, before anything looks at what it
    /// says. `Some(reason)` refuses it into quarantine; `None` admits it.
    ///
    /// Three checks, and each closes a hole that an unauthenticated identity
    /// field opens. There is no signature on a `WireOp` yet, so what stands in
    /// for one is the connection: an op arrives over an authenticated QUIC
    /// stream from a known Organ, and everything it claims about itself must
    /// agree with who is on the other end.
    ///
    /// 1. **The op's Organ must be the sending Organ.** `op.organ_uid` is
    ///    stamped straight onto `record.organ_uid`, so an unchecked field lets
    ///    any sync contact write records that claim to originate from anyone.
    ///    This holds only while relaying is off, which is the default and the
    ///    reason it is the default: with relay on, `from_organ` is a carrier
    ///    and this check has to become a signature.
    ///
    /// 2. **The op's Cell must belong to that Organ.** `(actor_cell, hlc)` is
    ///    the dedup key. A peer free to invent `actor_cell` can pre-insert
    ///    `(your_cell, some_future_hlc)`; your real op then arrives at every
    ///    contact holding that row and is dropped as an already-seen
    ///    duplicate. No error, no log — the same silent swallow the Organ/Cell
    ///    collision causes, reachable by anyone you sync with. Checked against
    ///    the signed roster, which is the only thing that says which Cells an
    ///    Organ has.
    ///
    ///    When we hold NO roster for that Organ the question is asked the
    ///    other way round — is this Cell somebody ELSE's — because refusing
    ///    outright is not available: a refusal is quarantined, the ring is
    ///    bounded and nothing replays it, so it would permanently drop the
    ///    traffic of an Organ that legitimately has no roster yet.
    ///    **Residual gap**: a sender with no roster claiming a Cell we have
    ///    never heard of is still admitted.
    ///
    /// 3. **The stamp must be close to now.** See `hlc::within_drift`.
    async fn inadmissible(
        &self,
        batch: &OpBatch,
        op: &WireOp,
    ) -> Result<Option<&'static str>, EngineError> {
        if op.organ_uid != batch.from_organ {
            return Ok(Some("op claims an Organ other than the sending one"));
        }
        if !nucleus::hlc::within_drift(op.hlc) {
            return Ok(Some("op is stamped too far in the future"));
        }
        // Person standing (C3) decides who may log in HERE, so only this
        // Organ's own Cells may write it. Refused at the admissibility gate
        // rather than filtered later, because unlike a Karma definition — which
        // a contact may hold inertly, doing nothing — a standing op that
        // materialised would be a contact locking us out of our own Cell.
        // The identity checked is the AUTHENTICATED sender, never
        // `record.organ_uid`, which is a column the sender fills in.
        if op.tbl == "record_extension" && store::people::is_standing_field(&op.field) {
            let ours = store::organs::local(&self.store.pool)
                .await?
                .is_some_and(|organ| organ.uid == batch.from_organ);
            if !ours {
                return Ok(Some(
                    "only this Organ's own Cells may set a Person's standing",
                ));
            }
        }
        if let Some(signed) = self.roster_of(&batch.from_organ).await? {
            if !signed
                .roster
                .cells
                .iter()
                .any(|cell| cell.cell_uid == op.actor_cell)
            {
                return Ok(Some("op claims a Cell that is not in the sender's roster"));
            }
        } else {
            // The floor for when we hold no roster for the sender, where the
            // check above cannot run. It asks the question the other way round:
            // not "is this Cell theirs" — which needs their roster — but "is
            // this Cell somebody ELSE's", which needs only what we already
            // hold. Cheap, no false positives, and it closes the poisoning
            // attack in exactly the cases that can hurt: a Cell we know of is
            // one whose ops we expect to receive.
            if let Some(holder) =
                store::roster::organ_holding_cell(&self.store.pool, &op.actor_cell).await?
            {
                if holder != batch.from_organ {
                    return Ok(Some("op claims a Cell that belongs to another Organ"));
                }
            }
            // And our OWN Cell by name, which the lookup above misses on a
            // first boot that has not published a roster yet. This is the
            // literal form of the attack — a contact pre-occupying our own
            // (cell, hlc) so our next real op is dropped everywhere as a
            // duplicate — so it is worth not depending on our own publishing
            // having happened.
            if let Some(ours) = store::cells::local(&self.store.pool).await? {
                if ours.uid == op.actor_cell {
                    return Ok(Some("op claims this Cell as its author"));
                }
            }
        }
        Ok(None)
    }

    /// Put an incoming op into the log under its ORIGINAL identity, and say
    /// whether it was new. `false` means we already hold `(actor_cell, hlc)`
    /// and the caller should stop — re-applying a duplicate is not wrong, but
    /// counting it as applied would be.
    ///
    /// Also the one place `hlc::observe` is called on the import path, so the
    /// clock advances exactly when an op is genuinely new.
    async fn log_incoming(
        &self,
        op: &WireOp,
        kind: OpKind,
        from: Option<&str>,
        replica_root: Option<&str>,
    ) -> Result<bool, EngineError> {
        let logged = sync_ops::append(
            &self.store.pool,
            &op.tbl,
            &op.uid,
            &op.field,
            kind,
            op.value.as_deref(),
            op.hlc,
            &op.actor_cell,
            &op.organ_uid,
            from,
            replica_root,
        )
        .await?
        .is_some();
        if logged {
            nucleus::hlc::observe(op.hlc);
        }
        Ok(logged)
    }

    async fn note_overwrite(
        &self,
        op: &WireOp,
        from_organ: &str,
        displaced_by: Option<&str>,
    ) -> Result<(), EngineError> {
        let pool = &self.store.pool;
        let displaced = match op.field.as_str() {
            "head" | "body" | "slug" | "kind" => {
                let sql = format!("SELECT {} AS v FROM record WHERE uid = ?", op.field);
                store::sqlx::query(&sql)
                    .bind(&op.uid)
                    .fetch_optional(pool)
                    .await?
                    .and_then(|r| store::sqlx::Row::get::<Option<String>, _>(&r, "v"))
            }
            _ => None,
        };
        let ours = store::organs::local(pool).await?.map(|organ| organ.uid);
        let displaced_local = match (displaced_by, &ours) {
            (Some(author), Some(ours)) => author == ours.as_str(),
            _ => false,
        };
        store::record_changes::note_remote_win(
            pool,
            &op.uid,
            &op.field,
            from_organ,
            displaced.as_deref(),
            displaced_local,
        )
        .await?;
        Ok(())
    }

    async fn import_ops(
        &self,
        batch: &OpBatch,
        replica_root: Option<&str>,
    ) -> Result<usize, EngineError> {
        let mut accept: Option<Vec<String>> = None;
        if let Some(contact) = store::organs::contact(&self.store.pool, &batch.from_organ).await? {
            if contact.trust == "blocked" {
                return Err(EngineError::Consequence(format!(
                    "organ {} is blocked",
                    batch.from_organ
                )));
            }
            accept = contact.accept_fields;
        }
        // Held for the whole batch: the read-compare-append-materialise
        // sequence below must not interleave with another peer's. See
        // `Engine::import_lock` for why this is a lock and not a transaction.
        let _import = self.import_lock.lock().await;
        let pool = &self.store.pool;
        let from = Some(batch.from_organ.as_str());
        let mut applied = 0usize;
        let mut touched: Vec<String> = Vec::new();
        // Records whose Karma definition changed in this batch, materialised
        // after the loop so one Record edited twice is imported once.
        let mut karma_definitions: Vec<String> = Vec::new();
        for op in &batch.ops {
            // What we are willing to TAKE from them, which is a different
            // question from what they were willing to send. Dropped silently
            // rather than quarantined: an out-of-scope op is our own policy
            // working, not the peer misbehaving, and quarantining it would
            // fill the ring on the first sync with any contact wider than our
            // acceptance — burying the reports that mean something.
            //
            // The same predicate the outbound side uses, so a delete still
            // arrives (refusing one would leave us holding a Record they
            // removed) and the Loro document is judged by the two columns it
            // actually carries.
            if !store::sync_ops::op_in_scope(&op.tbl, &op.kind, &op.field, accept.as_deref()) {
                continue;
            }
            if let Some(refusal) = self.inadmissible(batch, op).await? {
                store::organs::quarantine(
                    pool,
                    &batch.from_organ,
                    refusal,
                    &serde_json::to_string(op).unwrap_or_default(),
                )
                .await?;
                continue;
            }
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
                    // Read BEFORE `log_incoming` appends this op, or the
                    // arriving op is itself the latest and every overwrite
                    // looks like it displaced its own author.
                    let displaced_by = match prior {
                        Some(_) => {
                            sync_ops::latest_author_for_field(pool, "record", &op.uid, &op.field)
                                .await?
                        }
                        None => None,
                    };
                    let tomb = sync_ops::latest_hlc_for_field(pool, "record", &op.uid, "").await?;
                    if !self.log_incoming(op, kind, from, replica_root).await? {
                        continue; // duplicate identity
                    }
                    if op.hlc <= prior.unwrap_or(i64::MIN) {
                        continue; // older than the stored value: log only
                    }
                    if let Some(tomb) = tomb {
                        if op.hlc <= tomb {
                            continue; // deleted stays deleted; late sets lose
                        }
                    }
                    if prior.is_some() {
                        self.note_overwrite(op, &batch.from_organ, displaced_by.as_deref())
                            .await?;
                    }
                    let outcome = self
                        .materialise(Materialise {
                            op,
                            kind,
                            replica_root,
                            undelete: tomb.is_some(),
                        })
                        .await?;
                    let outcome = outcome.unwrap_or_default();
                    applied += outcome.applied;
                    touched.extend(outcome.touched);
                }
                ("record", OpKind::Tombstone) => {
                    let prior = sync_ops::latest_hlc_for_field(pool, "record", &op.uid, "").await?;
                    let latest_set =
                        sync_ops::latest_set_hlc_for_row(pool, "record", &op.uid).await?;
                    if !self.log_incoming(op, kind, from, replica_root).await? {
                        continue;
                    }
                    if op.hlc <= prior.unwrap_or(i64::MIN) {
                        continue;
                    }
                    // Alive iff the latest set beats the latest tombstone —
                    // "undelete is a newer write". A concurrent newer edit
                    // keeps the record alive; otherwise the tombstone lands.
                    if op.hlc > latest_set.unwrap_or(i64::MIN) {
                        let outcome = self
                            .materialise(Materialise {
                                op,
                                kind,
                                replica_root,
                                undelete: false,
                            })
                            .await?;
                        let outcome = outcome.unwrap_or_default();
                        applied += outcome.applied;
                        touched.extend(outcome.touched);
                    }
                }
                ("record_extension", OpKind::Set | OpKind::Tombstone)
                | ("record_assertion", OpKind::Set | OpKind::Tombstone)
                | ("concept", OpKind::Set | OpKind::Tombstone) => {
                    // All three are per-uid (or per-uid-and-field) LWW with no
                    // tombstone/undelete interplay, so one guard serves them.
                    let field = if op.tbl == "record_assertion" {
                        ""
                    } else {
                        &op.field
                    };
                    let prior =
                        sync_ops::latest_hlc_for_field(pool, &op.tbl, &op.uid, field).await?;
                    if !self.log_incoming(op, kind, from, replica_root).await? {
                        continue;
                    }
                    if op.hlc <= prior.unwrap_or(i64::MIN) {
                        continue;
                    }
                    let outcome = self
                        .materialise(Materialise {
                            op,
                            kind,
                            replica_root,
                            undelete: false,
                        })
                        .await?;
                    let outcome = outcome.unwrap_or_default();
                    applied += outcome.applied;
                    touched.extend(outcome.touched);
                    // Noted, not materialised here: the definition is
                    // re-derived once after the batch, so a Program revised and
                    // then activated in the same batch is built once, from the
                    // value that won LWW rather than from each op in turn.
                    if op.tbl == "record_extension"
                        && store::karma::sync::is_definition_field(&op.field)
                    {
                        karma_definitions.push(op.uid.clone());
                    }
                }
                // Both carry a base64 Loro blob and Loro imports the two
                // identically, so one arm serves both. A `snapshot` is a peer
                // Cell asserting a whole doc state; a `crdt` op is a tail.
                ("record", OpKind::Crdt | OpKind::Snapshot) => {
                    if op.value.is_none() {
                        store::organs::quarantine(
                            pool,
                            &batch.from_organ,
                            "collab op without a payload",
                            &serde_json::to_string(op).unwrap_or_default(),
                        )
                        .await?;
                        continue;
                    }
                    if !self.log_incoming(op, kind, from, replica_root).await? {
                        continue; // duplicate identity
                    }
                    match store::sync_apply::record_deleted(pool, &op.uid).await? {
                        // Tombstone freeze: a deleted record's doc takes no
                        // more updates — the op stays in the log, so a peer
                        // that has not heard about the delete is still served.
                        Some(true) => continue,
                        Some(false) => {}
                        None => {
                            store::sync_apply::ensure_record_stub(
                                pool,
                                &op.uid,
                                "plain",
                                &op.organ_uid,
                                replica_root,
                                Some(op.hlc),
                            )
                            .await?;
                        }
                    }
                    match self
                        .materialise(Materialise {
                            op,
                            kind,
                            replica_root,
                            undelete: false,
                        })
                        .await
                    {
                        Ok(outcome) => {
                            let outcome = outcome.unwrap_or_default();
                            applied += outcome.applied;
                            touched.extend(outcome.touched);
                        }
                        Err(EngineError::Consequence(reason)) => {
                            store::organs::quarantine(
                                pool,
                                &batch.from_organ,
                                &reason,
                                &serde_json::to_string(op).unwrap_or_default(),
                            )
                            .await?;
                        }
                        Err(other) => return Err(other),
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
        // Karma definitions become RUNNABLE rows only when they came from one
        // of our own Cells (Ontology C7, axis 1).
        //
        // The gate is the batch's authenticated origin, not anything the
        // payload says about itself. `inadmissible` has already refused any op
        // whose `organ_uid` disagrees with the Organ on the other end of the
        // connection, so `from_organ` is the one identity here that was not
        // filled in by the sender — and a Record column claiming to be ours
        // would be exactly what an attacker would write.
        //
        // A contact's Karma still arrives and is still stored: it sits in
        // `record_extension` like any other field, visible and inert. What it
        // never becomes is a row `freeze_next_epoch` can join, because a rule
        // that arrives over a socket and runs on receipt is the failure this
        // gate exists for.
        if !karma_definitions.is_empty() {
            let ours = store::organs::local(pool)
                .await?
                .is_some_and(|organ| organ.uid == batch.from_organ);
            if ours {
                karma_definitions.sort();
                karma_definitions.dedup();
                for record_uid in karma_definitions {
                    // A definition we cannot verify refuses itself and must not
                    // take the rest of the batch down with it: the ops are
                    // already logged and applied, and failing here would make
                    // one bad rule look like a broken sync.
                    if let Err(error) =
                        store::karma::sync::import_definition(pool, &record_uid).await
                    {
                        store::organs::quarantine(
                            pool,
                            &batch.from_organ,
                            "published Karma definition refused",
                            &format!("{record_uid}: {error}"),
                        )
                        .await?;
                    }
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
            &op.actor_cell,
            &op.organ_uid,
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
            &op.organ_uid,
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
        let mut tx = store::write_tx(&self.store.pool).await?;
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

/// What became of one batch handed to the sender.
///
/// Three outcomes rather than `Result`, because leaving mail is neither. A
/// deposited bundle means the queue rows may go — re-sealing the same ops
/// every pass would fill a carrier's quota with duplicates of something
/// already sitting there — but it must NOT move the retention floor: the
/// floor says "this peer holds these ops", and a bundle on a stranger's disk
/// says only "somebody agreed to hold it". Nothing has been applied by
/// anyone. Collapsing the two into `Ok(())` would let pruning drop ops whose
/// only remaining copy expires uncollected in thirty days.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    /// The peer answered and applied it.
    Sent,
    /// Sealed and left with a carrier the recipient published.
    Mailed,
    /// Neither. Everything stays queued.
    Failed(String),
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
        Fut: std::future::Future<Output = Delivery>,
    {
        let pool = &self.store.pool;
        let Some(from_organ) = store::organs::local(pool).await?.map(|o| o.uid) else {
            return Ok(0);
        };
        // Before reading what is queued, not after: a Record that has just
        // entered somebody's selection is enqueued by this call, and a pass
        // that read the queue first would leave it until the next one.
        crate::share::reconcile_all(self).await?;
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
            // WHICH Records travel to this contact, beside the column scope
            // that decides which of their fields do. Read once per contact per
            // pass, like `hidden` above.
            let feed = crate::share::open_feed(self, &contact).await?;
            let mut holdings = crate::share::Holdings::default();
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
                    // Karma definitions go to YOUR OWN CELLS and nowhere else
                    // (Ontology C7, axis 1). They ride the ordinary extension
                    // op path, and `op_in_scope` returns true for an
                    // un-narrowed contact before it ever looks at the field —
                    // so without this arm, publishing a rule would hand every
                    // sync contact the full text of every rule you run. That is
                    // not what "the axes are independent" meant, and it would
                    // be a privacy regression introduced by a sync feature.
                    //
                    // Own Cells are recognised by the contact BEING this Organ:
                    // a second Cell of yours syncs under your own Organ uid,
                    // which is the same identity the import gate checks on the
                    // other side. Deleted rather than left queued, like the
                    // scope arm below and for the same reason — it will never
                    // be owed to this contact.
                    Some(op)
                        if contact_uid != from_organ
                            && op.tbl == "record_extension"
                            && store::karma::sync::is_definition_field(&op.field) =>
                    {
                        sync_ops::outbox_delete(pool, &row).await?;
                    }
                    // Person standing is the same shape and the same rule
                    // (C3): our own Cells must agree on who may log in, and
                    // nobody else is owed our membership admin. It rides the
                    // same un-narrowed-contact hole, so it needs the same arm —
                    // and the leak here would be worse than a rule's text,
                    // because "who did this Organ turn off, and when" is a
                    // statement about a person rather than about a schedule.
                    Some(op)
                        if contact_uid != from_organ
                            && op.tbl == "record_extension"
                            && store::people::is_standing_field(&op.field) =>
                    {
                        sync_ops::outbox_delete(pool, &row).await?;
                    }
                    // The per-contact scope, applied to the PUSH path as well
                    // as the pull one (Ontology §12, C5). It was built into
                    // `FetchOpsSince` alone at first, which narrowed only the
                    // path a peer takes when it asks us — while push, the path
                    // we take to them and the one the sync runner actually
                    // drives, sent every column regardless. A narrowing that
                    // covers one of two delivery paths is not a narrowing.
                    //
                    // Same shape as the `sync_out` arm above and for the same
                    // reason: a replica grant is an explicit per-record
                    // permission the receiver accepted, and half-delivering a
                    // document somebody agreed to take is worse than not
                    // scoping it. The scope governs the broad feed.
                    Some(op)
                        if op.replica_root.is_none()
                            && !store::sync_ops::op_in_scope(
                                &op.tbl,
                                op.kind.as_str(),
                                &op.field,
                                contact.scope_fields.as_deref(),
                            ) =>
                    {
                        // Deleted rather than left queued: it will never be
                        // sent to this contact under this scope, and leaving
                        // it would retry it forever and hold the retention
                        // floor down behind an op nobody is owed.
                        sync_ops::outbox_delete(pool, &row).await?;
                    }
                    Some(op) => {
                        // Whole rows kept out of a contact's feed — the other
                        // half of §12's "hiding is per-record AND per-field".
                        // Async, so it cannot be a match guard like the two
                        // above; the guard-shaped ones stay guards.
                        // Hiding is decided INSIDE the feed rather than ahead
                        // of it. Deciding it here first would drop the
                        // tombstone of a Record they already hold — hiding is
                        // meant to stop what travels next, not to strand a
                        // copy on their disk that nothing can ever clean up.
                        //
                        // The selection governs the broad feed only, for the
                        // same reason the scope does: an accepted grant is a
                        // per-Record permission the receiver already took.
                        if op.replica_root.is_none() {
                            match crate::share::feed_carries(self, &feed, &op).await? {
                                // Not in their selection, and not something
                                // they hold. Deleted rather than left queued —
                                // it will never be owed to them under this
                                // selection, and a queued row nobody is owed
                                // holds the retention floor down behind it.
                                None => {
                                    sync_ops::outbox_delete(pool, &row).await?;
                                    continue;
                                }
                                Some(records) => holdings.note(&op, records),
                            }
                        }
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
            // The worst outcome across this contact's roots wins, and a real
            // failure stops the loop. Mailing does NOT stop it: each root is
            // its own channel and its own batch, so a contact that has to be
            // mailed is mailed once per root — the recipient needs all of
            // them, and stopping after the first would silently hold back
            // every conversation but one.
            let mut outcome = Delivery::Sent;
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
                match send(contact.clone(), root, batch).await {
                    Delivery::Sent => {}
                    Delivery::Mailed => {
                        if outcome == Delivery::Sent {
                            outcome = Delivery::Mailed;
                        }
                    }
                    failed @ Delivery::Failed(_) => {
                        outcome = failed;
                        break;
                    }
                }
            }
            match outcome {
                Delivery::Sent => {
                    // The peer accepted every batch, so everything we intended
                    // to send them at or below this seq is now on their side.
                    // That is the retention floor: pruning removes ops nobody
                    // is still owed, and an op this contact was never going to
                    // be sent (not visible to them, or superseded in the
                    // bounded outbox) is not owed either.
                    if let Some(high) = kept.iter().map(|row| row.seq).max() {
                        store::organs::advance_peer_acked_seq(pool, &contact_uid, high).await?;
                    }
                    // What they now hold, so a Record that later LEAVES the
                    // selection can still be deleted where it landed. Without
                    // this, dropping something from a selection would strand
                    // the copy on their disk with no way to reach it.
                    crate::share::note_delivery(self, &contact_uid, &holdings).await?;
                    for row in &kept {
                        sync_ops::outbox_delete(pool, row).await?;
                    }
                    sent += 1;
                }
                // Left with a carrier. The rows go, so the next pass does not
                // seal the same ops again; the retention floor stays exactly
                // where it was, so nothing gets pruned on the strength of a
                // bundle nobody has opened. If it expires uncollected, the
                // peer's own catch-up pull still finds these ops in our log —
                // that pull covers the general feed AND every root they hold
                // a grant on, which is what makes dropping the rows safe.
                Delivery::Mailed => {
                    for row in &kept {
                        sync_ops::outbox_delete(pool, row).await?;
                    }
                    store::organs::mark_mailed(pool, &contact_uid).await?;
                    sent += 1;
                }
                Delivery::Failed(_) => {
                    sync_ops::outbox_bump_attempts(pool, &contact_uid).await?;
                }
            }
        }
        // A handover only completes once the peer has actually acknowledged
        // the Record, which is a fact this pass has just established. Settling
        // before the send would let go of the only copy on a promise.
        crate::share::settle_moves(self).await?;
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
    pub async fn prune_op_log(&self, dry_run: bool) -> Result<sync_ops::PruneReport, EngineError> {
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

/// One op's effect on the READ MODEL, with the last-write-wins decision
/// already taken by the caller.
///
/// Split out of `import_ops` so that importing an op and rebuilding from the
/// log cannot disagree about what an op MEANS. Before this they would have
/// been two copies of the same per-table logic, and the copy that drifts is
/// the one nobody runs — the rebuild, which is used precisely when the read
/// model is already suspect.
///
/// What is NOT here, deliberately: appending to the log, advancing the clock,
/// deciding whether this op beats what is stored, and quarantine. Those are
/// import's concerns. A rebuild replays in HLC order, so "later wins" falls
/// out of the ordering and it needs none of them.
pub struct Materialise<'a> {
    pub op: &'a WireOp,
    pub kind: OpKind,
    pub replica_root: Option<&'a str>,
    /// A `set` that beat a tombstone revives the record.
    pub undelete: bool,
}

/// What one materialisation did, so the caller can count and refresh.
#[derive(Default)]
pub struct Materialised {
    pub applied: usize,
    pub touched: Vec<String>,
}

impl Engine {
    /// `None` means this `(table, kind)` pair is not one the read model knows
    /// how to apply. Import never sees it — its outer dispatch quarantines
    /// first — but the REBUILD calls this with whatever the log happens to
    /// hold, and the `kind` CHECK constrains the kind alone, not the pair. A
    /// `concept` row carrying `kind = 'crdt'` is storable, and a catch-all
    /// would have run `delete_concept` on it.
    pub async fn materialise(
        &self,
        m: Materialise<'_>,
    ) -> Result<Option<Materialised>, EngineError> {
        let pool = &self.store.pool;
        let Materialise {
            op,
            kind,
            replica_root,
            undelete,
        } = m;
        let mut out = Materialised::default();
        match (op.tbl.as_str(), kind) {
            ("record", OpKind::Set) => {
                // Once collab history exists, the record-doc owns the text: a
                // late create-era head/body set is log-only. Its undelete
                // power still applies — it won its HLC race.
                if (op.field == "head" || op.field == "body")
                    && store::record_docs::has_crdt_history(pool, &op.uid).await?
                {
                    if undelete {
                        store::sync_apply::undelete_record(pool, &op.uid).await?;
                        out.applied += 1;
                        out.touched.push(op.uid.clone());
                    }
                    return Ok(Some(out));
                }
                let value = json_value(op.value.as_deref());
                store::sync_apply::ensure_record_stub(
                    pool,
                    &op.uid,
                    if op.field == "kind" {
                        value.as_str().unwrap_or("plain")
                    } else {
                        "plain"
                    },
                    &op.organ_uid,
                    replica_root,
                    Some(op.hlc),
                )
                .await?;
                store::sync_apply::set_record_field(pool, &op.uid, &op.field, &value, undelete)
                    .await?;
                out.applied += 1;
                out.touched.push(op.uid.clone());
            }
            ("record", OpKind::Tombstone) => {
                store::sync_apply::tombstone_record(pool, &op.uid).await?;
                out.applied += 1;
                out.touched.push(op.uid.clone());
            }
            ("record_extension", OpKind::Set | OpKind::Tombstone) => {
                store::sync_apply::ensure_record_stub(
                    pool,
                    &op.uid,
                    "plain",
                    &op.organ_uid,
                    replica_root,
                    Some(op.hlc),
                )
                .await?;
                // Field is "{namespace}.{key}" — keys have no dots,
                // namespaces may. No dot at all = whole-value namespace.
                match op.field.rsplit_once('.') {
                    Some((namespace, key)) => match kind {
                        OpKind::Set => {
                            store::sync_apply::set_extension_key(
                                pool,
                                &op.uid,
                                namespace,
                                key,
                                json_value(op.value.as_deref()),
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
                        store::sync_apply::set_extension_whole(
                            pool,
                            &op.uid,
                            &op.field,
                            &json_value(op.value.as_deref()),
                        )
                        .await?;
                    }
                }
                out.applied += 1;
                out.touched.push(op.uid.clone());
            }
            ("record_assertion", OpKind::Set) => {
                let Some(value) = op
                    .value
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                else {
                    return Ok(Some(out));
                };
                if let Some(subject) = value.get("subject_uid").and_then(|v| v.as_str()) {
                    store::sync_apply::ensure_record_stub(
                        pool,
                        subject,
                        "plain",
                        &op.organ_uid,
                        replica_root,
                        Some(op.hlc),
                    )
                    .await?;
                    out.touched.push(subject.to_string());
                }
                store::sync_apply::upsert_assertion(pool, &op.uid, &value).await?;
                out.applied += 1;
            }
            ("record_assertion", OpKind::Tombstone) => {
                store::sync_apply::retract_assertion(pool, &op.uid).await?;
                out.applied += 1;
            }
            ("concept", OpKind::Set) => {
                let name = op
                    .value
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default();
                if !name.is_empty() {
                    store::sync_apply::upsert_concept(pool, &op.uid, &name, &op.organ_uid).await?;
                }
                out.applied += 1;
            }
            ("concept", OpKind::Tombstone) => {
                store::sync_apply::delete_concept(pool, &op.uid).await?;
                out.applied += 1;
            }
            ("record", OpKind::Crdt | OpKind::Snapshot) => {
                let value = op.value.as_deref().unwrap_or_default();
                // `Consequence` rather than a silent skip: import turns it
                // into quarantine, and a rebuild wants to know its own log
                // holds a blob Loro will not accept.
                match self
                    .apply_remote_crdt(&op.uid, value, matches!(kind, OpKind::Snapshot))
                    .await?
                {
                    Ok(()) => {
                        out.applied += 1;
                        out.touched.push(op.uid.clone());
                    }
                    Err(reason) => return Err(EngineError::Consequence(reason)),
                }
            }
            _ => return Ok(None),
        }
        Ok(Some(out))
    }
}

fn json_value(raw: Option<&str>) -> serde_json::Value {
    raw.and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or(serde_json::Value::Null)
}
