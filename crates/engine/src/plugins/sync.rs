use super::*;
use nucleus::{
    Cause, CauseKind, NewFact, RecordKind,
    plugins::{ImportedRecord, PreparedPublication, PublishRecord, PullResult},
};
use protein::Predicate;
use std::time::Duration;

pub(super) fn incoming_filters(value: &Value) -> Result<Vec<Predicate>, EngineError> {
    fn check(predicate: &Predicate, depth: usize, remaining: &mut usize) -> bool {
        if depth > 12 || *remaining == 0 {
            return false;
        }
        *remaining -= 1;
        match predicate {
            Predicate::All(items) | Predicate::Any(items) => {
                items.iter().all(|item| check(item, depth + 1, remaining))
            }
            Predicate::Not(item) => check(item, depth + 1, remaining),
            Predicate::TextContains(_)
            | Predicate::KindEq(_)
            | Predicate::QuantityEq(_)
            | Predicate::QuantityLt(_)
            | Predicate::QuantityLte(_)
            | Predicate::QuantityGt(_)
            | Predicate::QuantityGte(_) => true,
            _ => false,
        }
    }
    let filters: Vec<Predicate> = serde_json::from_value(value.clone())?;
    let mut remaining = 128;
    if !filters
        .iter()
        .all(|predicate| check(predicate, 0, &mut remaining))
    {
        return Err(invalid(
            "Incoming Protein selection supports text, kind, quantity, and Boolean groups",
        ));
    }
    Ok(filters)
}

fn matches(item: &ImportedRecord, filters: &[Predicate]) -> bool {
    fn test(item: &ImportedRecord, predicate: &Predicate) -> bool {
        let zero = store::exact::zero();
        match predicate {
            Predicate::All(items) => items.iter().all(|predicate| test(item, predicate)),
            Predicate::Any(items) => items.iter().any(|predicate| test(item, predicate)),
            Predicate::Not(predicate) => !test(item, predicate),
            Predicate::TextContains(text) => format!("{}\n{}", item.head, item.body)
                .to_lowercase()
                .contains(&text.to_lowercase()),
            Predicate::KindEq(kind) => kind == "plain",
            Predicate::QuantityEq(value) => zero == *value,
            Predicate::QuantityLt(value) => zero < *value,
            Predicate::QuantityLte(value) => zero <= *value,
            Predicate::QuantityGt(value) => zero > *value,
            Predicate::QuantityGte(value) => zero >= *value,
            _ => false,
        }
    }
    filters.iter().all(|predicate| test(item, predicate))
}

impl Engine {
    pub(crate) async fn plugin_sync_pending(
        &self,
    ) -> Result<(Vec<nucleus::sync::Pending>, u64, u64), EngineError> {
        let outgoing: i64 = store::sqlx::query_scalar(
            "SELECT COUNT(*) FROM plugin_outbox WHERE state IN ('queued','running')",
        )
        .fetch_one(&self.store.pool)
        .await?;
        let held: i64 = store::sqlx::query_scalar(
            "SELECT COUNT(*) FROM plugin_outbox WHERE state IN ('failed','uncertain')",
        )
        .fetch_one(&self.store.pool)
        .await?;
        let rows = store::sqlx::query("SELECT o.uid,o.record_uid,o.binding_uid,o.state,o.attempts,o.result,b.provider FROM plugin_outbox o JOIN plugin_binding b ON b.uid=o.binding_uid WHERE o.state<>'delivered' ORDER BY o.created_at,o.uid LIMIT 100").fetch_all(&self.store.pool).await?;
        let pending = rows
            .into_iter()
            .map(|row| nucleus::sync::Pending {
                id: row.get("uid"),
                instance: nucleus::sync::Instance::plugin(
                    &row.get::<String, _>("provider"),
                    &row.get::<String, _>("binding_uid"),
                ),
                direction: nucleus::sync::Direction::Outgoing,
                outcome: if matches!(
                    row.get::<String, _>("state").as_str(),
                    "failed" | "uncertain"
                ) {
                    nucleus::sync::Outcome::Failed
                } else {
                    nucleus::sync::Outcome::Pending
                },
                subject: Some(row.get("record_uid")),
                field: None,
                attempts: row.get::<i64, _>("attempts") as u64,
                message: Some(row.get("result")),
            })
            .collect();
        Ok((pending, outgoing as u64, held as u64))
    }

