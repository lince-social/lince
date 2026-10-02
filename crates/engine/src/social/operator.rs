use super::*;
use serde::Serialize;

#[derive(Clone, Copy)]
pub enum Worker {
    Publication,
    Send,
    Pickup,
    Gossip,
    Ask,
    Subscriptions,
}

impl Worker {
    pub fn name(self) -> &'static str {
        match self {
            Self::Publication => "publication-and-authorization",
            Self::Send => "private-send",
            Self::Pickup => "private-pickup",
            Self::Gossip => "gossip",
            Self::Ask => "contact-queries",
            Self::Subscriptions => "saved-searches",
        }
    }
}

#[derive(Clone, Default, Serialize)]
pub struct WorkerStatus {
    started_at: i64,
    last_completed_at: Option<i64>,
    last_succeeded: Option<bool>,
    running: bool,
    restarts: u32,
    failed_passes: u64,
}

impl Engine {
    pub fn social_set_deployment_settings(
        &self,
        settings: ServiceSettings,
    ) -> Result<(), EngineError> {
        descriptor::validate_settings(&settings)?;
        if settings.relay || settings.gossip {
            return Err(invalid(
                "Application relay hosting is not available. Configure gossip with its separate device/contact consent controls",
            ));
        }
        *self
            .social_deployment
            .lock()
            .map_err(|_| invalid("Cannot set deployment hosting policy"))? = Some(settings);
        self.notify_config_changed();
        Ok(())
    }

    pub fn social_services_managed(&self) -> bool {
        self.social_deployment
            .lock()
            .map_or(true, |settings| settings.is_some())
    }

    pub fn social_worker_started(&self, worker: Worker) {
        if let Ok(mut workers) = self.social_workers.lock() {
            let entry = workers.entry(worker.name()).or_default();
            if entry.started_at != 0 {
                entry.restarts = entry.restarts.saturating_add(1);
            }
            entry.started_at = nucleus::execution::now().timestamp();
            entry.running = true;
            entry.last_succeeded = None;
        }
    }

    pub fn social_worker_completed(&self, worker: Worker, succeeded: bool) {
        if let Ok(mut workers) = self.social_workers.lock() {
            let entry = workers.entry(worker.name()).or_default();
            entry.last_completed_at = Some(nucleus::execution::now().timestamp());
            entry.last_succeeded = Some(succeeded);
            if !succeeded {
                entry.failed_passes = entry.failed_passes.saturating_add(1);
            }
        }
    }

    pub fn social_worker_stopped(&self, worker: Worker) {
        if let Ok(mut workers) = self.social_workers.lock() {
            let entry = workers.entry(worker.name()).or_default();
            entry.running = false;
            entry.last_succeeded = Some(false);
        }
    }

