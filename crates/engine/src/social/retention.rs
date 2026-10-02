use super::*;
use nucleus::social::requests::*;
use store::sqlx::Row;

const DELIVERY_WINDOW: i64 = 30 * 86400;

impl Engine {
    pub(super) async fn social_maintain_contexts(&self, now: i64) -> Result<usize, EngineError> {
        self.social_require_local_write().await?;
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let contexts: Vec<String> = store::sqlx::query_scalar("SELECT e.record_uid FROM record_extension e JOIN record r ON r.uid=e.record_uid LEFT JOIN social_context_retention t ON t.context=e.record_uid WHERE e.namespace=? AND r.organ_uid=? ORDER BY COALESCE(t.checked_at,0),e.record_uid LIMIT 64")
            .bind(SESSION_AUTHORITY_NAMESPACE).bind(&organ.uid).fetch_all(&self.store.pool).await?;
        let mut retired = 0;
        for context in contexts {
            match self.social_maintain_context(&context, now).await {
                Ok(removed) => retired += usize::from(removed),
                Err(error) => {
                    let detail: String = error.to_string().chars().take(500).collect();
                    store::sqlx::query("INSERT INTO social_context_retention(context,state,checked_at,error) VALUES(?,'review',?,?) ON CONFLICT(context) DO UPDATE SET state='review',checked_at=excluded.checked_at,error=excluded.error")
                        .bind(&context).bind(now).bind(detail).execute(&self.store.pool).await?;
                    tracing::warn!(
                        "A private context needs valid retention metadata; its keys remain retained"
                    );
                }
            }
        }
        Ok(retired)
    }

