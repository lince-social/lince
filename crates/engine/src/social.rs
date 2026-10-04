use crate::{Engine, EngineError, actions::ActionOutcome, trust::Signer};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use chrono::{DateTime, Utc};
use nucleus::social::*;
use serde_json::{Value, json};

mod admission;
mod ask;
mod ask_worker;
mod authority;
mod cleanup;
pub mod conversation;
mod delivery;
mod delivery_worker;
pub(crate) mod history;
mod mailbox;
mod mailbox_client;
mod media;
mod moderation;
mod operator;
mod outbound;
mod owner;
mod reports;
mod resend;
mod subscriptions;
pub use operator::{Worker, WorkerStatus};
mod post_id;
mod posting;
mod profile;
mod profile_draft;
mod profile_media;
mod publication;
mod refusal;
pub mod request_auth;
mod retention;
mod retirement;
mod reveal;
pub use reveal::validate_binding as validate_private_profile_binding;
mod descriptor;
mod discovery_sources;
mod gossip;
mod gossip_store;
mod gossip_worker;
mod health;
mod servers;
mod service;
pub mod session;
mod source;

#[cfg(test)]
mod measurements;

#[cfg(test)]
mod qualification;

#[async_trait::async_trait]
pub trait Network: Send + Sync {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError>;
}

impl Engine {
    pub(super) async fn social_require_local_write(&self) -> Result<(), EngineError> {
        let mut tx = self.store.pool.begin().await?;
        self.social_require_local_write_on(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub fn attach_social_network(&self, network: std::sync::Arc<dyn Network>) {
        *self.social_network.lock().expect("social network") =
            Some(std::sync::Arc::downgrade(&network));
    }

    pub(super) fn social_network(&self) -> Result<std::sync::Arc<dyn Network>, EngineError> {
        self.social_network
            .lock()
            .expect("social network")
            .as_ref()
            .and_then(std::sync::Weak::upgrade)
            .ok_or_else(|| invalid("Waiting for the network connection"))
    }
}

pub fn signing_bytes<T: serde::Serialize>(
    domain: &str,
    document: &T,
) -> Result<Vec<u8>, EngineError> {
    let mut value = serde_json::to_value(document)?;
    value
        .as_object_mut()
        .ok_or_else(|| invalid("A public document must be an object"))?
        .remove("signature");
    let mut bytes = format!("lince/social/{domain}/1\n").into_bytes();
    canonical_value(&value, &mut bytes)?;
    Ok(bytes)
}

fn validate_search(query: &Search) -> Result<(), EngineError> {
    for value in [
        &query.text,
        &query.area,
        &query.language,
        &query.concept,
        &query.unit,
    ] {
        text(value, 160, true).map_err(invalid)?;
    }
    if query
        .after
        .as_deref()
        .is_some_and(|after| !nucleus::valid_uid(after, "post"))
    {
        return Err(invalid("Use a valid public announcement cursor"));
    }
    Ok(())
}

fn canonical_value(value: &Value, bytes: &mut Vec<u8>) -> Result<(), EngineError> {
    match value {
        Value::Object(object) => {
            let mut fields: Vec<_> = object.iter().collect();
            fields.sort_by(|(a, _), (b, _)| a.encode_utf16().cmp(b.encode_utf16()));
            bytes.push(b'{');
            for (index, (key, value)) in fields.into_iter().enumerate() {
                if index > 0 {
                    bytes.push(b',');
                }
                bytes.extend(serde_json::to_vec(key)?);
                bytes.push(b':');
                canonical_value(value, bytes)?;
            }
            bytes.push(b'}');
        }
        Value::Array(values) => {
            bytes.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    bytes.push(b',');
                }
                canonical_value(value, bytes)?;
            }
            bytes.push(b']');
        }
        Value::Number(number) => {
            if number
                .as_i64()
                .is_none_or(|value| value.unsigned_abs() > 9_007_199_254_740_991)
            {
                return Err(invalid(
                    "Public signed numbers must be safe integers; exact amounts/revisions use decimal strings",
                ));
            }
            bytes.extend(serde_json::to_vec(number)?);
        }
        _ => bytes.extend(serde_json::to_vec(value)?),
    }
    Ok(())
}

pub fn document_hash<T: serde::Serialize>(
    domain: &str,
    document: &T,
) -> Result<String, EngineError> {
    Ok(nucleus::fact::sha256_hex(&signing_bytes(domain, document)?))
}

pub(super) fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::Consequence(message.into())
}

pub(super) fn new_social_signer(organ: &str) -> Result<Signer, EngineError> {
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).map_err(|_| invalid("Secure randomness is unavailable"))?;
    Ok(Signer::from_bytes(organ, "social", secret))
}

