use super::*;
use nucleus::social::requests::*;
use std::collections::BTreeMap;

pub(super) const RETAINED_NAMESPACE: &str = "lince.social.retained-blocks";
pub(super) const EFFECTIVE_DECISIONS: &str = "WITH clock(now) AS (VALUES (?)), raw AS (
    SELECT e.record_uid AS context,v.key AS peer,CASE WHEN v.type='object' THEN v.value ELSE '{}' END AS body,0 AS override
    FROM record_extension e,json_each(CASE WHEN json_valid(e.fds) THEN e.fds ELSE '{}' END) v WHERE e.namespace='lince.social.blocks'
    UNION ALL SELECT json_extract(CASE WHEN v.type='object' THEN v.value ELSE '{}' END,'$.context'),json_extract(CASE WHEN v.type='object' THEN v.value ELSE '{}' END,'$.peer'),CASE WHEN v.type='object' THEN v.value ELSE '{}' END,1
    FROM record_extension e,json_each(CASE WHEN json_valid(e.fds) THEN e.fds ELSE '{}' END) v WHERE e.namespace='lince.social.retained-blocks'
), ranked AS (SELECT *,ROW_NUMBER() OVER (PARTITION BY context,peer ORDER BY json_extract(body,'$.window') DESC,override DESC) AS position FROM raw)";

fn occupies_slot(entry: &Value, now: i64) -> bool {
    entry["blocked"] == true
        || entry["window"]
            .as_i64()
            .is_some_and(|at| at.saturating_add(AUTHORITY_LIFETIME + 30 * 86400) > now)
}

async fn live_slots_on(
    tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
    now: i64,
) -> Result<i64, EngineError> {
    let malformed_map: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record_extension WHERE namespace IN ('lince.social.blocks','lince.social.retained-blocks') AND (NOT json_valid(fds) OR json_type(CASE WHEN json_valid(fds) THEN fds ELSE '{}' END)<>'object'))").fetch_one(&mut **tx).await?;
    if malformed_map {
        return Err(invalid(
            "Review malformed retained block maps before starting another window",
        ));
    }
    let sql = format!(
        "{EFFECTIVE_DECISIONS} SELECT COUNT(CASE WHEN json_extract(body,'$.blocked')=1 OR json_extract(body,'$.window')+{}>now THEN 1 END),COUNT(CASE WHEN typeof(context)<>'text' OR typeof(peer)<>'text' OR COALESCE(json_type(body,'$.blocked'),'') NOT IN ('true','false') OR COALESCE(json_type(body,'$.window'),'')<>'integer' OR json_extract(body,'$.window')<=0 THEN 1 END) FROM ranked,clock WHERE position=1",
        AUTHORITY_LIFETIME + 30 * 86400
    );
    let (count, malformed): (i64, i64) = store::sqlx::query_as(&sql)
        .bind(now)
        .fetch_one(&mut **tx)
        .await?;
    if malformed > 0 {
        return Err(invalid(
            "Review malformed retained block decisions before starting another window",
        ));
    }
    Ok(count)
}

pub(super) async fn map_on(
    tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
    context: &str,
) -> Result<Value, EngineError> {
    let organ: String = store::sqlx::query_scalar("SELECT organ_uid FROM record WHERE uid=?")
        .bind(context)
        .fetch_one(&mut **tx)
        .await?;
    let mut map = owner::extension_on(tx, context, BLOCK_NAMESPACE).await?;
    if !map.is_object() {
        return Err(invalid("Invalid private block map"));
    }
    for (peer, entry) in map.as_object().into_iter().flatten() {
        request_auth::ed_key(peer)?;
        if !entry["blocked"].is_boolean() || entry["window"].as_i64().is_none_or(|at| at <= 0) {
            return Err(invalid("Invalid private block decision"));
        }
    }
    let overrides = owner::extension_on(tx, &organ, RETAINED_NAMESPACE).await?;
    for entry in overrides
        .as_object()
        .into_iter()
        .flatten()
        .map(|(_, entry)| entry)
    {
        if entry["context"].as_str() == Some(context) {
            let peer = entry["peer"]
                .as_str()
                .ok_or_else(|| invalid("Invalid retained private block peer"))?;
            request_auth::ed_key(peer)?;
            if !entry["blocked"].is_boolean()
                || entry["window"].as_i64().is_none_or(|window| window <= 0)
            {
                return Err(invalid("Invalid retained private block decision"));
            }
            if entry["window"].as_i64() >= map[peer]["window"].as_i64() {
                map[peer] = entry.clone();
            }
        }
    }
    Ok(map)
}