    async fn social_maintain_context(&self, context: &str, now: i64) -> Result<bool, EngineError> {
        let local_cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?
            .uid;
        let members = self.social_reply_members().await?;
        let authority_snapshot =
            store::records::get_extension(&self.store.pool, context, SESSION_AUTHORITY_NAMESPACE)
                .await?
                .ok_or_else(|| invalid("No retained reply authority"))?;
        let authority_snapshot = self
            .social_rebind_owner_for_retention(context, authority_snapshot)
            .await?;
        if let Some(value) = authority_snapshot.get("binding") {
            let binding: PrivateOwnerBinding = serde_json::from_value(value.clone())?;
            self.social_validate_private_binding(context, &binding)
                .await?;
        }
        let owner_root = self
            .social_checked_root_signer()
            .await?
            .map(|signer| signer.public_key_b64());
        let mut tx = self.social_write_tx().await?;
        let deleted: bool =
            store::sqlx::query_scalar("SELECT deleted_at IS NOT NULL FROM record WHERE uid=?")
                .bind(context)
                .fetch_one(&mut *tx)
                .await?;
        let publication = owner::extension_on(&mut tx, context, PUBLICATION_NAMESPACE).await?;
        let draft = owner::extension_on(&mut tx, context, REQUEST_DRAFT_NAMESPACE).await?;
        let archived = deleted || publication["archived"] == true || draft["archived"] == true;
        let activity: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record_extension p JOIN record r ON r.uid=p.record_uid WHERE p.namespace=? AND r.organ_uid=(SELECT organ_uid FROM record WHERE uid=?) AND r.deleted_at IS NULL AND (NOT json_valid(p.fds) OR (json_extract(CASE WHEN json_valid(p.fds) THEN p.fds ELSE '{}' END,'$.context')=? AND (json_extract(CASE WHEN json_valid(p.fds) THEN p.fds ELSE '{}' END,'$.state')='accepted' OR (json_extract(CASE WHEN json_valid(p.fds) THEN p.fds ELSE '{}' END,'$.state')='pending' AND json_extract(CASE WHEN json_valid(p.fds) THEN p.fds ELSE '{}' END,'$.started_at')+?>?))))) OR EXISTS(SELECT 1 FROM social_message_work WHERE context=? AND expires_at>?) OR EXISTS(SELECT 1 FROM social_private_outbox WHERE context=? AND expires_at>?) OR EXISTS(SELECT 1 FROM social_receive_failure WHERE context=? AND expires_at>?) OR EXISTS(SELECT 1 FROM social_publication_job WHERE kind='snippet' AND state='pending' AND expires_at>? AND (NOT json_valid(body) OR json_extract(CASE WHEN json_valid(body) THEN body ELSE '{}' END,'$.id')=?))")
            .bind(PARTICIPANTS_NAMESPACE).bind(context).bind(context).bind(AUTHORITY_LIFETIME).bind(now)
            .bind(context).bind(now).bind(context).bind(now).bind(context).bind(now)
            .bind(now).bind(publication["published"]["id"].as_str().unwrap_or(""))
            .fetch_one(&mut *tx).await?;
        let blocks = admission::map_on(&mut tx, context).await?;
        let admission_activity = blocks
            .as_object()
            .ok_or_else(|| invalid("Invalid retained block map"))?
            .values()
            .any(|entry| {
                entry["blocked"] == true
                    || entry["window"]
                        .as_i64()
                        .is_some_and(|at| at.saturating_add(AUTHORITY_LIFETIME) > now)
            });
        let active_post = publication["published"]["state"] == "active"
            && publication["published"]["expires_at"]
                .as_i64()
                .is_some_and(|at| at > now);
        let previous: Option<(String, i64)> = store::sqlx::query_as(
            "SELECT state,retire_after FROM social_context_retention WHERE context=?",
        )
        .bind(context)
        .fetch_optional(&mut *tx)
        .await?;
        let mut state = "active";
        let mut retire_after = 0;
        let mut removed = false;
        let floor = if let Some(owner_key) = authority_snapshot["binding"]["owner_key"].as_str() {
            retirement::on(
                &mut tx,
                authority_snapshot["binding"]["organ"]
                    .as_str()
                    .unwrap_or_default(),
                context,
                owner_key,
            )
            .await?
            .map_or(0, |decision| decision.request_floor)
        } else {
            0
        };
        let pending_authorization =
            retirement::pending_requests(&authority_snapshot, context, &members, floor, now);
        if archived && !activity && !admission_activity && !active_post && !pending_authorization {
            let authority =
                owner::extension_on(&mut tx, context, SESSION_AUTHORITY_NAMESPACE).await?;
            if authority != authority_snapshot {
                return Err(invalid(
                    "Reply authority changed before key retention; retry its current state",
                ));
            }
            let control: OwnerControl = serde_json::from_value(authority["control"].clone())?;
            let mut required_after = control.expires_at.saturating_add(DELIVERY_WINDOW);
            retire_after = previous
                .as_ref()
                .filter(|(state, at)| {
                    *at > 0 && matches!(state.as_str(), "dormant" | "retired" | "review")
                })
                .map(|(_, at)| *at)
                .unwrap_or_else(|| now.saturating_add(DELIVERY_WINDOW));
            retire_after = retire_after.max(control.expires_at.saturating_add(DELIVERY_WINDOW));
            for (field, value) in authority.as_object().into_iter().flatten() {
                if field.starts_with("authorized_") && !value.is_null() {
                    let certificate: DeviceCertificate =
                        serde_json::from_value(value["certificate"].clone())?;
                    let route_expiry = value
                        .get("expires_at")
                        .map(|at| {
                            at.as_i64()
                                .ok_or_else(|| invalid("Invalid retained route expiry"))
                        })
                        .transpose()?
                        .unwrap_or(0);
                    retire_after = retire_after
                        .max(certificate.expires_at.saturating_add(DELIVERY_WINDOW))
                        .max(route_expiry);
                    required_after = required_after
                        .max(certificate.expires_at.saturating_add(DELIVERY_WINDOW))
                        .max(route_expiry);
                }
            }
            let binding: PrivateOwnerBinding =
                serde_json::from_value(authority["binding"].clone())?;
            if binding.context != context
                || binding.owner_key != control.owner_key
                || !crate::roster::verify_with(
                    &binding.root_key,
                    &signing_bytes("private-owner-binding", &binding)?,
                    &binding.signature,
                )
            {
                return Err(invalid("Retained context owner binding is invalid"));
            }
            let owner_device: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid=? AND slug='local-cell' AND kind='device' AND organ_uid=?)")
                .bind(&binding.owner_cell).bind(&binding.organ)
                .fetch_one(&mut *tx).await?;
            let owner_device = owner_device && owner_root.as_ref() == Some(&binding.root_key);
            if owner_device {
                let id = format!("authority:{context}");
                let body: String = store::sqlx::query_scalar("SELECT body FROM social_device_state WHERE id=? AND kind='authority' AND context=?")
                    .bind(&id).bind(context).fetch_one(&mut *tx).await?;
                let key = self.social_authority_storage_key().await?;
                let wallet: owner::OwnerWallet = session::open_local(&id, &body, &key)?;
                if wallet.control.owner_key != control.owner_key {
                    return Err(invalid(
                        "Retained owner authority conflicts with this context",
                    ));
                }
                retire_after =
                    retire_after.max(wallet.control.expires_at.saturating_add(DELIVERY_WINDOW));
            }
            state = "dormant";
            if owner_device && retire_after <= now {
                let decision = self
                    .social_retire_reply_requests_on(
                        &mut tx,
                        context,
                        &binding.organ,
                        &binding.owner_key,
                        now,
                    )
                    .await?;
                retire_after = retire_after.max(decision.retired_at);
                if retire_after <= now {
                    state = "retired";
                }
            } else if !owner_device
                && let Some(decision) =
                    retirement::on(&mut tx, &binding.organ, context, &binding.owner_key).await?
            {
                let request = &authority[format!("request_{local_cell}")];
                let newer_request = request["requested_at"]
                    .as_i64()
                    .is_some_and(|at| at > decision.request_floor);
                if !newer_request
                    && required_after <= decision.retired_at
                    && decision.retired_at <= now
                {
                    retire_after = required_after.max(decision.retired_at);
                    state = "retired";
                }
            }
            if state == "retired" {
                cleanup::inactive_on(&mut tx, context, &binding.organ, deleted, now).await?;
                let changed = store::sqlx::query("DELETE FROM social_device_state WHERE context=? AND kind IN ('account','session')")
                    .bind(context).execute(&mut *tx).await?;
                removed = changed.rows_affected() > 0;
                store::sqlx::query("DELETE FROM social_pickup_work WHERE context=?")
                    .bind(context)
                    .execute(&mut *tx)
                    .await?;
                store::sqlx::query(
                    "DELETE FROM social_message_work WHERE context=? AND expires_at<=?",
                )
                .bind(context)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                store::sqlx::query(
                    "DELETE FROM social_private_outbox WHERE context=? AND expires_at<=?",
                )
                .bind(context)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                store::sqlx::query(
                    "DELETE FROM social_receive_failure WHERE context=? AND expires_at<=?",
                )
                .bind(context)
                .bind(now)
                .execute(&mut *tx)
                .await?;
            }
        }
        store::sqlx::query("INSERT INTO social_context_retention(context,state,retire_after,checked_at) VALUES(?,?,?,?) ON CONFLICT(context) DO UPDATE SET state=excluded.state,retire_after=excluded.retire_after,checked_at=excluded.checked_at,error=NULL")
            .bind(context).bind(state).bind(retire_after).bind(now).execute(&mut *tx).await?;
        tx.commit().await?;
        let local_grant = &authority_snapshot[format!("authorized_{local_cell}")];
        if state == "dormant"
            && !deleted
            && local_grant.get("expires_at").is_none()
            && local_grant["certificate"]["expires_at"]
                .as_i64()
                .is_some_and(|at| at > now)
        {
            self.social_finish_reply_route(context).await?;
        }
        Ok(removed)
    }

    pub(super) async fn social_context_retention_status(
        &self,
        context: &str,
    ) -> Result<Option<Value>, EngineError> {
        let row = store::sqlx::query("SELECT state,retire_after,error FROM social_context_retention WHERE context=? AND state<>'active'")
            .bind(context).fetch_optional(&self.store.pool).await?;
        Ok(row.map(|row| {
            let state: String = row.get("state");
            if state == "review" {
                json!({"record":context,"reply_keys":"retention-needs-review","detail":row.get::<Option<String>,_>("error"),"status":"Current context metadata is incomplete or invalid. Keys and history remain retained; restore current owner authority before continuing"})
            } else {
                json!({"record":context,"reply_keys":state,"retire_after":row.get::<i64,_>("retire_after"),"status":"This archived context no longer renews sending leases. History, blocks and owner authority remain. Other devices keep transport keys until the owner's signed retirement decision arrives through Own sync; newer activity still protects its keys"})
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fixture() -> (Engine, tempfile::TempDir, String) {
        let engine = Engine::open_memory().await.unwrap();
        let directory = tempfile::tempdir().unwrap();
        engine.set_sealing_keyring_path(directory.path().join("owner/sealing.json"));
        let root_path = directory.path().join("root.key");
        std::fs::write(&root_path, [221; 32]).unwrap();
        engine.set_root_key_path(root_path);
        let organ = store::organs::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let cell = store::cells::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let signer = Signer::from_bytes(&organ, &crate::roster::cell_key_id(&cell), [222; 32]);
        engine.set_signer(signer.clone()).await.unwrap();
        engine.set_organ_signer(signer.clone()).await.unwrap();
        let root = Signer::from_bytes(&organ, crate::roster::ROOT_KEY_ID, [221; 32]);
        engine.publish_root_key(&root).await.unwrap();
        engine
            .publish_roster(
                &root,
                vec![crate::roster::CellEntry {
                    cell_uid: cell,
                    node_id: "owner".into(),
                    label: "Owner".into(),
                    operational_key: signer.public_key_b64(),
                    sealing_key: None,
                    front_door: false,
                    capabilities: crate::roster::full_capabilities(),
                }],
            )
            .await
            .unwrap();
        let saved = engine
            .social_command(
                Command::SaveDraft {
                    record: None,
                    source: None,
                    draft: PostDraft {
                        title: "Dormant reply context".into(),
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
        let context = saved["record"].as_str().unwrap().to_owned();
        engine
            .social_prepare_reply_keys(
                &context,
                vec![iroh::SecretKey::from_bytes(&[223; 32]).public().to_string()],
            )
            .await
            .unwrap();
        engine
            .social_command(
                Command::ArchivePost {
                    record: context.clone(),
                },
                None,
                nucleus::execution::now(),
            )
            .await
            .unwrap();
        (engine, directory, context)
    }

    #[tokio::test]
    async fn dormant_owner_keys_retire_after_delivery_windows_without_deleting_authority_or_history()
     {
        let (engine, _directory, context) = fixture().await;
        let now = nucleus::execution::now().timestamp();
        let account = format!("account:{context}");
        store::sqlx::query("INSERT INTO social_publication_job(hash,destination,kind,body,expires_at,state) VALUES('empty-receipt','host','snippet','',?,'accepted')")
            .bind(now + DELIVERY_WINDOW).execute(&engine.store.pool).await.unwrap();
        let wallet = format!("authority:{context}");
        let authority = store::social::device_state(&engine.store.pool, &wallet)
            .await
            .unwrap()
            .unwrap();
        let saved = store::records::get_extension(
            &engine.store.pool,
            &context,
            SESSION_AUTHORITY_NAMESPACE,
        )
        .await
        .unwrap();
        assert!(!engine.social_maintain_context(&context, now).await.unwrap());
        assert_eq!(
            engine.social_reply_key_status(&context).await.unwrap()["reply_keys"],
            "dormant"
        );
        assert_eq!(
            engine.social_refresh_reply_authorizations().await.unwrap(),
            0
        );
        assert_eq!(
            store::records::get_extension(
                &engine.store.pool,
                &context,
                SESSION_AUTHORITY_NAMESPACE
            )
            .await
            .unwrap(),
            saved
        );
        let deadline = engine.social_reply_key_status(&context).await.unwrap()["retire_after"]
            .as_i64()
            .unwrap();
        assert!(deadline >= now + AUTHORITY_LIFETIME + DELIVERY_WINDOW);
        assert!(
            !engine
                .social_maintain_context(&context, deadline - 1)
                .await
                .unwrap()
        );
        assert!(
            store::social::device_state(&engine.store.pool, &account)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            engine
                .social_maintain_context(&context, deadline)
                .await
                .unwrap()
        );
        assert_eq!(
            engine.social_reply_key_status(&context).await.unwrap()["reply_keys"],
            "retired"
        );
        assert!(
            store::social::device_state(&engine.store.pool, &account)
                .await
                .unwrap()
                .is_none()
        );
        let retained = store::social::device_state(&engine.store.pool, &wallet)
            .await
            .unwrap()
            .unwrap();
        let key = engine.social_authority_storage_key().await.unwrap();
        let before: Value = session::open_local(&wallet, &authority.0, &key).unwrap();
        let after: Value = session::open_local(&wallet, &retained.0, &key).unwrap();
        assert!(after["secret"] == before["secret"]);
        assert_eq!(after["control"], before["control"]);
        assert!(after["requests"].as_object().unwrap().is_empty());
        assert_eq!(after["retired_before"], deadline);
        assert!(
            store::records::get(&engine.store.pool, &context)
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(
            store::records::get_extension(
                &engine.store.pool,
                &context,
                SESSION_AUTHORITY_NAMESPACE
            )
            .await
            .unwrap(),
            saved
        );
    }

    #[tokio::test]
    async fn new_activity_and_pending_discard_prevent_retirement_and_blocks_keep_deleted_authority_renewable()
     {
        let (engine, _directory, context) = fixture().await;
        let now = nucleus::execution::now().timestamp();
        engine.social_maintain_context(&context, now).await.unwrap();
        let deadline = engine.social_reply_key_status(&context).await.unwrap()["retire_after"]
            .as_i64()
            .unwrap();
        store::sqlx::query("INSERT INTO social_receive_failure(context,service,envelope,reference,error,expires_at,discard) VALUES(?,'host','envelope','{}','waiting',?,1)")
            .bind(&context).bind(deadline + 10).execute(&engine.store.pool).await.unwrap();
        assert!(
            !engine
                .social_maintain_context(&context, deadline)
                .await
                .unwrap()
        );
        assert!(
            engine
                .social_context_retention_status(&context)
                .await
                .unwrap()
                .is_none()
        );
        store::sqlx::query("DELETE FROM social_receive_failure")
            .execute(&engine.store.pool)
            .await
            .unwrap();
        store::records::set_extension(
            &engine.store.pool,
            &context,
            PARTICIPANTS_NAMESPACE,
            &json!({"context":context,"state":"accepted","started_at":now}),
        )
        .await
        .unwrap();
        assert!(
            !engine
                .social_maintain_context(&context, deadline + 20)
                .await
                .unwrap()
        );
        assert!(
            engine
                .social_context_retention_status(&context)
                .await
                .unwrap()
                .is_none()
        );
        store::records::set_extension(
            &engine.store.pool,
            &context,
            PARTICIPANTS_NAMESPACE,
            &json!({"context":context,"state":"closed","started_at":now}),
        )
        .await
        .unwrap();
        let peer = Signer::from_bytes("", "", [224; 32]).public_key_b64();
        let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
        admission::block_on(&mut tx, &context, &peer, true, now)
            .await
            .unwrap();
        store::records::mark_deleted_on(&mut tx, &context)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert!(
            !engine
                .social_maintain_context(&context, deadline + 40)
                .await
                .unwrap()
        );
        assert!(
            engine
                .social_context_retention_status(&context)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            engine.social_refresh_reply_authorizations().await.unwrap(),
            1
        );
        let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
        assert_eq!(
            admission::map_on(&mut tx, &context).await.unwrap()[peer]["blocked"],
            true
        );
        tx.commit().await.unwrap();
    }

    #[tokio::test]
    async fn fresh_authority_postpones_retirement_and_explicit_preparation_recovers_fresh_owner_keys()
     {
        let clock = nucleus::execution::Execution::new([225; 32], 1_790_899_200_000).unwrap();
        clock
            .scope(Box::pin(async {
                let (engine, _directory, context) = fixture().await;
                let now = nucleus::execution::now().timestamp();
                engine.social_maintain_context(&context, now).await.unwrap();
                let deadline =
                    engine.social_reply_key_status(&context).await.unwrap()["retire_after"]
                        .as_i64()
                        .unwrap();
                let ready = engine
                    .social_prepare_reply_keys(
                        &context,
                        vec![iroh::SecretKey::from_bytes(&[223; 32]).public().to_string()],
                    )
                    .await
                    .unwrap();
                assert_eq!(ready["reply_keys"], "ready");
                let mut held = store::records::get_extension(
                    &engine.store.pool,
                    &context,
                    SESSION_AUTHORITY_NAMESPACE,
                )
                .await
                .unwrap()
                .unwrap();
                held["control"]["expires_at"] = json!(deadline + 100);
                store::records::set_extension(
                    &engine.store.pool,
                    &context,
                    SESSION_AUTHORITY_NAMESPACE,
                    &held,
                )
                .await
                .unwrap();
                assert!(
                    !engine
                        .social_maintain_context(&context, deadline)
                        .await
                        .unwrap()
                );
                let later = engine.social_reply_key_status(&context).await.unwrap()["retire_after"]
                    .as_i64()
                    .unwrap();
                assert!(later >= deadline + 100 + DELIVERY_WINDOW);
                assert!(
                    engine
                        .social_maintain_context(&context, later)
                        .await
                        .unwrap()
                );
                clock.set_time(later * 1000).unwrap();
                engine.renew_local_roster().await.unwrap();
                let prepared = engine
                    .social_prepare_reply_keys(
                        &context,
                        vec![iroh::SecretKey::from_bytes(&[223; 32]).public().to_string()],
                    )
                    .await
                    .unwrap();
                assert_eq!(prepared["reply_keys"], "ready");
                assert_eq!(prepared["route"]["control"]["generation"], "2");
            }))
            .await;
    }

    #[tokio::test]
    async fn malformed_retention_metadata_is_isolated_without_blocking_other_contexts() {
        let (engine, _directory, context) = fixture().await;
        let broken = store::records::create(
            &engine.store.pool,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Broken archived context",
                body: "",
                quantity: store::exact::from_f64(1.0),
            },
        )
        .await
        .unwrap()
        .uid;
        store::records::set_extension(
            &engine.store.pool,
            &broken,
            PUBLICATION_NAMESPACE,
            &json!({"archived":true}),
        )
        .await
        .unwrap();
        store::records::set_extension(
            &engine.store.pool,
            &broken,
            SESSION_AUTHORITY_NAMESPACE,
            &json!({"control":"incomplete"}),
        )
        .await
        .unwrap();
        assert_eq!(
            engine
                .social_maintain_contexts(nucleus::execution::now().timestamp())
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            engine.social_reply_key_status(&context).await.unwrap()["reply_keys"],
            "dormant"
        );
        assert_eq!(
            engine.social_reply_key_status(&broken).await.unwrap()["reply_keys"],
            "retention-needs-review"
        );
        assert_eq!(
            engine.social_refresh_reply_authorizations().await.unwrap(),
            0
        );
        assert!(
            store::social::device_state(&engine.store.pool, &format!("account:{context}"))
                .await
                .unwrap()
                .is_some()
        );
    }
}