    pub async fn social_service_health(&self) -> Result<Value, EngineError> {
        let now = nucleus::execution::now().timestamp();
        let workers: Vec<Value> = self
            .social_workers
            .lock()
            .map_err(|_| invalid("Cannot inspect social worker health"))?
            .iter()
            .map(|(name, state)| json!({"name":name,"state":state}))
            .collect();
        let mut tx = self.store.pool.begin().await?;
        let settings = self.social_settings_on(&mut tx).await?;
        let publication: (i64, Option<i64>) = store::sqlx::query_as("WITH jobs AS (SELECT CASE WHEN json_valid(body) THEN body ELSE '{}' END AS body FROM social_publication_job WHERE state='pending' AND expires_at>?) SELECT COUNT(*),MIN(CASE WHEN json_type(body,'$.issued_at')='integer' THEN json_extract(body,'$.issued_at') WHEN json_type(body,'$.document.issued_at')='integer' THEN json_extract(body,'$.document.issued_at') END) FROM jobs").bind(now).fetch_one(&mut *tx).await?;
        let delivery = health::snapshot(&mut tx, now).await?;
        let (mailbox_entries, mailbox_bytes) = mailbox::usage_on(&mut tx).await?;
        let (mailbox_entry_limit, mailbox_byte_limit) = mailbox::limits(&settings, false);
        let (mailbox_data_entry_limit, mailbox_data_byte_limit) = mailbox::limits(&settings, true);
        let gossip: i64 = store::sqlx::query_scalar(
            "SELECT COUNT(*) FROM social_gossip_forward WHERE state='pending'",
        )
        .fetch_one(&mut *tx)
        .await?;
        let queries: i64 = store::sqlx::query_scalar(
            "SELECT COUNT(*) FROM social_ask_query WHERE state='pending' AND deadline>?",
        )
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        let cache: (i64, i64) = store::sqlx::query_as(
            "SELECT COUNT(*),COALESCE(SUM(length(CAST(body AS BLOB))),0) FROM social_document",
        )
        .fetch_one(&mut *tx)
        .await?;
        let mail: (i64, i64) = store::sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(length(CAST(body AS BLOB))),0) FROM social_service_envelope WHERE expires_at>?").bind(now).fetch_one(&mut *tx).await?;
        let controls: i64 = store::sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM social_ended_post)+(SELECT COUNT(*) FROM social_posting_authority)+(SELECT COUNT(*) FROM social_profile_authority)+(SELECT COUNT(*) FROM social_owner_control)").fetch_one(&mut *tx).await?;
        let pages: i64 = store::sqlx::query_scalar("PRAGMA page_count")
            .fetch_one(&mut *tx)
            .await?;
        let page_size: i64 = store::sqlx::query_scalar("PRAGMA page_size")
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        let age = |issued: Option<i64>| {
            issued
                .filter(|at| *at > 0 && *at <= now)
                .map(|issued| now.saturating_sub(issued))
        };
        Ok(
            json!({"service_health":{"observed_at":now,"managed":self.social_services_managed(),"settings":settings,"workers":workers,"queues":{"publication":publication.0,"publication_oldest_age_seconds":age(publication.1),"messages":delivery["preparation"]["messages"],"message_oldest_age_seconds":delivery["preparation"]["oldest_age_seconds"],"gossip":gossip,"queries":queries},"delivery":delivery,"storage":{"cache_entries":cache.0,"public_payload_bytes":cache.1,"mail_entries":mail.0,"mail_payload_bytes":mail.1,"authority_floor_entries":controls,"main_database_allocated_bytes":pages.saturating_mul(page_size),"mailbox_reserved_entries":mailbox_entries,"mailbox_reserved_budget_bytes":mailbox_bytes,"mailbox_entry_limit":mailbox_entry_limit,"mailbox_byte_limit":mailbox_byte_limit,"mailbox_data_entry_limit":mailbox_data_entry_limit,"mailbox_data_byte_limit":mailbox_data_byte_limit}},"status":"Health is a local snapshot of retained local metadata, not an independent delivery audit. Admission accounting includes reserved estimated overhead. Payload quotas differ from total disk use; database allocation excludes the WAL, blobs and backups"}),
        )
    }

    pub async fn social_rebuild_public_index(&self) -> Result<Value, EngineError> {
        self.social_require_local_write().await?;
        let mut tx = self.social_write_tx().await?;
        store::sqlx::query("DELETE FROM social_search")
            .execute(&mut *tx)
            .await?;
        let mut after = String::new();
        let mut indexed = 0;
        let mut skipped = 0;
        loop {
            let rows: Vec<(String, String)> = store::sqlx::query_as("SELECT id,body FROM social_document WHERE kind='snippet' AND state='active' AND id>? ORDER BY id LIMIT 100").bind(&after).fetch_all(&mut *tx).await?;
            if rows.is_empty() {
                break;
            }
            for (id, body) in rows {
                after = id;
                let document = serde_json::from_str::<Snippet>(&body).ok();
                if let Some(document) = document
                    && document.id == after
                    && ask::allowed_on(&mut tx, &document).await?
                {
                    store::sqlx::query("INSERT INTO social_search(id,title,text) VALUES(?,?,?)")
                        .bind(&document.id)
                        .bind(&document.title)
                        .bind(&document.text)
                        .execute(&mut *tx)
                        .await?;
                    indexed += 1;
                } else {
                    skipped += 1;
                }
            }
        }
        tx.commit().await?;
        self.notify_query_changed();
        Ok(
            json!({"indexed":indexed,"skipped":skipped,"status":"Rebuilt the public search index from currently valid signed announcements. Ending and authority floors remain retained"}),
        )
    }
}