    pub fn start_plugin_sync(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                let now = nucleus::execution::now().timestamp();
                let due: Result<Vec<String>, _> = store::sqlx::query_scalar("SELECT uid FROM plugin_binding WHERE enabled=1 AND last_run+interval_seconds<=? ORDER BY last_run,uid LIMIT 4").bind(now).fetch_all(&self.store.pool).await;
                let Ok(due) = due else {
                    continue;
                };
                for uid in due {
                    if let Err(error) = self.run_plugin_binding(&uid).await {
                        tracing::debug!(%uid, %error, "Plugin Sync refresh did not complete");
                    }
                }
            }
        })
    }

    pub(super) async fn active_binding(&self, binding: &Binding) -> Result<(), EngineError> {
        let revision: Option<i64> = store::sqlx::query_scalar(
            "SELECT revision FROM plugin_binding WHERE uid=? AND enabled=1",
        )
        .bind(&binding.uid)
        .fetch_optional(&self.store.pool)
        .await?;
        if revision != Some(binding.revision as i64) {
            return Err(invalid("Sync binding was stopped or revised"));
        }
        let plugin = binding
            .provider
            .split_once('/')
            .ok_or_else(|| invalid("Invalid provider"))?
            .0;
        let approved = self.approved_plugin(plugin).await?;
        if !self
            .plugin_host()?
            .packages()
            .iter()
            .any(|package| package.manifest.id == plugin && package.digest == approved)
        {
            return Err(invalid("Plugin version changed"));
        }
        Ok(())
    }

    pub async fn run_plugin_binding(&self, uid: &str) -> Result<usize, EngineError> {
        if nucleus::execution::current().is_some() {
            return Err(invalid("Plugin Sync is unavailable during Simulation"));
        }
        let _guard = self
            .plugin_work
            .try_lock()
            .map_err(|_| invalid("Plugin Sync is already running"))?;
        let row = store::sqlx::query("SELECT * FROM plugin_binding WHERE uid=?")
            .bind(uid)
            .fetch_optional(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("Sync binding is unavailable"))?;
        let binding = binding_row(&row)?;
        let cursor: Value = serde_json::from_str(&row.get::<String, _>("cursor"))?;
        let previous_export = cursor["successful_at_ms"]
            .as_i64()
            .and_then(chrono::DateTime::from_timestamp_millis);
        let direction = match binding.direction {
            Direction::Incoming => nucleus::sync::Direction::Incoming,
            Direction::Outgoing => nucleus::sync::Direction::Outgoing,
            Direction::Both => nucleus::sync::Direction::Both,
        };
        let activity = nucleus::sync::Activity {
            instance: nucleus::sync::Instance::plugin(&binding.provider, uid),
            direction,
            update: nucleus::sync::Update::Reconcile,
        };
        let result = self
            .sync_service
            .run(
                &self.store,
                activity,
                self.sync_plugin_pass(&binding, cursor),
                |count| nucleus::sync::Summary::new(nucleus::sync::Outcome::Applied, *count),
            )
            .await;
        let status = match &result {
            Ok(count) if self.document_format(&binding)?.is_some() => format!(
                "Exported {count} calendar or document entries; last successful refresh {}",
                nucleus::execution::now().to_rfc3339()
            ),
            Ok(count) => {
                format!("Refreshed {count} Records; history is limited to the configured scope")
            }
            Err(error) => match previous_export {
                Some(at) => format!("{}; last successful export {}", error, at.to_rfc3339())
                    .chars()
                    .take(1024)
                    .collect(),
                None => error.to_string().chars().take(1024).collect(),
            },
        };
        let last_run = if result.as_ref().is_err_and(document::updating) {
            0
        } else {
            nucleus::execution::now().timestamp()
        };
        store::sqlx::query("UPDATE plugin_binding SET status=?,last_run=? WHERE uid=? AND revision=? AND enabled=1").bind(status).bind(last_run).bind(uid).bind(binding.revision as i64).execute(&self.store.pool).await?;
        self.notify_query_changed();
        result
    }

    async fn sync_plugin_pass(
        &self,
        binding: &Binding,
        cursor: Value,
    ) -> Result<usize, EngineError> {
        self.active_binding(binding).await?;
        if self.document_format(binding)?.is_some() {
            return self.export_plugin_document(binding, cursor).await;
        }
        let mut count = 0;
        if binding.direction.incoming() {
            let pulled = self
                .plugin_call(
                    &binding.provider,
                    "pull",
                    json!({"settings":binding.settings,"cursor":cursor}),
                )
                .await?;
            let pulled: PullResult = serde_json::from_value(pulled)?;
            self.active_binding(binding).await?;
            count += self.retain_plugin_records(binding, pulled).await?;
        }
        if binding.direction.outgoing() {
            self.active_binding(binding).await?;
            self.enqueue_plugin_records(binding).await?;
            count += self.deliver_plugin_records(binding).await?;
        }
        Ok(count)
    }

    async fn retain_plugin_records(
        &self,
        binding: &Binding,
        pulled: PullResult,
    ) -> Result<usize, EngineError> {
        if pulled.records.len() > nucleus::plugins::MAX_ITEMS
            || serde_json::to_vec(&pulled.cursor)?.len() > 16384
        {
            return Err(invalid("Plugin exceeded its import or checkpoint budget"));
        }
        let filters = incoming_filters(&binding.incoming_filter)?;
        let (manifest, provider) = self
            .plugin_host()?
            .contribution(&binding.provider)
            .map_err(invalid)?;
        let Contribution::Sync { schema, .. } = provider else {
            return Err(invalid("Unavailable Sync provider"));
        };
        let schema = manifest
            .contributions
            .iter()
            .find(|contribution| contribution.id() == schema)
            .ok_or_else(|| invalid("Missing plugin schema"))?;
        let Contribution::Schema {
            namespace, fields, ..
        } = schema
        else {
            return Err(invalid("Invalid plugin schema"));
        };
        for item in &pulled.records {
            validate_import(item, fields)?;
        }
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let signer = self.signer.lock().await.clone();
        let now = nucleus::execution::now();
        let mut tx = store::write_tx(&self.store.pool).await?;
        let revision: Option<i64> = store::sqlx::query_scalar(
            "SELECT revision FROM plugin_binding WHERE uid=? AND enabled=1",
        )
        .bind(&binding.uid)
        .fetch_optional(&mut *tx)
        .await?;
        let approved: bool = store::sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM plugin_package WHERE id=? AND enabled=1)",
        )
        .bind(&manifest.id)
        .fetch_one(&mut *tx)
        .await?;
        if revision != Some(binding.revision as i64) || !approved {
            return Err(invalid("Plugin or binding was disabled during import"));
        }
        let mut facts = Vec::new();
        let mut count = 0;
        for item in &pulled.records {
            if !matches(item, &filters) {
                continue;
            }
            let content_digest =
                digest(&json!({"head":item.head,"body":item.body,"metadata":item.metadata}))?;
            let previous: Option<String> = store::sqlx::query_scalar(
                "SELECT content_digest FROM plugin_object WHERE binding_uid=? AND external_id=?",
            )
            .bind(&binding.uid)
            .bind(&item.external_id)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some(previous) = previous {
                if previous != content_digest {
                    return Err(invalid(
                        "External object changed; retained content needs conflict review",
                    ));
                }
                continue;
            }
            let uid = nucleus::new_uid("r");
            let row = store::records::create_with_uid_on(
                &mut tx,
                &uid,
                store::records::NewRecord {
                    slug: None,
                    kind: RecordKind::Plain,
                    head: &item.head,
                    body: &item.body,
                    quantity: store::exact::zero(),
                },
                &organ.uid,
                None,
            )
            .await?;
            store::records::set_extension_on(&mut tx, &row.uid, namespace, &item.metadata).await?;
            store::records::set_extension_on(&mut tx, &row.uid, "lince.source", &json!({"plugin":manifest.id,"provider":binding.provider,"binding":binding.uid,"external_id":item.external_id,"received_at":now.to_rfc3339(),"schema":namespace,"digest":content_digest})).await?;
            store::sqlx::query("INSERT INTO plugin_object(binding_uid,external_id,record_uid,content_digest) VALUES(?,?,?,?)").bind(&binding.uid).bind(&item.external_id).bind(&row.uid).bind(&content_digest).execute(&mut *tx).await?;
            if let Some(fact) = crate::append::append_one_in_transaction(
                &mut tx,
                NewFact {
                    uid: None,
                    record_uid: row.uid,
                    delta: nucleus::fact::zero_delta(),
                    at: None,
                    actor_uid: None,
                    cause: Cause {
                        kind: CauseKind::Sync,
                        uid: Some(binding.uid.clone()),
                    },
                    payload: Some(
                        json!({"plugin":manifest.id,"observed_external_id":item.external_id})
                            .to_string(),
                    ),
                },
                now,
                signer.as_ref(),
            )
            .await?
            {
                facts.push(fact);
            }
            count += 1;
        }
        store::sqlx::query("UPDATE plugin_binding SET cursor=? WHERE uid=?")
            .bind(pulled.cursor.to_string())
            .bind(&binding.uid)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        for fact in facts {
            let _ = self.bus.send(fact);
        }
        Ok(count)
    }

    async fn enqueue_plugin_records(&self, binding: &Binding) -> Result<(), EngineError> {
        let rows = self.plugin_selection(binding).await?;
        let now = nucleus::execution::now().timestamp();
        let mut tx = store::write_tx(&self.store.pool).await?;
        let pending: i64 = store::sqlx::query_scalar(
            "SELECT COUNT(*) FROM plugin_outbox WHERE state<>'delivered'",
        )
        .fetch_one(&mut *tx)
        .await?;
        if pending >= 1024 {
            return Err(invalid(
                "Plugin delivery queue is full; resolve retained deliveries",
            ));
        }
        let total: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM plugin_outbox")
            .fetch_one(&mut *tx)
            .await?;
        if total >= 10000 {
            return Err(invalid("Plugin delivery receipt budget is full"));
        }
        for row in rows
            .into_iter()
            .take((1024 - pending).min(10000 - total) as usize)
        {
            if row.body.len() > 16384 {
                return Err(invalid(
                    "Selected Record exceeds the publication content budget",
                ));
            }
            let content_digest = digest(&json!({"head":row.head,"body":row.body}))?;
            let input = PublishRecord {
                uid: row.uid.clone(),
                head: row.head,
                body: row.body,
                created_at: now.max(0) as u64,
            };
            store::sqlx::query("INSERT INTO plugin_outbox(uid,binding_uid,record_uid,content_digest,input,state,created_at) VALUES(?,?,?,?,?,'queued',?) ON CONFLICT(binding_uid,record_uid,content_digest) DO NOTHING").bind(nucleus::new_uid("r")).bind(&binding.uid).bind(&row.uid).bind(&content_digest).bind(serde_json::to_string(&input)?).bind(now).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn deliver_plugin_records(&self, binding: &Binding) -> Result<usize, EngineError> {
        let queued = store::sqlx::query("SELECT * FROM plugin_outbox WHERE binding_uid=? AND state='queued' ORDER BY created_at,uid LIMIT 2").bind(&binding.uid).fetch_all(&self.store.pool).await?;
        let mut count = 0;
        for row in queued {
            self.active_binding(binding).await?;
            let uid: String = row.get("uid");
            let record: String = row.get("record_uid");
            let expected: String = row.get("content_digest");
            let selected = self.plugin_selection(binding).await?;
            let current = selected.iter().find(|record_row| record_row.uid == record);
            if !current.is_some_and(|record_row| {
                digest(&json!({"head":record_row.head,"body":record_row.body}))
                    .is_ok_and(|digest| digest == expected)
            }) {
                store::sqlx::query("UPDATE plugin_outbox SET state='failed',result='Record changed or left the selected Protein; this snapshot was not published' WHERE uid=?").bind(&uid).execute(&self.store.pool).await?;
                continue;
            }
            let prepared: Option<String> = row.get("prepared");
            let prepared = match prepared {
                Some(prepared) => serde_json::from_str(&prepared)?,
                None => {
                    let input: Value = serde_json::from_str(&row.get::<String, _>("input"))?;
                    let prepared = self
                        .plugin_call(
                            &binding.provider,
                            "prepare",
                            json!({"settings":binding.settings,"record":input}),
                        )
                        .await?;
                    if serde_json::to_vec(&prepared)?.len() > 65536 {
                        return Err(invalid("Prepared publication exceeds its budget"));
                    }
                    store::sqlx::query(
                        "UPDATE plugin_outbox SET prepared=? WHERE uid=? AND state='queued'",
                    )
                    .bind(prepared.to_string())
                    .bind(&uid)
                    .execute(&self.store.pool)
                    .await?;
                    prepared
                }
            };
            self.active_binding(binding).await?;
            let publication: PreparedPublication = serde_json::from_value(prepared.clone())?;
            let (manifest, provider) = self
                .plugin_host()?
                .contribution(&binding.provider)
                .map_err(invalid)?;
            let Contribution::Sync { schema, .. } = provider else {
                return Err(invalid("Missing Sync provider"));
            };
            let fields = manifest
                .contributions
                .iter()
                .find(|contribution| contribution.id() == schema)
                .ok_or_else(|| invalid("Missing publication schema"))?
                .fields();
            validate_import(&publication.record, fields)?;
            let canonical_digest = digest(
                &json!({"head":publication.record.head,"body":publication.record.body,"metadata":publication.record.metadata}),
            )?;
            let selected = self.plugin_selection(binding).await?;
            if !selected.iter().any(|item| {
                item.uid == record
                    && digest(&json!({"head":item.head,"body":item.body}))
                        .is_ok_and(|value| value == expected)
            }) {
                store::sqlx::query("UPDATE plugin_outbox SET state='failed',result='Record changed while preparing; publication cancelled' WHERE uid=?").bind(&uid).execute(&self.store.pool).await?;
                continue;
            }
            let mut tx = store::write_tx(&self.store.pool).await?;
            let changed = store::sqlx::query("UPDATE plugin_outbox SET state='running',attempts=attempts+1 WHERE uid=? AND state='queued' AND EXISTS(SELECT 1 FROM plugin_binding WHERE uid=? AND revision=? AND enabled=1) AND EXISTS(SELECT 1 FROM plugin_package WHERE id=? AND enabled=1)").bind(&uid).bind(&binding.uid).bind(binding.revision as i64).bind(&manifest.id).execute(&mut *tx).await?;
            if changed.rows_affected() != 1 {
                continue;
            }
            let previous: Option<(String, Option<String>)> = store::sqlx::query_as("SELECT content_digest,record_uid FROM plugin_object WHERE binding_uid=? AND external_id=?").bind(&binding.uid).bind(&publication.record.external_id).fetch_optional(&mut *tx).await?;
            if previous.is_some_and(|(digest, uid)| {
                digest != canonical_digest || uid.as_deref() != Some(&record)
            }) {
                return Err(invalid(
                    "Prepared publication conflicts with an existing external identity",
                ));
            }
            store::sqlx::query("INSERT INTO plugin_object(binding_uid,external_id,record_uid,content_digest) VALUES(?,?,?,?) ON CONFLICT DO NOTHING").bind(&binding.uid).bind(&publication.record.external_id).bind(&record).bind(&canonical_digest).execute(&mut *tx).await?;
            tx.commit().await?;
            let delivered = self
                .plugin_call(
                    &binding.provider,
                    "push",
                    json!({"settings":binding.settings,"prepared":prepared}),
                )
                .await;
            let (state, result) = match delivered {
                Ok(result) if result["accepted"] == true => {
                    count += 1;
                    ("delivered", result.to_string())
                }
                Ok(result) => ("failed", result.to_string()),
                Err(error) => ("uncertain", error.to_string()),
            };
            let result: String = result.chars().take(4096).collect();
            let now = nucleus::execution::now();
            let signer = self.signer.lock().await.clone();
            let mut tx = store::write_tx(&self.store.pool).await?;
            store::sqlx::query("UPDATE plugin_outbox SET state=?,result=? WHERE uid=?")
                .bind(state)
                .bind(&result)
                .bind(&uid)
                .execute(&mut *tx)
                .await?;
            let exists: bool =
                store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid=?)")
                    .bind(&record)
                    .fetch_one(&mut *tx)
                    .await?;
            let fact = if exists {
                crate::append::append_one_in_transaction(&mut tx, NewFact { uid: None, record_uid: record.clone(), delta: nucleus::fact::zero_delta(), at: None, actor_uid: None,
                    cause: Cause { kind: CauseKind::Sync, uid: Some(binding.uid.clone()) }, payload: Some(json!({"plugin_delivery":uid,"state":state,"external_id":publication.record.external_id}).to_string()) },now,signer.as_ref()).await?
            } else {
                None
            };
            tx.commit().await?;
            if let Some(fact) = fact {
                let _ = self.bus.send(fact);
            }
            let activity = nucleus::sync::Activity {
                instance: nucleus::sync::Instance::plugin(&binding.provider, &binding.uid),
                direction: nucleus::sync::Direction::Outgoing,
                update: nucleus::sync::Update::Incremental,
            };
            let summary = nucleus::sync::Summary {
                outcome: if state == "delivered" {
                    nucleus::sync::Outcome::Delivered
                } else {
                    nucleus::sync::Outcome::Failed
                },
                count: 1,
                subjects: vec![record],
                message: Some(format!("Plugin delivery {state}")),
            };
            let _ = store::sync_activity::append(
                &self.store.pool,
                &activity,
                &summary,
                now.timestamp(),
            )
            .await;
        }
        Ok(count)
    }
}

fn validate_import(
    item: &ImportedRecord,
    fields: &[nucleus::plugins::Field],
) -> Result<(), EngineError> {
    if item.external_id.is_empty()
        || item.external_id.len() > 1024
        || item.head.trim().is_empty()
        || item.head.len() > 1024
        || item.body.len() > 16384
    {
        return Err(invalid("Invalid or oversized imported Record"));
    }
    nucleus::plugins::validate_values(fields, &item.metadata).map_err(invalid)
}
