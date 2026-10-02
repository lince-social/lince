use super::*;
use nucleus::social::requests::*;
use store::sqlx::{Row, Sqlite, Transaction};

pub(super) async fn anchor_control(
    tx: &mut Transaction<'_, Sqlite>,
    control: &OwnerControl,
) -> Result<(), EngineError> {
    let generation = control
        .generation
        .parse::<i64>()
        .map_err(|_| invalid("Invalid reply authority generation"))?;
    let floor: Option<i64> =
        store::sqlx::query_scalar("SELECT generation FROM social_owner_control WHERE owner=?")
            .bind(&control.owner_key)
            .fetch_optional(&mut **tx)
            .await?;
    if floor.is_some_and(|held| held > generation) {
        return Err(invalid("These private reply keys were revoked"));
    }
    store::sqlx::query("INSERT INTO social_owner_control(owner,generation,body,expires_at) VALUES(?,?,?,?) ON CONFLICT(owner) DO UPDATE SET generation=excluded.generation,body=CASE WHEN excluded.generation>generation OR excluded.expires_at>expires_at THEN excluded.body ELSE body END,expires_at=CASE WHEN excluded.generation>generation THEN excluded.expires_at ELSE MAX(expires_at,excluded.expires_at) END")
        .bind(&control.owner_key).bind(generation).bind(serde_json::to_string(control)?).bind(control.expires_at).execute(&mut **tx).await?;
    Ok(())
}

pub(super) async fn require_generation(
    tx: &mut Transaction<'_, Sqlite>,
    owner: &str,
    generation: &str,
) -> Result<(), EngineError> {
    let held: Option<i64> =
        store::sqlx::query_scalar("SELECT generation FROM social_owner_control WHERE owner=?")
            .bind(owner)
            .fetch_optional(&mut **tx)
            .await?;
    let current = generation
        .parse::<i64>()
        .map_err(|_| invalid("Invalid reply authority generation"))?;
    if held.is_some_and(|held| held > current) {
        return Err(invalid("These private reply keys were revoked"));
    }
    Ok(())
}

async fn route_on(
    tx: &mut Transaction<'_, Sqlite>,
    mailbox: &str,
    now: i64,
) -> Result<(CertifiedRoute, String), EngineError> {
    let row =
        store::sqlx::query("SELECT body,CASE WHEN state='closed' OR EXISTS(SELECT 1 FROM social_ended_post e WHERE e.id=social_reply_route.post) OR (post IS NOT NULL AND NOT EXISTS(SELECT 1 FROM social_document d WHERE d.kind='snippet' AND d.id=social_reply_route.post AND d.state='active' AND d.expires_at>?2)) THEN 'closed' ELSE state END AS state FROM social_reply_route WHERE id=?1 AND expires_at>?2")
            .bind(mailbox)
            .bind(now)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| invalid("This private mailbox route is unavailable or expired"))?;
    let route: CertifiedRoute = serde_json::from_str(&row.get::<String, _>("body"))?;
    require_generation(tx, &route.control.owner_key, &route.control.generation).await?;
    Ok((route, row.get("state")))
}

async fn access_on(
    tx: &mut Transaction<'_, Sqlite>,
    access: &MailboxAccess,
    now: i64,
) -> Result<CertifiedRoute, EngineError> {
    let (route, _) = route_on(tx, &access.mailbox, now).await?;
    request_auth::validate_access(access, &route, now)?;
    anchor_control(tx, &access.control).await?;
    let sequence: i64 = access
        .sequence
        .parse()
        .map_err(|_| invalid("Invalid pickup sequence"))?;
    let changed = store::sqlx::query("UPDATE social_mailbox_pin SET pickup_sequence=? WHERE mailbox=? AND pickup_key=? AND pickup_sequence<?")
        .bind(sequence).bind(&access.mailbox).bind(&route.route.pickup_key).bind(sequence)
        .execute(&mut **tx).await?;
    if changed.rows_affected() != 1 {
        return Err(invalid(
            "This pickup request was already used or has an older sequence",
        ));
    }
    Ok(route)
}