pub fn validate_snippet(doc: &Snippet, now: i64) -> Result<(), EngineError> {
    if doc.protocol != "lince.snippet.1" || !nucleus::valid_uid(&doc.id, "post") {
        return Err(invalid("Unsupported public snippet"));
    }
    if doc.id
        != post_id::post_id(
            doc.anonymous
                .as_ref()
                .map_or(doc.signing_key.as_str(), |authority| {
                    authority.owner_key.as_str()
                }),
            &doc.nonce,
            doc.mode,
            &doc.alias,
            doc.profile
                .as_ref()
                .map(|authority| authority.organ.as_str()),
            &doc.destinations,
        )?
    {
        return Err(invalid(
            "This post ID does not match its immutable posting identity and destinations",
        ));
    }
    let revision: i64 = doc
        .revision
        .parse()
        .map_err(|_| invalid("Invalid revision"))?;
    if revision <= 0
        || doc.resolves.len() > 8
        || doc
            .resolves
            .iter()
            .any(|p| p.len() != 64 || !p.bytes().all(|b| b.is_ascii_hexdigit()))
        || revision.to_string() != doc.revision
        || doc
            .parent
            .as_ref()
            .is_some_and(|p| p.len() != 64 || !p.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(invalid("Invalid revision or parent"));
    }
    if doc.created_at > doc.issued_at
        || doc.created_at <= 0
        || doc.issued_at > now + 300
        || doc.expires_at <= now
        || doc.expires_at <= doc.issued_at
        || doc.expires_at.saturating_sub(doc.issued_at) > MAX_LIFETIME
    {
        return Err(invalid("Expired or invalid publication lifetime"));
    }
    let draft = PostDraft {
        title: doc.title.clone(),
        text: doc.text.clone(),
        direction: doc.direction,
        mode: doc.mode,
        alias: doc.alias.clone(),
        quantity: doc.quantity.clone(),
        unit: doc.unit.clone(),
        concept: doc.concept.clone(),
        language: doc.language.clone(),
        area: doc.area.clone(),
        availability: doc.availability.clone(),
        redistribute: doc.redistribute,
        destinations: doc.destinations.clone(),
        lifetime_days: None,
    };
    draft.validate().map_err(invalid)?;
    if let Some(reply) = &doc.reply {
        request_auth::validate_route(reply, now)?;
        if doc.expires_at > reply.control.expires_at || doc.expires_at > reply.expires_at {
            return Err(invalid(
                "The announcement outlives its authorized reply route",
            ));
        }
    }
    match doc.mode {
        AuthorMode::Anonymous => {
            if doc.profile.is_some() {
                return Err(invalid("Anonymous posts cannot carry an Organ profile"));
            }
            let authority = doc
                .anonymous
                .as_ref()
                .ok_or_else(|| invalid("Anonymous posts need owner-authorized editing keys"))?;
            posting::validate_authority(authority, now)?;
            if authority.editor_key != doc.signing_key
                || doc.issued_at < authority.issued_at
                || doc.expires_at > authority.expires_at
            {
                return Err(invalid(
                    "The anonymous post differs from its editing authority",
                ));
            }
        }
        AuthorMode::Identified => {
            if doc.anonymous.is_some() {
                return Err(invalid("Identified posts cannot carry anonymous authority"));
            }
            let authority = doc
                .profile
                .as_ref()
                .ok_or_else(|| invalid("An identified post needs profile authority"))?;
            if doc.state == PostState::Withdrawn {
                profile::validate_ending_delegation(authority, now)?;
            } else {
                profile::validate_delegation(authority, now)?;
            }
            if authority.editor_key != doc.signing_key
                || doc.expires_at > authority.expires_at
                || doc.issued_at < authority.issued_at
            {
                return Err(invalid(
                    "The posting key differs from its profile authority",
                ));
            }
        }
    }
    if serde_json::to_vec(doc)?.len() > MAX_SNIPPET_BYTES {
        return Err(invalid("The signed snippet exceeds 6 KiB"));
    }
    if !crate::roster::verify_with(
        &doc.signing_key,
        &signing_bytes("snippet", doc)?,
        &doc.signature,
    ) {
        return Err(invalid("Invalid snippet signature"));
    }
    Ok(())
}

