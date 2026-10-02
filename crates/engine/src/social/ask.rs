use super::*;
use nucleus::social::ask::{self as contract, ContactConsent, Reply, Request, Settings};

pub(super) fn validate_request(request: &Request, now: i64) -> Result<(), EngineError> {
    validate_search(&request.query)?;
    if !nucleus::valid_uid(&request.id, "ask")
        || request.query.after.is_some()
        || request.issued_at <= 0
        || request.issued_at > now + 5
        || !(1..=30).contains(&request.deadline.saturating_sub(request.issued_at))
        || request.deadline <= now
        || request.deadline > now + 35
        || !(1..=contract::MAX_WORK).contains(&request.work)
        || !(1024..=contract::MAX_BYTES).contains(&request.bytes)
        || !(1..=contract::MAX_RESULTS).contains(&request.results)
        || request.depth > contract::MAX_DEPTH
    {
        return Err(invalid(
            "Use a fresh bounded contact query without a page cursor",
        ));
    }
    Ok(())
}

pub(super) fn validate_reply(
    reply: &Reply,
    request: &Request,
    now: i64,
) -> Result<(), EngineError> {
    if reply.id != request.id
        || reply.documents.len() > usize::from(request.results)
        || serde_json::to_vec(&reply.documents)?.len() > request.bytes as usize
    {
        return Err(invalid("Contact answers exceed the assigned result budget"));
    }
    for document in &reply.documents {
        validate_snippet(document, now)?;
        if document.state != PostState::Active
            || !document.redistribute
            || document.destinations.is_empty()
        {
            return Err(invalid(
                "Contact answers require permitted active public announcements",
            ));
        }
    }
    Ok(())
}

pub(super) fn clip(reply: &mut Reply, request: &Request) -> Result<(), EngineError> {
    let original = reply.documents.len();
    reply.documents.truncate(usize::from(request.results));
    while serde_json::to_vec(&reply.documents)?.len() > request.bytes as usize {
        if reply.documents.pop().is_none() {
            break;
        }
    }
    reply.partial |= reply.documents.len() != original;
    Ok(())
}

pub(super) fn split(
    request: &Request,
    used_results: usize,
    used_bytes: usize,
    peers: usize,
) -> Vec<Request> {
    let work = request.work.saturating_sub(1);
    let results = usize::from(request.results).saturating_sub(used_results);
    let bytes = (request.bytes as usize).saturating_sub(used_bytes);
    let count = peers
        .min(3)
        .min(usize::from(work))
        .min(results)
        .min(bytes / 1024);
    if request.depth == 0 || count == 0 {
        return Vec::new();
    }
    (0..count)
        .map(|index| Request {
            work: (usize::from(work) / count + usize::from(index < usize::from(work) % count))
                as u8,
            results: (results / count + usize::from(index < results % count)) as u8,
            bytes: (bytes / count + usize::from(index < bytes % count)) as u32,
            depth: request.depth - 1,
            ..request.clone()
        })
        .collect()
}