pub(super) async fn usage_on(tx: &mut Transaction<'_, Sqlite>) -> Result<(i64, i64), EngineError> {
    let (count, bytes): (i64, i64) = store::sqlx::query_as(
        "SELECT SUM(n),SUM(b) FROM (SELECT COUNT(*) n,COALESCE(SUM(length(CAST(body AS BLOB))),0)+COUNT(*)*2048 b FROM social_service_envelope UNION ALL SELECT COUNT(*),COUNT(*)*1024+COALESCE(SUM(length(CAST(receipt AS BLOB))),0) FROM social_service_completed UNION ALL SELECT COUNT(*),COALESCE(SUM(length(CAST(body AS BLOB))),0)+COUNT(*)*1024 FROM social_reply_route UNION ALL SELECT COUNT(*),COUNT(*)*512 FROM social_mailbox_pin UNION ALL SELECT COUNT(*),COALESCE(SUM(length(CAST(body AS BLOB))),0)+COUNT(*)*512 FROM social_owner_control UNION ALL SELECT COUNT(*),COALESCE(SUM(length(CAST(body AS BLOB))),0)+COUNT(*)*512 FROM social_sender_admission UNION ALL SELECT COUNT(*),COUNT(*)*256 FROM social_sender_counter)",
    ).fetch_one(&mut **tx).await?;
    let envelope_count: i64 =
        store::sqlx::query_scalar("SELECT COUNT(*) FROM social_service_envelope")
            .fetch_one(&mut **tx)
            .await?;
    Ok((count + envelope_count * 2, bytes + envelope_count * 4096))
}

pub(super) fn limits(settings: &ServiceSettings, traffic: bool) -> (i64, u64) {
    let entries = i64::from(settings.cache_entries);
    let entry_limit = if traffic {
        entries - (entries / 4).max(8)
    } else {
        entries
    };
    let storage = settings.storage_bytes / 2;
    let byte_limit = if traffic { storage * 3 / 4 } else { storage };
    (entry_limit, byte_limit)
}

async fn capacity(
    tx: &mut Transaction<'_, Sqlite>,
    settings: &ServiceSettings,
    traffic: bool,
) -> Result<(), EngineError> {
    let (count, bytes) = usage_on(tx).await?;
    let (entry_limit, byte_limit) = limits(settings, traffic);
    if count > entry_limit || bytes as u64 > byte_limit {
        return Err(invalid(
            "The mailbox service's configured storage or retained-ledger capacity is full",
        ));
    }
    Ok(())
}

