use super::*;
use nucleus::social::requests::*;
use store::sqlx::Row;

pub(super) async fn enqueue(
    tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
    request: &PublicRequest,
    destinations: &[String],
) -> Result<(), EngineError> {
    let (kind, _, expiry) = response_identity(request)?;
    let identity = format!("{kind}:{}", document_hash("reply-work", request)?);
    store::social::enqueue_on(
        tx,
        kind,
        &identity,
        &serde_json::to_string(request)?,
        destinations,
        expiry,
    )
    .await?;
    Ok(())
}

pub(super) fn response_identity(
    request: &PublicRequest,
) -> Result<(&'static str, String, i64), EngineError> {
    Ok(match request {
        PublicRequest::RegisterReplyRoute { document, .. } => (
            "reply-route",
            document_hash("reply-route", document)?,
            document.expires_at,
        ),
        PublicRequest::UpdateReplyAuthority { document } => (
            "reply-control",
            document_hash("reply-owner", document)?,
            document.expires_at,
        ),
        PublicRequest::EndReplyPost { document } => (
            "reply-ending",
            document_hash("snippet", document)?,
            document.expires_at,
        ),
        PublicRequest::AdmitPrivateSender { document } => (
            "reply-admission",
            document_hash("sender-admission", document)?,
            document.expires_at,
        ),
        _ => return Err(invalid("Unsupported durable private reply work")),
    })
}