pub(super) async fn allowed_on(
    tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
    document: &Snippet,
) -> Result<bool, EngineError> {
    if validate_snippet(document, nucleus::execution::now().timestamp()).is_err()
        || document.state != PostState::Active
    {
        return Ok(false);
    }
    let conflict:bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM social_document WHERE kind='snippet' AND id=? AND state='conflict')")
        .bind(&document.id).fetch_one(&mut **tx).await?;
    if conflict {
        return Ok(false);
    }
    let ended: bool =
        store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM social_ended_post WHERE id=?)")
            .bind(&document.id)
            .fetch_one(&mut **tx)
            .await?;
    if ended {
        return Ok(false);
    }
    let removed: bool = store::sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM social_listing_removal WHERE post=?)",
    )
    .bind(&document.id)
    .fetch_one(&mut **tx)
    .await?;
    if removed {
        return Ok(false);
    }
    if let Some(authority) = &document.anonymous {
        let held: Option<(i64, String)> = store::sqlx::query_as(
            "SELECT generation,editor FROM social_posting_authority WHERE owner=?",
        )
        .bind(&authority.owner_key)
        .fetch_optional(&mut **tx)
        .await?;
        let generation: i64 = authority
            .generation
            .parse()
            .map_err(|_| invalid("Invalid posting generation"))?;
        if held.is_some_and(|(floor, editor)| {
            floor > generation || floor == generation && editor != authority.editor_key
        }) {
            return Ok(false);
        }
    }
    if let Some(authority) = &document.profile {
        let held: Option<(i64, String, String)> = store::sqlx::query_as(
            "SELECT generation,editor_key,root_key FROM social_profile_authority WHERE organ=?",
        )
        .bind(&authority.organ)
        .fetch_optional(&mut **tx)
        .await?;
        let generation: i64 = authority
            .generation
            .parse()
            .map_err(|_| invalid("Invalid editing generation"))?;
        if held.is_some_and(|(floor, editor, root)| {
            floor > generation
                || floor == generation && editor != authority.editor_key
                || root != authority.root_key
                    && !authority
                        .successions
                        .iter()
                        .any(|edge| edge.old_key == root)
        }) {
            return Ok(false);
        }
    }
    let current: Option<(String, i64, i64, String)> = store::sqlx::query_as(
        "SELECT state,generation,revision,hash FROM social_document WHERE kind='snippet' AND id=?",
    )
    .bind(&document.id)
    .fetch_optional(&mut **tx)
    .await?;
    let generation = document
        .anonymous
        .as_ref()
        .map(|authority| &authority.generation)
        .or_else(|| {
            document
                .profile
                .as_ref()
                .map(|authority| &authority.generation)
        })
        .map(|generation| generation.parse::<i64>())
        .transpose()
        .map_err(|_| invalid("Invalid posting generation"))?
        .unwrap_or(1);
    let revision = document
        .revision
        .parse::<i64>()
        .map_err(|_| invalid("Invalid public revision"))?;
    let hash = document_hash("snippet", document)?;
    Ok(
        current.is_none_or(|(state, held_generation, held_revision, held_hash)| {
            (held_generation, held_revision) < (generation, revision)
                || (held_generation, held_revision) == (generation, revision)
                    && state == "active"
                    && held_hash == hash
        }),
    )
}

impl Engine {
    pub(super) async fn social_ask_filter(&self, reply: &mut Reply) -> Result<(), EngineError> {
        let mut tx = self.social_write_tx().await?;
        let mut retained = Vec::new();
        for document in std::mem::take(&mut reply.documents) {
            if allowed_on(&mut tx, &document).await? {
                retained.push(document);
            } else {
                reply.partial = true;
            }
        }
        tx.commit().await?;
        reply.documents = retained;
        Ok(())
    }
    pub(super) async fn social_ask_settings(&self) -> Result<Settings, EngineError> {
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        let settings: Settings =
            store::records::get_extension(&self.store.pool, &cell.uid, contract::NAMESPACE)
                .await?
                .map(serde_json::from_value)
                .transpose()?
                .unwrap_or_default();
        let mut seen = std::collections::BTreeSet::new();
        if settings.peers.len() > contract::MAX_PEERS
            || settings
                .peers
                .iter()
                .any(|peer| !nucleus::valid_uid(&peer.organ, "r") || !seen.insert(&peer.organ))
        {
            return Err(invalid(
                "Choose at most thirty-two distinct valid query contacts",
            ));
        }
        Ok(settings)
    }