pub(super) async fn contexts(
    pool: &store::sqlx::SqlitePool,
    organ: &str,
) -> Result<Vec<String>, EngineError> {
    let sql = format!(
        "{EFFECTIVE_DECISIONS} SELECT DISTINCT context FROM ranked,clock WHERE position=1 AND (json_extract(body,'$.blocked')=1 OR json_extract(body,'$.window')+{AUTHORITY_LIFETIME}>now) AND EXISTS(SELECT 1 FROM record r WHERE r.uid=context AND r.organ_uid=?) ORDER BY context LIMIT 257"
    );
    let rows: Vec<String> = store::sqlx::query_scalar(&sql)
        .bind(nucleus::execution::now().timestamp())
        .bind(organ)
        .fetch_all(pool)
        .await?;
    if rows.len() > 256 {
        return Err(invalid(
            "The retained private block contexts exceed their bound",
        ));
    }
    Ok(rows)
}

pub(super) async fn block_on(
    tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
    context: &str,
    peer: &str,
    blocked: bool,
    now: i64,
) -> Result<(), EngineError> {
    let mut map = map_on(tx, context).await?;
    if !map.is_object() {
        map = json!({});
    }
    if !occupies_slot(&map[peer], now) {
        let count = live_slots_on(tx, now).await?;
        if map
            .as_object()
            .is_some_and(|m| m.values().filter(|entry| occupies_slot(entry, now)).count() >= 256)
            || count >= 256
        {
            return Err(invalid("Your retained block list is full"));
        }
    }
    let window = now.max(
        map[peer]["window"]
            .as_i64()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| invalid("Block window exhausted"))?,
    );
    let row: (String, bool) =
        store::sqlx::query_as("SELECT organ_uid,deleted_at IS NOT NULL FROM record WHERE uid=?")
            .bind(context)
            .fetch_one(&mut **tx)
            .await?;
    if row.1 {
        let mut retained = owner::extension_on(tx, &row.0, RETAINED_NAMESPACE).await?;
        retained[format!("{context}:{peer}")] =
            json!({"context":context,"peer":peer,"blocked":blocked,"window":window});
        if serde_json::to_vec(&retained)?.len() > 256 * 1024 {
            return Err(invalid("The retained private block ledger is full"));
        }
        store::records::set_extension_on(tx, &row.0, RETAINED_NAMESPACE, &retained).await?;
    } else {
        map[peer] = json!({"blocked":blocked,"window":window});
        store::records::set_extension_on(tx, context, BLOCK_NAMESPACE, &map).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn permanent_deleted_unblocks_free_live_slots_without_restarting_windows_or_bypassing_reblock_capacity()
     {
        let clock = nucleus::execution::Execution::new([247; 32], 1_790_899_200_000).unwrap();
        clock
            .scope(Box::pin(async {
                let engine = Engine::open_memory().await.unwrap();
                let organ = store::organs::local(&engine.store.pool)
                    .await
                    .unwrap()
                    .unwrap()
                    .uid;
                let mut records = Vec::new();
                for title in ["Permanent decisions", "Fresh intake"] {
                    records.push(
                        engine
                            .social_command(
                                Command::SaveDraft {
                                    record: None,
                                    source: None,
                                    draft: PostDraft {
                                        title: title.into(),
                                        ..Default::default()
                                    },
                                },
                                None,
                                clock.now(),
                            )
                            .await
                            .unwrap()
                            .data
                            .unwrap()["record"]
                            .as_str()
                            .unwrap()
                            .to_owned(),
                    );
                }
                let peers: Vec<String> = (0u16..257)
                    .map(|index| {
                        let mut key = [248; 32];
                        key[..2].copy_from_slice(&index.to_le_bytes());
                        Signer::from_bytes("", "social", key).public_key_b64()
                    })
                    .collect();
                let now = clock.now().timestamp();
                let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
                for peer in &peers[..256] {
                    block_on(&mut tx, &records[0], peer, true, now)
                        .await
                        .unwrap();
                }
                store::records::mark_deleted_on(&mut tx, &records[0])
                    .await
                    .unwrap();
                for peer in &peers[..256] {
                    block_on(&mut tx, &records[0], peer, false, now)
                        .await
                        .unwrap();
                }
                assert_eq!(live_slots_on(&mut tx, now).await.unwrap(), 256);
                tx.commit().await.unwrap();
                let later = now + AUTHORITY_LIFETIME + 30 * 86400 + 2;
                clock.set_time(later * 1000).unwrap();
                assert!(
                    contexts(&engine.store.pool, &organ)
                        .await
                        .unwrap()
                        .is_empty()
                );
                let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
                assert_eq!(live_slots_on(&mut tx, later).await.unwrap(), 0);
                let retained = map_on(&mut tx, &records[0]).await.unwrap();
                assert_eq!(retained.as_object().unwrap().len(), 256);
                assert!(
                    retained
                        .as_object()
                        .unwrap()
                        .values()
                        .all(|entry| entry["blocked"] == false)
                );
                for peer in &peers[..256] {
                    block_on(&mut tx, &records[1], peer, true, later)
                        .await
                        .unwrap();
                }
                assert_eq!(live_slots_on(&mut tx, later).await.unwrap(), 256);
                assert!(
                    block_on(&mut tx, &records[1], &peers[256], true, later)
                        .await
                        .is_err()
                );
                assert!(
                    block_on(&mut tx, &records[0], &peers[0], true, later)
                        .await
                        .is_err()
                );
                assert_eq!(
                    map_on(&mut tx, &records[0]).await.unwrap()[&peers[0]]["blocked"],
                    false
                );
                tx.commit().await.unwrap();
            }))
            .await;
    }

    #[tokio::test]
    async fn original_and_retained_denials_share_one_global_bound_without_double_counting() {
        let engine = Engine::open_memory().await.unwrap();
        let saved = engine
            .social_command(
                Command::SaveDraft {
                    record: None,
                    source: None,
                    draft: PostDraft {
                        title: "Bounded block context".into(),
                        ..Default::default()
                    },
                },
                None,
                nucleus::execution::now(),
            )
            .await
            .unwrap()
            .data
            .unwrap();
        let context = saved["record"].as_str().unwrap();
        let now = nucleus::execution::now().timestamp();
        let peers: Vec<String> = (0u16..257)
            .map(|index| {
                let mut key = [183; 32];
                key[..2].copy_from_slice(&index.to_le_bytes());
                Signer::from_bytes("", "social", key).public_key_b64()
            })
            .collect();
        let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
        for peer in &peers[..256] {
            block_on(&mut tx, context, peer, true, now).await.unwrap();
        }
        assert!(
            block_on(&mut tx, context, &peers[256], true, now)
                .await
                .is_err()
        );
        store::records::mark_deleted_on(&mut tx, context)
            .await
            .unwrap();
        block_on(&mut tx, context, &peers[0], false, now)
            .await
            .unwrap();
        assert!(
            block_on(&mut tx, context, &peers[256], true, now)
                .await
                .is_err()
        );
        let merged = map_on(&mut tx, context).await.unwrap();
        assert_eq!(merged.as_object().unwrap().len(), 256);
        assert_eq!(merged[&peers[0]]["blocked"], false);
        assert_eq!(merged[&peers[1]]["blocked"], true);
        tx.commit().await.unwrap();
    }

    #[tokio::test]
    async fn deleting_a_context_keeps_its_block_visible_and_deliberately_reversible() {
        let engine = Engine::open_memory().await.unwrap();
        let directory = tempfile::tempdir().unwrap();
        engine.set_sealing_keyring_path(directory.path().join("sealing.json"));
        let draft = engine
            .social_command(
                Command::SaveDraft {
                    record: None,
                    source: None,
                    draft: PostDraft {
                        title: "Temporary announcement".into(),
                        ..Default::default()
                    },
                },
                None,
                nucleus::execution::now(),
            )
            .await
            .unwrap()
            .data
            .unwrap();
        let context = draft["record"].as_str().unwrap();
        let peer = Signer::from_bytes("", "social", [182; 32]).public_key_b64();
        let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
        block_on(
            &mut tx,
            context,
            &peer,
            true,
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
        store::records::mark_deleted_on(&mut tx, context)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert!(engine.social_own_record(context, None).await.is_err());
        let visible = engine.social_requests(None, None).await.unwrap();
        assert_eq!(visible["blocks"], json!([{"context":context,"peer":peer}]));
        assert!(
            engine
                .social_block_context_visible(context, Some("unknown actor"))
                .await
                .is_err()
        );
        engine
            .social_unblock_participant(context, &peer, None)
            .await
            .unwrap();
        let after = engine.social_requests(None, None).await.unwrap();
        assert!(after["blocks"].as_array().unwrap().is_empty());
        assert!(
            store::records::get(&engine.store.pool, context)
                .await
                .unwrap()
                .is_none()
        );
        let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
        let retained = map_on(&mut tx, context).await.unwrap();
        assert_eq!(retained[&peer]["blocked"], false);
        tx.commit().await.unwrap();
    }
}

impl Engine {
    pub(super) async fn social_block_context_visible(
        &self,
        context: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        if self.social_own_record(context, actor).await.is_ok() {
            return Ok(());
        }
        self.require_permission(actor, "organ:update").await?;
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let retained:bool=store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid=? AND organ_uid=? AND deleted_at IS NOT NULL)")
            .bind(context).bind(&organ.uid).fetch_one(&self.store.pool).await?;
        if !retained {
            return Err(EngineError::Forbidden(
                "This retained block context does not belong to your Organ".into(),
            ));
        }
        Ok(())
    }

    pub(super) async fn social_unblock_participant(
        &self,
        context: &str,
        peer: &str,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.social_block_context_visible(context, actor).await?;
        request_auth::ed_key(peer)?;
        let mut tx = self.social_write_tx().await?;
        if map_on(&mut tx, context).await?[peer]["blocked"] != true {
            return Err(invalid("This private identity is not blocked"));
        }
        block_on(
            &mut tx,
            context,
            peer,
            false,
            nucleus::execution::now().timestamp(),
        )
        .await?;
        tx.commit().await?;
        self.social_reconcile_private_admissions().await?;
        Ok(
            json!({"status":"Unblock saved for this private identity. Old conversations stay closed. Selected mailboxes update when current device authority is available; the new introduction window remains separate from retained history"}),
        )
    }

    pub async fn social_reconcile_private_admissions(&self) -> Result<usize, EngineError> {
        self.social_require_local_write().await?;
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?
            .uid;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?
            .uid;
        let key = self.social_storage_key().await?;
        let now = nucleus::execution::now().timestamp();
        let roots:Vec<String>=store::sqlx::query_scalar("SELECT e.fds FROM record_extension e JOIN record r ON r.uid=e.record_uid WHERE e.namespace=? AND r.organ_uid=? AND r.deleted_at IS NULL ORDER BY e.record_uid LIMIT 257")
            .bind(PARTICIPANTS_NAMESPACE).bind(&organ).fetch_all(&self.store.pool).await?;
        if roots.len() > 256 {
            return Err(invalid(
                "Archive older Requests before retaining more than 256 conversations",
            ));
        }
        let mut groups: BTreeMap<(String, String), (AdmissionState, i64)> = BTreeMap::new();
        for body in roots {
            let p: ConversationParticipant = serde_json::from_str(&body)?;
            let state = if matches!(
                p.state,
                ConversationState::Accepted | ConversationState::Pending
            ) && p.local_accepted
            {
                AdmissionState::Accepted
            } else if p.state == ConversationState::Pending
                && p.started_at + AUTHORITY_LIFETIME > now
            {
                AdmissionState::Provisional
            } else {
                AdmissionState::Closed
            };
            let entry = groups
                .entry((p.context, p.peer_owner))
                .or_insert((state, p.started_at));
            if state == AdmissionState::Accepted
                || state == AdmissionState::Provisional && entry.0 == AdmissionState::Closed
            {
                *entry = (state, p.started_at);
            }
        }
        for context in contexts(&self.store.pool, &organ).await? {
            let mut tx = self.social_write_tx().await?;
            let map = map_on(&mut tx, &context).await?;
            tx.commit().await?;
            for (peer, entry) in map
                .as_object()
                .ok_or_else(|| invalid("Invalid private block map"))?
            {
                let tuple = (context.clone(), peer.clone());
                if entry["blocked"] == true {
                    groups.insert(tuple, (AdmissionState::Blocked, now));
                } else {
                    groups
                        .entry(tuple)
                        .and_modify(|e| {
                            if e.0 == AdmissionState::Closed {
                                *e = (AdmissionState::Provisional, now);
                            }
                        })
                        .or_insert((AdmissionState::Provisional, now));
                }
            }
        }
        if groups.len() > 512 {
            return Err(invalid(
                "The retained private admission set exceeds its bound",
            ));
        }
        let mut queued = 0;
        for ((context, peer), _) in groups {
            match self
                .social_queue_peer_admission(&context, &peer, &cell, &key, now)
                .await
            {
                Ok(true) => queued += 1,
                Ok(false) => {}
                Err(error) => {
                    tracing::debug!(%error,"Private sender admission waits for current device authorization")
                }
            }
        }
        Ok(queued)
    }

    async fn social_queue_peer_admission(
        &self,
        context: &str,
        peer: &str,
        cell: &str,
        key: &[u8; 32],
        now: i64,
    ) -> Result<bool, EngineError> {
        let authority =
            store::records::get_extension(&self.store.pool, context, SESSION_AUTHORITY_NAMESPACE)
                .await?
                .ok_or_else(|| invalid("No private reply authority"))?;
        let binding: PrivateOwnerBinding = serde_json::from_value(authority["binding"].clone())?;
        self.social_validate_private_binding(context, &binding)
            .await?;
        let route: CertifiedRoute =
            serde_json::from_value(authority[format!("authorized_{cell}")].clone())?;
        request_auth::validate_route(&route, now)?;
        if binding.owner_key != route.control.owner_key
            || serde_json::to_value(&route.control)? != authority["control"]
        {
            return Err(invalid("Private admission needs current owner authority"));
        }
        let mut tx = self.social_write_tx().await?;
        if owner::extension_on(&mut tx, context, SESSION_AUTHORITY_NAMESPACE).await? != authority {
            return Err(invalid("Private authority changed before saving admission"));
        }
        let blocked = map_on(&mut tx, context).await?;
        let window = blocked[peer]["window"].as_i64().unwrap_or(0);
        let roots:Vec<String>=store::sqlx::query_scalar("SELECT e.fds FROM record_extension e JOIN record r ON r.uid=e.record_uid WHERE e.namespace=? AND r.organ_uid=? AND r.deleted_at IS NULL AND json_extract(e.fds,'$.context')=? AND json_extract(e.fds,'$.peer_owner')=? LIMIT 257").bind(PARTICIPANTS_NAMESPACE).bind(&binding.organ).bind(context).bind(peer).fetch_all(&mut *tx).await?;
        if roots.len() > 256 {
            return Err(invalid(
                "The retained private conversations exceed their bound",
            ));
        }
        let mut state = AdmissionState::Closed;
        let mut started = window;
        for body in roots {
            let p: ConversationParticipant = serde_json::from_str(&body)?;
            if p.local_owner != binding.owner_key {
                return Err(invalid(
                    "The admission's retained participant owner differs",
                ));
            }
            if matches!(
                p.state,
                ConversationState::Pending | ConversationState::Accepted
            ) && p.local_accepted
            {
                state = AdmissionState::Accepted;
            } else if p.state == ConversationState::Pending
                && p.started_at + AUTHORITY_LIFETIME > now
                && state != AdmissionState::Accepted
            {
                state = AdmissionState::Provisional;
                started = started.max(p.started_at);
            }
        }
        if blocked[peer]["blocked"] == true {
            state = AdmissionState::Blocked;
        } else if blocked[peer]["blocked"] == false && state == AdmissionState::Closed {
            state = AdmissionState::Provisional;
        }
        let namespace = format!("lince.social.admission-{cell}");
        let deleted: bool =
            store::sqlx::query_scalar("SELECT deleted_at IS NOT NULL FROM record WHERE uid=?")
                .bind(context)
                .fetch_one(&mut *tx)
                .await?;
        let cache_record = if deleted {
            binding.organ.as_str()
        } else {
            context
        };
        let field = if deleted {
            format!("{context}:{peer}")
        } else {
            peer.to_owned()
        };
        let mut map = owner::extension_on(&mut tx, cache_record, &namespace).await?;
        if !map.is_object() {
            map = json!({});
        }
        let previous = map[&field].clone();
        let same = serde_json::from_value::<SenderAdmission>(previous.clone())
            .ok()
            .filter(|a| {
                a.state == state
                    && a.window == window
                    && a.control == route.control
                    && a.certificate == route.certificate
                    && a.expires_at > now + 86400
            });
        let admission = if let Some(a) = same {
            a
        } else {
            let account_id = format!("account:{context}");
            let body: String = store::sqlx::query_scalar(
                "SELECT body FROM social_device_state WHERE id=? AND kind='account'",
            )
            .bind(&account_id)
            .fetch_one(&mut *tx)
            .await?;
            let account: session::AccountState = session::open_local(&account_id, &body, key)?;
            if account.route != route.route {
                return Err(invalid("The admission's live device keys differ"));
            }
            let at = now.max(
                previous["issued_at"]
                    .as_i64()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or_else(|| invalid("Admission revision exhausted"))?,
            );
            let expiry =
                route
                    .certificate
                    .expires_at
                    .min(if state == AdmissionState::Provisional {
                        started + AUTHORITY_LIFETIME
                    } else {
                        now + AUTHORITY_LIFETIME
                    });
            if expiry <= at {
                return Err(invalid("This private admission has expired"));
            }
            let mut a = SenderAdmission {
                mailbox: route.route.mailbox.clone(),
                sender_owner: peer.into(),
                state,
                window,
                issued_at: at,
                expires_at: expiry,
                control: route.control.clone(),
                certificate: route.certificate.clone(),
                signature: String::new(),
            };
            a.signature = account
                .signing_key()?
                .sign_bytes(&signing_bytes("sender-admission", &a)?);
            request_auth::validate_admission(&a, &route, now)?;
            map[&field] = serde_json::to_value(&a)?;
            if serde_json::to_vec(&map)?.len() > 1024 * 1024 {
                return Err(invalid("The retained admission document exceeds its bound"));
            }
            store::records::set_extension_on(&mut tx, cache_record, &namespace, &map).await?;
            a
        };
        mailbox_client::enqueue(
            &mut tx,
            &PublicRequest::AdmitPrivateSender {
                document: admission,
            },
            &route.route.services,
        )
        .await?;
        tx.commit().await?;
        Ok(true)
    }
}
