use super::*;
use nucleus::social::requests::*;
use serde::{Deserialize, Serialize};
use store::sqlx::Row;

#[derive(Serialize, Deserialize)]
pub(super) struct OwnerWallet {
    secret: String,
    generation: i64,
    members: std::collections::BTreeMap<String, String>,
    requests: std::collections::BTreeMap<String, DeviceAuthorizationRequest>,
    revocation_version: i64,
    pub(super) retired_before: i64,
    pub(super) control: OwnerControl,
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn command(engine: &Engine, request: Command) -> Value {
        engine
            .social_command(request, None, nucleus::execution::now())
            .await
            .unwrap()
            .data
            .unwrap()
    }

    #[tokio::test]
    async fn archived_reply_post_keeps_renewable_private_authority_and_stays_hidden() {
        let engine = Engine::open_memory().await.unwrap();
        let directory = tempfile::tempdir().unwrap();
        engine.set_sealing_keyring_path(directory.path().join("owner/sealing.json"));
        let root_path = directory.path().join("root.key");
        std::fs::write(&root_path, [211; 32]).unwrap();
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
        let device = Signer::from_bytes(&organ, &crate::roster::cell_key_id(&cell), [213; 32]);
        engine.set_signer(device.clone()).await.unwrap();
        engine.set_organ_signer(device.clone()).await.unwrap();
        let root = Signer::from_bytes(&organ, crate::roster::ROOT_KEY_ID, [211; 32]);
        engine.publish_root_key(&root).await.unwrap();
        engine
            .publish_roster(
                &root,
                vec![crate::roster::CellEntry {
                    cell_uid: cell,
                    node_id: "owner".into(),
                    label: "Owner".into(),
                    operational_key: device.public_key_b64(),
                    sealing_key: None,
                    front_door: false,
                    capabilities: crate::roster::full_capabilities(),
                }],
            )
            .await
            .unwrap();
        let saved = command(
            &engine,
            Command::SaveDraft {
                record: None,
                source: None,
                draft: PostDraft {
                    title: "Archived announcement with ongoing conversation keys".into(),
                    ..Default::default()
                },
            },
        )
        .await;
        let context = saved["record"].as_str().unwrap().to_owned();
        command(
            &engine,
            Command::PrepareReplyKeys {
                record: context.clone(),
                services: vec![iroh::SecretKey::from_bytes(&[212; 32]).public().to_string()],
            },
        )
        .await;
        for state in [PostState::Active, PostState::Withdrawn] {
            let preview = command(
                &engine,
                Command::Preview {
                    record: context.clone(),
                    state,
                },
            )
            .await;
            command(
                &engine,
                Command::Publish {
                    record: context.clone(),
                    preview_hash: preview["preview_hash"].as_str().unwrap().into(),
                    document: serde_json::from_value(preview["document"].clone()).unwrap(),
                },
            )
            .await;
        }
        command(
            &engine,
            Command::ArchivePost {
                record: context.clone(),
            },
        )
        .await;
        assert!(
            store::records::get(&engine.store.pool, &context)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            command(&engine, Command::Overview).await["posts"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
        admission::block_on(
            &mut tx,
            &context,
            &Signer::from_bytes("", "", [219; 32]).public_key_b64(),
            true,
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let key = engine.social_storage_key().await.unwrap();
        let id = format!("account:{context}");
        let (body, version) = store::social::device_state(&engine.store.pool, &id)
            .await
            .unwrap()
            .unwrap();
        let mut account: session::AccountState = session::open_local(&id, &body, &key).unwrap();
        let old_prekey = account.route.prekey.clone();
        account.expires_at = nucleus::execution::now().timestamp() - 1;
        let sealed = session::seal_local(&id, &account, &key).unwrap();
        let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
        store::social::put_device_state_on(
            &mut tx,
            &id,
            "account",
            &context,
            &sealed,
            Some(version),
            nucleus::execution::now().timestamp(),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            engine.social_refresh_reply_authorizations().await.unwrap(),
            1
        );
        let (renewed, _) = store::social::device_state(&engine.store.pool, &id)
            .await
            .unwrap()
            .unwrap();
        let renewed: session::AccountState = session::open_local(&id, &renewed, &key).unwrap();
        assert_ne!(renewed.route.prekey, old_prekey);
        assert!(renewed.expires_at > nucleus::execution::now().timestamp());
        assert!(
            command(&engine, Command::Overview).await["posts"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            engine
                .social_command(
                    Command::SaveDraft {
                        record: Some(context),
                        source: None,
                        draft: PostDraft::default()
                    },
                    None,
                    nucleus::execution::now()
                )
                .await
                .is_err()
        );
    }
}

pub(super) async fn extension_on(
    tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
    context: &str,
    namespace: &str,
) -> Result<Value, EngineError> {
    let body: Option<String> = store::sqlx::query_scalar(
        "SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?",
    )
    .bind(context)
    .bind(namespace)
    .fetch_optional(&mut **tx)
    .await?;
    let body = body.unwrap_or_else(|| "{}".into());
    if body.len()
        > if namespace == PROFILE_NAMESPACE {
            store::records::MAX_EXTENSION_BYTES
        } else if namespace == PUBLICATION_NAMESPACE {
            512 * 1024
        } else if namespace.starts_with("lince.social.admission-") {
            1024 * 1024
        } else {
            256 * 1024
        }
    {
        return Err(invalid(
            "Resolve this oversized private reply authorization state",
        ));
    }
    Ok(serde_json::from_str(&body)?)
}

impl Engine {
    pub async fn social_validate_backup_wallets(
        &self,
        root: &Signer,
        wallet_key: Option<&[u8; 32]>,
    ) -> Result<(), EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("The backup has no owner Organ"))?;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("The backup has no owner Cell"))?;
        if root.actor_uid != organ.uid
            || cell.organ_uid != organ.uid
            || !self.key_chains(&organ.uid, &root.public_key_b64()).await?
        {
            return Err(invalid(
                "The backup root does not match this Organ's trusted current identity",
            ));
        }
        let mut cursor = String::new();
        loop {
            let rows: Vec<(String, String, String)> = store::sqlx::query_as(
                "SELECT id,context,body FROM social_device_state WHERE kind='authority' AND id>? ORDER BY id LIMIT 64",
            ).bind(&cursor).fetch_all(&self.store.pool).await?;
            if rows.is_empty() {
                break;
            }
            for (id, context, body) in rows {
                cursor = id.clone();
                let key = wallet_key
                    .ok_or_else(|| invalid("The backup is missing its authority-wallet key"))?;
                if id == format!("posting:{context}") {
                    self.social_validate_backup_posting_wallet(&id, &context, &body, key, root)
                        .await?;
                    continue;
                }
                if id != format!("authority:{context}") {
                    return Err(invalid("The backup has an invalid authority wallet scope"));
                }
                let mut wallet: OwnerWallet = session::open_local(&id, &body, key)?;
                let secret = zeroize::Zeroizing::new(std::mem::take(&mut wallet.secret));
                let held = store::records::get_extension(
                    &self.store.pool,
                    &context,
                    SESSION_AUTHORITY_NAMESPACE,
                )
                .await?
                .ok_or_else(|| invalid("The backup wallet has no owner binding"))?;
                let mut binding: PrivateOwnerBinding =
                    serde_json::from_value(held["binding"].clone())?;
                let signer = session::secret_signer(&secret)?;
                self.social_rebind_owner_binding(&context, &mut binding, root, &signer)
                    .await?;
                request_auth::validate_control(&wallet.control, wallet.control.issued_at)?;
                if binding.context != context
                    || binding.organ != organ.uid
                    || binding.owner_cell != cell.uid
                    || binding.owner_key != signer.public_key_b64()
                    || wallet.control.owner_key != binding.owner_key
                    || wallet.control.generation != wallet.generation.to_string()
                    || wallet.members.len() > 64
                    || wallet.requests.len() > 64
                    || !crate::roster::verify_with(
                        &binding.root_key,
                        &signing_bytes("private-owner-binding", &binding)?,
                        &binding.signature,
                    )
                {
                    return Err(invalid(
                        "The backup authority wallet does not match its signed owner binding",
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) async fn social_checked_root_signer(&self) -> Result<Option<Signer>, EngineError> {
        let Some(root) = self.root_signer().await? else {
            return Ok(None);
        };
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let known = crate::trust::keys_of(&self.store, &organ.uid)
            .await?
            .iter()
            .any(|(id, _)| crate::roster::is_root_key_id(id));
        if known && !self.key_chains(&organ.uid, &root.public_key_b64()).await? {
            return Err(invalid(
                "The configured owner key does not match this Organ's trusted identity",
            ));
        }
        if !known {
            self.publish_root_key(&root).await?;
        }
        Ok(Some(root))
    }

    pub(super) async fn social_reply_members(
        &self,
    ) -> Result<std::collections::BTreeMap<String, String>, EngineError> {
        self.social_require_local_write().await?;
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        if let Some(roster) = self.roster_of(&organ.uid).await? {
            if !crate::roster::roster_signature_is_valid(&roster) || roster.roster.cells.len() > 64
            {
                return Err(invalid(
                    "Refresh a valid bounded device roster before authorizing replies",
                ));
            }
            Ok(roster
                .roster
                .cells
                .into_iter()
                .filter(|entry| entry.may(crate::roster::CAP_WRITE))
                .map(|entry| (entry.cell_uid, entry.operational_key))
                .collect())
        } else {
            let cell = store::cells::local(&self.store.pool)
                .await?
                .ok_or_else(|| invalid("No local Cell"))?;
            let key = self.operational_key_for(&organ.uid).await?;
            Ok([(cell.uid, key.public_key_b64())].into_iter().collect())
        }
    }

    pub(super) async fn social_validate_private_binding(
        &self,
        context: &str,
        binding: &PrivateOwnerBinding,
    ) -> Result<(), EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        if binding.organ != organ.uid
            || binding.context != context
            || !nucleus::valid_uid(&binding.owner_cell, "r")
            || !self.key_chains(&organ.uid, &binding.root_key).await?
            || !crate::roster::verify_with(
                &binding.root_key,
                &signing_bytes("private-owner-binding", binding)?,
                &binding.signature,
            )
        {
            return Err(invalid(
                "The private reply owner binding does not match the trusted Organ",
            ));
        }
        Ok(())
    }

    pub(super) async fn social_prepare_reply_keys(
        &self,
        context: &str,
        mut services: Vec<String>,
    ) -> Result<Value, EngineError> {
        services.sort();
        services.dedup();
        if services.is_empty()
            || services.len() > 8
            || services
                .iter()
                .any(|id| id.parse::<iroh::EndpointId>().is_err())
        {
            return Err(invalid(
                "Choose one to eight pinned mailbox hosts before preparing private replies",
            ));
        }
        self.social_require_local_write().await?;
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        let key = self.social_storage_key().await?;
        let account_id = format!("account:{context}");
        let account_held = store::social::device_state(&self.store.pool, &account_id).await?;
        let now = nucleus::execution::now().timestamp();
        let mut account: session::AccountState = match &account_held {
            Some((body, _)) => session::open_local(&account_id, body, &key)?,
            None => session::new_account(&key, services.clone(), now + 32 * 86400)?,
        };
        let account_changed = account.expires_at <= now;
        if account_changed {
            let mut olm = account.account(&key)?;
            olm.generate_fallback_key();
            let fallback = olm
                .fallback_key()
                .into_values()
                .next()
                .ok_or_else(|| invalid("No replacement offline reply key was generated"))?;
            olm.mark_keys_as_published();
            account.account = olm.pickle().encrypt(&key);
            account.route.prekey = fallback.to_base64();
            account.expires_at = now + 32 * 86400;
        }
        if account.route.services != services {
            return Err(invalid(
                "This live reply account already uses its selected hosts. Keep them until its retained messages expire; prepare another context to choose different hosts",
            ));
        }
        let operational = self.operational_key_for(&organ.uid).await?;
        let members = self.social_reply_members().await?;
        if members.get(&cell.uid) != Some(&operational.public_key_b64()) {
            return Err(invalid(
                "The device's operational key does not match its current write authority",
            ));
        }
        let mut request = DeviceAuthorizationRequest {
            context: context.into(),
            cell: cell.uid.clone(),
            operational_key: operational.public_key_b64(),
            route: account.route.clone(),
            requested_at: now,
            signature: String::new(),
        };
        let held =
            store::records::get_extension(&self.store.pool, context, SESSION_AUTHORITY_NAMESPACE)
                .await?
                .unwrap_or_else(|| json!({}));
        if let Ok(previous) = serde_json::from_value::<DeviceAuthorizationRequest>(
            held[format!("request_{}", cell.uid)].clone(),
        ) {
            if previous.route == request.route
                && previous.operational_key == request.operational_key
                && request_auth::validate_device_request(&previous, now).is_ok()
            {
                request = previous;
            } else {
                request.requested_at = now.max(
                    previous
                        .requested_at
                        .checked_add(1)
                        .ok_or_else(|| invalid("Device request time exhausted"))?,
                );
            }
        }
        if let Some(owner_key) = held["binding"]["owner_key"].as_str() {
            let floor = self
                .social_retired_request_floor(&organ.uid, context, owner_key)
                .await?;
            if floor > 0 {
                request.requested_at = request.requested_at.max(
                    floor
                        .checked_add(1)
                        .ok_or_else(|| invalid("Retired request time exhausted"))?,
                );
            }
        }
        request.signature =
            operational.sign_bytes(&signing_bytes("private-device-request", &request)?);
        request_auth::validate_device_request(&request, now)?;
        let mut new_owner = None;
        if held.get("binding").is_none()
            && let Some(root) = self.social_checked_root_signer().await?
        {
            let owner_key = self.social_authority_storage_key().await?;
            let signer = new_social_signer("")?;
            let mut binding = PrivateOwnerBinding {
                organ: organ.uid,
                context: context.into(),
                owner_cell: cell.uid.clone(),
                owner_key: signer.public_key_b64(),
                root_key: root.public_key_b64(),
                signature: String::new(),
            };
            binding.signature = root.sign_bytes(&signing_bytes("private-owner-binding", &binding)?);
            let mut control = OwnerControl {
                owner_key: signer.public_key_b64(),
                generation: "1".into(),
                issued_at: now,
                expires_at: now + AUTHORITY_LIFETIME,
                signature: String::new(),
            };
            control.signature = signer.sign_bytes(&signing_bytes("reply-owner", &control)?);
            let wallet = OwnerWallet {
                secret: B64.encode(signer.secret_bytes()),
                generation: 1,
                members,
                requests: std::collections::BTreeMap::new(),
                revocation_version: 0,
                retired_before: 0,
                control,
            };
            let id = format!("authority:{context}");
            new_owner = Some((
                binding,
                id.clone(),
                session::seal_local(&id, &wallet, &owner_key)?,
            ));
        } else if held.get("binding").is_some() {
            let binding = serde_json::from_value(held["binding"].clone())?;
            self.social_validate_private_binding(context, &binding)
                .await?;
        }
        let body = session::seal_local(&account_id, &account, &key)?;
        let mut tx = self.social_write_tx().await?;
        let mut state = extension_on(&mut tx, context, SESSION_AUTHORITY_NAMESPACE).await?;
        if state.get("services").is_none() {
            state["services"] = json!(services);
        }
        if let Some((binding, id, wallet)) = new_owner {
            if state.get("binding").is_some() {
                return Err(invalid(
                    "Another device initialized reply ownership. Refresh before preparing keys",
                ));
            }
            store::social::put_device_state_on(
                &mut tx,
                &id,
                "authority",
                context,
                &wallet,
                None,
                now,
            )
            .await?;
            state["binding"] = serde_json::to_value(binding)?;
            state["services"] = json!(services);
        } else if state["binding"] != held["binding"] {
            return Err(invalid(
                "Reply ownership changed while preparing device keys; refresh",
            ));
        }
        if account_held.is_none() || account_changed {
            store::social::put_device_state_on(
                &mut tx,
                &account_id,
                "account",
                context,
                &body,
                account_held.as_ref().map(|(_, version)| *version),
                now,
            )
            .await?;
        }
        state[format!("request_{}", cell.uid)] = serde_json::to_value(request)?;
        if serde_json::to_vec(&state)?.len() > 256 * 1024 {
            return Err(invalid("Private reply authorization state is full"));
        }
        store::records::set_extension_on(&mut tx, context, SESSION_AUTHORITY_NAMESPACE, &state)
            .await?;
        store::sqlx::query("DELETE FROM social_context_retention WHERE context=?")
            .bind(context)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.social_authorize_reply_context(context).await?;
        self.social_reply_key_status(context).await
    }

    pub(super) async fn social_retire_reply_requests_on(
        &self,
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        context: &str,
        organ: &str,
        owner_key: &str,
        now: i64,
    ) -> Result<retirement::Decision, EngineError> {
        let id = format!("authority:{context}");
        let (body, version): (String, i64) = store::sqlx::query_as(
            "SELECT body,version FROM social_device_state WHERE id=? AND kind='authority' AND context=?",
        )
        .bind(&id)
        .bind(context)
        .fetch_one(&mut **tx)
        .await?;
        let key = self.social_authority_storage_key().await?;
        let mut wallet: OwnerWallet = session::open_local(&id, &body, &key)?;
        let signer = session::secret_signer(&wallet.secret)?;
        if signer.public_key_b64() != owner_key || wallet.control.owner_key != owner_key {
            return Err(invalid(
                "The retirement wallet belongs to another private owner",
            ));
        }
        if wallet.requests.is_empty()
            && let Some(decision) = retirement::on(tx, organ, context, owner_key).await?
            && decision.request_floor == wallet.retired_before
        {
            return Ok(decision);
        }
        let floor = wallet
            .requests
            .values()
            .map(|request| request.requested_at)
            .max()
            .unwrap_or(0)
            .max(now)
            .max(wallet.retired_before);
        let mut decision = retirement::Decision {
            context: context.into(),
            owner_key: owner_key.into(),
            request_floor: floor,
            retired_at: floor,
            signature: String::new(),
        };
        decision.signature =
            signer.sign_bytes(&signing_bytes("private-key-retirement", &decision)?);
        wallet.retired_before = floor;
        wallet.requests.clear();
        retirement::save_on(tx, organ, &decision).await?;
        store::social::put_device_state_on(
            tx,
            &id,
            "authority",
            context,
            &session::seal_local(&id, &wallet, &key)?,
            Some(version),
            now,
        )
        .await?;
        Ok(decision)
    }

    async fn social_rebind_owner_binding(
        &self,
        context: &str,
        binding: &mut PrivateOwnerBinding,
        root: &Signer,
        owner: &Signer,
    ) -> Result<(), EngineError> {
        if binding.organ != root.actor_uid
            || binding.context != context
            || owner.public_key_b64() != binding.owner_key
            || !crate::roster::verify_with(
                &binding.root_key,
                &signing_bytes("private-owner-binding", binding)?,
                &binding.signature,
            )
        {
            return Err(invalid(
                "Verify the retained private owner binding before renewing it",
            ));
        }
        if binding.root_key != root.public_key_b64() {
            if !self.key_chains(&binding.organ, &binding.root_key).await?
                && !self
                    .published_successions(&binding.organ)
                    .await?
                    .iter()
                    .any(|change| change.old_key == binding.root_key)
            {
                return Err(invalid(
                    "Verify the owner identity succession before renewing reply authority",
                ));
            }
            binding.root_key = root.public_key_b64();
            binding.signature = root.sign_bytes(&signing_bytes("private-owner-binding", binding)?);
        }
        self.social_validate_private_binding(context, binding).await
    }

    pub(super) async fn social_rebind_owner_for_retention(
        &self,
        context: &str,
        held: Value,
    ) -> Result<Value, EngineError> {
        let Some(value) = held.get("binding") else {
            return Ok(held);
        };
        let mut binding: PrivateOwnerBinding = serde_json::from_value(value.clone())?;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        if binding.owner_cell != cell.uid {
            return Ok(held);
        }
        let Some(root) = self.social_checked_root_signer().await? else {
            return Ok(held);
        };
        if binding.root_key == root.public_key_b64() {
            return Ok(held);
        }
        let id = format!("authority:{context}");
        let wallet: Option<(String,i64)> = store::sqlx::query_as("SELECT body,version FROM social_device_state WHERE id=? AND kind='authority' AND context=?")
            .bind(&id).bind(context).fetch_optional(&self.store.pool).await?;
        let wallet = wallet.ok_or_else(|| {
            invalid("Restore the separate owner wallet before renewing its private binding")
        })?;
        let key = self.social_authority_storage_key().await?;
        let material: OwnerWallet = session::open_local(&id, &wallet.0, &key)?;
        let owner = session::secret_signer(&material.secret)?;
        self.social_rebind_owner_binding(context, &mut binding, &root, &owner)
            .await?;
        let mut current = held.clone();
        current["binding"] = serde_json::to_value(binding)?;
        let mut tx = self.social_write_tx().await?;
        self.social_require_local_write_on(&mut tx).await?;
        let revoked: bool = store::sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM identity_revocation WHERE organ_uid=? AND revoked_key=?)",
        )
        .bind(&root.actor_uid)
        .bind(root.public_key_b64())
        .fetch_one(&mut *tx)
        .await?;
        let latest_wallet: Option<(String,i64)> = store::sqlx::query_as("SELECT body,version FROM social_device_state WHERE id=? AND kind='authority' AND context=?")
            .bind(&id).bind(context).fetch_optional(&mut *tx).await?;
        if revoked
            || extension_on(&mut tx, context, SESSION_AUTHORITY_NAMESPACE).await? != held
            || latest_wallet.as_ref() != Some(&wallet)
        {
            return Err(invalid(
                "Owner authority changed while renewing its private binding",
            ));
        }
        store::records::set_extension_on(&mut tx, context, SESSION_AUTHORITY_NAMESPACE, &current)
            .await?;
        tx.commit().await?;
        Ok(current)
    }

    async fn social_authorize_reply_context(&self, context: &str) -> Result<(), EngineError> {
        let Some(root) = self.social_checked_root_signer().await? else {
            return Ok(());
        };
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        let held =
            store::records::get_extension(&self.store.pool, context, SESSION_AUTHORITY_NAMESPACE)
                .await?
                .unwrap_or_else(|| json!({}));
        let Some(value) = held.get("binding") else {
            return Ok(());
        };
        let mut binding: PrivateOwnerBinding = serde_json::from_value(value.clone())?;
        if binding.owner_cell != cell.uid {
            return Ok(());
        }
        let wallet_id = format!("authority:{context}");
        let Some((body, version)) =
            store::social::device_state(&self.store.pool, &wallet_id).await?
        else {
            return Err(invalid(
                "Restore this owner's separate authority wallet before authorizing fresh keys",
            ));
        };
        let wallet_key = self.social_authority_storage_key().await?;
        let mut wallet: OwnerWallet = session::open_local(&wallet_id, &body, &wallet_key)?;
        let previous_wallet = serde_json::to_value(&wallet)?;
        wallet.retired_before = wallet.retired_before.max(
            self.social_retired_request_floor(&binding.organ, context, &binding.owner_key)
                .await?,
        );
        wallet
            .requests
            .retain(|_, request| request.requested_at > wallet.retired_before);
        let recovering_retired = wallet.retired_before > 0 && wallet.requests.is_empty();
        let owner = session::secret_signer(&wallet.secret)?;
        if owner.public_key_b64() != binding.owner_key {
            return Err(invalid(
                "The authority wallet does not match this private reply owner",
            ));
        }
        self.social_rebind_owner_binding(context, &mut binding, &root, &owner)
            .await?;
        let members = self.social_reply_members().await?;
        let revocation_version = held["revocation_version"].as_i64().unwrap_or(0);
        let mut removed = revocation_version > wallet.revocation_version
            || wallet
                .members
                .iter()
                .any(|(cell, key)| members.get(cell) != Some(key));
        let now = nucleus::execution::now().timestamp();
        wallet
            .requests
            .retain(|cell, request| members.get(cell) == Some(&request.operational_key));
        let mut fresh_request = false;
        for (field, value) in held.as_object().into_iter().flatten() {
            let Some(cell) = field.strip_prefix("request_") else {
                continue;
            };
            let Ok(request) = serde_json::from_value::<DeviceAuthorizationRequest>(value.clone())
            else {
                continue;
            };
            if request.cell != cell
                || request.context != context
                || request.requested_at <= wallet.retired_before
                || members.get(cell) != Some(&request.operational_key)
                || request_auth::validate_device_request(&request, now).is_err()
            {
                continue;
            }
            if let Some(previous) = wallet.requests.get(cell) {
                if request.requested_at < previous.requested_at
                    || request.requested_at == previous.requested_at
                        && serde_json::to_value(&request)? != serde_json::to_value(previous)?
                {
                    continue;
                }
                removed |= request.route.signing_key != previous.route.signing_key
                    || request.route.identity_key != previous.route.identity_key
                    || request.route.pickup_key != previous.route.pickup_key
                    || request.route.mailbox != previous.route.mailbox;
            }
            fresh_request |= wallet.requests.get(cell).is_none_or(|previous| {
                previous.requested_at != request.requested_at
                    || previous.operational_key != request.operational_key
                    || previous.route != request.route
            });
            wallet.requests.insert(cell.to_owned(), request);
        }
        removed |= recovering_retired && fresh_request;
        if removed {
            wallet.generation = wallet
                .generation
                .checked_add(1)
                .ok_or_else(|| invalid("Private reply authority generation exhausted"))?;
        }
        if removed || fresh_request || wallet.control.expires_at <= now + 86400 {
            wallet.control = OwnerControl {
                owner_key: binding.owner_key.clone(),
                generation: wallet.generation.to_string(),
                issued_at: now,
                expires_at: now + AUTHORITY_LIFETIME,
                signature: String::new(),
            };
            wallet.control.signature =
                owner.sign_bytes(&signing_bytes("reply-owner", &wallet.control)?);
        }
        wallet.members = members.clone();
        wallet.revocation_version = wallet.revocation_version.max(revocation_version);
        let mut state = held.clone();
        state["binding"] = serde_json::to_value(&binding)?;
        state["control"] = serde_json::to_value(&wallet.control)?;
        for (cell, request) in &wallet.requests {
            state[format!("request_{cell}")] = serde_json::to_value(request)?;
            let auth_field = format!("authorized_{cell}");
            let existing = &state[&auth_field];
            if existing["control"] == serde_json::to_value(&wallet.control)?
                && existing["route"] == serde_json::to_value(&request.route)?
                && serde_json::from_value::<DeviceCertificate>(existing["certificate"].clone())
                    .ok()
                    .is_some_and(|certificate| {
                        request_auth::validate_certificate(&wallet.control, &certificate, now)
                            .is_ok()
                    })
            {
                continue;
            }
            let mut certificate = DeviceCertificate {
                owner_key: binding.owner_key.clone(),
                signing_key: request.route.signing_key.clone(),
                identity_key: request.route.identity_key.clone(),
                pickup_key: request.route.pickup_key.clone(),
                mailbox: request.route.mailbox.clone(),
                generation: wallet.generation.to_string(),
                issued_at: now.max(wallet.control.issued_at).max(request.requested_at),
                expires_at: wallet.control.expires_at,
                signature: String::new(),
            };
            certificate.signature = owner.sign_bytes(&signing_bytes("reply-device", &certificate)?);
            state[auth_field] =
                json!({"route":request.route,"control":wallet.control,"certificate":certificate});
        }
        for (field, value) in state
            .as_object_mut()
            .into_iter()
            .flat_map(|object| object.iter_mut())
        {
            if let Some(cell) = field.strip_prefix("authorized_") {
                if !members.contains_key(cell) {
                    *value = Value::Null;
                }
            }
        }
        if state == held && serde_json::to_value(&wallet)? == previous_wallet {
            return self.social_finish_reply_route(context).await;
        }
        let sealed = session::seal_local(&wallet_id, &wallet, &wallet_key)?;
        let mut tx = self.social_write_tx().await?;
        let current = extension_on(&mut tx, context, SESSION_AUTHORITY_NAMESPACE).await?;
        if current != held {
            return Err(invalid(
                "Device authorization requests changed; retry with their latest state",
            ));
        }
        if serde_json::to_vec(&state)?.len() > 256 * 1024 {
            return Err(invalid("Private reply authorization state is full"));
        }
        store::social::put_device_state_on(
            &mut tx,
            &wallet_id,
            "authority",
            context,
            &sealed,
            Some(version),
            now,
        )
        .await?;
        store::records::set_extension_on(&mut tx, context, SESSION_AUTHORITY_NAMESPACE, &state)
            .await?;
        tx.commit().await?;
        self.social_finish_reply_route(context).await
    }

    pub(super) async fn social_finish_reply_route(&self, context: &str) -> Result<(), EngineError> {
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        let key = self.social_storage_key().await?;
        let account_id = format!("account:{context}");
        let Some((body, _)) = store::social::device_state(&self.store.pool, &account_id).await?
        else {
            return Ok(());
        };
        let account: session::AccountState = session::open_local(&account_id, &body, &key)?;
        let mut tx = self.social_write_tx().await?;
        let mut state = extension_on(&mut tx, context, SESSION_AUTHORITY_NAMESPACE).await?;
        let field = format!("authorized_{}", cell.uid);
        let Some(value) = state.get(&field).filter(|value| !value.is_null()) else {
            return Ok(());
        };
        let certificate: DeviceCertificate = serde_json::from_value(value["certificate"].clone())?;
        let control: OwnerControl = serde_json::from_value(value["control"].clone())?;
        if certificate.signing_key != account.route.signing_key
            || certificate.identity_key != account.route.identity_key
            || certificate.pickup_key != account.route.pickup_key
            || certificate.mailbox != account.route.mailbox
            || control != serde_json::from_value::<OwnerControl>(state["control"].clone())?
        {
            return Err(invalid(
                "This reply authorization belongs to another account or was superseded",
            ));
        }
        let mut route = CertifiedRoute {
            route: account.route.clone(),
            accepting_introductions: true,
            control,
            certificate,
            expires_at: nucleus::execution::now().timestamp() + 30 * 86400,
            signature: String::new(),
        };
        if let Ok(existing) = serde_json::from_value::<CertifiedRoute>(value.clone()) {
            if existing.certificate == route.certificate
                && existing.route == route.route
                && request_auth::validate_route(&existing, nucleus::execution::now().timestamp())
                    .is_ok()
            {
                return Ok(());
            }
        }
        route.expires_at = route.control.issued_at + 30 * 86400;
        route.signature = account
            .signing_key()?
            .sign_bytes(&signing_bytes("reply-route", &route)?);
        request_auth::validate_route(&route, nucleus::execution::now().timestamp())?;
        state[field] = serde_json::to_value(route)?;
        store::records::set_extension_on(&mut tx, context, SESSION_AUTHORITY_NAMESPACE, &state)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn social_reply_key_status(
        &self,
        context: &str,
    ) -> Result<Value, EngineError> {
        if let Some(status) = self.social_context_retention_status(context).await? {
            return Ok(status);
        }
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        let state =
            store::records::get_extension(&self.store.pool, context, SESSION_AUTHORITY_NAMESPACE)
                .await?
                .unwrap_or_else(|| json!({}));
        let Some(binding) = state.get("binding") else {
            return Ok(if state.get(format!("request_{}", cell.uid)).is_some() {
                json!({"record":context,"reply_keys":"waiting-for-owner","status":"Private keys are saved on this device. Retained history syncs now; the owner will initialize and authorize replies after receiving this request"})
            } else {
                json!({"record":context,"reply_keys":"unprepared","status":"Prepare private reply keys on the owner device first"})
            });
        };
        self.social_validate_private_binding(context, &serde_json::from_value(binding.clone())?)
            .await?;
        let route = state
            .get(format!("authorized_{}", cell.uid))
            .and_then(|value| serde_json::from_value::<CertifiedRoute>(value.clone()).ok());
        let status = if let Some(route) = &route {
            if route.control.generation != state["control"]["generation"].as_str().unwrap_or("") {
                "revoked"
            } else if request_auth::validate_route(route, nucleus::execution::now().timestamp())
                .is_ok()
            {
                "ready"
            } else {
                "expired"
            }
        } else {
            "waiting-for-owner"
        };
        Ok(
            json!({"record":context,"reply_keys":status,"route":route.filter(|_| status == "ready"),"status":match status {
                "ready" => "This device has its own authorized reply keys. Mailbox registration and delivery must still be enabled",
                "expired" => "Reply authorization expired. Retained history is available; wait for the owner to renew sending authority",
                "revoked" => "These reply keys were revoked. Retained history remains separate",
                _ => "Keys prepared on this device. Retained history syncs now; sending waits for owner authorization",
            }}),
        )
    }

    pub async fn social_refresh_reply_authorizations(&self) -> Result<usize, EngineError> {
        self.social_require_local_write().await?;
        self.social_maintain_contexts(nucleus::execution::now().timestamp())
            .await?;
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let rows = store::sqlx::query("SELECT e.record_uid,e.fds FROM record_extension e JOIN record r ON r.uid=e.record_uid WHERE e.namespace=? AND r.organ_uid=? AND NOT EXISTS(SELECT 1 FROM social_context_retention t WHERE t.context=e.record_uid AND t.state IN ('dormant','retired','review')) ORDER BY e.record_uid LIMIT 257")
            .bind(SESSION_AUTHORITY_NAMESPACE).bind(&organ.uid).fetch_all(&self.store.pool).await?;
        if rows.len() > 256 {
            return Err(invalid(
                "Resolve old reply contexts before preparing more than 256",
            ));
        }
        let mut ready = 0;
        for row in rows {
            let context: String = row.get("record_uid");
            let state: Value = serde_json::from_str(&row.get::<String, _>("fds"))?;
            let account_id = format!("account:{context}");
            let account = store::social::device_state(&self.store.pool, &account_id).await?;
            let expired = if let Some((body, _)) = &account {
                let key = self.social_storage_key().await?;
                session::open_local::<session::AccountState>(&account_id, body, &key)?.expires_at
                    <= nucleus::execution::now().timestamp()
            } else {
                false
            };
            if account.is_none() || expired || state.get("binding").is_none() {
                let services: Vec<String> = serde_json::from_value(state["services"].clone())?;
                self.social_prepare_reply_keys(&context, services).await?;
            } else {
                self.social_authorize_reply_context(&context).await?;
                self.social_finish_reply_route(&context).await?;
            }
            if self.social_reply_key_status(&context).await?["reply_keys"] == "ready" {
                ready += 1;
            }
        }
        Ok(ready)
    }

    pub(super) async fn social_note_reply_revocation(
        &self,
        previous: Option<&crate::roster::Roster>,
        next: &crate::roster::Roster,
    ) -> Result<(), EngineError> {
        let removed = previous.is_some_and(|held| {
            held.cells
                .iter()
                .filter(|cell| cell.may(crate::roster::CAP_WRITE))
                .any(|old| {
                    !next.cells.iter().any(|new| {
                        new.cell_uid == old.cell_uid
                            && new.operational_key == old.operational_key
                            && new.may(crate::roster::CAP_WRITE)
                    })
                })
        });
        if !removed {
            return Ok(());
        }
        let contexts: Vec<String> = store::sqlx::query_scalar("SELECT e.record_uid FROM record_extension e JOIN record r ON r.uid=e.record_uid WHERE e.namespace=? AND r.organ_uid=? AND (r.deleted_at IS NULL OR EXISTS(SELECT 1 FROM record_extension p JOIN record c ON c.uid=p.record_uid WHERE p.namespace='lince.social.participants' AND c.organ_uid=r.organ_uid AND c.deleted_at IS NULL AND json_extract(p.fds,'$.context')=e.record_uid AND json_extract(p.fds,'$.state') IN ('pending','accepted'))) LIMIT 257")
            .bind(SESSION_AUTHORITY_NAMESPACE).bind(&next.organ_uid).fetch_all(&self.store.pool).await?;
        if contexts.len() > 256 {
            return Err(invalid("Private reply context limit exceeded"));
        }
        let mut tx = self.social_write_tx().await?;
        for context in contexts {
            let mut state = extension_on(&mut tx, &context, SESSION_AUTHORITY_NAMESPACE).await?;
            let version = state["revocation_version"]
                .as_i64()
                .unwrap_or(0)
                .max(next.version);
            state["revocation_version"] = json!(version);
            store::records::set_extension_on(
                &mut tx,
                &context,
                SESSION_AUTHORITY_NAMESPACE,
                &state,
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}