    pub(super) async fn social_configure_ask(
        &self,
        enabled: Option<bool>,
        choice: Option<ContactConsent>,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        let mut tx = self.social_write_tx().await?;
        let held = owner::extension_on(&mut tx, &cell.uid, contract::NAMESPACE).await?;
        let mut settings: Settings = if held == json!({}) {
            Settings::default()
        } else {
            serde_json::from_value(held)?
        };
        if let Some(enabled) = enabled {
            settings.enabled = enabled;
        }
        if let Some(choice) = choice {
            if !nucleus::valid_uid(&choice.organ, "r") {
                return Err(invalid("Choose a valid known contact"));
            }
            let known: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organ_contact c JOIN record r ON r.uid=c.record_uid WHERE c.record_uid=? AND c.trust='known' AND r.deleted_at IS NULL)").bind(&choice.organ).fetch_one(&mut *tx).await?;
            if (choice.ask || choice.answer || choice.forward) && !known {
                return Err(invalid("Only known contacts can receive query consent"));
            }
            settings.peers.retain(|peer| peer.organ != choice.organ);
            if choice.ask || choice.answer || choice.forward {
                settings.peers.push(choice);
            }
        }
        if settings.peers.len() > contract::MAX_PEERS {
            return Err(invalid("Choose at most thirty-two query contacts"));
        }
        settings.peers.sort_by(|a, b| a.organ.cmp(&b.organ));
        store::records::set_extension_on(
            &mut tx,
            &cell.uid,
            contract::NAMESPACE,
            &serde_json::to_value(settings)?,
        )
        .await?;
        tx.commit().await?;
        self.notify_query_changed();
        self.social_ask_view(actor).await
    }

    pub(super) async fn social_ask_peer(
        &self,
        endpoint: &str,
        outgoing: bool,
    ) -> Result<ContactConsent, EngineError> {
        let settings = self.social_ask_settings().await?;
        let contact = store::organs::contact_by_node_id(&self.store.pool, endpoint)
            .await?
            .ok_or_else(|| invalid("Contact queries require a pinned known endpoint"))?;
        let choice = settings
            .peers
            .into_iter()
            .find(|peer| peer.organ == contact.record_uid)
            .ok_or_else(|| invalid("This contact has not consented to queries"))?;
        if !settings.enabled
            || contact.trust != "known"
            || contact.unreachable_since.is_some()
            || store::records::get(&self.store.pool, &contact.record_uid)
                .await?
                .is_none()
            || if outgoing {
                !choice.ask
            } else {
                !choice.answer
            }
        {
            return Err(invalid(
                "This device or contact has not consented to this query direction",
            ));
        }
        Ok(choice)
    }

    pub(super) async fn social_ask_peer_on(
        &self,
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        endpoint: &str,
        organ: &str,
        outgoing: bool,
    ) -> Result<ContactConsent, EngineError> {
        let cell: String = store::sqlx::query_scalar(
            "SELECT uid FROM record WHERE slug=? AND kind=? AND deleted_at IS NULL LIMIT 1",
        )
        .bind(store::cells::LOCAL_CELL_SLUG)
        .bind(nucleus::RecordKind::Device.as_str())
        .fetch_one(&mut **tx)
        .await?;
        let body = owner::extension_on(tx, &cell, contract::NAMESPACE).await?;
        let settings: Settings = if body == json!({}) {
            Settings::default()
        } else {
            serde_json::from_value(body)?
        };
        let pinned: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organ_contact c JOIN record r ON r.uid=c.record_uid WHERE c.record_uid=? AND c.node_id=? AND c.trust='known' AND c.unreachable_since IS NULL AND r.deleted_at IS NULL)")
            .bind(organ).bind(endpoint).fetch_one(&mut **tx).await?;
        let choice = settings.peers.into_iter().find(|peer| peer.organ == organ);
        if !settings.enabled
            || !pinned
            || choice
                .as_ref()
                .is_none_or(|peer| if outgoing { !peer.ask } else { !peer.answer })
        {
            return Err(invalid(
                "The original query contact or its consent changed; review before retrying",
            ));
        }
        Ok(choice.unwrap())
    }

    pub(super) async fn social_ask_eligible(
        &self,
        selected: Option<&[String]>,
        except: Option<&str>,
    ) -> Result<Vec<(String, String)>, EngineError> {
        let settings = self.social_ask_settings().await?;
        if !settings.enabled {
            return Ok(Vec::new());
        }
        let mut peers = Vec::new();
        for contact in store::organs::contacts(&self.store.pool).await? {
            if contact.trust == "known"
                && contact.unreachable_since.is_none()
                && selected.is_none_or(|selected| selected.contains(&contact.record_uid))
                && settings
                    .peers
                    .iter()
                    .any(|choice| choice.organ == contact.record_uid && choice.ask)
                && let Some(endpoint) = contact.node_id.filter(|endpoint| {
                    endpoint.parse::<iroh::EndpointId>().is_ok()
                        && except != Some(endpoint.as_str())
                })
            {
                peers.push((contact.record_uid, endpoint));
            }
        }
        let mut random = Vec::new();
        for peer in peers {
            let mut bytes = [0; 8];
            getrandom::fill(&mut bytes)
                .map_err(|_| invalid("Secure query peer selection is unavailable"))?;
            random.push((u64::from_le_bytes(bytes), peer));
        }
        random.sort_by_key(|(key, _)| *key);
        Ok(random.into_iter().take(3).map(|(_, peer)| peer).collect())
    }

    pub(super) async fn social_ask_local(&self, request: &Request) -> Result<Reply, EngineError> {
        let now = nucleus::execution::now().timestamp();
        let mut reply = Reply {
            id: request.id.clone(),
            ..Default::default()
        };
        for row in store::social::search(&self.store.pool, &request.query, now).await? {
            let document: Snippet = serde_json::from_value(row["document"].clone())?;
            if document.redistribute
                && !document.destinations.is_empty()
                && validate_snippet(&document, now).is_ok()
            {
                reply.documents.push(document);
            }
        }
        clip(&mut reply, request)?;
        Ok(reply)
    }

    pub(super) async fn social_ask_receive(
        &self,
        source: &str,
        node: &str,
        request: Request,
        now: i64,
    ) -> Result<Value, EngineError> {
        validate_request(&request, now)?;
        let consent = self.social_ask_peer(source, false).await?;
        let binding = document_hash(
            "contact-query",
            &json!({"query":request.query,"issued_at":request.issued_at,"deadline":request.deadline}),
        )?;
        let mut tx = self.social_write_tx().await?;
        store::sqlx::query("DELETE FROM social_ask_seen WHERE deadline<=?")
            .bind(now - 300)
            .execute(&mut *tx)
            .await?;
        let held: Option<(String, Option<String>)> =
            store::sqlx::query_as("SELECT binding,reply FROM social_ask_seen WHERE id=?")
                .bind(&request.id)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some((previous, body)) = held {
            if previous != binding {
                return Err(invalid(
                    "This request ID already belongs to another question",
                ));
            }
            tx.commit().await?;
            let mut reply = if let Some(body) = body {
                serde_json::from_str::<Reply>(&body)?
            } else {
                let mut reply = self.social_ask_local(&request).await?;
                reply.partial = true;
                reply
            };
            clip(&mut reply, &request)?;
            self.social_ask_filter(&mut reply).await?;
            if self.social_ask_peer(source, false).await?.organ != consent.organ {
                return Err(invalid(
                    "The pinned query contact changed while preparing its answer",
                ));
            }
            if nucleus::execution::now().timestamp() >= request.deadline {
                return Err(invalid("This contact question has expired"));
            }
            return Ok(json!({"service":node,"reply":reply}));
        }
        let (count, active, bytes): (i64,i64,i64) = store::sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(reply IS NULL AND deadline>?),0),COALESCE(SUM(CASE WHEN reply IS NULL THEN reserved ELSE length(CAST(reply AS BLOB)) END),0) FROM social_ask_seen").bind(now).fetch_one(&mut *tx).await?;
        let source_active: i64 = store::sqlx::query_scalar(
            "SELECT COUNT(*) FROM social_ask_seen WHERE source=? AND reply IS NULL AND deadline>?",
        )
        .bind(source)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        if count >= 64
            || active >= 8
            || source_active >= 2
            || bytes + i64::from(request.bytes) + 1024 > 4 * 1024 * 1024
        {
            return Err(invalid(
                "Contact query admission or saved reply capacity is full",
            ));
        }
        store::sqlx::query(
            "INSERT INTO social_ask_seen(id,binding,source,deadline,reserved) VALUES(?,?,?,?,?)",
        )
        .bind(&request.id)
        .bind(&binding)
        .bind(source)
        .bind(request.deadline)
        .bind(i64::from(request.bytes) + 1024)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        let mut reply = self.social_ask_local(&request).await?;
        if consent.forward {
            let peers = self.social_ask_eligible(None, Some(source)).await?;
            let (answers, failed) = self
                .social_ask_children(
                    &request,
                    &reply,
                    &peers,
                    None,
                    Some((source, &consent.organ)),
                )
                .await?;
            reply.documents.extend(answers);
            reply.partial |= failed;
        }
        clip(&mut reply, &request)?;
        self.social_ask_filter(&mut reply).await?;
        if self.social_ask_peer(source, false).await?.organ != consent.organ {
            return Err(invalid(
                "The pinned query contact changed while preparing its answer",
            ));
        }
        if nucleus::execution::now().timestamp() >= request.deadline {
            return Err(invalid("This contact question has expired"));
        }
        let mut tx = self.social_write_tx().await?;
        self.social_require_local_write_on(&mut tx).await?;
        self.social_ask_peer_on(&mut tx, source, &consent.organ, false)
            .await?;
        if nucleus::execution::now().timestamp() >= request.deadline {
            return Err(invalid("This contact question has expired"));
        }
        store::sqlx::query("UPDATE social_ask_seen SET reply=? WHERE id=? AND binding=? AND source=? AND deadline>?")
            .bind(serde_json::to_string(&reply)?)
            .bind(&request.id)
            .bind(&binding)
            .bind(source)
            .bind(nucleus::execution::now().timestamp())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(json!({"service":node,"reply":reply}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_lifetime_stays_thirty_seconds_with_bounded_clock_skew() {
        let request = Request {
            id: nucleus::new_uid("ask"),
            query: Search::default(),
            issued_at: 100,
            deadline: 130,
            work: 12,
            bytes: contract::MAX_BYTES,
            results: 50,
            depth: 2,
        };
        assert!(validate_request(&request, 99).is_ok());
        assert!(validate_request(&request, 94).is_err());
        assert!(validate_request(&request, 130).is_err());
        assert!(
            validate_request(
                &Request {
                    deadline: 131,
                    ..request
                },
                100
            )
            .is_err()
        );
    }

    #[test]
    fn child_budgets_partition_remaining_work_bytes_and_results() {
        let request = Request {
            id: nucleus::new_uid("ask"),
            query: Search::default(),
            issued_at: 1,
            deadline: 30,
            work: 12,
            bytes: contract::MAX_BYTES,
            results: 50,
            depth: 2,
        };
        let children = split(&request, 7, 17000, 32);
        assert_eq!(children.len(), 3);
        assert_eq!(
            children
                .iter()
                .map(|child| u32::from(child.work))
                .sum::<u32>(),
            11
        );
        assert_eq!(
            children
                .iter()
                .map(|child| u32::from(child.results))
                .sum::<u32>(),
            43
        );
        assert_eq!(
            children.iter().map(|child| child.bytes).sum::<u32>(),
            request.bytes - 17000
        );
        assert!(children.iter().all(|child| child.id == request.id
            && child.deadline == request.deadline
            && child.depth == 1));
        assert!(split(&request, 50, 0, 3).is_empty());
        assert!(split(&Request { work: 1, ..request }, 0, 0, 3).is_empty());
    }
}