impl Engine {
    async fn social_own_record(&self, uid: &str, actor: Option<&str>) -> Result<(), EngineError> {
        let row = store::records::get(&self.store.pool, uid)
            .await?
            .ok_or_else(|| invalid("This Record is missing"))?;
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("The local Organ is missing"))?;
        if row.organ_uid.as_deref() != Some(&organ.uid) || !self.may_read_record(actor, uid).await?
        {
            return Err(EngineError::Forbidden(
                "This social Record belongs to another Organ or is not visible".into(),
            ));
        }
        Ok(())
    }

    async fn social_post(&self, uid: &str, actor: Option<&str>) -> Result<Value, EngineError> {
        self.social_own_record(uid, actor).await?;
        store::records::get_extension(&self.store.pool, uid, PUBLICATION_NAMESPACE)
            .await?
            .ok_or_else(|| invalid("This is not a social publication"))
    }

    async fn social_post_signer(
        &self,
        state: &Value,
        draft: &PostDraft,
    ) -> Result<(Signer, Option<Delegation>), EngineError> {
        if draft.mode == AuthorMode::Identified {
            if let Some(saved) = state.get("posting_authority") {
                let authority: Delegation = serde_json::from_value(saved["authority"].clone())?;
                let secret: [u8; 32] = B64
                    .decode(saved["secret"].as_str().unwrap_or_default())
                    .map_err(|_| invalid("The retained posting key is unavailable"))?
                    .try_into()
                    .map_err(|_| invalid("Invalid retained posting key"))?;
                let signer = Signer::from_bytes(&authority.organ, "social-profile", secret);
                if signer.public_key_b64() != authority.editor_key {
                    return Err(invalid("Retained posting authority does not match its key"));
                }
                return Ok((signer, Some(authority)));
            }
            let organ = store::organs::local(&self.store.pool)
                .await?
                .ok_or_else(|| invalid("No local Organ"))?;
            let authority =
                store::records::get_extension(&self.store.pool, &organ.uid, PRIVATE_NAMESPACE)
                    .await?;
            if authority
                .as_ref()
                .and_then(|value| value.get("profile_signer"))
                .is_none()
            {
                return Err(invalid(
                    "An authorized editor must set up your public profile before identified publication",
                ));
            }
            let (signer, delegation) = self.social_profile_signer().await?;
            return Ok((signer, Some(delegation)));
        }
        let secret: [u8; 32] = B64
            .decode(state["secret"].as_str().unwrap_or_default())
            .map_err(|_| invalid("Posting identity is unavailable"))?
            .try_into()
            .map_err(|_| invalid("Posting identity is invalid"))?;
        Ok((Signer::from_bytes("", "social", secret), None))
    }

    async fn social_preview(
        &self,
        uid: &str,
        actor: Option<&str>,
        status: PostState,
        mut issued_at: i64,
    ) -> Result<Snippet, EngineError> {
        self.social_post(uid, actor).await?;
        self.social_prepare_posting_authority(uid, actor).await?;
        let state = self.social_post(uid, actor).await?;
        let draft: PostDraft = serde_json::from_value(state["draft"].clone())?;
        draft.validate().map_err(invalid)?;
        let previous: Option<Snippet> = state
            .get("published")
            .filter(|v| !v.is_null())
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()?;
        let mut revisions = std::collections::HashMap::new();
        let mut resolved = std::collections::HashSet::new();
        for (key, value) in state.as_object().into_iter().flatten() {
            if let Some(hash) = key.strip_prefix("revision_") {
                let doc: Snippet = serde_json::from_value(value.clone())?;
                if let Some(parent) = &doc.parent {
                    resolved.insert(parent.clone());
                }
                resolved.extend(doc.resolves.iter().cloned());
                revisions.insert(hash.to_owned(), doc);
            }
        }
        let mut heads: Vec<String> = revisions
            .keys()
            .filter(|hash| !resolved.contains(*hash))
            .cloned()
            .collect();
        heads.sort();
        heads.truncate(8);
        let anonymous: Option<PostingAuthority> = state
            .get("anonymous_authority")
            .map(|authority| serde_json::from_value(authority.clone()))
            .transpose()?;
        if draft.mode == AuthorMode::Anonymous && anonymous.is_none() {
            return Err(invalid(
                "The owner device must authorize this anonymous post before publication",
            ));
        }
        if let Some(authority) = &anonymous {
            issued_at = issued_at.max(authority.issued_at);
        }
        let next_revision = revisions
            .values()
            .filter(|doc| {
                doc.anonymous
                    .as_ref()
                    .map(|authority| &authority.generation)
                    == anonymous.as_ref().map(|authority| &authority.generation)
            })
            .map(|doc| {
                doc.revision
                    .parse::<i64>()
                    .map_err(|_| invalid("Invalid retained revision"))
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| invalid("Revision exhausted"))?;
        if previous
            .as_ref()
            .is_some_and(|old| old.mode != draft.mode || old.alias != draft.alias)
        {
            return Err(invalid(
                "Create a fresh post to change its publication identity or alias; withdraw the old post separately",
            ));
        }
        let (signer, mut profile) = self.social_post_signer(&state, &draft).await?;
        if let Some(authority) = &mut profile {
            issued_at = issued_at.max(authority.issued_at);
            let organ = store::organs::local(&self.store.pool)
                .await?
                .ok_or_else(|| invalid("No local Organ"))?;
            let current =
                store::records::get_extension(&self.store.pool, &organ.uid, PRIVATE_NAMESPACE)
                    .await?
                    .unwrap_or_else(|| json!({}));
            if status != PostState::Withdrawn
                && current["profile_signer"]["authority"]["generation"].as_str()
                    != Some(authority.generation.as_str())
            {
                return Err(invalid(
                    "This identified post used revoked editing authority. Withdraw it and create a fresh post",
                ));
            }
            let root = self.social_checked_root_signer().await?;
            let current_generation = current["profile_signer"]["authority"]["generation"]
                .as_str()
                .unwrap_or(&authority.generation)
                .to_owned();
            if authority.expires_at <= issued_at
                || status == PostState::Withdrawn
                    && root.is_some()
                    && authority.generation != current_generation
                || root
                    .as_ref()
                    .is_some_and(|root| root.public_key_b64() != authority.root_key)
            {
                let root = root.ok_or_else(|| {
                    invalid(
                        "Renew this post's narrow authority on the owner device before ending it",
                    )
                })?;
                authority.root_key = root.public_key_b64();
                if status == PostState::Withdrawn {
                    authority.generation = current_generation;
                }
                authority.successions = self
                    .published_successions(&organ.uid)
                    .await?
                    .into_iter()
                    .rev()
                    .take(8)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .map(|change| RootSuccession {
                        old_key: change.old_key,
                        new_key: change.new_key,
                        created_at: change.created_at,
                        signature: change.signature,
                    })
                    .collect();
                authority.issued_at = issued_at;
                authority.expires_at = issued_at + MAX_LIFETIME;
                authority.signature = root.sign_bytes(&signing_bytes(
                    if status == PostState::Withdrawn {
                        "profile-ending-authority"
                    } else {
                        "profile-authority"
                    },
                    authority,
                )?);
            }
        }
        if previous
            .as_ref()
            .is_some_and(|old| old.state == PostState::Withdrawn)
            && status != PostState::Withdrawn
        {
            return Err(invalid(
                "A withdrawn announcement stays ended. Create a fresh post to publish again",
            ));
        }
        let reply = if status == PostState::Withdrawn {
            None
        } else {
            let prepared = self.social_reply_key_status(uid).await?;
            match prepared["reply_keys"].as_str() {
                Some("ready") => Some(serde_json::from_value(prepared["route"].clone())?),
                Some("unprepared") => None,
                _ => {
                    return Err(invalid(
                        "Wait for the owner to authorize or renew this device's reply keys before publishing its reply route",
                    ));
                }
            }
        };
        let mut doc = Snippet {
            protocol: "lince.snippet.1".into(),
            id: post_id::post_id(
                anonymous
                    .as_ref()
                    .map_or(signer.public_key_b64(), |authority| {
                        authority.owner_key.clone()
                    })
                    .as_str(),
                state["nonce"]
                    .as_str()
                    .ok_or_else(|| invalid("Missing posting nonce"))?,
                draft.mode,
                &draft.alias,
                profile.as_ref().map(|authority| authority.organ.as_str()),
                &draft.destinations,
            )?,
            nonce: state["nonce"]
                .as_str()
                .ok_or_else(|| invalid("Missing posting nonce"))?
                .into(),
            revision: next_revision.to_string(),
            parent: previous
                .as_ref()
                .map(|doc| document_hash("snippet", doc))
                .transpose()?,
            resolves: heads,
            created_at: previous.as_ref().map_or(issued_at, |doc| doc.created_at),
            issued_at,
            expires_at: issued_at + i64::from(draft.lifetime_days.unwrap_or(7)) * 86400,
            mode: draft.mode,
            signing_key: signer.public_key_b64(),
            profile,
            anonymous,
            alias: draft.alias,
            title: draft.title,
            text: draft.text,
            direction: draft.direction,
            quantity: draft.quantity,
            unit: draft.unit,
            concept: draft.concept,
            language: draft.language,
            area: draft.area,
            availability: draft.availability,
            state: status,
            redistribute: draft.redistribute,
            destinations: draft.destinations,
            reply,
            signature: String::new(),
        };
        if let Some(authority) = &doc.profile {
            doc.expires_at = doc.expires_at.min(authority.expires_at);
        }
        if let Some(authority) = &doc.anonymous {
            doc.issued_at = doc.issued_at.max(authority.issued_at);
            doc.expires_at = doc.expires_at.min(authority.expires_at);
        }
        if let Some(reply) = &doc.reply {
            doc.expires_at = doc
                .expires_at
                .min(reply.control.expires_at)
                .min(reply.expires_at);
        }
        if previous.as_ref().is_some_and(|old| old.id != doc.id) {
            return Err(invalid(
                "The posting authority changed. Create a fresh post and use the retained original authority to end the old one",
            ));
        }
        doc.signature = signer.sign_bytes(&signing_bytes("snippet", &doc)?);
        validate_snippet(&doc, nucleus::execution::now().timestamp())?;
        Ok(doc)
    }

    pub fn social_command<'a>(
        &'a self,
        command: Command,
        actor: Option<&'a str>,
        now: DateTime<Utc>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ActionOutcome, EngineError>> + Send + 'a>,
    > {
        Box::pin(self.social_authorized(actor, command.permission(), Box::pin(async move {
            self.require_permission(actor, command.permission()).await?;
            if command.permission() != "view:stream" {
                self.social_require_local_write().await?;
            }
            let mut outcome = ActionOutcome::default();
            let post_cursor = match &command {
                Command::PostPage { after } => {
                    if !nucleus::valid_uid(after, "r") {
                        return Err(invalid("Invalid My posts page"));
                    }
                    Some(after.clone())
                }
                _ => None,
            };
            match command {
                Command::SaveSubscription {filter} => {
                    outcome.data = Some(self.social_save_subscription(*filter,actor).await?);
                }
                Command::RemoveSubscription {id} => {
                    outcome.data = Some(self.social_remove_subscription(&id).await?);
                }
                Command::ConfigureSubscriptions {enabled} => {
                    outcome.data = Some(self.social_configure_subscriptions(enabled).await?);
                }
                Command::Subscriptions {after} => {
                    self.require_permission(actor,"organ:update").await?;
                    outcome.data = Some(self.social_subscriptions(after.as_deref()).await?);
                }
                Command::SubscriptionResults {id} => {
                    self.require_permission(actor,"organ:update").await?;
                    outcome.data = Some(self.social_subscription_results(&id).await?);
                }
                Command::ClearSubscriptionMatches {id} => {
                    outcome.data = Some(self.social_clear_subscription_matches(&id).await?);
                }
                Command::PreviewReport {post,service,explanation} => {
                    outcome.data = Some(self.social_preview_report(&post,&service,&explanation).await?);
                }
                Command::SendReport {document,preview_hash} => {
                    outcome.data = Some(self.social_queue_report(&document,&preview_hash,actor).await?);
                }
                Command::Reports => {
                    self.require_permission(actor,"organ:update").await?;
                    outcome.data = Some(self.social_reports(actor).await?);
                }
                Command::ClearReports => {
                    outcome.data = Some(self.social_clear_reports(actor).await?);
                }
                Command::ReceivedReports {after} => {
                    self.require_permission(actor,"organ:update").await?;
                    outcome.data = Some(self.social_received_reports(after.as_deref()).await?);
                }
                Command::DismissReport {id} => {
                    outcome.data = Some(self.social_dismiss_report(&id).await?);
                }
                Command::MutePost { post, whole_author } => {
                    outcome.data = Some(self.social_mute_post(&post, whole_author).await?);
                }
                Command::Unmute { key } => {
                    outcome.data = Some(self.social_unmute(&key).await?);
                }
                Command::Mutes { after } => {
                    self.require_permission(actor, "organ:update").await?;
                    outcome.data = Some(self.social_mutes(after.as_deref()).await?);
                }
                Command::RemoveListing { post, reason } => {
                    outcome.data = Some(self.social_remove_listing(&post, &reason).await?);
                }
                Command::RestoreListing { post } => {
                    outcome.data = Some(self.social_restore_listing(&post).await?);
                }
                Command::RemovedListings { after } => {
                    self.require_permission(actor, "organ:update").await?;
                    outcome.data = Some(self.social_removed_listings(after.as_deref()).await?);
                }
                Command::ServiceHealth => {
                    self.require_permission(actor, "organ:update").await?;
                    outcome.data = Some(self.social_service_health().await?);
                }
                Command::RebuildPublicIndex => {
                    outcome.data = Some(self.social_rebuild_public_index().await?);
                }
                Command::InspectService { endpoint } => {
                    let mut data = self.social_inspect_service(&endpoint).await?;
                    data["can_manage_services"] = json!(
                        self.require_permission(actor, "organ:update").await.is_ok()
                            && self.social_require_local_write().await.is_ok()
                    );
                    outcome.data = Some(data);
                }
                Command::OpenRequest {
                    post,
                    text,
                    alias,
                    services,
                } => {
                    outcome.data = Some(
                        self.social_open_request(*post, text, alias, services, actor)
                            .await?,
                    );
                }
                Command::SendPrivate { conversation, text } => {
                    outcome.data = Some(self.social_send_text(&conversation, text, actor).await?);
                }
                Command::DecideRequest {
                    conversation,
                    decision,
                } => {
                    outcome.data = Some(
                        self.social_decide_request(&conversation, decision, actor)
                            .await?,
                    );
                }
                Command::Requests { after } => {
                    outcome.data = Some(self.social_requests(actor, after.as_deref()).await?);
                }
                Command::ResumePrivate { message } => {
                    outcome.data = Some(self.social_resume_private(&message, actor).await?);
                }
                Command::PrivateDeliveryStatus { message } => {
                    outcome.data = Some(self.social_private_delivery_status(&message, actor).await?);
                }
                Command::ResendExpiredPrivate { message } => {
                    outcome.data = Some(self.social_resend_expired_private(&message, actor).await?);
                }
                Command::ResumeRequest { record } => {
                    outcome.data = Some(self.social_resume_request(&record, actor).await?);
                }
                Command::ArchiveRequest { record } => {
                    outcome.data = Some(self.social_archive_request(&record, actor).await?);
                }
                Command::UnblockParticipant { context, peer } => {
                    outcome.data = Some(
                        self.social_unblock_participant(&context, &peer, actor)
                            .await?,
                    );
                }
                Command::DiscardPrivate {
                    context,
                    service,
                    envelope,
                } => {
                    outcome.data = Some(
                        self.social_discard_private(&context, &service, &envelope, actor)
                            .await?,
                    );
                }
                Command::RevealProfile { conversation } => {
                    outcome.data = Some(self.social_reveal_profile(&conversation, actor).await?);
                }
                Command::ConnectParticipant { conversation } => {
                    outcome.data = Some(
                        self.social_connect_participant(&conversation, actor)
                            .await?,
                    );
                }
                Command::ResetPrivateSessions => {
                    outcome.data = Some(self.social_reset_private_sessions().await?);
                }
                Command::PrepareReplyKeys { record, services } => {
                    self.social_own_record(&record, actor).await?;
                    outcome.data = Some(self.social_prepare_reply_keys(&record, services).await?);
                }
                Command::ReplyKeyStatus { record } => {
                    self.social_own_record(&record, actor).await?;
                    outcome.data = Some(self.social_reply_key_status(&record).await?);
                }
                Command::ArchivePost { record } => Box::pin(async {
                    let state = self.social_post(&record, actor).await?;
                    if let Some(document) = state.get("published").filter(|value| !value.is_null())
                    {
                        let doc: Snippet = serde_json::from_value(document.clone())?;
                        if doc.expires_at + 600 > now.timestamp() {
                            if doc.state != PostState::Withdrawn {
                                return Err(invalid(
                                    "Withdraw this announcement before archiving it, or wait until its public lifetime ends",
                                ));
                            }
                            let hash = document_hash("snippet", &doc)?;
                            for host in &doc.destinations {
                                let accepted: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM social_publication_job WHERE hash=? AND destination=? AND state='accepted')")
                                .bind(&hash).bind(host).fetch_one(&self.store.pool).await?;
                                if !accepted {
                                    return Err(invalid(
                                        "A selected host has not accepted the withdrawal yet. Keep this post until it is accepted or its public lifetime ends",
                                    ));
                                }
                            }
                        }
                    }
                    let mut tx = self.social_write_tx().await?;
                    let current: String = store::sqlx::query_scalar(
                        "SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?",
                    )
                    .bind(&record)
                    .bind(PUBLICATION_NAMESPACE)
                    .fetch_one(&mut *tx)
                    .await?;
                    if serde_json::from_str::<Value>(&current)?["published"] != state["published"] {
                        return Err(invalid(
                            "Another device updated this announcement; refresh it before archiving",
                        ));
                    }
                    let private_context:bool=store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record_extension WHERE record_uid=? AND namespace=?)")
                        .bind(&record).bind(nucleus::social::requests::SESSION_AUTHORITY_NAMESPACE).fetch_one(&mut *tx).await?;
                    if private_context {
                        let mut retained: Value = serde_json::from_str(&current)?;
                        retained["archived"] = json!(true);
                        store::records::set_extension_on(
                            &mut tx,
                            &record,
                            PUBLICATION_NAMESPACE,
                            &retained,
                        )
                        .await?;
                    } else {
                        store::records::mark_deleted_on(&mut tx, &record).await?;
                    }
                    tx.commit().await?;
                    outcome.data = Some(
                        json!({"status":"Ended announcement archived on your devices. Public copies and separately accepted conversations are unaffected"}),
                    );
                    Ok::<(), EngineError>(())
                }).await?,
                Command::ImportProfileImage { path } => {
                    outcome.data = Some(self.social_import_image(path, actor).await?);
                }
                Command::ImportProfileImageData { encoded } => {
                    outcome.data = Some(self.social_import_image_data(encoded).await?);
                }
                Command::FetchProfileImage {
                    organ,
                    hash,
                    services,
                } => {
                    outcome.data = Some(self.social_load_image(&organ, &hash, &services).await?);
                }
                Command::PrepareFromRecord { source } => {
                    let (source, draft) = self.social_source_draft(&source, actor).await?;
                    outcome.data = Some(
                        json!({"source":source,"draft":draft,"status":"Review the selected source fields before saving a public draft"}),
                    );
                }
                Command::ConfigureServices { settings } => {
                    self.social_configure_services(&settings).await?;
                    outcome.data =
                        Some(json!({"settings":settings,"status":"Service roles saved"}));
                }
                Command::SaveServer { choice } => {
                    let endpoint = choice.endpoint.clone();
                    outcome.data = Some(self.social_save_server(Some(choice), &endpoint).await?);
                }
                Command::RemoveServer { endpoint } => {
                    outcome.data = Some(self.social_save_server(None, &endpoint).await?);
                }
                Command::ConfigureGossip { enabled } => {
                    outcome.data = Some(
                        self.social_configure_gossip(Some(enabled), None, actor)
                            .await?,
                    );
                }
                Command::SetGossipContact { choice } => {
                    outcome.data = Some(
                        self.social_configure_gossip(None, Some(choice), actor)
                            .await?,
                    );
                }
                Command::ConfigureAsk { enabled } => {
                    outcome.data = Some(
                        self.social_configure_ask(Some(enabled), None, actor)
                            .await?,
                    );
                }
                Command::SetAskContact { choice } => {
                    outcome.data =
                        Some(self.social_configure_ask(None, Some(choice), actor).await?);
                }
                Command::StartAsk { query, contacts } => {
                    outcome.data = Some(self.social_start_ask(query, contacts, actor).await?);
                }
                Command::CancelAsk { id } => {
                    outcome.data = Some(self.social_cancel_ask(Some(&id), actor).await?);
                }
                Command::ClearAsks => {
                    outcome.data = Some(self.social_cancel_ask(None, actor).await?);
                }
                Command::AskStatus => {
                    outcome.data = Some(self.social_ask_view(actor).await?);
                }
                Command::AskResults { id } => {
                    outcome.data = Some(self.social_ask_results(&id, actor).await?);
                }
                Command::Overview | Command::PostPage { .. } => Box::pin(async {
                    self.social_refresh_sources().await?;
                    let states = self.social_publications().await?;
                    let mut posts = Vec::new();
                    let mut next_posts_after = None;
                    for (uid, state) in states {
                        if post_cursor.as_ref().is_some_and(|after| &uid <= after) {
                            continue;
                        }
                        if self.social_own_record(&uid, actor).await.is_ok() {
                            if posts.len() >= 8
                                || serde_json::to_vec(&posts)?.len()
                                    > 1024 * 1024 - state.to_string().len().min(512 * 1024)
                            {
                                next_posts_after = posts
                                    .last()
                                    .and_then(|post: &Value| post["record"].as_str())
                                    .map(str::to_owned);
                                break;
                            }
                            let drafts: Vec<_> = state
                                .as_object()
                                .into_iter()
                                .flatten()
                                .filter(|(key, _)| key.starts_with("draft_"))
                                .map(|(_, value)| value)
                                .take(32)
                                .collect();
                            let source_visible = match (actor, state["source"].as_str()) {
                                (Some(_), Some(source)) => {
                                    self.social_source_draft(source, actor).await.is_ok()
                                }
                                _ => true,
                            };
                            posts.push(json!({"record":uid,"draft":state["draft"],"source":state["source"],"drafts":drafts,"published":state["published"],"posting_authority":state["anonymous_authority"],"posting_authority_changed":state["published"]["mode"]=="anonymous" && state["anonymous_authority"]["editor_key"].is_string() && state["published"]["signing_key"]!=state["anonymous_authority"]["editor_key"],"source_draft":if source_visible { state["source_draft"].clone() } else { Value::Null },"source_error":state["source_error"]}));
                        }
                    }
                    posts.sort_by(|a, b| a["record"].as_str().cmp(&b["record"].as_str()));
                    let organ = store::organs::local(&self.store.pool)
                        .await?
                        .ok_or_else(|| invalid("No local Organ"))?;
                    let mut profile = Some(store::records::get_extension(
                        &self.store.pool,
                        &organ.uid,
                        PROFILE_NAMESPACE,
                    )
                    .await?.unwrap_or_else(|| json!({})));
                    if let Some(state) = &mut profile {
                        for (key,draft) in state.as_object_mut().into_iter().flatten() {
                            if key.starts_with("pending_profile_") {
                                profile_media::omit_encoded_images(draft);
                            }
                        }
                        state["editor"] = profile::profile_editor(state);
                        let cell = store::cells::local(&self.store.pool)
                            .await?
                            .ok_or_else(|| invalid("No local Cell"))?;
                        let pending = state.get(format!("pending_profile_{}", cell.uid)).cloned();
                        if let Some(pending) = pending {
                            state["editor"]["fields"] = pending["fields"].clone();
                            state["destinations"] = pending["destinations"].clone();
                        }
                        state["editor"]["pending_drafts"] = json!(
                            state
                                .as_object()
                                .into_iter()
                                .flatten()
                                .filter(|(key, _)| key.starts_with("pending_profile_"))
                                .map(|(_, draft)| draft.clone())
                                .collect::<Vec<_>>()
                        );
                        state["editor"]["can_edit"] = json!(
                            self.social_require_local_write().await.is_ok()
                                && self.require_permission(actor, "organ:update").await.is_ok()
                        );
                        state["editor"]["can_rotate"] = json!(
                            state["editor"]["can_edit"] == true
                                && self.root_signer().await?.is_some()
                        );
                        let private = store::records::get_extension(
                            &self.store.pool,
                            &organ.uid,
                            PRIVATE_NAMESPACE,
                        )
                        .await?
                        .unwrap_or_else(|| json!({}));
                        state["editor"]["rotation_pending"] =
                            json!(private["editor_rotation_required"] == true);
                    }
                    outcome.data = Some(
                        json!({"posts":posts,"next_posts_after":next_posts_after,"profile":profile,"jobs":store::social::jobs(&self.store.pool).await?,"settings":self.social_settings().await?,"services_managed":self.social_services_managed(),"servers":self.social_servers().await?,"gossip":self.social_gossip_view(actor).await?["gossip"],"asks":self.social_ask_view(actor).await?["asks"],"can_manage_services":self.require_permission(actor,"organ:update").await.is_ok() && self.social_require_local_write().await.is_ok()}),
                    );
                    Ok::<(), EngineError>(())
                }).await?,
                Command::SaveDraft {
                    record,
                    source,
                    mut draft,
                } => Box::pin(async {
                    draft.validate().map_err(invalid)?;
                    if let Some(amount) = &draft.quantity {
                        draft.quantity = Some(
                            nucleus::DecimalValue::parse_inferred(amount)
                                .map_err(|_| invalid("Invalid quantity"))?
                                .to_string(),
                        );
                    }
                    for destination in &draft.destinations {
                        destination
                            .parse::<iroh::EndpointId>()
                            .map_err(|_| invalid("Use a valid service endpoint ID"))?;
                    }
                    let prior = match &record {
                        Some(uid) => Some(self.social_post(uid, actor).await?),
                        None => None,
                    };
                    let mut source_projection = None;
                    if let Some(uid) = &source {
                        match self.social_source_draft(uid, actor).await {
                            Ok((_, projection)) => source_projection = Some(projection),
                            Err(error)
                                if prior.as_ref().is_none_or(|state| state["source"] != *uid) =>
                            {
                                return Err(error);
                            }
                            Err(_) => {}
                        }
                    }
                    if let Some(uid) = &record {
                        self.social_post(uid, actor).await?;
                    }
                    let uid = record.unwrap_or_else(|| nucleus::new_uid("r"));
                    let existing = store::records::get_extension(
                        &self.store.pool,
                        &uid,
                        PUBLICATION_NAMESPACE,
                    )
                    .await?;
                    if existing.is_some() {
                        self.social_own_record(&uid, actor).await?;
                    }
                    let mut state = existing.unwrap_or_else(
                        || json!({"public_id":nucleus::new_uid("post"),"published":null}),
                    );
                    if state["archived"] == true {
                        return Err(invalid(
                            "This announcement is archived; create a fresh post instead of editing its retained private key context",
                        ));
                    }
                    if state.get("nonce").is_none() {
                        let mut nonce = [0u8; 16];
                        getrandom::fill(&mut nonce)
                            .map_err(|_| invalid("Secure randomness is unavailable"))?;
                        state["nonce"] = json!(B64.encode(nonce));
                    }
                    let source = source.or_else(|| state["source"].as_str().map(str::to_owned));
                    if state.get("secret").is_none() {
                        let mut secret = None;
                        if draft.mode == AuthorMode::Anonymous && !draft.alias.is_empty() {
                            for (key, value) in self.social_publications().await? {
                                if self.social_own_record(&key, actor).await.is_ok()
                                    && value["draft"]["alias"] == draft.alias
                                    && value["draft"]["mode"] == "anonymous"
                                {
                                    secret = value["secret"].as_str().map(str::to_owned);
                                    break;
                                }
                            }
                        }
                        let secret = match secret {
                            Some(secret) => secret,
                            None => B64.encode(new_social_signer("")?.secret_bytes()),
                        };
                        state["secret"] = json!(secret);
                    }
                    if let Some(published) = state.get("published").filter(|v| !v.is_null()) {
                        if published["mode"] != serde_json::to_value(draft.mode)?
                            || published["alias"] != draft.alias
                        {
                            return Err(invalid(
                                "Create a fresh post to change anonymous/identified mode or its alias",
                            ));
                        }
                        if published["destinations"] != serde_json::to_value(&draft.destinations)? {
                            return Err(invalid(
                                "Create a fresh post to change publication services, and withdraw the old post from its original services",
                            ));
                        }
                        if published["redistribute"] != draft.redistribute {
                            return Err(invalid(
                                "Withdraw this announcement and create a fresh one to change redistribution consent",
                            ));
                        }
                    }
                    let draft_hash = nucleus::fact::sha256_hex(&serde_json::to_vec(&draft)?);
                    state[format!("draft_{draft_hash}")] = serde_json::to_value(&draft)?;
                    state["draft"] = serde_json::to_value(draft)?;
                    state["source"] = json!(source);
                    if let Some(projection) = source_projection {
                        state["source_fingerprint"] =
                            json!(nucleus::fact::sha256_hex(&serde_json::to_vec(&projection)?));
                        state["source_error"] = Value::Null;
                    }
                    state["source_draft"] = Value::Null;
                    let signer = self.signer.lock().await.clone();
                    let organ = store::organs::local(&self.store.pool)
                        .await?
                        .ok_or_else(|| invalid("No local Organ"))?;
                    let mut tx = self.social_write_tx().await?;
                    if store::sqlx::query_scalar::<_, i64>(
                        "SELECT COUNT(*) FROM record WHERE uid=?",
                    )
                    .bind(&uid)
                    .fetch_one(&mut *tx)
                    .await?
                        == 0
                    {
                        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record_extension e JOIN record r ON r.uid=e.record_uid WHERE e.namespace=? AND r.organ_uid=? AND r.deleted_at IS NULL AND COALESCE(json_extract(e.fds,'$.archived'),0)=0")
                        .bind(PUBLICATION_NAMESPACE).bind(&organ.uid).fetch_one(&mut *tx).await?;
                        if count >= source::MAX_OWN_POSTS {
                            return Err(invalid(
                                "The device supports 256 retained posts. Archive an ended announcement before adding another",
                            ));
                        }
                        store::records::create_with_uid_on(
                            &mut tx,
                            &uid,
                            store::records::NewRecord {
                                slug: None,
                                kind: nucleus::RecordKind::Plain,
                                head: "Public announcement",
                                body: "",
                                quantity: store::exact::zero(),
                            },
                            &organ.uid,
                            None,
                        )
                        .await?;
                    }
                    if let Some(current) = store::sqlx::query_scalar::<_, String>(
                        "SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?",
                    )
                    .bind(&uid)
                    .bind(PUBLICATION_NAMESPACE)
                    .fetch_optional(&mut *tx)
                    .await?
                    {
                        let mut merged: Value = serde_json::from_str(&current)?;
                        if merged["secret"] != state["secret"] {
                            return Err(invalid(
                                "The posting authority changed; refresh the draft",
                            ));
                        }
                        merged[format!("draft_{draft_hash}")] = state["draft"].clone();
                        merged["draft"] = state["draft"].clone();
                        merged["source"] = state["source"].clone();
                        merged["source_draft"] = Value::Null;
                        if state.get("source_fingerprint").is_some() {
                            merged["source_fingerprint"] = state["source_fingerprint"].clone();
                            merged["source_error"] = state["source_error"].clone();
                        }
                        state = merged;
                    }
                    source::trim_publication_state(&mut state)?;
                    store::records::set_extension_on(&mut tx, &uid, PUBLICATION_NAMESPACE, &state)
                        .await?;
                    let fact = crate::append::append_one_in_transaction(
                        &mut tx,
                        nucleus::NewFact {
                            uid: None,
                            record_uid: uid.clone(),
                            delta: store::exact::zero(),
                            at: Some(now),
                            actor_uid: actor.map(str::to_owned),
                            cause: nucleus::Cause::user_edit(),
                            payload: Some(json!({"social":"draft-saved"}).to_string()),
                        },
                        now,
                        signer.as_ref(),
                    )
                    .await?;
                    tx.commit().await?;
                    outcome.created = Some(uid.clone());
                    outcome.facts.extend(fact);
                    outcome.data =
                        Some(json!({"record":uid,"status":"draft","draft":state["draft"]}));
                    Ok::<(), EngineError>(())
                }).await?,
                Command::Preview { record, state } => Box::pin(async {
                    let doc = self
                        .social_preview(&record, actor, state, now.timestamp())
                        .await?;
                    outcome.data = Some(
                        json!({"record":record,"preview_hash":document_hash("snippet",&doc)?,"document":doc}),
                    );
                    Ok::<(), EngineError>(())
                }).await?,
                Command::Publish {
                    record,
                    preview_hash,
                    document,
                } => {
                    return Box::pin(async move {
                    validate_snippet(&document, now.timestamp())?;
                    let held = self.social_post(&record, actor).await?;
                    if held["published"] == serde_json::to_value(&document)?
                        && document_hash("snippet", &document)? == preview_hash
                    {
                        outcome.data = Some(
                            json!({"record":record,"document":document,"status":"already queued for selected services"}),
                        );
                        return Ok(outcome);
                    }
                    if now.timestamp().saturating_sub(document.issued_at) > 600 {
                        return Err(invalid("Refresh the publication preview before publishing"));
                    }
                    let expected = self
                        .social_preview(&record, actor, document.state, document.issued_at)
                        .await?;
                    if expected != document || document_hash("snippet", &document)? != preview_hash
                    {
                        return Err(invalid(
                            "The draft or publication identity changed; preview it again",
                        ));
                    }
                    let mut state = self.social_post(&record, actor).await?;
                    let post_draft: PostDraft = serde_json::from_value(state["draft"].clone())?;
                    let (posting_signer, _) = self.social_post_signer(&state, &post_draft).await?;
                    let previous = state["published"].clone();
                    state[format!("revision_{preview_hash}")] = serde_json::to_value(&document)?;
                    state["published"] = serde_json::to_value(&document)?;
                    let signer = self.signer.lock().await.clone();
                    let mut tx = self.social_write_tx().await?;
                    let stored: String = store::sqlx::query_scalar(
                        "SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?",
                    )
                    .bind(&record)
                    .bind(PUBLICATION_NAMESPACE)
                    .fetch_one(&mut *tx)
                    .await?;
                    let mut current: Value = serde_json::from_str(&stored)?;
                    if current["published"] != previous
                        || current["draft"] != state["draft"]
                        || current["secret"] != state["secret"]
                    {
                        return Err(invalid(
                            "Another device updated this post; refresh its preview",
                        ));
                    }
                    current[format!("revision_{preview_hash}")] = serde_json::to_value(&document)?;
                    current["published"] = serde_json::to_value(&document)?;
                    current["public_id"] = json!(document.id);
                    if let Some(authority) = &document.profile {
                        current["posting_authority"] = json!({"secret":B64.encode(posting_signer.secret_bytes()),"authority":authority});
                    }
                    state = current;
                    source::trim_publication_state(&mut state)?;
                    store::records::set_extension_on(
                        &mut tx,
                        &record,
                        PUBLICATION_NAMESPACE,
                        &state,
                    )
                    .await?;
                    store::social::put_snippet_on(
                        &mut tx,
                        &document,
                        &preview_hash,
                        "local",
                        now.timestamp(),
                    )
                    .await?;
                    store::social::enqueue_on(
                        &mut tx,
                        "snippet",
                        &preview_hash,
                        &serde_json::to_string(&document)?,
                        &document.destinations,
                        document.expires_at,
                    )
                    .await?;
                    let fact = crate::append::append_one_in_transaction(
                        &mut tx,
                        nucleus::NewFact {
                            uid: None,
                            record_uid: record.clone(),
                            delta: store::exact::zero(),
                            at: Some(now),
                            actor_uid: actor.map(str::to_owned),
                            cause: nucleus::Cause::user_edit(),
                            payload: Some(
                                json!({"social":"published","revision":document.revision})
                                    .to_string(),
                            ),
                        },
                        now,
                        signer.as_ref(),
                    )
                    .await?;
                    tx.commit().await?;
                    outcome.facts.extend(fact);
                    outcome.data = Some(
                        json!({"record":record,"document":document,"status":if document.destinations.is_empty() { "Saved in your local discovery cache; no publication hosts selected" } else { "Saved and queued for the selected services" }}),
                    );
                    self.notify_query_changed();
                    Ok(outcome)
                    }).await;
                }
                Command::SaveProfile {
                    fields,
                    parents,
                    destinations,
                } => {
                    return self
                        .social_save_profile(
                            fields,
                            parents,
                            destinations,
                            PostState::Active,
                            actor,
                            now,
                            None,
                            None,
                        )
                        .await;
                }
                Command::RotateProfileAuthority => {
                    return self.social_rotate_profile_authority(actor, now).await;
                }
                Command::Search { query, services } => {
                    text(&query.text, 160, true).map_err(invalid)?;
                    let mut results = self.social_search(query, services, now.timestamp()).await?;
                    results["can_manage_services"] = json!(self.require_permission(actor, "organ:update").await.is_ok() && self.social_require_local_write().await.is_ok());
                    outcome.data = Some(results);
                }
                Command::FetchProfile { organ, services } => {
                    outcome.data = Some(
                        self.social_fetch_profile(&organ, services, now.timestamp())
                            .await?,
                    );
                }
                Command::WithdrawProfile { parents } => {
                    let organ = store::organs::local(&self.store.pool)
                        .await?
                        .ok_or_else(|| invalid("No local Organ"))?;
                    let state = store::records::get_extension(
                        &self.store.pool,
                        &organ.uid,
                        PROFILE_NAMESPACE,
                    )
                    .await?
                    .ok_or_else(|| invalid("No public profile to withdraw"))?;
                    let fields = serde_json::from_value(state["published"]["fields"].clone())?;
                    let destinations =
                        serde_json::from_value(state["published"]["destinations"].clone())?;
                    return self
                        .social_save_profile(
                            fields,
                            parents,
                            destinations,
                            PostState::Withdrawn,
                            actor,
                            now,
                            None,
                            None,
                        )
                        .await;
                }
            }
            self.notify_query_changed();
            Ok(outcome)
        })))
    }
}
