use chrono::Utc;
use nucleus::{Cause, CauseKind, Fact, NewFact};
use serde::{Deserialize, Serialize};
use store::sync_ops::{self, OpKind, OpRow};

use crate::Engine;
use crate::error::EngineError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireOp {
    pub tbl: String,
    pub uid: String,
    pub field: String,
    pub kind: String,
    pub value: Option<String>,
    pub hlc: i64,
    pub actor_cell: String,
    pub organ_uid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact: Option<Fact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpBatch {
    pub from_organ: String,
    pub ops: Vec<WireOp>,
}

impl Engine {
    pub async fn hydrate_ops(&self, rows: Vec<OpRow>) -> Result<Vec<WireOp>, EngineError> {
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let fact = if row.kind == "fact" {
                store::facts::get(&self.store.pool, &row.uid).await?
            } else {
                None
            };
            let mut value = row.value;
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

    pub async fn ops_after(
        &self,
        after: i64,
        limit: i64,
    ) -> Result<(Vec<WireOp>, i64), EngineError> {
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

    pub async fn import_op_batch(&self, batch: &OpBatch) -> Result<usize, EngineError> {
        self.import_ops(batch, None).await
    }

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
            None => {
                return Err(EngineError::Consequence(
                    "mailed batch was signed by a Cell no roster we hold names".into(),
                ));
            }
        }
        match &opened.root {
            Some(root) => self.import_grant_batch(root, &opened.batch).await,
            None => self.import_ops(&opened.batch, None).await,
        }
    }

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
            if let Some(holder) =
                store::roster::organ_holding_cell(&self.store.pool, &op.actor_cell).await?
            {
                if holder != batch.from_organ {
                    return Ok(Some("op claims a Cell that belongs to another Organ"));
                }
            }
            if let Some(ours) = store::cells::local(&self.store.pool).await? {
                if ours.uid == op.actor_cell {
                    return Ok(Some("op claims this Cell as its author"));
                }
            }
        }
        Ok(None)
    }

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
        if let Some(reason) = store::contact_rate::backing_off(
            &self.store.pool,
            &batch.from_organ,
            store::contact_rate::RateKind::Refusal,
        )
        .await?
        {
            return Err(EngineError::Consequence(reason));
        }
        let ours = store::organs::local(&self.store.pool)
            .await?
            .is_some_and(|organ| organ.uid == batch.from_organ);
        if !ours && self.roster_of(&batch.from_organ).await?.is_none() {
            store::organs::mark_awaiting_roster(&self.store.pool, &batch.from_organ).await?;
            if store::organs::awaiting_roster_longer_than(
                &self.store.pool,
                &batch.from_organ,
                chrono::Duration::days(crate::ROSTER_GRACE_DAYS),
            )
            .await?
            {
                return Err(EngineError::Consequence(format!(
                    "no device list for organ {} — reconnect to it to finish pairing",
                    batch.from_organ
                )));
            }
        } else {
            store::organs::clear_awaiting_roster(&self.store.pool, &batch.from_organ).await?;
        }
        let _import = self.import_lock.lock().await;
        let pool = &self.store.pool;
        let from = Some(batch.from_organ.as_str());
        let mut applied = 0usize;
        let mut touched: Vec<String> = Vec::new();
        let mut karma_definitions: Vec<String> = Vec::new();
        for op in &batch.ops {
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
                    let prior =
                        sync_ops::latest_hlc_for_field(pool, "record", &op.uid, &op.field).await?;
                    let displaced_by = match prior {
                        Some(_) => {
                            sync_ops::latest_author_for_field(pool, "record", &op.uid, &op.field)
                                .await?
                        }
                        None => None,
                    };
                    let tomb = sync_ops::latest_hlc_for_field(pool, "record", &op.uid, "").await?;
                    if !self.log_incoming(op, kind, from, replica_root).await? {
                        continue;
                    }
                    if op.hlc <= prior.unwrap_or(i64::MIN) {
                        continue;
                    }
                    if let Some(tomb) = tomb {
                        if op.hlc <= tomb {
                            continue;
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
                    if op.tbl == "record_extension"
                        && store::karma::sync::is_definition_field(&op.field)
                    {
                        karma_definitions.push(op.uid.clone());
                    }
                }
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
                        continue;
                    }
                    match store::sync_apply::record_deleted(pool, &op.uid).await? {
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
        if !karma_definitions.is_empty() {
            let ours = store::organs::local(pool)
                .await?
                .is_some_and(|organ| organ.uid == batch.from_organ);
            if ours {
                karma_definitions.sort();
                karma_definitions.dedup();
                for record_uid in karma_definitions {
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
            uid: Some(fact.uid.clone()),
            record_uid: fact.record_uid.clone(),
            delta: fact.delta,
            at: Some(fact.at),
            actor_uid: fact.actor_uid.clone(),
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
        sealed.signature = fact.signature.clone();
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
    pub keys: Vec<(String, String)>,
}

impl Engine {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    Sent,
    Mailed,
    Failed(String),
}

impl Engine {
    pub async fn drain_outbox<F, Fut>(&self, mut send: F) -> Result<usize, EngineError>
    where
        F: FnMut(store::organs::Contact, Option<String>, OpBatch) -> Fut,
        Fut: std::future::Future<Output = Delivery>,
    {
        let pool = &self.store.pool;
        let Some(from_organ) = store::organs::local(pool).await?.map(|o| o.uid) else {
            return Ok(0);
        };
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
            let feed = crate::share::open_feed(self, &contact).await?;
            let mut holdings = crate::share::Holdings::default();
            let mut log_rows = Vec::new();
            let mut kept = Vec::new();
            for row in rows {
                match sync_ops::get_by_seq(pool, row.seq).await? {
                    Some(op) if !contact.sync_out && op.replica_root.is_none() => {
                        sync_ops::outbox_delete(pool, &row).await?;
                    }
                    Some(op)
                        if contact_uid != from_organ
                            && op.tbl == "record_extension"
                            && store::karma::sync::is_definition_field(&op.field) =>
                    {
                        sync_ops::outbox_delete(pool, &row).await?;
                    }
                    Some(op)
                        if contact_uid != from_organ
                            && op.tbl == "record_extension"
                            && store::people::is_standing_field(&op.field) =>
                    {
                        sync_ops::outbox_delete(pool, &row).await?;
                    }
                    Some(op)
                        if op.replica_root.is_none()
                            && !store::sync_ops::op_in_scope(
                                &op.tbl,
                                op.kind.as_str(),
                                &op.field,
                                contact.scope_fields.as_deref(),
                            ) =>
                    {
                        sync_ops::outbox_delete(pool, &row).await?;
                    }
                    Some(op) => {
                        if op.replica_root.is_none() {
                            match crate::share::feed_carries(self, &feed, &op).await? {
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
                    None => sync_ops::outbox_delete(pool, &row).await?,
                }
            }
            if kept.is_empty() {
                continue;
            }
            log_rows.sort_by_key(|op| op.seq);
            let mut roots: Vec<Option<String>> = Vec::new();
            for op in &log_rows {
                if !roots.contains(&op.replica_root) {
                    roots.push(op.replica_root.clone());
                }
            }
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
                    if let Some(high) = kept.iter().map(|row| row.seq).max() {
                        store::organs::advance_peer_acked_seq(pool, &contact_uid, high).await?;
                    }
                    crate::share::note_delivery(self, &contact_uid, &holdings).await?;
                    for row in &kept {
                        sync_ops::outbox_delete(pool, row).await?;
                    }
                    sent += 1;
                }
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
        crate::share::settle_moves(self).await?;
        Ok(sent)
    }

    pub async fn prune_op_log(&self, dry_run: bool) -> Result<sync_ops::PruneReport, EngineError> {
        Ok(sync_ops::prune(&self.store.pool, dry_run).await?)
    }

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
                confidence: 0.5,
            });
        }
        Ok(out)
    }

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

pub struct Materialise<'a> {
    pub op: &'a WireOp,
    pub kind: OpKind,
    pub replica_root: Option<&'a str>,
    pub undelete: bool,
}

#[derive(Default)]
pub struct Materialised {
    pub applied: usize,
    pub touched: Vec<String>,
}

impl Engine {
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
        let stamp = store::sync_apply::Stamp {
            tbl: &op.tbl,
            uid: &op.uid,
            field: match (op.tbl.as_str(), kind) {
                ("record", OpKind::Tombstone) => "",
                ("record_assertion", _) => "",
                _ => &op.field,
            },
            hlc: op.hlc,
        };
        let mut out = Materialised::default();
        match (op.tbl.as_str(), kind) {
            ("record", OpKind::Set) => {
                if (op.field == "head" || op.field == "body")
                    && store::record_docs::has_crdt_history(pool, &op.uid).await?
                {
                    if undelete {
                        store::sync_apply::undelete_record(pool, &op.uid, stamp).await?;
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
                store::sync_apply::set_record_field(
                    pool, &op.uid, &op.field, &value, undelete, stamp,
                )
                .await?;
                out.applied += 1;
                out.touched.push(op.uid.clone());
            }
            ("record", OpKind::Tombstone) => {
                store::sync_apply::tombstone_record(pool, &op.uid, stamp).await?;
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
                match op.field.rsplit_once('.') {
                    Some((namespace, key)) => match kind {
                        OpKind::Set => {
                            store::sync_apply::set_extension_key(
                                pool,
                                &op.uid,
                                namespace,
                                key,
                                json_value(op.value.as_deref()),
                                stamp,
                            )
                            .await?;
                        }
                        _ => {
                            store::sync_apply::tombstone_extension_key(
                                pool, &op.uid, namespace, key, stamp,
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
                            stamp,
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
                store::sync_apply::upsert_assertion(pool, &op.uid, &value, stamp).await?;
                out.applied += 1;
            }
            ("record_assertion", OpKind::Tombstone) => {
                store::sync_apply::retract_assertion(pool, &op.uid, stamp).await?;
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
                    store::sync_apply::upsert_concept(pool, &op.uid, &name, &op.organ_uid, stamp)
                        .await?;
                }
                out.applied += 1;
            }
            ("concept", OpKind::Tombstone) => {
                store::sync_apply::delete_concept(pool, &op.uid, stamp).await?;
                out.applied += 1;
            }
            ("record", OpKind::Crdt | OpKind::Snapshot) => {
                let value = op.value.as_deref().unwrap_or_default();
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
