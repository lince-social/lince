use super::*;
use n0_future::{BufferedStreamExt, StreamExt};
use nucleus::social::requests::EnvelopePurpose;
use std::borrow::Cow;
use store::sqlx::Row;

#[derive(serde::Deserialize)]
struct RequestHint<'a> {
    #[serde(borrow)]
    request: Cow<'a, str>,
}

#[derive(serde::Deserialize)]
struct DocumentHint<T> {
    document: T,
}

#[derive(serde::Deserialize)]
struct StateHint {
    state: PostState,
}

#[derive(serde::Deserialize)]
struct EnvelopeHint {
    envelope: PurposeHint,
}

#[derive(serde::Deserialize)]
struct PurposeHint {
    purpose: EnvelopePurpose,
}

fn hinted_budget_class(bytes: &[u8], verb: &str) -> store::social::BudgetClass {
    let control = match verb {
        "publish-snippet" | "end-reply-post" if bytes.len() <= MAX_FRAME_BYTES => {
            serde_json::from_slice::<DocumentHint<StateHint>>(bytes)
                .is_ok_and(|hint| hint.document.state != PostState::Active)
        }
        "publish-authority"
        | "publish-posting-authority"
        | "update-reply-authority"
        | "admit-private-sender"
        | "collect-private"
        | "acknowledge-private"
        | "discard-private"
        | "inspect-private" => true,
        "deliver-private" => serde_json::from_slice::<DocumentHint<EnvelopeHint>>(bytes)
            .is_ok_and(|hint| hint.document.envelope.purpose == EnvelopePurpose::Control),
        _ => false,
    };
    if control {
        store::social::BudgetClass::Control
    } else {
        store::social::BudgetClass::Ordinary
    }
}

fn optional_rows<'a>(data: &'a Value, key: &str) -> Result<&'a [Value], EngineError> {
    data.get(key)
        .map(|value| {
            value
                .as_array()
                .filter(|rows| rows.len() <= 50)
                .map(Vec::as_slice)
                .ok_or_else(|| invalid("Invalid bounded directory control array"))
        })
        .transpose()
        .map(|rows| rows.unwrap_or_default())
}

fn budget_class(request: &PublicRequest) -> store::social::BudgetClass {
    let control = match request {
        PublicRequest::PublishSnippet { document } => document.state != PostState::Active,
        PublicRequest::EndReplyPost { document } => document.state != PostState::Active,
        PublicRequest::PublishAuthority { .. }
        | PublicRequest::PublishPostingAuthority { .. }
        | PublicRequest::UpdateReplyAuthority { .. }
        | PublicRequest::AdmitPrivateSender { .. }
        | PublicRequest::CollectPrivate { .. }
        | PublicRequest::AcknowledgePrivate { .. }
        | PublicRequest::DiscardPrivate { .. }
        | PublicRequest::InspectPrivate { .. } => true,
        PublicRequest::DeliverPrivate { document } => {
            document.envelope.purpose == EnvelopePurpose::Control
        }
        _ => false,
    };
    if control {
        store::social::BudgetClass::Control
    } else {
        store::social::BudgetClass::Ordinary
    }
}

async fn directory_search(
    network: Option<std::sync::Arc<dyn Network>>,
    service: String,
    request: PublicRequest,
) -> (String, Result<Value, EngineError>) {
    let response = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        network
            .ok_or_else(|| invalid("Waiting for the network connection"))?
            .request(&service, request)
            .await
    })
    .await
    .unwrap_or_else(|_| Err(invalid("The directory search deadline ended")));
    (service, response)
}

impl Engine {
    pub async fn social_settings(&self) -> Result<ServiceSettings, EngineError> {
        let mut tx = self.store.pool.begin().await?;
        let settings = self.social_settings_on(&mut tx).await?;
        tx.commit().await?;
        Ok(settings)
    }