impl Engine {
    pub(super) async fn social_private_service(
        &self,
        node: &str,
        request: PublicRequest,
        settings: &ServiceSettings,
        now: i64,
    ) -> Result<Value, EngineError> {
        let traffic = matches!(&request, PublicRequest::DeliverPrivate { document } if document.envelope.purpose==EnvelopePurpose::Content);
        let mut tx = self.social_write_tx().await?;
        store::sqlx::query("DELETE FROM social_service_envelope WHERE expires_at<=?")
            .bind(now)
            .execute(&mut *tx)
            .await?;
        store::sqlx::query("DELETE FROM social_service_completed WHERE expires_at<=?")
            .bind(now)
            .execute(&mut *tx)
            .await?;
        store::sqlx::query("DELETE FROM social_sender_counter WHERE expires_at<=?")
            .bind(now)
            .execute(&mut *tx)
            .await?;
        let response = match request {
            PublicRequest::UpdateReplyAuthority { document } => {
                request_auth::validate_control(&document, now)?;
                let known: bool = store::sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM social_owner_control WHERE owner=?)",
                )
                .bind(&document.owner_key)
                .fetch_one(&mut *tx)
                .await?;
                if !known {
                    return Err(invalid(
                        "Register this reply owner at the selected host first",
                    ));
                }
                anchor_control(&mut tx, &document).await?;
                store::sqlx::query("DELETE FROM social_service_envelope WHERE sender=? AND CAST(json_extract(body,'$.authorization.control.generation') AS INTEGER)<?")
                    .bind(&document.owner_key).bind(document.generation.parse::<i64>().map_err(|_| invalid("Invalid reply generation"))?)
                    .execute(&mut *tx).await?;
                json!({"accepted":true,"service":node,"hash":document_hash("reply-owner", &document)?,"expires_at":document.expires_at})
            }
            PublicRequest::LookupReplyRoutes { owner, after } => {
                request_auth::ed_key(&owner)?;
                if after
                    .as_ref()
                    .is_some_and(|id| !nucleus::valid_uid(id, "mail"))
                {
                    return Err(invalid("Invalid private route page"));
                }
                let bodies: Vec<String> = store::sqlx::query_scalar(
                    "SELECT r.body FROM social_reply_route r JOIN social_owner_control c ON c.owner=r.owner WHERE r.owner=? AND r.expires_at>? AND CAST(json_extract(r.body,'$.control.generation') AS INTEGER)>=c.generation AND CAST(json_extract(r.body,'$.certificate.expires_at') AS INTEGER)>? AND r.id>? ORDER BY r.id LIMIT 8",
                ).bind(&owner).bind(now).bind(now).bind(after.unwrap_or_default()).fetch_all(&mut *tx).await?;
                let full = bodies.len() == 8;
                let mut next_after = None;
                let mut routes = Vec::new();
                for body in bodies {
                    let route: CertifiedRoute = serde_json::from_str(&body)?;
                    if full {
                        next_after = Some(route.route.mailbox.clone());
                    }
                    if request_auth::validate_route(&route, now).is_ok()
                        && require_generation(&mut tx, &owner, &route.control.generation)
                            .await
                            .is_ok()
                        && route.route.services.iter().any(|host| host == node)
                    {
                        routes.push(route);
                    }
                }
                json!({"service":node,"owner":owner,"routes":routes,"next_after":next_after})
            }
            PublicRequest::InspectPrivate { document } => {
                let hash = request_auth::validate_delivery(&document, now)?;
                require_generation(
                    &mut tx,
                    &document.envelope.sender_owner,
                    &document.authorization.control.generation,
                )
                .await?;
                let completed: Option<(String, Option<String>, String)> = store::sqlx::query_as(
                    "SELECT hash,receipt,stage FROM social_service_completed WHERE id=? AND sender=? AND route=?",
                ).bind(&document.envelope.id).bind(&document.envelope.sender_owner)
                    .bind(&document.envelope.route).fetch_optional(&mut *tx).await?;
                let stored: Option<String> = store::sqlx::query_scalar(
                    "SELECT hash FROM social_service_envelope WHERE id=? AND sender=? AND route=?",
                )
                .bind(&document.envelope.id)
                .bind(&document.envelope.sender_owner)
                .bind(&document.envelope.route)
                .fetch_optional(&mut *tx)
                .await?;
                if completed.as_ref().is_some_and(|(held, _, _)| held != &hash)
                    || stored.as_ref().is_some_and(|held| held != &hash)
                {
                    return Err(invalid("This envelope identity names different ciphertext"));
                }
                let receipt = completed
                    .as_ref()
                    .and_then(|(_, receipt, _)| receipt.as_ref())
                    .map(|receipt| serde_json::from_str::<RecipientReceipt>(receipt))
                    .transpose()?;
                json!({"service":node,"envelope":document.envelope.id,"hash":hash,
                    "stage":completed.as_ref().map(|(_,_,stage)|stage.as_str()).unwrap_or(if stored.is_some() { "stored" } else { "unknown" }),
                    "receipt":receipt})
            }
            PublicRequest::RegisterReplyRoute { document, post } => {
                request_auth::validate_route(&document, now)?;
                if !document
                    .route
                    .services
                    .iter()
                    .any(|service| service == node)
                {
                    return Err(invalid(
                        "This host was not selected for the private reply route",
                    ));
                }
                let post_id = if let Some(post) = post {
                    validate_snippet(&post, now)?;
                    if post.state != PostState::Active
                        || post.reply.as_ref().is_none_or(|reply| {
                            reply.control.owner_key != document.control.owner_key
                        })
                    {
                        return Err(invalid(
                            "The reply owner is not bound to this active public post",
                        ));
                    }
                    let ended: bool = store::sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM social_ended_post WHERE id=?)",
                    )
                    .bind(&post.id)
                    .fetch_one(&mut *tx)
                    .await?;
                    if ended {
                        return Err(invalid("This public post has ended"));
                    }
                    let hash = document_hash("snippet", &post)?;
                    store::social::put_snippet_on(&mut tx, &post, &hash, node, now).await?;
                    let current: String = store::sqlx::query_scalar(
                        "SELECT hash FROM social_document WHERE kind='snippet' AND id=?",
                    )
                    .bind(&post.id)
                    .fetch_one(&mut *tx)
                    .await?;
                    if current != hash {
                        return Err(invalid(
                            "A newer public post already controls this reply route",
                        ));
                    }
                    self.social_capacity(
                        &mut tx,
                        settings.cache_entries,
                        settings.storage_bytes,
                        &post.id,
                        "snippet",
                        0,
                    )
                    .await?;
                    Some(post.id)
                } else {
                    None
                };
                let pin: Option<(String, String, String, String)> = store::sqlx::query_as("SELECT owner,signing_key,identity_key,pickup_key FROM social_mailbox_pin WHERE mailbox=?")
                    .bind(&document.route.mailbox).fetch_optional(&mut *tx).await?;
                let identity = (
                    document.control.owner_key.clone(),
                    document.route.signing_key.clone(),
                    document.route.identity_key.clone(),
                    document.route.pickup_key.clone(),
                );
                if pin.as_ref().is_some_and(|pin| pin != &identity) {
                    return Err(invalid(
                        "A retained private mailbox identity cannot be reassigned",
                    ));
                }
                anchor_control(&mut tx, &document.control).await?;
                store::sqlx::query("INSERT INTO social_mailbox_pin(mailbox,owner,signing_key,identity_key,pickup_key) VALUES(?,?,?,?,?) ON CONFLICT DO NOTHING")
                    .bind(&document.route.mailbox).bind(&identity.0).bind(&identity.1).bind(&identity.2).bind(&identity.3).execute(&mut *tx).await?;
                let previous: Option<(Option<String>, String)> =
                    store::sqlx::query_as("SELECT post,state FROM social_reply_route WHERE id=?")
                        .bind(&document.route.mailbox)
                        .fetch_optional(&mut *tx)
                        .await?;
                if previous.as_ref().is_some_and(|(post, _)| {
                    post.is_some() && post_id.is_some() && post != &post_id
                }) {
                    return Err(invalid(
                        "A registered post reply route cannot be moved to another post",
                    ));
                }
                let state = if !document.accepting_introductions {
                    "closed".to_owned()
                } else if post_id.is_some() {
                    "open".to_owned()
                } else {
                    previous.map_or("open".to_owned(), |(_, state)| state)
                };
                store::sqlx::query("INSERT INTO social_reply_route(id,owner,signing_key,pickup_key,body,post,state,created_at,expires_at) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET body=excluded.body,expires_at=excluded.expires_at,post=COALESCE(post,excluded.post),state=excluded.state")
                    .bind(&document.route.mailbox).bind(&document.control.owner_key).bind(&document.route.signing_key).bind(&document.route.pickup_key)
                    .bind(serde_json::to_string(&document)?).bind(post_id).bind(state).bind(now).bind(document.expires_at).execute(&mut *tx).await?;
                json!({"accepted":true,"service":node,"mailbox":document.route.mailbox,"hash":document_hash("reply-route", &document)?,"expires_at":document.expires_at})
            }
            PublicRequest::EndReplyPost { document } => {
                validate_snippet(&document, now)?;
                if document.state != PostState::Withdrawn {
                    return Err(invalid("Only a signed withdrawal ends a reply post"));
                }
                let hash = document_hash("snippet", &document)?;
                store::social::put_snippet_on(&mut tx, &document, &hash, node, now).await?;
                self.social_capacity(
                    &mut tx,
                    settings.cache_entries,
                    settings.storage_bytes,
                    &document.id,
                    "snippet",
                    0,
                )
                .await?;
                let ending: String =
                    store::sqlx::query_scalar("SELECT hash FROM social_ended_post WHERE id=?")
                        .bind(&document.id)
                        .fetch_one(&mut *tx)
                        .await?;
                if ending != hash {
                    return Err(invalid("A newer ending is already retained for this post"));
                }
                store::sqlx::query("UPDATE social_reply_route SET state='closed' WHERE post=?")
                    .bind(&document.id)
                    .execute(&mut *tx)
                    .await?;
                store::sqlx::query("DELETE FROM social_service_envelope WHERE route IN (SELECT id FROM social_reply_route WHERE post=?) AND NOT EXISTS(SELECT 1 FROM social_sender_admission a WHERE a.route=social_service_envelope.route AND a.sender=social_service_envelope.sender AND a.state='accepted' AND a.expires_at>?)")
                    .bind(&document.id).bind(now).execute(&mut *tx).await?;
                json!({"accepted":true,"service":node,"post":document.id,"hash":hash,"expires_at":document.expires_at,"state":"closed"})
            }
            PublicRequest::AdmitPrivateSender { document } => {
                let (route, _) = route_on(&mut tx, &document.mailbox, now).await?;
                request_auth::validate_admission(&document, &route, now)?;
                anchor_control(&mut tx, &document.control).await?;
                let previous: Option<(i64, String)> = store::sqlx::query_as(
                    "SELECT issued_at,body FROM social_sender_admission WHERE route=? AND sender=?",
                )
                .bind(&document.mailbox)
                .bind(&document.sender_owner)
                .fetch_optional(&mut *tx)
                .await?;
                let body = serde_json::to_string(&document)?;
                let previous_window = previous
                    .as_ref()
                    .map(|(_, body)| serde_json::from_str::<SenderAdmission>(body))
                    .transpose()?
                    .map_or(0, |a| a.window);
                if document.window < previous_window {
                    return Err(invalid(
                        "An older introduction window cannot replace the retained admission",
                    ));
                }
                if previous.as_ref().is_some_and(|(at, prior)| {
                    *at > document.issued_at || *at == document.issued_at && prior != &body
                }) {
                    return Err(invalid(
                        "A newer or conflicting sender admission is already retained",
                    ));
                }
                let state = serde_json::to_value(document.state)?;
                if document.state == AdmissionState::Provisional
                    && document.expires_at - document.issued_at > 7 * 86400
                {
                    return Err(invalid("Provisional admission lasts at most seven days"));
                }
                store::sqlx::query("INSERT INTO social_sender_admission(route,sender,state,body,issued_at,expires_at) VALUES(?,?,?,?,?,?) ON CONFLICT(route,sender) DO UPDATE SET state=excluded.state,body=excluded.body,issued_at=excluded.issued_at,expires_at=excluded.expires_at")
                    .bind(&document.mailbox).bind(&document.sender_owner).bind(state.as_str().unwrap_or_default()).bind(body).bind(document.issued_at).bind(document.expires_at).execute(&mut *tx).await?;
                if document.window > previous_window
                    && document.state == AdmissionState::Provisional
                {
                    store::sqlx::query(
                        "DELETE FROM social_sender_counter WHERE route=? AND sender=?",
                    )
                    .bind(&document.mailbox)
                    .bind(&document.sender_owner)
                    .execute(&mut *tx)
                    .await?;
                }
                if matches!(
                    document.state,
                    AdmissionState::Blocked | AdmissionState::Closed
                ) {
                    store::sqlx::query(
                        "DELETE FROM social_service_envelope WHERE route=? AND sender=?",
                    )
                    .bind(&document.mailbox)
                    .bind(&document.sender_owner)
                    .execute(&mut *tx)
                    .await?;
                }
                json!({"accepted":true,"service":node,"mailbox":document.mailbox,"hash":document_hash("sender-admission", &document)?,"expires_at":document.expires_at})
            }
            PublicRequest::DeliverPrivate { document } => {
                let hash = request_auth::validate_delivery(&document, now)?;
                let envelope = &document.envelope;
                let (route, state) = route_on(&mut tx, &envelope.route, now).await?;
                request_auth::validate_route(&route, now)?;
                anchor_control(&mut tx, &document.authorization.control).await?;
                let previous: Option<(String, String)> = store::sqlx::query_as("SELECT hash,'stored' FROM social_service_envelope WHERE id=? UNION ALL SELECT hash,stage FROM social_service_completed WHERE id=? LIMIT 1")
                    .bind(&envelope.id).bind(&envelope.id).fetch_optional(&mut *tx).await?;
                if let Some((held, stage)) = previous {
                    if held != hash {
                        return Err(invalid(
                            "The private envelope identity already names different ciphertext",
                        ));
                    }
                    json!({"accepted":true,"service":node,"envelope":envelope.id,"hash":hash,"expires_at":envelope.expires_at,"stage":stage})
                } else {
                    let admission: Option<(String, i64)> = store::sqlx::query_as("SELECT state,expires_at FROM social_sender_admission WHERE route=? AND sender=?")
                        .bind(&envelope.route).bind(&envelope.sender_owner).fetch_optional(&mut *tx).await?;
                    let admission = admission
                        .filter(|(_, expiry)| *expiry > now)
                        .map(|(state, _)| state);
                    if matches!(admission.as_deref(), Some("blocked" | "closed"))
                        || state == "closed" && admission.as_deref() != Some("accepted")
                    {
                        return Err(invalid(
                            "This sender or public introduction route is closed",
                        ));
                    }
                    if envelope.purpose != EnvelopePurpose::Introduction && admission.is_none() {
                        return Err(invalid(
                            "Private replies and controls require a prior introduction and recipient admission",
                        ));
                    }
                    let partition = if envelope.purpose == EnvelopePurpose::Control {
                        "control"
                    } else if admission.as_deref() == Some("accepted") {
                        "trusted"
                    } else {
                        "stranger"
                    };
                    if partition == "stranger"
                        && envelope.expires_at - envelope.created_at > AUTHORITY_LIFETIME
                    {
                        return Err(invalid("Stranger messages expire within seven days"));
                    }
                    let counts: Option<(i64, i64)> = store::sqlx::query_as("SELECT introductions,provisional FROM social_sender_counter WHERE route=? AND sender=?")
                        .bind(&envelope.route).bind(&envelope.sender_owner).fetch_optional(&mut *tx).await?;
                    let (introductions, provisional) = counts.unwrap_or_default();
                    if partition == "stranger"
                        && (envelope.purpose == EnvelopePurpose::Content && provisional >= 3
                            || envelope.purpose == EnvelopePurpose::Introduction
                                && introductions >= 1)
                    {
                        return Err(invalid(
                            "This provisional sender has reached its introduction or reply limit",
                        ));
                    }
                    let body = serde_json::to_string(&document)?;
                    if partition == "stranger" && body.len() > MAX_ENVELOPE_BYTES {
                        return Err(invalid(
                            "Accept this sender before receiving larger attachments",
                        ));
                    }
                    let (pending, bytes): (i64, i64) = store::sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(length(CAST(body AS BLOB))),0) FROM social_service_envelope WHERE route=? AND partition=?")
                        .bind(&envelope.route).bind(partition).fetch_one(&mut *tx).await?;
                    let (maximum, maximum_bytes) = if partition == "control" {
                        (32, 256 * 1024)
                    } else if partition == "stranger" {
                        (32, 1024 * 1024)
                    } else {
                        (4096, 64 * 1024 * 1024)
                    };
                    if pending >= maximum || bytes + body.len() as i64 > maximum_bytes {
                        return Err(invalid("This mailbox traffic partition is full"));
                    }
                    if partition == "control" {
                        let held:i64=store::sqlx::query_scalar("SELECT COUNT(*) FROM social_service_envelope WHERE route=? AND sender=? AND partition='control'")
                            .bind(&envelope.route).bind(&envelope.sender_owner).fetch_one(&mut *tx).await?;
                        if held >= 8 {
                            return Err(invalid(
                                "This sender's pending private control partition is full",
                            ));
                        }
                    }
                    if partition == "stranger" {
                        let initial = i64::from(envelope.purpose == EnvelopePurpose::Introduction);
                        let provisional = i64::from(envelope.purpose == EnvelopePurpose::Content);
                        store::sqlx::query("INSERT INTO social_sender_counter(route,sender,introductions,provisional,expires_at) VALUES(?,?,?,?,?) ON CONFLICT(route,sender) DO UPDATE SET introductions=introductions+excluded.introductions,provisional=provisional+excluded.provisional,expires_at=MAX(expires_at,excluded.expires_at)")
                            .bind(&envelope.route).bind(&envelope.sender_owner).bind(initial).bind(provisional).bind(now + 7 * 86400).execute(&mut *tx).await?;
                    }
                    store::sqlx::query("INSERT INTO social_service_envelope(id,route,sender,hash,body,partition,created_at,expires_at) VALUES(?,?,?,?,?,?,?,?)")
                        .bind(&envelope.id).bind(&envelope.route).bind(&envelope.sender_owner).bind(&hash).bind(body).bind(partition).bind(now).bind(envelope.expires_at).execute(&mut *tx).await?;
                    json!({"accepted":true,"service":node,"envelope":envelope.id,"hash":hash,"expires_at":envelope.expires_at,"stage":"stored"})
                }
            }
            PublicRequest::CollectPrivate { access } => {
                if !access.envelopes.is_empty() {
                    return Err(invalid("Collection cannot acknowledge messages"));
                }
                access_on(&mut tx, &access, now).await?;
                store::sqlx::query("DELETE FROM social_service_envelope WHERE route=? AND EXISTS(SELECT 1 FROM social_owner_control c WHERE c.owner=social_service_envelope.sender AND c.generation>CAST(json_extract(social_service_envelope.body,'$.authorization.control.generation') AS INTEGER))")
                    .bind(&access.mailbox).execute(&mut *tx).await?;
                let rows = store::sqlx::query("SELECT id,body,created_at FROM social_service_envelope WHERE route=? ORDER BY next_pickup,CASE partition WHEN 'control' THEN 0 WHEN 'trusted' THEN 1 ELSE 2 END,created_at,id LIMIT 8")
                    .bind(&access.mailbox).fetch_all(&mut *tx).await?;
                let mut envelopes = Vec::new();
                let previous:i64=store::sqlx::query_scalar("SELECT COALESCE(MAX(next_pickup),0) FROM social_service_envelope WHERE route=?").bind(&access.mailbox).fetch_one(&mut *tx).await?;
                let picked_at = now.max(
                    previous
                        .checked_add(1)
                        .ok_or_else(|| invalid("Mailbox collection counter exhausted"))?,
                );
                for row in &rows {
                    let document: PrivateDelivery =
                        serde_json::from_str(&row.get::<String, _>("body"))?;
                    if require_generation(
                        &mut tx,
                        &document.envelope.sender_owner,
                        &document.authorization.control.generation,
                    )
                    .await
                    .is_err()
                    {
                        continue;
                    }
                    let item =
                        json!({"document":document,"accepted_at":row.get::<i64, _>("created_at")});
                    let length = serde_json::to_vec(&item)?.len();
                    let total: usize = envelopes
                        .iter()
                        .map(|item: &Value| {
                            serde_json::to_vec(item)
                                .map_or(MAX_PRIVATE_FRAME_BYTES, |bytes| bytes.len())
                        })
                        .sum();
                    if total + length + 2048 > MAX_PRIVATE_FRAME_BYTES {
                        break;
                    }
                    envelopes.push(item);
                    store::sqlx::query(
                        "UPDATE social_service_envelope SET next_pickup=? WHERE id=?",
                    )
                    .bind(picked_at)
                    .bind(row.get::<String, _>("id"))
                    .execute(&mut *tx)
                    .await?;
                }
                json!({"service":node,"mailbox":access.mailbox,"envelopes":envelopes,"more":rows.len()==8})
            }
            request @ (PublicRequest::AcknowledgePrivate { .. }
            | PublicRequest::DiscardPrivate { .. }) => {
                let (access, receipts, expected, stage) = match request {
                    PublicRequest::AcknowledgePrivate { access, receipts } => (
                        access,
                        receipts,
                        ReceiptStage::RecipientDurable,
                        "recipient-durable",
                    ),
                    PublicRequest::DiscardPrivate { access, receipts } => (
                        access,
                        receipts,
                        ReceiptStage::RecipientRefused,
                        "recipient-refused",
                    ),
                    _ => unreachable!(),
                };
                let route = access_on(&mut tx, &access, now).await?;
                if access.envelopes.is_empty() || receipts.len() != access.envelopes.len() {
                    return Err(invalid(
                        "Acknowledge exactly the committed message receipts",
                    ));
                }
                let mut seen = std::collections::HashSet::new();
                for receipt in &receipts {
                    if !seen.insert(&receipt.envelope)
                        || !access.envelopes.contains(&receipt.envelope)
                        || receipt.stage != expected
                        || receipt.at.abs_diff(now) > 300
                        || receipt.certificate != access.certificate
                        || !crate::roster::verify_with(
                            &receipt.certificate.signing_key,
                            &signing_bytes("recipient-receipt", receipt)?,
                            &receipt.signature,
                        )
                    {
                        return Err(invalid(
                            "The recipient durable-import receipt cannot be verified",
                        ));
                    }
                    let held: Option<(String, String)> = store::sqlx::query_as(
                        "SELECT hash,body FROM social_service_envelope WHERE id=? AND route=?",
                    )
                    .bind(&receipt.envelope)
                    .bind(&route.route.mailbox)
                    .fetch_optional(&mut *tx)
                    .await?;
                    if let Some((hash, body)) = held {
                        let document: PrivateDelivery = serde_json::from_str(&body)?;
                        if hash != receipt.envelope_hash
                            || document.envelope.message != receipt.message
                            || document.envelope.content_hash != receipt.content_hash
                        {
                            return Err(invalid(
                                "This receipt does not describe the stored message",
                            ));
                        }
                        store::sqlx::query("INSERT INTO social_service_completed(id,route,sender,hash,expires_at,receipt,stage) VALUES(?,?,?,?,?,?,?) ON CONFLICT DO NOTHING")
                            .bind(&receipt.envelope).bind(&route.route.mailbox).bind(&document.envelope.sender_owner).bind(&hash)
                            .bind(document.envelope.expires_at).bind(serde_json::to_string(receipt)?).bind(stage).execute(&mut *tx).await?;
                        store::sqlx::query("DELETE FROM social_service_envelope WHERE id=?")
                            .bind(&receipt.envelope)
                            .execute(&mut *tx)
                            .await?;
                    } else {
                        let done: Option<(String, String)> = store::sqlx::query_as(
                            "SELECT hash,stage FROM social_service_completed WHERE id=? AND route=?",
                        )
                        .bind(&receipt.envelope)
                        .bind(&route.route.mailbox)
                        .fetch_optional(&mut *tx)
                        .await?;
                        if done.as_ref().is_none_or(|(hash, held_stage)| {
                            hash != &receipt.envelope_hash || held_stage != stage
                        }) {
                            return Err(invalid("This mailbox has no matching completed envelope"));
                        }
                    }
                }
                json!({"accepted":true,"service":node,"mailbox":access.mailbox,"stage":stage,"envelopes":access.envelopes})
            }
            _ => return Err(invalid("Unsupported private mailbox request")),
        };
        capacity(&mut tx, settings, traffic).await?;
        tx.commit().await?;
        Ok(response)
    }
}
