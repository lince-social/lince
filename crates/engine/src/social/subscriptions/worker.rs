use super::*;

impl Engine {
    pub(in crate::social) async fn social_subscription_current_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        guard: &Guard,
    ) -> Result<bool, EngineError> {
        let (_organ, map, filters) = self.social_subscription_config_on(tx).await?;
        if !within_limits(&map, &filters)?
            || map[key(&guard.filter.id)] != serde_json::to_value(&guard.filter)?
            || !guard.filter.enabled
        {
            return Ok(false);
        }
        let cell: String = store::sqlx::query_scalar(
            "SELECT uid FROM record WHERE slug=? AND kind=? AND deleted_at IS NULL LIMIT 1",
        )
        .bind(store::cells::LOCAL_CELL_SLUG)
        .bind(nucleus::RecordKind::Device.as_str())
        .fetch_one(&mut **tx)
        .await?;
        if owner::extension_on(tx, &cell, DEVICE_NAMESPACE).await?["enabled"] != true {
            return Ok(false);
        }
        if let Some(actor) = guard.filter.actor.as_deref() {
            let user = store::auth::principal_on(&mut **tx, actor).await?;
            if user.is_none_or(|user| !user.permits("organ:update")) {
                return Ok(false);
            }
        }
        match self.social_require_local_write_on(tx).await {
            Ok(()) => {}
            Err(EngineError::Forbidden(_)) => return Ok(false),
            Err(error) => return Err(error),
        }
        let current:bool=store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM social_subscription_job WHERE id=? AND config_hash=? AND lease_token=?)")
            .bind(&guard.filter.id).bind(document_hash("saved-search",&guard.filter)?).bind(&guard.lease).fetch_one(&mut **tx).await?;
        Ok(current)
    }

    async fn social_subscription_current(&self, guard: &Guard) -> Result<bool, EngineError> {
        let mut tx = self.social_write_tx().await?;
        let current = self.social_subscription_current_on(&mut tx, guard).await?;
        tx.commit().await?;
        Ok(current)
    }

    async fn social_subscription_claim(
        &self,
        now: i64,
        online: bool,
    ) -> Result<Option<Guard>, EngineError> {
        let mut tx = self.social_write_tx().await?;
        store::sqlx::query("DELETE FROM social_subscription_seen WHERE expires_at<=?")
            .bind(now)
            .execute(&mut *tx)
            .await?;
        let (_, map, filters) = self.social_subscription_config_on(&mut tx).await?;
        if !within_limits(&map, &filters)? {
            return Err(invalid(
                "Resolve the merged saved-search limits before periodic work",
            ));
        }
        let ids: Vec<_> = filters.iter().map(|filter| filter.id.as_str()).collect();
        store::sqlx::query(
            "DELETE FROM social_subscription_job WHERE id NOT IN (SELECT value FROM json_each(?))",
        )
        .bind(serde_json::to_string(&ids)?)
        .execute(&mut *tx)
        .await?;
        for filter in &filters {
            let hash = document_hash("saved-search", filter)?;
            let existing: Option<(String, Option<i64>)> = store::sqlx::query_as(
                "SELECT config_hash,last_attempt FROM social_subscription_job WHERE id=?",
            )
            .bind(&filter.id)
            .fetch_optional(&mut *tx)
            .await?;
            match existing {
                None => {
                    store::sqlx::query(
                        "INSERT INTO social_subscription_job(id,config_hash) VALUES(?,?)",
                    )
                    .bind(&filter.id)
                    .bind(hash)
                    .execute(&mut *tx)
                    .await?;
                }
                Some((held, last)) if held != hash => {
                    store::sqlx::query("UPDATE social_subscription_job SET config_hash=?,lease_token='',lease_until=0,next_attempt=?,error=NULL,results='[]',source='Settings changed; waiting for refresh' WHERE id=?")
                        .bind(hash).bind(last.map_or(now,|time|time.saturating_add(i64::from(filter.interval_minutes)*60)).max(now)).bind(&filter.id).execute(&mut *tx).await?;
                }
                _ => {}
            }
            if !filter.enabled || !filter.notifications {
                store::sqlx::query("UPDATE social_subscription_seen SET notified=1 WHERE subscription=? AND notified=0").bind(&filter.id).execute(&mut *tx).await?;
            }
        }
        let mut due = Vec::new();
        for filter in filters.into_iter().filter(|filter| filter.enabled) {
            let (next, lease, last, network): (i64, i64, Option<i64>, Option<i64>) = store::sqlx::query_as(
                "SELECT next_attempt,lease_until,last_attempt,last_network_attempt FROM social_subscription_job WHERE id=?",
            )
            .bind(&filter.id)
            .fetch_one(&mut *tx)
            .await?;
            let reconnect =
                online && !filter.services.is_empty() && last.is_some() && network < last;
            if lease <= now && (next <= now || lease > 0 || reconnect) {
                due.push((next, filter));
            }
        }
        due.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));
        let guard = if let Some((_, filter)) = due.into_iter().next() {
            let lease = nucleus::new_uid("work");
            store::sqlx::query("UPDATE social_subscription_job SET lease_token=?,lease_until=?,last_attempt=?,last_network_attempt=CASE WHEN ? THEN ? ELSE last_network_attempt END,next_attempt=?,error='Refresh in progress' WHERE id=?")
                .bind(&lease).bind(now+35).bind(now).bind(online && !filter.services.is_empty()).bind(now).bind(now+i64::from(filter.interval_minutes)*60).bind(&filter.id).execute(&mut *tx).await?;
            Some(Guard { filter, lease })
        } else {
            None
        };
        tx.commit().await?;
        Ok(guard)
    }

    async fn social_subscription_refresh(
        &self,
        guard: &Guard,
        online: bool,
        now: i64,
    ) -> Result<(Vec<Value>, bool), EngineError> {
        let known = self
            .social_discovery_known_refs(&guard.filter.query, now)
            .await?;
        let mut partial = false;
        if online && !guard.filter.services.is_empty() {
            let network = self.social_network()?;
            for endpoint in &guard.filter.services {
                if !self.social_subscription_current(guard).await? {
                    return Err(invalid(
                        "Saved search was changed, disabled or lost permission",
                    ));
                }
                let request = PublicRequest::Search {
                    query: guard.filter.query.clone(),
                    known: known.clone(),
                };
                let settings = self.social_settings().await?;
                store::social::spend(
                    &self.store.pool,
                    endpoint,
                    "out",
                    serde_json::to_vec(&request)?.len(),
                    settings.outgoing_bytes_per_minute,
                    now,
                )
                .await?;
                let response = network.request(endpoint, request).await;
                match response {
                    Ok(data) => {
                        store::social::spend(
                            &self.store.pool,
                            endpoint,
                            "in",
                            serde_json::to_vec(&data)?.len(),
                            settings.incoming_bytes_per_minute,
                            now,
                        )
                        .await?;
                        partial |= data["refresh_incomplete"] == true;
                        if self
                            .social_cache_search_checked(endpoint, &data, now, Some(guard))
                            .await
                            .is_err()
                        {
                            partial = true;
                        }
                    }
                    Err(_) => partial = true,
                }
            }
        }
        if !self.social_subscription_current(guard).await? {
            return Err(invalid(
                "Saved search no longer has current participation and permission",
            ));
        }
        let current = self
            .social_search(guard.filter.query.clone(), vec![], now)
            .await?;
        Ok((
            current["results"].as_array().cloned().unwrap_or_default(),
            partial,
        ))
    }

    async fn social_subscription_save_matches(
        &self,
        guard: &Guard,
        mut rows: Vec<Value>,
        partial: bool,
        online: bool,
        now: i64,
    ) -> Result<(), EngineError> {
        let mut tx = self.social_write_tx().await?;
        if !self.social_subscription_current_on(&mut tx, guard).await? {
            return Err(invalid(
                "Saved search changed before its results could commit",
            ));
        }
        self.social_filter_local_results_on(&mut tx, &mut rows)
            .await?;
        let mut refs = Vec::new();
        let mut exhausted = false;
        for row in rows {
            let document: Snippet = serde_json::from_value(row["document"].clone())?;
            let hash = document_hash("snippet", &document)?;
            if row["hash"] != hash || !ask::allowed_on(&mut tx, &document).await? {
                continue;
            }
            let id = nucleus::fact::sha256_hex(
                format!("{}\n{}\n{hash}", guard.filter.id, document.id).as_bytes(),
            );
            let exists: bool = store::sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM social_subscription_seen WHERE id=?)",
            )
            .bind(&id)
            .fetch_one(&mut *tx)
            .await?;
            if !exists {
                let (local,total):(i64,i64)=store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM social_subscription_seen WHERE subscription=?),(SELECT COUNT(*) FROM social_subscription_seen)")
                    .bind(&guard.filter.id).fetch_one(&mut *tx).await?;
                if local >= 256 || total >= 4096 {
                    exhausted = true;
                    continue;
                }
                store::sqlx::query("INSERT INTO social_subscription_seen(id,subscription,post,hash,expires_at,matched_at,notified) VALUES(?,?,?,?,?,?,?)")
                    .bind(id).bind(&guard.filter.id).bind(&document.id).bind(&hash).bind(document.expires_at.saturating_add(300)).bind(now).bind(i64::from(!guard.filter.notifications)).execute(&mut *tx).await?;
            }
            refs.push(json!({"post":document.id,"hash":hash}));
        }
        let source = if online && !guard.filter.services.is_empty() {
            "Selected directories and validated cache; availability is unconfirmed"
        } else {
            "Validated local cache; selected directories were not queried"
        };
        let error = if exhausted {
            Some(
                "Saved-match tracking is full. Existing entries remain until expiry; new alerts are paused",
            )
        } else if partial {
            Some(
                "One or more selected services did not return a complete valid refresh; shown matches are validated cached evidence",
            )
        } else {
            None
        };
        store::sqlx::query("UPDATE social_subscription_job SET lease_token='',lease_until=0,last_completed=?,error=?,source=?,results=? WHERE id=? AND lease_token=?")
            .bind(now).bind(error).bind(source).bind(serde_json::to_string(&refs)?).bind(&guard.filter.id).bind(&guard.lease).execute(&mut *tx).await?;
        tx.commit().await?;
        self.notify_query_changed();
        Ok(())
    }

    pub async fn social_subscriptions_once(
        &self,
        online: bool,
    ) -> Result<Vec<String>, EngineError> {
        self.social_require_local_write().await?;
        if !self.social_subscription_device_enabled().await? {
            return Ok(vec![]);
        }
        let now = nucleus::execution::now().timestamp();
        if let Some(guard) = self.social_subscription_claim(now, online).await? {
            let result = tokio::time::timeout(std::time::Duration::from_secs(30), async {
                let (rows, partial) = self
                    .social_subscription_refresh(&guard, online, now)
                    .await?;
                self.social_subscription_save_matches(&guard, rows, partial, online, now)
                    .await
            })
            .await;
            if !matches!(result, Ok(Ok(()))) {
                store::sqlx::query("UPDATE social_subscription_job SET lease_token='',lease_until=0,next_attempt=?,error='Waiting for an unchanged filter, current permission and a bounded refresh' WHERE id=? AND lease_token=?")
                    .bind(now+300).bind(&guard.filter.id).bind(&guard.lease).execute(&self.store.pool).await?;
                self.notify_query_changed();
            }
        }
        self.social_subscription_notice(now).await
    }

    async fn social_subscription_notice(&self, now: i64) -> Result<Vec<String>, EngineError> {
        let mut tx = self.social_write_tx().await?;
        let (_, map, filters) = self.social_subscription_config_on(&mut tx).await?;
        if !within_limits(&map, &filters)? {
            return Ok(vec![]);
        }
        let mut candidates = Vec::new();
        for filter in filters
            .into_iter()
            .filter(|filter| filter.enabled && filter.notifications && !quiet(filter, now))
        {
            let pending:Option<i64>=store::sqlx::query_scalar("SELECT MIN(matched_at) FROM social_subscription_seen WHERE subscription=? AND notified=0 AND expires_at>?").bind(&filter.id).bind(now).fetch_one(&mut *tx).await?;
            if let Some(time) = pending {
                candidates.push((time, filter));
            }
        }
        candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));
        let mut notices = Vec::new();
        for (_, filter) in candidates {
            let guard = Guard {
                filter,
                lease: String::new(),
            };
            if !self.social_subscription_current_on(&mut tx, &guard).await? {
                continue;
            }
            let pending:Vec<(String,Option<String>,Option<String>)>=store::sqlx::query_as("SELECT s.id,d.body,d.hash FROM social_subscription_seen s LEFT JOIN social_document d ON d.id=s.post AND d.kind='snippet' AND d.hash=s.hash WHERE s.subscription=? AND s.notified=0 AND s.expires_at>? ORDER BY s.matched_at,s.id LIMIT 50")
                .bind(&guard.filter.id).bind(now).fetch_all(&mut *tx).await?;
            let mut visible = Vec::new();
            for (id, body, hash) in pending {
                if let Some(body) = body {
                    let document: Snippet = serde_json::from_str(&body)?;
                    if ask::allowed_on(&mut tx, &document).await? {
                        visible.push(json!({"document":document,"hash":hash}));
                    }
                }
                store::sqlx::query("UPDATE social_subscription_seen SET notified=1 WHERE id=?")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
            self.social_filter_local_results_on(&mut tx, &mut visible)
                .await?;
            if !visible.is_empty() {
                notices.push(nucleus::fact::sha256_hex(guard.filter.id.as_bytes()));
                break;
            }
        }
        tx.commit().await?;
        Ok(notices)
    }

    pub(in crate::social) async fn social_subscription_results(
        &self,
        id: &str,
    ) -> Result<Value, EngineError> {
        if !nucleus::valid_uid(id, "sub") {
            return Err(invalid("Choose a saved filter"));
        }
        let mut tx = self.social_write_tx().await?;
        let (_, map, filters) = self.social_subscription_config_on(&mut tx).await?;
        let filter = filters
            .into_iter()
            .find(|filter| filter.id == id)
            .ok_or_else(|| invalid("This saved filter was removed"))?;
        let state:Option<(String,String,String,Option<i64>,Option<String>)>=store::sqlx::query_as("SELECT config_hash,results,source,last_completed,error FROM social_subscription_job WHERE id=?")
            .bind(id).fetch_optional(&mut *tx).await?;
        let mut results = Vec::new();
        let mut source = "No refresh retained on this device".to_owned();
        let mut checked = None;
        let mut error = None;
        if let Some((hash, body, origin, time, held_error)) = state {
            source = origin;
            checked = time;
            error = held_error;
            if hash == document_hash("saved-search", &filter)? {
                for reference in serde_json::from_str::<Vec<Value>>(&body)? {
                    let row:Option<(String,String,String)>=store::sqlx::query_as("SELECT body,hash,source FROM social_document WHERE kind='snippet' AND id=? AND hash=?")
                        .bind(reference["post"].as_str().unwrap_or_default()).bind(reference["hash"].as_str().unwrap_or_default()).fetch_optional(&mut *tx).await?;
                    if let Some((body, hash, origin)) = row {
                        let document: Snippet = serde_json::from_str(&body)?;
                        if ask::allowed_on(&mut tx, &document).await? {
                            results.push(json!({"document":document,"hash":hash,"source":origin}));
                        }
                    }
                }
            } else {
                error = Some("Saved settings changed; these old results were not reused".into());
            }
        }
        self.social_filter_local_results_on(&mut tx, &mut results)
            .await?;
        tx.commit().await?;
        Ok(
            json!({"results":results,"query":filter.query,"services":[],"source":source,"checked_at":checked,"status":error.unwrap_or_else(||"Saved matches were checked against current expiry, withdrawals, authority and hidden targets. Viewing makes no network request".into()),"over_limit":!within_limits(&map,&entries(&map)?)?,"can_manage_services":self.social_require_local_write().await.is_ok()}),
        )
    }

    pub(in crate::social) async fn social_clear_subscription_matches(
        &self,
        id: &str,
    ) -> Result<Value, EngineError> {
        if !nucleus::valid_uid(id, "sub") {
            return Err(invalid("Choose a saved filter"));
        }
        store::sqlx::query("UPDATE social_subscription_job SET results='[]' WHERE id=?")
            .bind(id)
            .execute(&self.store.pool)
            .await?;
        self.notify_query_changed();
        self.social_subscriptions(None).await
    }
}