impl Engine {
    pub async fn social_private_access(
        &self,
        context: &str,
        envelopes: Vec<String>,
    ) -> Result<MailboxAccess, EngineError> {
        self.social_require_local_write().await?;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?
            .uid;
        let state =
            store::records::get_extension(&self.store.pool, context, SESSION_AUTHORITY_NAMESPACE)
                .await?
                .ok_or_else(|| invalid("Prepare this device's private reply keys first"))?;
        let binding: PrivateOwnerBinding = serde_json::from_value(state["binding"].clone())?;
        self.social_validate_private_binding(context, &binding)
            .await?;
        let route: CertifiedRoute =
            serde_json::from_value(state[format!("authorized_{cell}")].clone())?;
        let now = nucleus::execution::now().timestamp();
        request_auth::validate_route(&route, now)?;
        if route.control.owner_key != binding.owner_key
            || serde_json::to_value(&route.control)? != state["control"]
        {
            return Err(invalid(
                "Wait for current owner authorization before mailbox pickup",
            ));
        }
        let key = self.social_storage_key().await?;
        let id = format!("account:{context}");
        let mut tx = self.social_write_tx().await?;
        if owner::extension_on(&mut tx, context, SESSION_AUTHORITY_NAMESPACE).await? != state {
            return Err(invalid("Reply authorization changed before pickup"));
        }
        let (body, version): (String, i64) = store::sqlx::query_as(
            "SELECT body,version FROM social_device_state WHERE id=? AND kind='account'",
        )
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| invalid("This device's live reply account is unavailable"))?;
        let mut account: session::AccountState = session::open_local(&id, &body, &key)?;
        if account.route != route.route {
            return Err(invalid(
                "The pickup authority belongs to another live account",
            ));
        }
        account.pickup_counter = account
            .pickup_counter
            .checked_add(1)
            .ok_or_else(|| invalid("Pickup counter exhausted; provision fresh device keys"))?;
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce).map_err(|_| invalid("Secure randomness is unavailable"))?;
        let mut access = MailboxAccess {
            mailbox: route.route.mailbox.clone(),
            sequence: account.pickup_counter.to_string(),
            at: now,
            nonce: B64.encode(nonce),
            envelopes,
            control: route.control.clone(),
            certificate: route.certificate.clone(),
            signature: String::new(),
        };
        access.signature = account
            .pickup_key()?
            .sign_bytes(&signing_bytes("mailbox-access", &access)?);
        request_auth::validate_access(&access, &route, now)?;
        store::social::put_device_state_on(
            &mut tx,
            &id,
            "account",
            context,
            &session::seal_local(&id, &account, &key)?,
            Some(version),
            now,
        )
        .await?;
        tx.commit().await?;
        Ok(access)
    }

    pub async fn social_reconcile_reply_routes(&self) -> Result<usize, EngineError> {
        self.social_require_local_write().await?;
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        let rows = store::sqlx::query("SELECT e.record_uid,e.fds FROM record_extension e JOIN record r ON r.uid=e.record_uid LEFT JOIN social_context_retention t ON t.context=e.record_uid WHERE e.namespace=? AND r.organ_uid=? AND COALESCE(t.state,'active') NOT IN ('retired','review') ORDER BY COALESCE(t.queue_checked_at,0),e.record_uid LIMIT 64")
            .bind(SESSION_AUTHORITY_NAMESPACE).bind(&organ.uid).fetch_all(&self.store.pool).await?;
        let now = nucleus::execution::now().timestamp();
        let mut queued = 0;
        for row in rows {
            let context: String = row.get("record_uid");
            store::sqlx::query("INSERT INTO social_context_retention(context,state,checked_at,queue_checked_at) VALUES(?,'active',0,?) ON CONFLICT(context) DO UPDATE SET queue_checked_at=excluded.queue_checked_at")
                .bind(&context).bind(now).execute(&self.store.pool).await?;
            let state: Value = serde_json::from_str(&row.get::<String, _>("fds"))?;
            let result = self
                .social_queue_reply_context(&context, &state, &cell.uid, now)
                .await;
            match result {
                Ok(count) => queued += count,
                Err(error) => {
                    tracing::debug!(%error, "This reply context still needs registration or owner authorization")
                }
            }
        }
        Ok(queued)
    }

    async fn social_queue_reply_context(
        &self,
        context: &str,
        state: &Value,
        cell: &str,
        now: i64,
    ) -> Result<usize, EngineError> {
        let binding: PrivateOwnerBinding = serde_json::from_value(state["binding"].clone())?;
        self.social_validate_private_binding(context, &binding)
            .await?;
        let services: Vec<String> = serde_json::from_value(state["services"].clone())?;
        if services.is_empty()
            || services.len() > 8
            || services
                .iter()
                .any(|id| id.parse::<iroh::EndpointId>().is_err())
        {
            return Err(invalid("Invalid selected private mailbox hosts"));
        }
        let publication =
            store::records::get_extension(&self.store.pool, context, PUBLICATION_NAMESPACE)
                .await?
                .unwrap_or_else(|| json!({}));
        let post: Option<Snippet> = publication
            .get("published")
            .filter(|post| !post.is_null())
            .map(|post| serde_json::from_value(post.clone()))
            .transpose()?;
        let mut work = Vec::new();
        if let Ok(control) = serde_json::from_value::<OwnerControl>(state["control"].clone())
            && request_auth::validate_control(&control, now).is_ok()
            && control.owner_key == binding.owner_key
        {
            work.push(PublicRequest::UpdateReplyAuthority { document: control });
        }
        if let Ok(mut route) =
            serde_json::from_value::<CertifiedRoute>(state[format!("authorized_{cell}")].clone())
            && request_auth::validate_route(&route, now).is_ok()
            && route.control.owner_key == binding.owner_key
            && serde_json::to_value(&route.control)? == state["control"]
        {
            let active_post = post
                .as_ref()
                .filter(|post| {
                    post.state == PostState::Active
                        && validate_snippet(post, now).is_ok()
                        && post
                            .reply
                            .as_ref()
                            .is_some_and(|reply| reply.control.owner_key == binding.owner_key)
                })
                .cloned();
            if post.is_some() && active_post.is_none() {
                route.accepting_introductions = false;
                let key = self.social_storage_key().await?;
                let id = format!("account:{context}");
                let (body, _) = store::social::device_state(&self.store.pool, &id)
                    .await?
                    .ok_or_else(|| invalid("Missing live reply account"))?;
                let account: session::AccountState = session::open_local(&id, &body, &key)?;
                if account.route != route.route {
                    return Err(invalid("The closing route belongs to another live account"));
                }
                route.signature = account
                    .signing_key()?
                    .sign_bytes(&signing_bytes("reply-route", &route)?);
            }
            work.insert(
                0,
                PublicRequest::RegisterReplyRoute {
                    document: route,
                    post: active_post.map(Box::new),
                },
            );
        }
        if let Some(post) = post.filter(|post| {
            post.state == PostState::Withdrawn && validate_snippet(post, now).is_ok()
        }) {
            work.push(PublicRequest::EndReplyPost {
                document: Box::new(post),
            });
        }
        let mut tx = self.social_write_tx().await?;
        let current = owner::extension_on(&mut tx, context, SESSION_AUTHORITY_NAMESPACE).await?;
        if current != *state {
            return Err(invalid(
                "Reply authority changed while preparing mailbox work",
            ));
        }
        for request in &work {
            enqueue(&mut tx, request, &services).await?;
        }
        tx.commit().await?;
        Ok(work.len())
    }
}