    pub(super) async fn social_settings_on(
        &self,
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
    ) -> Result<ServiceSettings, EngineError> {
        if let Some(settings) = self
            .social_deployment
            .lock()
            .map_err(|_| invalid("Cannot read deployment hosting policy"))?
            .clone()
        {
            return Ok(settings);
        }
        let cell: String = store::sqlx::query_scalar(
            "SELECT uid FROM record WHERE slug='local-cell' AND kind='device' LIMIT 1",
        )
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| invalid("No local Cell"))?;
        let body: Option<String> = store::sqlx::query_scalar("SELECT fds FROM record_extension WHERE record_uid=? AND namespace='lince.social.services'").bind(cell).fetch_optional(&mut **tx).await?;
        let settings = body
            .map(|body| serde_json::from_str(&body))
            .transpose()?
            .unwrap_or_default();
        Ok(settings)
    }

    pub(super) async fn social_configure_services(
        &self,
        settings: &ServiceSettings,
    ) -> Result<(), EngineError> {
        if self.social_services_managed() {
            return Err(invalid(
                "Hosting roles are managed by this server's deployment configuration. Update that configuration and restart the service",
            ));
        }
        descriptor::validate_settings(settings)?;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        let mut tx = self.social_write_tx().await?;
        store::records::set_extension_on(
            &mut tx,
            &cell.uid,
            "lince.social.services",
            &serde_json::to_value(settings)?,
        )
        .await?;
        tx.commit().await?;
        self.notify_config_changed();
        Ok(())
    }

    pub async fn social_public_request(
        &self,
        source: &str,
        node_id: &str,
        request: PublicRequest,
        now: i64,
    ) -> Result<Value, EngineError> {
        self.social_public_frame(source, node_id, &serde_json::to_vec(&request)?, now)
            .await
    }

    pub async fn social_public_frame(
        &self,
        source: &str,
        node_id: &str,
        bytes: &[u8],
        now: i64,
    ) -> Result<Value, EngineError> {
        let settings = self.social_settings().await?;
        let request_bytes = bytes.len();
        if request_bytes > MAX_PRIVATE_FRAME_BYTES {
            return Err(invalid("The social request is too large"));
        }
        let hint = serde_json::from_slice::<RequestHint<'_>>(bytes);
        let class = hint
            .as_ref()
            .map(|hint| hinted_budget_class(bytes, &hint.request))
            .unwrap_or(store::social::BudgetClass::Ordinary);
        store::social::spend_class(
            &self.store.pool,
            source,
            "in",
            request_bytes,
            settings.incoming_bytes_per_minute,
            now,
            class,
        )
        .await?;
        let allowed = match hint.as_ref().map(|hint| hint.request.as_ref()) {
            Ok("deliver-private" | "inspect-private" | "collect-private") => {
                MAX_PRIVATE_FRAME_BYTES
            }
            _ => MAX_FRAME_BYTES,
        };
        if request_bytes > allowed {
            return Err(invalid(
                "The social request exceeds its operation size limit",
            ));
        }
        let request: PublicRequest = serde_json::from_slice(bytes)?;
        let class = budget_class(&request);
        let frame_limit = request.frame_limit();
        if request_bytes > frame_limit {
            return Err(invalid(
                "The social request exceeds its operation size limit",
            ));
        }
        store::social::prune(&self.store.pool, now).await?;
        if !settings.directory
            && !settings.townsquare
            && !settings.mailbox
            && !matches!(
                request,
                PublicRequest::DescribeService
                    | PublicRequest::GossipOffer { .. }
                    | PublicRequest::GossipDeliver { .. }
                    | PublicRequest::AskContacts { .. }
            )
        {
            return Err(invalid("This Cell has not enabled public social services"));
        }
        let response = match request {
            PublicRequest::DescribeService => {
                json!({"descriptor":descriptor::describe(node_id,&settings,now)?})
            }
            PublicRequest::SubmitReport { document } => {
                self.social_receive_report(source, node_id, &document, now)
                    .await?
            }
            PublicRequest::GossipOffer { offer } => {
                self.social_gossip_offer(source, node_id, offer, now)
                    .await?
            }
            PublicRequest::GossipDeliver { payload } => {
                self.social_gossip_deliver(source, node_id, &payload, now)
                    .await?
            }
            PublicRequest::AskContacts { document } => {
                self.social_ask_receive(source, node_id, *document, now)
                    .await?
            }
            PublicRequest::PublishPostingAuthority { document } => {
                if !settings.directory
                    || document.destinations.len() > 8
                    || !document
                        .destinations
                        .iter()
                        .any(|destination| destination == node_id)
                {
                    return Err(invalid(
                        "This host was not selected for anonymous posting authority",
                    ));
                }
                posting::validate_authority(&document.authority, now)?;
                let mut tx = self.social_write_tx().await?;
                self.social_capacity(
                    &mut tx,
                    settings.cache_entries,
                    settings.storage_bytes,
                    &document.authority.owner_key,
                    "posting",
                    0,
                )
                .await?;
                store::social::anchor_posting_authority_on(&mut tx, &document.authority).await?;
                tx.commit().await?;
                json!({"accepted":true,"service":node_id,"hash":document_hash("posting-control", &document)?,"expires_at":document.authority.expires_at})
            }
            request @ (PublicRequest::UpdateReplyAuthority { .. }
            | PublicRequest::LookupReplyRoutes { .. }
            | PublicRequest::InspectPrivate { .. }
            | PublicRequest::RegisterReplyRoute { .. }
            | PublicRequest::EndReplyPost { .. }
            | PublicRequest::AdmitPrivateSender { .. }
            | PublicRequest::DeliverPrivate { .. }
            | PublicRequest::CollectPrivate { .. }
            | PublicRequest::DiscardPrivate { .. }
            | PublicRequest::AcknowledgePrivate { .. }) => {
                if !settings.mailbox {
                    return Err(invalid("Private mailbox hosting is disabled"));
                }
                self.social_private_service(node_id, request, &settings, now)
                    .await?
            }
            PublicRequest::PublishAuthority { document } => {
                if !settings.directory
                    || document.destinations.len() > 8
                    || !document.destinations.iter().any(|id| id == node_id)
                {
                    return Err(invalid(
                        "This service was not selected to retain public editing authority",
                    ));
                }
                profile::validate_delegation(&document.authority, now)?;
                let hash = document_hash("authority", &document)?;
                let mut tx = self.social_write_tx().await?;
                self.social_capacity(
                    &mut tx,
                    settings.cache_entries,
                    settings.storage_bytes,
                    &document.authority.organ,
                    "profile",
                    0,
                )
                .await?;
                store::social::anchor_profile_authority_on(&mut tx, &document.authority, false)
                    .await?;
                tx.commit().await?;
                json!({"accepted":true,"service":node_id,"hash":hash,"revision":document.authority.generation,"expires_at":document.authority.expires_at})
            }
            PublicRequest::PublishProfileImage { document } => {
                if !settings.directory {
                    return Err(invalid("Public image hosting is disabled"));
                }
                self.social_publish_asset(
                    node_id,
                    document.profile,
                    document.hash,
                    document.encoded,
                    now,
                )
                .await?
            }
            PublicRequest::FetchProfileImage { organ, hash } => {
                self.social_fetch_asset(&organ, &hash, node_id, now).await?
            }
            PublicRequest::PublishSnippet { document } => {
                if !settings.directory {
                    return Err(invalid("Directory publication is disabled"));
                }
                validate_snippet(&document, now)?;
                source
                    .parse::<iroh::EndpointId>()
                    .map_err(|_| invalid("Invalid authenticated service peer"))?;
                let their_allowed = document.destinations.iter().any(|id| id == node_id);
                if !their_allowed {
                    return Err(invalid(
                        "The author did not select this publication service",
                    ));
                }
                let hash = document_hash("snippet", &document)?;
                let mut tx = self.social_write_tx().await?;
                self.social_cache_snippet_on(
                    &mut tx,
                    &document,
                    &hash,
                    "directory",
                    &settings,
                    now,
                )
                .await?;
                let receipt =
                    Self::social_receipt(&mut tx, "snippet", &document.id, &hash, node_id).await?;
                tx.commit().await?;
                receipt
            }
            PublicRequest::PublishProfile { document } => {
                if !settings.directory {
                    return Err(invalid("Public profile hosting is disabled"));
                }
                profile::validate_profile(&document, now)?;
                if !document.destinations.iter().any(|id| id == node_id) {
                    return Err(invalid("The author did not select this profile host"));
                }
                let hash = document_hash("profile", &document)?;
                let mut tx = self.social_write_tx().await?;
                self.social_capacity(
                    &mut tx,
                    settings.cache_entries,
                    settings.storage_bytes,
                    &document.authority.organ,
                    "profile",
                    serde_json::to_vec(&document)?.len(),
                )
                .await?;
                store::social::put_profile_on(&mut tx, &document, &hash, "profile host").await?;
                let receipt = Self::social_receipt(
                    &mut tx,
                    "profile",
                    &document.authority.organ,
                    &hash,
                    node_id,
                )
                .await?;
                tx.commit().await?;
                receipt
            }
            PublicRequest::Search { query, known } => {
                if !settings.townsquare && (!settings.directory || query.text.trim().is_empty()) {
                    return Err(invalid("Public browsing and search are disabled"));
                }
                validate_search(&query)?;
                if known.len() > 50 {
                    return Err(invalid("Refresh at most fifty known announcement hashes"));
                }
                let mut updates = Vec::new();
                let mut authorities = std::collections::BTreeMap::<String, Delegation>::new();
                let mut posting_authorities =
                    std::collections::BTreeMap::<String, PostingAuthority>::new();
                let mut updated_ids = std::collections::HashSet::new();
                for held in known {
                    let (id, hash) = held
                        .split_once(':')
                        .ok_or_else(|| invalid("Invalid known announcement hash"))?;
                    if !nucleus::valid_uid(id, "post")
                        || hash.len() != 64
                        || !hash.bytes().all(|b| b.is_ascii_hexdigit())
                    {
                        return Err(invalid("Invalid known announcement hash"));
                    }
                    if let Some(body) = store::sqlx::query_scalar::<_, String>("SELECT p.body FROM social_profile_authority p JOIN social_document d ON p.organ=json_extract(d.body,'$.profile.organ') WHERE p.generation>0 AND d.kind='snippet' AND d.id=?")
                        .bind(id).fetch_optional(&self.store.pool).await? {
                        let authority: Delegation = serde_json::from_str(&body)?;
                        authorities.insert(authority.organ.clone(), authority);
                    }
                    if let Some(body) = store::sqlx::query_scalar::<_, String>("SELECT p.body FROM social_posting_authority p JOIN social_document d ON p.owner=json_extract(d.body,'$.anonymous.owner_key') WHERE d.kind='snippet' AND d.id=?")
                        .bind(id).fetch_optional(&self.store.pool).await? {
                        let authority: PostingAuthority = serde_json::from_str(&body)?;
                        posting_authorities.insert(authority.owner_key.clone(), authority);
                    }
                    if let Some(row) = store::sqlx::query("SELECT body,hash FROM social_document WHERE kind='snippet' AND state NOT IN ('revoked','conflict') AND (state='withdrawn' OR NOT EXISTS(SELECT 1 FROM social_listing_removal WHERE post=social_document.id)) AND id=? AND hash<>? AND expires_at>? AND (EXISTS(SELECT 1 FROM json_each(body,'$.destinations') WHERE value=?) OR (json_extract(body,'$.redistribute')=1 AND json_array_length(body,'$.destinations')>0))")
                        .bind(id).bind(hash).bind(now).bind(node_id).fetch_optional(&self.store.pool).await? {
                        updates.push(json!({"document":serde_json::from_str::<Value>(&row.get::<String,_>("body"))?,"hash":row.get::<String,_>("hash"),"source":"directory"}));
                        updated_ids.insert(id.to_owned());
                    }
                }
                let mut results =
                    store::social::search_public(&self.store.pool, &query, now, node_id).await?;
                results.retain(|row| {
                    !row["document"]["id"]
                        .as_str()
                        .is_some_and(|id| updated_ids.contains(id))
                });
                results.truncate(50usize.saturating_sub(updates.len()));
                let mut authorities: Vec<_> = authorities.into_values().collect();
                let mut posting_authorities: Vec<_> = posting_authorities.into_values().collect();
                let mut refresh_incomplete = false;
                while serde_json::to_vec(
                    &json!({"results":results,"updates":updates,"authorities":authorities,"posting_authorities":posting_authorities,"refresh_incomplete":refresh_incomplete}),
                )?
                .len()
                    > MAX_FRAME_BYTES - 1024
                {
                    refresh_incomplete = true;
                    if results.pop().is_none() && updates.pop().is_none()
                        && authorities.pop().is_none() && posting_authorities.pop().is_none()
                    {
                        return Err(invalid("The directory reply cannot fit its frame bound"));
                    }
                }
                json!({"results":results,"updates":updates,"authorities":authorities,"posting_authorities":posting_authorities,"refresh_incomplete":refresh_incomplete})
            }
            PublicRequest::FetchProfile { organ } => {
                if !nucleus::valid_uid(&organ, "r") {
                    return Err(invalid("Invalid public Organ identity"));
                }
                let row=store::sqlx::query("SELECT body,state FROM social_document WHERE kind='profile' AND id=? AND expires_at>?").bind(&organ).bind(now).fetch_optional(&self.store.pool).await?;
                match row {
                    Some(row) => {
                        let doc: Profile = serde_json::from_str(&row.get::<String, _>("body"))?;
                        if doc.destinations.iter().any(|host| host == node_id) {
                            json!({"profile":doc,"state":row.get::<String,_>("state")})
                        } else {
                            json!({"profile":null,"state":"unavailable"})
                        }
                    }
                    None => json!({"profile":null,"state":"unavailable"}),
                }
            }
        };
        let response_bytes = serde_json::to_vec(&response)?.len();
        if response_bytes > frame_limit - 1024 {
            return Err(invalid("The social response is too large"));
        }
        store::social::spend_class(
            &self.store.pool,
            source,
            "out",
            response_bytes,
            settings.outgoing_bytes_per_minute,
            now,
            class,
        )
        .await?;
        Ok(response)
    }

    pub(super) async fn social_cache_snippet_on(
        &self,
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        document: &Snippet,
        hash: &str,
        source: &str,
        settings: &ServiceSettings,
        now: i64,
    ) -> Result<bool, EngineError> {
        if discovery_sources::quarantine_on(tx, document, hash, settings, now).await? {
            discovery_sources::observe_on(tx, document, hash, source, settings, now).await?;
            return Ok(false);
        }
        let duplicate: bool = store::sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM social_document WHERE kind='snippet' AND id=? AND hash=?)",
        )
        .bind(&document.id)
        .bind(hash)
        .fetch_one(&mut **tx)
        .await?;
        if document.state != PostState::Withdrawn && !duplicate {
            self.social_capacity(
                tx,
                settings.cache_entries,
                settings.storage_bytes,
                &document.id,
                "snippet",
                serde_json::to_vec(document)?.len(),
            )
            .await?;
        }
        let changed = store::social::put_snippet_on(tx, document, hash, source, now).await?;
        if changed {
            discovery_sources::advance_on(tx, document, hash).await?;
        }
        discovery_sources::observe_on(tx, document, hash, source, settings, now).await?;
        self.social_capacity(
            tx,
            settings.cache_entries,
            settings.storage_bytes,
            &document.id,
            "snippet",
            0,
        )
        .await?;
        Ok(changed)
    }

    pub(super) async fn social_capacity(
        &self,
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        limit: u32,
        bytes: u64,
        id: &str,
        kind: &str,
        added: usize,
    ) -> Result<(), EngineError> {
        let known: bool = store::sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM social_document WHERE kind=?1 AND id=?2 UNION ALL SELECT 1 FROM social_ended_post WHERE ?1='snippet' AND id=?2 UNION ALL SELECT 1 FROM social_profile_authority WHERE ?1='profile' AND organ=?2 UNION ALL SELECT 1 FROM social_posting_authority WHERE ?1='posting' AND owner=?2)",
        )
        .bind(kind)
        .bind(id)
        .fetch_one(&mut **tx)
        .await?;
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM (SELECT kind,id FROM social_document UNION SELECT 'snippet',id FROM social_ended_post UNION SELECT 'profile',organ FROM social_profile_authority UNION SELECT 'posting',owner FROM social_posting_authority)")
            .fetch_one(&mut **tx)
            .await?;
        let held: i64 = store::sqlx::query_scalar(
            "SELECT (SELECT COALESCE(SUM(length(CAST(body AS BLOB))),0)*4 FROM social_revision)+(SELECT COUNT(*)*8192 FROM social_profile_authority)+(SELECT COUNT(*)*16384 FROM social_posting_authority)+(SELECT COUNT(*)*512 FROM social_ended_post)+(SELECT COUNT(*)*1024 FROM social_discovery_source)+(SELECT COALESCE(SUM(length(CAST(first_body AS BLOB))+length(CAST(second_body AS BLOB))),0)*4 FROM social_discovery_conflict)",
        )
        .fetch_one(&mut **tx)
        .await?;
        let reserve = if known {
            0
        } else if kind == "posting" {
            16384
        } else if kind == "profile" {
            8192
        } else {
            512
        };
        if count > limit as i64
            || !known && count >= limit as i64
            || held.saturating_add(added as i64 * 4 + reserve) as u64 > bytes / 2
        {
            return Err(invalid("The public service cache is full"));
        }
        Ok(())
    }

    pub(super) async fn social_fetch_profile(
        &self,
        organ: &str,
        services: Vec<String>,
        now: i64,
    ) -> Result<Value, EngineError> {
        if !nucleus::valid_uid(organ, "r") || services.len() > 8 {
            return Err(invalid(
                "Choose a public Organ identity and at most eight profile hosts",
            ));
        }
        let mut failures = Vec::new();
        for service in services {
            let result = async {
                let reply = self
                    .social_network()?
                    .request(
                        &service,
                        PublicRequest::FetchProfile {
                            organ: organ.into(),
                        },
                    )
                    .await?;
                let doc: Profile = serde_json::from_value(reply["profile"].clone())?;
                profile::validate_profile(&doc, now)?;
                if doc.authority.organ != organ {
                    return Err(invalid("The profile host returned another identity"));
                }
                if let Some(anchor) =
                    crate::trust::key_of(&self.store, organ, crate::roster::ROOT_KEY_ID).await?
                    && anchor != doc.authority.root_key
                    && !self.key_chains(organ, &doc.authority.root_key).await?
                {
                    return Err(invalid(
                        "The hosted profile does not match the known Organ key",
                    ));
                }
                let hash = document_hash("profile", &doc)?;
                let settings = self.social_settings().await?;
                let mut tx = self.social_write_tx().await?;
                self.social_capacity(
                    &mut tx,
                    settings.cache_entries,
                    settings.storage_bytes,
                    organ,
                    "profile",
                    serde_json::to_vec(&doc)?.len(),
                )
                .await?;
                store::social::put_profile_on(&mut tx, &doc, &hash, &service).await?;
                tx.commit().await?;
                Ok::<_, EngineError>(())
            }
            .await;
            if result.is_err() {
                failures.push(json!({"service":service,"error":"Profile unavailable or authority could not be verified"}));
            }
        }
        let row = store::sqlx::query("SELECT body,state,source,hash FROM social_document WHERE kind='profile' AND id=? AND expires_at>?").bind(organ).bind(now).fetch_optional(&self.store.pool).await?;
        Ok(match row {
            Some(row) => {
                json!({"profile":serde_json::from_str::<Value>(&row.get::<String,_>("body"))?,"state":row.get::<String,_>("state"),"source":row.get::<String,_>("source"),"hash":row.get::<String,_>("hash"),"failures":failures,"status":"Signed cached profile; identity is unverified until you recognize its key"})
            }
            None => {
                json!({"profile":null,"state":"unavailable","failures":failures,"status":"No current public profile is available"})
            }
        })
    }

    async fn social_receipt(
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        kind: &str,
        id: &str,
        expected: &str,
        node_id: &str,
    ) -> Result<Value, EngineError> {
        let row = store::sqlx::query(
            "SELECT hash,revision,expires_at,state FROM social_document WHERE kind=? AND id=?",
        )
        .bind(kind)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?;
        let Some(row) = row else {
            if kind == "snippet" {
                let ended: Option<(String, i64)> =
                    store::sqlx::query_as("SELECT hash,revision FROM social_ended_post WHERE id=?")
                        .bind(id)
                        .fetch_optional(&mut **tx)
                        .await?;
                if let Some((hash, revision)) = ended {
                    return Ok(
                        json!({"accepted":false,"service":node_id,"id":id,"hash":hash,"revision":revision.to_string(),"expires_at":0,"state":"withdrawn"}),
                    );
                }
            }
            return Err(invalid("No retained publication receipt is available"));
        };
        let hash: String = row.get("hash");
        let state: String = row.get("state");
        Ok(
            json!({"accepted":hash == expected && state != "conflict","service":node_id,"id":id,"hash":hash,"revision":row.get::<i64,_>("revision").to_string(),"expires_at":row.get::<i64,_>("expires_at"),"state":state}),
        )
    }

    pub(super) async fn social_search(
        &self,
        query: Search,
        services: Vec<String>,
        now: i64,
    ) -> Result<Value, EngineError> {
        validate_search(&query)?;
        if services.len() > 8 {
            return Err(invalid("Search at most eight chosen services"));
        }
        let mut failures = Vec::new();
        let known = if services.is_empty() {
            Vec::new()
        } else {
            self.social_discovery_known_refs(&query, now).await?
        };
        for service in &services {
            service
                .parse::<iroh::EndpointId>()
                .map_err(|_| invalid("Choose a valid directory endpoint ID"))?;
        }
        let network = self.social_network().ok();
        let searches: Vec<_> = services
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .map(|service| {
                directory_search(
                    network.clone(),
                    service,
                    PublicRequest::Search {
                        query: query.clone(),
                        known: known.clone(),
                    },
                )
            })
            .collect();
        let mut requests = n0_future::stream::iter(searches).buffered_unordered(3);
        while let Some((service, response)) = requests.next().await {
            let accepted = match response {
                Ok(data) => {
                    let result = self.social_cache_search(&service, &data, now).await;
                    if result.is_ok() && data["refresh_incomplete"] == true {
                        failures.push(json!({"service":service,"error":"This service returned a bounded partial page; cached results may be missing newer revisions or authority controls"}));
                    }
                    result
                }
                Err(error) => Err(error),
            };
            if accepted.is_err() {
                failures.push(json!({"service":service,"error":"This selected service is unavailable or returned an invalid result page"}));
            }
        }
        let mut results = store::social::search(&self.store.pool, &query, now).await?;
        let next = results
            .last()
            .and_then(|row| row["document"]["id"].as_str())
            .map(str::to_owned);
        self.social_filter_local_results(&mut results).await?;
        self.social_discovery_details(&mut results).await?;
        discovery_sources::rank_page(&query, &mut results);
        let (conflicts, next_conflict_after) = self.social_discovery_conflicts(&query).await?;
        Ok(
            json!({"results":results,"conflicts":conflicts,"next_conflict_after":next_conflict_after,"failures":failures,"query":query,"services":services,"next_after":next,"ranking":"Within this bounded page: matching words in the title/text, then author revision freshness. Page continuation retains its original ID cursor. Contact routes do not prove physical nearness","freshness":"Last checked means a valid signed copy was observed; it does not renew expiry or confirm the author's availability. Source counts are copies, not independent votes"}),
        )
    }

    async fn social_cache_search(
        &self,
        source: &str,
        data: &Value,
        now: i64,
    ) -> Result<(), EngineError> {
        self.social_cache_search_checked(source, data, now, None)
            .await
    }

    pub(super) async fn social_cache_search_checked(
        &self,
        source: &str,
        data: &Value,
        now: i64,
        guard: Option<&subscriptions::Guard>,
    ) -> Result<(), EngineError> {
        if serde_json::to_vec(data)?.len() > MAX_FRAME_BYTES - 1024 {
            return Err(invalid("The directory result page is too large"));
        }
        let rows = data["results"]
            .as_array()
            .filter(|rows| rows.len() <= 50)
            .ok_or_else(|| invalid("Invalid directory result page"))?;
        let updates = optional_rows(data, "updates")?;
        if rows.len() + updates.len() > 50 {
            return Err(invalid(
                "The directory page exceeded its aggregate result count",
            ));
        }
        let mut validated = Vec::new();
        for row in rows.iter().chain(updates.iter()) {
            let document: Snippet = serde_json::from_value(row["document"].clone())?;
            validate_snippet(&document, now)?;
            let hash = document_hash("snippet", &document)?;
            if row["hash"].as_str() != Some(hash.as_str()) {
                return Err(invalid(
                    "The directory result hash differs from its signed document",
                ));
            }
            validated.push((document, hash));
        }
        let settings = self.social_settings().await?;
        let mut tx = self.social_write_tx().await?;
        if let Some(guard) = guard
            && !self.social_subscription_current_on(&mut tx, guard).await?
        {
            return Err(invalid(
                "The saved filter or device permission changed before result import",
            ));
        }
        let posting_authorities = optional_rows(data, "posting_authorities")?;
        for value in posting_authorities {
            let authority: PostingAuthority = serde_json::from_value(value.clone())?;
            posting::validate_authority(&authority, authority.issued_at)?;
            let known: bool = store::sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM social_posting_authority WHERE owner=?)",
            )
            .bind(&authority.owner_key)
            .fetch_one(&mut *tx)
            .await?;
            if !known
                && !validated.iter().any(|(doc, _)| {
                    doc.anonymous
                        .as_ref()
                        .is_some_and(|own| own.owner_key == authority.owner_key)
                })
            {
                return Err(invalid(
                    "The directory sent unrelated anonymous editing authority",
                ));
            }
            self.social_capacity(
                &mut tx,
                settings.cache_entries,
                settings.storage_bytes,
                &authority.owner_key,
                "posting",
                0,
            )
            .await?;
            store::social::anchor_posting_authority_on(&mut tx, &authority).await?;
        }
        let authorities = optional_rows(data, "authorities")?;
        for value in authorities {
            let authority: Delegation = serde_json::from_value(value.clone())?;
            profile::validate_delegation(&authority, authority.issued_at)?;
            let cached: Option<String> = store::sqlx::query_scalar(
                "SELECT root_key FROM social_profile_authority WHERE organ=?",
            )
            .bind(&authority.organ)
            .fetch_optional(&mut *tx)
            .await?;
            let expected = cached.or_else(|| {
                validated.iter().find_map(|(doc, _)| {
                    doc.profile
                        .as_ref()
                        .filter(|profile| profile.organ == authority.organ)
                        .map(|profile| profile.root_key.clone())
                })
            });
            let Some(expected) = expected else {
                return Err(invalid(
                    "The directory supplied authority for an unrelated identity",
                ));
            };
            if authority.root_key != expected
                && !authority
                    .successions
                    .iter()
                    .any(|change| change.old_key == expected)
            {
                return Err(invalid(
                    "The directory changed a known identity without signed succession",
                ));
            }
            self.social_capacity(
                &mut tx,
                settings.cache_entries,
                settings.storage_bytes,
                &authority.organ,
                "profile",
                0,
            )
            .await?;
            store::social::anchor_profile_authority_on(&mut tx, &authority, false).await?;
        }
        for (document, hash) in validated {
            if let Some(authority) = &document.anonymous {
                let floor: Option<i64> = store::sqlx::query_scalar(
                    "SELECT generation FROM social_posting_authority WHERE owner=?",
                )
                .bind(&authority.owner_key)
                .fetch_optional(&mut *tx)
                .await?;
                if floor.is_some_and(|floor| {
                    authority
                        .generation
                        .parse::<i64>()
                        .ok()
                        .is_none_or(|generation| floor > generation)
                }) {
                    continue;
                }
            }
            self.social_cache_snippet_on(&mut tx, &document, &hash, source, &settings, now)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn social_publish_once(&self) -> Result<usize, EngineError> {
        let now = nucleus::execution::now().timestamp();
        self.social_require_local_write().await?;
        if let Err(error) = self.social_reconcile_reply_routes().await {
            tracing::debug!(%error, "Private reply hosting remains queued");
        }
        self.social_refresh_editor_members().await?;
        if let Err(error) = self.social_publish_profile_drafts().await {
            tracing::debug!(%error, "Pending profile edits remain saved");
        }
        self.social_refresh_posting_authorities().await?;
        if let Err(error) = self.social_renew_profile().await {
            tracing::debug!(%error, "Public profile renewal will retry");
        }
        store::social::prune(&self.store.pool, now).await?;
        store::sqlx::query("DELETE FROM social_public_asset WHERE touched_at<? AND NOT EXISTS(SELECT 1 FROM social_document WHERE kind='profile' AND expires_at>? AND (json_extract(body,'$.fields.avatar')=social_public_asset.hash OR json_extract(body,'$.fields.banner')=social_public_asset.hash))")
            .bind(now - MAX_LIFETIME - 600).bind(now - 600).execute(&self.store.pool).await?;
        self.social_refresh_sources().await?;
        self.social_reconcile_publications(now).await?;
        let network = self.social_network()?;
        let mut accepted = 0;
        for job in store::social::due_publications(&self.store.pool, now).await? {
            match self.social_require_local_write().await {
                Ok(()) => {}
                Err(EngineError::Forbidden(_)) => break,
                Err(error) => return Err(error),
            }
            let request = match job.kind.as_str() {
                "reply-route" | "reply-control" | "reply-ending" | "reply-admission" => {
                    serde_json::from_str(&job.body)
                }
                "snippet" => serde_json::from_str(&job.body)
                    .map(|document| PublicRequest::PublishSnippet { document }),
                "profile" => serde_json::from_str(&job.body)
                    .map(|document| PublicRequest::PublishProfile { document }),
                "authority" => serde_json::from_str(&job.body)
                    .map(|document| PublicRequest::PublishAuthority { document }),
                "posting-authority" => serde_json::from_str(&job.body)
                    .map(|document| PublicRequest::PublishPostingAuthority { document }),
                "image" => serde_json::from_str(&job.body)
                    .map(|document| PublicRequest::PublishProfileImage { document }),
                _ => {
                    store::social::fail_publication(
                        &self.store.pool,
                        &job,
                        "Unsupported saved publication job",
                    )
                    .await?;
                    continue;
                }
            };
            let request = match request {
                Ok(request) => request,
                Err(_) => {
                    store::social::fail_publication(
                        &self.store.pool,
                        &job,
                        "Malformed saved publication job",
                    )
                    .await?;
                    continue;
                }
            };
            if !self.social_publication_is_current(&job, &request).await? {
                store::social::fail_publication(
                    &self.store.pool,
                    &job,
                    "This publication used obsolete authority or was superseded; review the current version",
                )
                .await?;
                continue;
            }
            let expected_hash = if job.kind.starts_with("reply-") {
                let (kind, hash, expiry) = mailbox_client::response_identity(&request)?;
                if kind != job.kind || expiry != job.expires_at {
                    store::social::fail_publication(
                        &self.store.pool,
                        &job,
                        "Saved reply work changed its contract",
                    )
                    .await?;
                    continue;
                }
                hash
            } else {
                job.hash.clone()
            };
            let reply = network.request(&job.destination, request.clone()).await;
            if reply
                .as_ref()
                .is_ok_and(|reply| reply["accepted"] == false && reply["hash"].is_string())
            {
                store::social::fail_publication(&self.store.pool, &job, "Host retained a newer or conflicting revision; refresh and resolve before publishing").await?;
                continue;
            }
            let success = reply.is_ok_and(|reply| {
                reply["accepted"] == true
                    && reply["service"] == job.destination
                    && reply["hash"] == expected_hash
                    && reply["expires_at"].as_i64() == Some(job.expires_at)
            });
            if self
                .social_finish_publication(&job, &request, &expected_hash, success, now)
                .await?
            {
                accepted += 1;
            }
        }
        Ok(accepted)
    }

    async fn social_reconcile_publications(&self, now: i64) -> Result<(), EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let authority =
            store::records::get_extension(&self.store.pool, &organ.uid, PRIVATE_NAMESPACE)
                .await?
                .unwrap_or_else(|| json!({}));
        let generation = authority["profile_signer"]["authority"]["generation"]
            .as_str()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0);
        let states = self.social_publications().await?;
        let mut documents = Vec::new();
        for (uid, state) in states {
            if self.social_own_record(&uid, None).await.is_err() {
                continue;
            }
            for control in state["anonymous_controls"].as_array().into_iter().flatten() {
                if let Ok(doc) =
                    serde_json::from_value::<PostingAuthorityPublication>(control.clone())
                    && posting::validate_authority(&doc.authority, now).is_ok()
                {
                    documents.push((
                        "posting-authority",
                        document_hash("posting-control", &doc)?,
                        serde_json::to_string(&doc)?,
                        doc.destinations,
                        doc.authority.expires_at,
                    ));
                }
            }
            let mut heads = std::collections::HashSet::new();
            let mut resolved = std::collections::HashSet::new();
            for (key, doc) in state.as_object().into_iter().flatten() {
                if let Some(hash) = key.strip_prefix("revision_") {
                    heads.insert(hash);
                    if let Some(parent) = doc["parent"].as_str() {
                        resolved.insert(parent);
                    }
                    resolved.extend(
                        doc["resolves"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str),
                    );
                }
            }
            heads.retain(|head| !resolved.contains(head));
            if heads.len() > 1 {
                continue;
            }
            let Ok(doc) = serde_json::from_value::<Snippet>(state["published"].clone()) else {
                continue;
            };
            if validate_snippet(&doc, now).is_err() {
                continue;
            }
            if doc.anonymous.as_ref().is_some_and(|authority| {
                state["anonymous_authority"]["generation"].as_str()
                    != Some(authority.generation.as_str())
            }) {
                continue;
            }
            if doc.state != PostState::Withdrawn
                && doc.profile.as_ref().is_some_and(|authority| {
                    authority
                        .generation
                        .parse::<i64>()
                        .ok()
                        .is_none_or(|held| held < generation)
                })
            {
                continue;
            }
            documents.push((
                "snippet",
                document_hash("snippet", &doc)?,
                serde_json::to_string(&doc)?,
                doc.destinations,
                doc.expires_at,
            ));
        }
        if let Some(state) =
            store::records::get_extension(&self.store.pool, &organ.uid, PROFILE_NAMESPACE).await?
        {
            for control in state["authority_controls"].as_array().into_iter().flatten() {
                if let Ok(doc) = serde_json::from_value::<AuthorityPublication>(control.clone())
                    && profile::validate_delegation(&doc.authority, now).is_ok()
                {
                    documents.push((
                        "authority",
                        document_hash("authority", &doc)?,
                        serde_json::to_string(&doc)?,
                        doc.destinations,
                        doc.authority.expires_at,
                    ));
                }
            }
            let mut heads = std::collections::HashSet::new();
            let mut parents = std::collections::HashSet::new();
            for (key, doc) in state.as_object().into_iter().flatten() {
                if let Some(hash) = key.strip_prefix("revision_") {
                    heads.insert(hash);
                    parents.extend(
                        doc["parents"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str),
                    );
                }
            }
            heads.retain(|head| !parents.contains(head));
            if heads.len() == 1
                && let Ok(doc) = serde_json::from_value::<Profile>(
                    state[format!("revision_{}", heads.iter().next().unwrap())].clone(),
                )
                && profile::validate_profile(&doc, now).is_ok()
                && doc
                    .authority
                    .generation
                    .parse::<i64>()
                    .is_ok_and(|held| held >= generation)
            {
                documents.push((
                    "profile",
                    document_hash("profile", &doc)?,
                    serde_json::to_string(&doc)?,
                    doc.destinations,
                    doc.expires_at,
                ));
            }
        }
        let mut tx = self.social_write_tx().await?;
        for (kind, hash, body, destinations, expiry) in documents {
            if kind == "snippet" {
                let doc = serde_json::from_str(&body)?;
                store::social::put_snippet_on(&mut tx, &doc, &hash, "own post", now).await?;
            } else if kind == "authority" {
                let doc: AuthorityPublication = serde_json::from_str(&body)?;
                store::social::anchor_profile_authority_on(&mut tx, &doc.authority, false).await?;
            }
            store::social::enqueue_on(&mut tx, kind, &hash, &body, &destinations, expiry).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}
