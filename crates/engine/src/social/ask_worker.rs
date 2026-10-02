use super::*;
use nucleus::social::ask::{self as contract, Reply, Request};
use store::sqlx::Row;

impl Engine {
    pub(super) async fn social_ask_results(
        &self,
        id: &str,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.require_permission(actor, "organ:update").await?;
        if !nucleus::valid_uid(id, "ask") {
            return Err(invalid("Choose a valid retained local query"));
        }
        let held: Option<(String, String)> = store::sqlx::query_as(
            "SELECT request,results FROM social_ask_query WHERE id=? AND (? IS NULL OR actor=?)",
        )
        .bind(id)
        .bind(actor)
        .bind(actor)
        .fetch_optional(&self.store.pool)
        .await?;
        let (request, body) = held.ok_or_else(|| invalid("This local query was cleared"))?;
        let request: Request = serde_json::from_str(&request)?;
        let answers: Vec<Value> = serde_json::from_str(&body)?;
        let mut results = Vec::new();
        let mut tx = self.social_write_tx().await?;
        for answer in answers {
            let document: Snippet = serde_json::from_value(answer["document"].clone())?;
            if super::ask::allowed_on(&mut tx, &document).await? {
                results.push(answer);
            }
        }
        tx.commit().await?;
        self.social_filter_local_results(&mut results).await?;
        Ok(
            json!({"results":results,"query":request.query,"can_manage_services":self.social_require_local_write().await.is_ok(),"status":"Saved contact answers, filtered against expiry, known withdrawals/revocations and your hidden targets. Availability remains unconfirmed; viewing makes no network request"}),
        )
    }

    pub(super) async fn social_ask_active(&self, id: &str) -> Result<bool, EngineError> {
        let now = nucleus::execution::now().timestamp();
        let held: Option<(Option<String>,String)> = store::sqlx::query_as("SELECT actor,peers FROM social_ask_query WHERE id=? AND state='pending' AND deadline>?").bind(id).bind(now).fetch_optional(&self.store.pool).await?;
        let Some((actor, body)) = held else {
            return Ok(false);
        };
        if self
            .require_permission(actor.as_deref(), "organ:update")
            .await
            .is_err()
            || self.social_require_local_write().await.is_err()
        {
            return Ok(false);
        }
        let peers: Vec<(String, String)> = serde_json::from_str(&body)?;
        for (organ, _) in peers {
            if !self.may_read_record(actor.as_deref(), &organ).await? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn social_ask_active_on(
        &self,
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        id: &str,
    ) -> Result<bool, EngineError> {
        let actor: Option<Option<String>> = store::sqlx::query_scalar(
            "SELECT actor FROM social_ask_query WHERE id=? AND state='pending' AND deadline>?",
        )
        .bind(id)
        .bind(nucleus::execution::now().timestamp())
        .fetch_optional(&mut **tx)
        .await?;
        let Some(actor) = actor else {
            return Ok(false);
        };
        match self
            .social_require_actor_on(tx, actor.as_deref(), "organ:update")
            .await
        {
            Ok(()) => {}
            Err(EngineError::Forbidden(_)) => return Ok(false),
            Err(error) => return Err(error),
        }
        self.social_require_local_write_on(tx).await?;
        let cell: String = store::sqlx::query_scalar(
            "SELECT uid FROM record WHERE slug=? AND kind=? AND deleted_at IS NULL LIMIT 1",
        )
        .bind(store::cells::LOCAL_CELL_SLUG)
        .bind(nucleus::RecordKind::Device.as_str())
        .fetch_one(&mut **tx)
        .await?;
        Ok(owner::extension_on(tx, &cell, contract::NAMESPACE).await?["enabled"] == true)
    }

    pub(super) async fn social_ask_children(
        &self,
        request: &Request,
        own: &Reply,
        peers: &[(String, String)],
        local: Option<&str>,
        incoming: Option<(&str, &str)>,
    ) -> Result<(Vec<Snippet>, bool), EngineError> {
        let budgets = super::ask::split(
            request,
            own.documents.len(),
            serde_json::to_vec(&own.documents)?.len(),
            peers.len(),
        );
        if budgets.is_empty() {
            return Ok((Vec::new(), false));
        }
        let network = self.social_network()?;
        let settings = self.social_settings().await?;
        let futures = peers.iter().zip(budgets).map(|((organ, endpoint), child)| {
            let network = network.clone();
            let settings = &settings;
            async move {
                if let Some(id) = local && !self.social_ask_active(id).await? { return Err(invalid("Cancelled or expired query")); }
                if let Some((source, original)) = incoming {
                    let consent = self.social_ask_peer(source,false).await?;
                    if consent.organ != original || !consent.forward { return Err(invalid("Onward query consent changed")); }
                }
                if self.social_ask_peer(endpoint,true).await?.organ != *organ { return Err(invalid("The pinned query contact changed")); }
                let message = PublicRequest::AskContacts { document: Box::new(child.clone()) };
                let now = nucleus::execution::now().timestamp();
                let mut tx = self.social_write_tx().await?;
                self.social_require_local_write_on(&mut tx).await?;
                if let Some(id) = local && !self.social_ask_active_on(&mut tx,id).await? { return Err(invalid("Cancelled or expired query")); }
                if let Some((source, original)) = incoming && !self.social_ask_peer_on(&mut tx, source, original, false).await?.forward { return Err(invalid("Onward query consent changed")); }
                self.social_ask_peer_on(&mut tx,endpoint,organ,true).await?;
                if now >= child.deadline { return Err(invalid("This contact question has expired")); }
                store::social::spend_on(&mut tx, endpoint, "out", serde_json::to_vec(&message)?.len(), settings.outgoing_bytes_per_minute, now).await?;
                tx.commit().await?;
                let timeout = if request.depth == contract::MAX_DEPTH {8} else {4};
                let until = std::time::Duration::from_secs((child.deadline-now).clamp(1, timeout) as u64);
                let response = tokio::time::timeout(until, network.request(endpoint, message));
                tokio::pin!(response);
                let response = loop {
                    tokio::select! {
                        result = &mut response => break result.map_err(|_| invalid("A contact did not answer within the query deadline"))??,
                        _ = tokio::time::sleep(std::time::Duration::from_millis(250)), if local.is_some() => {
                            if !self.social_ask_active(local.unwrap()).await? { return Err(invalid("Cancelled or expired query")); }
                        }
                    }
                };
                let size = serde_json::to_vec(&response)?.len();
                if size > MAX_FRAME_BYTES-1024 || response["service"].as_str() != Some(endpoint) { return Err(invalid("Invalid pinned contact answer")); }
                store::social::spend(&self.store.pool,endpoint,"in",size,settings.incoming_bytes_per_minute,nucleus::execution::now().timestamp()).await?;
                let answer: Reply = serde_json::from_value(response["reply"].clone())?;
                super::ask::validate_reply(&answer,&child,nucleus::execution::now().timestamp())?;
                if self.social_ask_peer(endpoint,true).await?.organ != *organ { return Err(invalid("The pinned query contact changed while awaiting its answer")); }
                Ok::<_,EngineError>(answer)
            }
        });
        let mut documents = Vec::new();
        let mut partial = false;
        let mut seen: std::collections::BTreeSet<_> = own
            .documents
            .iter()
            .map(|document| document_hash("snippet", document))
            .collect::<Result<_, _>>()?;
        let mut futures = futures.collect::<Vec<_>>().into_iter();
        let first = futures.next();
        let second = futures.next();
        let third = futures.next();
        let results = tokio::join!(
            async {
                if let Some(future) = first {
                    Some(future.await)
                } else {
                    None
                }
            },
            async {
                if let Some(future) = second {
                    Some(future.await)
                } else {
                    None
                }
            },
            async {
                if let Some(future) = third {
                    Some(future.await)
                } else {
                    None
                }
            },
        );
        for result in [results.0, results.1, results.2].into_iter().flatten() {
            match result {
                Ok(answer) => {
                    partial |= answer.partial;
                    for document in answer.documents {
                        if seen.insert(document_hash("snippet", &document)?) {
                            documents.push(document);
                        }
                    }
                }
                Err(_) => partial = true,
            }
        }
        Ok((documents, partial))
    }

    pub(super) async fn social_start_ask(
        &self,
        mut query: Search,
        contacts: Vec<String>,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.require_permission(actor, "organ:update").await?;
        query.after = None;
        validate_search(&query)?;
        if contacts.is_empty()
            || contacts.len() > 3
            || contacts.iter().any(|uid| !nucleus::valid_uid(uid, "r"))
        {
            return Err(invalid(
                "Choose one to three separately consenting contacts",
            ));
        }
        let peers = self.social_ask_eligible(Some(&contacts), None).await?;
        if peers.len() != contacts.len() {
            return Err(invalid(
                "Each selected contact needs query consent, a pinned endpoint and reachability",
            ));
        }
        for (organ, _) in &peers {
            if !self.may_read_record(actor, organ).await? {
                return Err(invalid(
                    "Read permission is required for each selected query contact",
                ));
            }
        }
        let now = nucleus::execution::now().timestamp();
        let request = Request {
            id: nucleus::new_uid("ask"),
            query: query.clone(),
            issued_at: now,
            deadline: now + 30,
            work: contract::MAX_WORK,
            bytes: contract::MAX_BYTES,
            results: contract::MAX_RESULTS,
            depth: contract::MAX_DEPTH,
        };
        let mut tx = self.social_write_tx().await?;
        store::sqlx::query(
            "UPDATE social_ask_query SET state='expired' WHERE state='pending' AND deadline<=?",
        )
        .bind(now)
        .execute(&mut *tx)
        .await?;
        let (active, total): (i64, i64) = store::sqlx::query_as(
            "SELECT COALESCE(SUM(state='pending'),0),COUNT(*) FROM social_ask_query",
        )
        .fetch_one(&mut *tx)
        .await?;
        if active >= 4 || total >= 20 {
            return Err(invalid(
                "Keep at most four active and twenty recent queries; clear finished history to ask again",
            ));
        }
        store::sqlx::query(
            "INSERT INTO social_ask_query(id,actor,request,peers,deadline) VALUES(?,?,?,?,?)",
        )
        .bind(&request.id)
        .bind(actor)
        .bind(serde_json::to_string(&request)?)
        .bind(serde_json::to_string(&peers)?)
        .bind(request.deadline)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.notify_query_changed();
        let mut view = self.social_ask_view(actor).await?;
        view["results"] = json!(store::social::search(&self.store.pool, &query, now).await?);
        view["status"] = json!(
            "Local cached results shown first. Your saved question will be sent to the selected contacts, who can read it; their onward consent and the shared budget limit further disclosure"
        );
        Ok(view)
    }

    pub(super) async fn social_cancel_ask(
        &self,
        id: Option<&str>,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.require_permission(actor, "organ:update").await?;
        let mut tx = self.social_write_tx().await?;
        store::sqlx::query(
            "UPDATE social_ask_query SET state='expired' WHERE state='pending' AND deadline<=?",
        )
        .bind(nucleus::execution::now().timestamp())
        .execute(&mut *tx)
        .await?;
        if let Some(id) = id {
            if !nucleus::valid_uid(id, "ask") {
                return Err(invalid("Choose a valid local query"));
            }
            store::sqlx::query(
                "UPDATE social_ask_query SET state='cancelled' WHERE id=? AND state='pending' AND (? IS NULL OR actor=?)",
            )
            .bind(id)
            .bind(actor).bind(actor)
            .execute(&mut *tx)
            .await?;
        } else {
            store::sqlx::query(
                "DELETE FROM social_ask_query WHERE state<>'pending' AND (? IS NULL OR actor=?)",
            )
            .bind(actor)
            .bind(actor)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.notify_query_changed();
        self.social_ask_view(actor).await
    }

    pub async fn social_ask_once(&self) -> Result<usize, EngineError> {
        let result = self.social_ask_step().await;
        if let Err(error) = &result {
            let message = error.to_string().chars().take(500).collect::<String>();
            store::sqlx::query("UPDATE social_ask_query SET error=? WHERE id=(SELECT id FROM social_ask_query WHERE state='pending' ORDER BY deadline,id LIMIT 1)").bind(message).execute(&self.store.pool).await?;
            self.notify_query_changed();
        }
        result
    }

    async fn social_ask_step(&self) -> Result<usize, EngineError> {
        self.social_require_local_write().await?;
        let now = nucleus::execution::now().timestamp();
        let expired = store::sqlx::query(
            "UPDATE social_ask_query SET state='expired' WHERE state='pending' AND deadline<=?",
        )
        .bind(now)
        .execute(&self.store.pool)
        .await?
        .rows_affected();
        if expired > 0 {
            self.notify_query_changed();
        }
        if !self.social_ask_settings().await?.enabled {
            return Ok(0);
        }
        let row = store::sqlx::query("SELECT id,request,peers FROM social_ask_query WHERE state='pending' AND deadline>? ORDER BY deadline,id LIMIT 1").bind(now).fetch_optional(&self.store.pool).await?;
        let Some(row) = row else {
            return Ok(0);
        };
        let request: Request = serde_json::from_str(&row.get::<String, _>("request"))?;
        let peers: Vec<(String, String)> = serde_json::from_str(&row.get::<String, _>("peers"))?;
        super::ask::validate_request(&request, now)?;
        let mut reply = self.social_ask_local(&request).await?;
        let outcome = self
            .social_ask_children(&request, &reply, &peers, Some(&request.id), None)
            .await;
        let error = match outcome {
            Ok((documents, partial)) => {
                reply.documents.extend(documents);
                reply.partial |= partial;
                None
            }
            Err(_) => {
                reply.partial = true;
                Some("Waiting for a network connection within the original deadline")
            }
        };
        if !self.social_ask_active(&request.id).await? {
            return Ok(0);
        }
        if let Some(error) = error {
            store::sqlx::query(
                "UPDATE social_ask_query SET error=? WHERE id=? AND state='pending'",
            )
            .bind(error)
            .bind(&request.id)
            .execute(&self.store.pool)
            .await?;
            self.notify_query_changed();
            return Ok(0);
        }
        super::ask::clip(&mut reply, &request)?;
        self.social_require_local_write().await?;
        if !self.social_ask_settings().await?.enabled {
            return Ok(0);
        }
        let settings = self.social_settings().await?;
        let mut tx = self.social_write_tx().await?;
        if !self.social_ask_active_on(&mut tx, &request.id).await? {
            return Ok(0);
        }
        for (organ, endpoint) in &peers {
            match self
                .social_ask_peer_on(&mut tx, endpoint, organ, true)
                .await
            {
                Ok(_) => {}
                Err(EngineError::Consequence(_)) => {
                    reply.documents.clear();
                    reply.partial = true;
                    break;
                }
                Err(error) => return Err(error),
            }
        }
        let mut results = Vec::new();
        for document in reply.documents {
            if !super::ask::allowed_on(&mut tx, &document).await? {
                continue;
            }
            let hash = document_hash("snippet", &document)?;
            self.social_cache_snippet_on(
                &mut tx,
                &document,
                &hash,
                "contact search (availability unconfirmed)",
                &settings,
                nucleus::execution::now().timestamp(),
            )
            .await?;
            results.push(json!({"document":document,"hash":hash,"source":"contact search; immediate contacts listed in query history","freshness":"availability unconfirmed"}));
        }
        let mut current_results = Vec::new();
        for result in results {
            let document: Snippet = serde_json::from_value(result["document"].clone())?;
            if super::ask::allowed_on(&mut tx, &document).await? {
                current_results.push(result);
            }
        }
        store::sqlx::query("UPDATE social_ask_query SET state='completed',results=?,error=? WHERE id=? AND state='pending'").bind(serde_json::to_string(&current_results)?).bind(if reply.partial { Some("Partial contact search: a source failed, timed out or had a bounded incomplete answer") } else { None }).bind(&request.id).execute(&mut *tx).await?;
        tx.commit().await?;
        self.notify_query_changed();
        Ok(1)
    }

    pub(super) async fn social_ask_view(&self, actor: Option<&str>) -> Result<Value, EngineError> {
        if self
            .require_permission(actor, "organ:update")
            .await
            .is_err()
        {
            return Ok(json!({}));
        }
        let settings = self.social_ask_settings().await?;
        let known = store::organs::contacts(&self.store.pool).await?;
        let mut contacts = Vec::new();
        for choice in &settings.peers {
            if let Some(contact) = known
                .iter()
                .find(|contact| contact.record_uid == choice.organ)
            {
                if !self.may_read_record(actor, &choice.organ).await? {
                    continue;
                }
                contacts.push(json!({"choice":choice,"name":contact.head,"endpoint":contact.node_id,"retired":contact.trust!="known","unreachable":contact.unreachable_since.is_some()}));
            } else {
                contacts.push(json!({"choice":choice,"name":"Removed contact","retired":true}));
            }
        }
        for contact in known {
            if contacts.len() >= contract::MAX_PEERS {
                break;
            }
            if contact.trust == "known"
                && !settings
                    .peers
                    .iter()
                    .any(|peer| peer.organ == contact.record_uid)
                && self.may_read_record(actor, &contact.record_uid).await?
            {
                contacts.push(json!({"choice":contract::ContactConsent { organ:contact.record_uid, ..Default::default() },"name":contact.head,"endpoint":contact.node_id,"unreachable":contact.unreachable_since.is_some()}));
            }
        }
        let mut queries = Vec::new();
        let mut results = Vec::new();
        for row in store::sqlx::query("SELECT id,request,peers,deadline,state,error,results FROM social_ask_query WHERE (? IS NULL OR actor=?) ORDER BY deadline DESC,id DESC LIMIT 20").bind(actor).bind(actor).fetch_all(&self.store.pool).await? {
            let request: Request = serde_json::from_str(&row.get::<String,_>("request"))?;
            let peers: Vec<(String,String)> = serde_json::from_str(&row.get::<String,_>("peers"))?;
            let answers: Vec<Value> = serde_json::from_str(&row.get::<String,_>("results"))?;
            if queries.is_empty() {
                let mut tx=self.social_write_tx().await?;
                for answer in &answers {
                    let document: Snippet=serde_json::from_value(answer["document"].clone())?;
                    if super::ask::allowed_on(&mut tx,&document).await? { results.push(answer.clone()); }
                }
                tx.commit().await?;
            }
            let state: String = row.get("state");
            let state = if state=="pending" && request.deadline<=nucleus::execution::now().timestamp() { "expired" } else { &state };
            queries.push(json!({"id":row.get::<String,_>("id"),"query":request.query,"deadline":request.deadline,"state":state,"error":row.get::<Option<String>,_>("error"),"peers":peers,"count":answers.len()}));
        }
        Ok(
            json!({"asks":{"enabled":settings.enabled,"contacts":contacts,"queries":queries},"results":results,"can_manage_services":self.social_require_local_write().await.is_ok(),"status":"Contact queries are deliberate and disclose their words to selected contacts. Cancellation stops local work; clearing removes finished local query history"}),
        )
    }
}
