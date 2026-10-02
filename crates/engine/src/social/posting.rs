use super::*;
use nucleus::social::requests::PrivateOwnerBinding;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Wallet {
    secret: String,
    editor_secret: String,
    members: Value,
    revocation_version: i64,
    authority: PostingAuthority,
    destinations: Vec<String>,
}

pub(super) fn validate_authority(
    authority: &PostingAuthority,
    now: i64,
) -> Result<(), EngineError> {
    if authority
        .generation
        .parse::<i64>()
        .ok()
        .is_none_or(|generation| generation <= 0 || generation.to_string() != authority.generation)
        || authority.issued_at > now + 300
        || authority.expires_at <= now
        || authority.expires_at <= authority.issued_at
        || authority.expires_at - authority.issued_at > MAX_LIFETIME
        || !crate::roster::verify_with(
            &authority.owner_key,
            &signing_bytes("posting-authority", authority)?,
            &authority.signature,
        )
    {
        return Err(invalid(
            "The anonymous posting authority is invalid, expired or revoked",
        ));
    }
    for key in [&authority.owner_key, &authority.editor_key] {
        let bytes = B64
            .decode(key)
            .map_err(|_| invalid("Invalid anonymous posting key"))?;
        if bytes.len() != 32 || B64.encode(bytes) != *key {
            return Err(invalid("Invalid anonymous posting key"));
        }
    }
    Ok(())
}

impl Engine {
    pub(super) async fn social_validate_backup_posting_wallet(
        &self,
        id: &str,
        context: &str,
        body: &str,
        key: &[u8; 32],
        root: &Signer,
    ) -> Result<(), EngineError> {
        let mut wallet: Wallet = session::open_local(id, body, key)?;
        let secret = zeroize::Zeroizing::new(std::mem::take(&mut wallet.secret));
        let editor_secret = zeroize::Zeroizing::new(std::mem::take(&mut wallet.editor_secret));
        let owner = session::secret_signer(&secret)?;
        let editor = session::secret_signer(&editor_secret)?;
        let held = store::records::get_extension(&self.store.pool, context, PUBLICATION_NAMESPACE)
            .await?
            .ok_or_else(|| invalid("The backup anonymous wallet has no owner binding"))?;
        let binding: PrivateOwnerBinding =
            serde_json::from_value(held["anonymous_binding"].clone())?;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("The backup has no owner Cell"))?;
        validate_authority(&wallet.authority, wallet.authority.issued_at)?;
        let trusted = self.key_chains(&root.actor_uid, &binding.root_key).await?
            || self
                .published_successions(&root.actor_uid)
                .await?
                .iter()
                .any(|row| row.old_key == binding.root_key);
        if binding.context != context
            || binding.organ != root.actor_uid
            || binding.owner_cell != cell.uid
            || binding.owner_key != owner.public_key_b64()
            || wallet.authority.owner_key != binding.owner_key
            || wallet.authority.editor_key != editor.public_key_b64()
            || wallet
                .members
                .as_object()
                .is_none_or(|members| members.len() > 64)
            || wallet.destinations.len() > 64
            || wallet
                .destinations
                .iter()
                .any(|endpoint| endpoint.parse::<iroh::EndpointId>().is_err())
            || !trusted
            || !crate::roster::verify_with(
                &binding.root_key,
                &signing_bytes("private-posting-owner-binding", &binding)?,
                &binding.signature,
            )
        {
            return Err(invalid(
                "The backup anonymous wallet does not match its signed owner binding",
            ));
        }
        Ok(())
    }

    async fn social_posting_wallet_key(&self) -> Result<[u8; 32], EngineError> {
        if let Some(key) = *self
            .social_memory_wallet_key
            .lock()
            .expect("memory authority key")
        {
            return Ok(key);
        }
        let path = self
            .sealing_keyring_path
            .lock()
            .expect("device key path")
            .clone()
            .or_else(|| self.root_key_path.lock().expect("owner key path").clone());
        if let Some(path) = path {
            return tokio::task::spawn_blocking(move || {
                session::storage_key(&path.with_file_name("social-authority-wallet-v1.key"))
            })
            .await
            .map_err(|_| invalid("Anonymous authority wallet loading stopped"))?;
        }
        let file: String =
            store::sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name='main'")
                .fetch_one(&self.store.pool)
                .await?;
        if !file.is_empty() {
            return Err(invalid(
                "Configure the owner's private key directory before anonymous publication",
            ));
        }
        let mut held = self
            .social_memory_wallet_key
            .lock()
            .expect("memory authority key");
        if let Some(key) = *held {
            return Ok(key);
        }
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).map_err(|_| invalid("Secure randomness is unavailable"))?;
        *held = Some(key);
        Ok(key)
    }

    pub(super) async fn social_prepare_posting_authority(
        &self,
        context: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        self.social_require_local_write().await?;
        let mut state =
            store::records::get_extension(&self.store.pool, context, PUBLICATION_NAMESPACE)
                .await?
                .ok_or_else(|| invalid("No announcement draft"))?;
        let expected_state = state.clone();
        let draft: PostDraft = serde_json::from_value(state["draft"].clone())?;
        if draft.mode != AuthorMode::Anonymous {
            return Ok(());
        }
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        let root = self.social_checked_root_signer().await?;
        if root.is_none() && self.roster_of(&organ.uid).await?.is_some() {
            return Ok(());
        }
        let now = nucleus::execution::now().timestamp();
        let members = self.social_editor_members().await?;
        let private =
            store::records::get_extension(&self.store.pool, &organ.uid, PRIVATE_NAMESPACE)
                .await?
                .unwrap_or_else(|| json!({}));
        let revocation_version = private["posting_revocation_version"].as_i64().unwrap_or(0);
        let mut binding: PrivateOwnerBinding;
        if let Some(saved) = state.get("anonymous_binding") {
            binding = serde_json::from_value(saved.clone())?;
            if binding.organ != organ.uid || binding.owner_cell != cell.uid {
                return Ok(());
            }
        } else {
            if !draft.alias.is_empty() {
                for (uid, post) in self.social_publications().await? {
                    if self.social_own_record(&uid, actor).await.is_ok()
                        && post["draft"]["mode"] == "anonymous"
                        && post["draft"]["alias"] == draft.alias
                        && post.get("anonymous_binding").is_some()
                    {
                        state["anonymous_binding"] = post["anonymous_binding"].clone();
                        state["anonymous_authority"] = post["anonymous_authority"].clone();
                        state["secret"] = post["secret"].clone();
                        break;
                    }
                }
            }
            if let Some(saved) = state.get("anonymous_binding") {
                binding = serde_json::from_value(saved.clone())?;
                if binding.owner_cell != cell.uid {
                    let mut tx = self.social_write_tx().await?;
                    store::records::set_extension_on(
                        &mut tx,
                        context,
                        PUBLICATION_NAMESPACE,
                        &state,
                    )
                    .await?;
                    tx.commit().await?;
                    return Ok(());
                }
            } else {
                binding = PrivateOwnerBinding {
                    organ: organ.uid.clone(),
                    context: context.into(),
                    owner_cell: cell.uid.clone(),
                    owner_key: String::new(),
                    root_key: String::new(),
                    signature: String::new(),
                };
            }
        }
        let id = format!("posting:{}", binding.context);
        let held = store::social::device_state(&self.store.pool, &id).await?;
        let key = self.social_posting_wallet_key().await?;
        let mut wallet: Wallet = if let Some((body, _)) = &held {
            session::open_local(&id, body, &key)?
        } else {
            if !binding.owner_key.is_empty() {
                return Err(invalid(
                    "Restore the separate anonymous authority wallet before renewing this post",
                ));
            }
            let owner = new_social_signer("")?;
            let editor = session::secret_signer(state["secret"].as_str().unwrap_or_default())?;
            binding.owner_key = owner.public_key_b64();
            Wallet {
                secret: B64.encode(owner.secret_bytes()),
                editor_secret: B64.encode(editor.secret_bytes()),
                members: members.clone(),
                revocation_version,
                authority: PostingAuthority {
                    owner_key: owner.public_key_b64(),
                    editor_key: editor.public_key_b64(),
                    generation: "1".into(),
                    issued_at: now,
                    expires_at: now + MAX_LIFETIME,
                    signature: String::new(),
                },
                destinations: vec![],
            }
        };
        let previous = serde_json::to_value(&wallet)?;
        let owner = session::secret_signer(&wallet.secret)?;
        if binding.organ != organ.uid
            || binding.owner_key != owner.public_key_b64()
            || wallet.authority.owner_key != owner.public_key_b64()
        {
            return Err(invalid(
                "The anonymous owner binding differs from its retained wallet",
            ));
        }
        if !binding.root_key.is_empty()
            && root
                .as_ref()
                .is_some_and(|root| root.public_key_b64() != binding.root_key)
            && !self.key_chains(&organ.uid, &binding.root_key).await?
            && !self
                .published_successions(&organ.uid)
                .await?
                .iter()
                .any(|change| change.old_key == binding.root_key)
        {
            return Err(invalid(
                "Verify anonymous ownership before renewing authority",
            ));
        }
        if !binding.root_key.is_empty()
            && !crate::roster::verify_with(
                &binding.root_key,
                &signing_bytes("private-posting-owner-binding", &binding)?,
                &binding.signature,
            )
        {
            return Err(invalid(
                "The private anonymous ownership binding cannot be verified",
            ));
        }
        let removed = wallet.revocation_version < revocation_version
            || wallet
                .members
                .as_object()
                .into_iter()
                .flatten()
                .any(|(cell, key)| members.get(cell) != Some(key));
        if removed {
            let editor = new_social_signer("")?;
            wallet.editor_secret = B64.encode(editor.secret_bytes());
            wallet.authority.editor_key = editor.public_key_b64();
            wallet.authority.generation = wallet
                .authority
                .generation
                .parse::<i64>()
                .map_err(|_| invalid("Invalid anonymous authority generation"))?
                .checked_add(1)
                .ok_or_else(|| invalid("Anonymous authority exhausted"))?
                .to_string();
        }
        let renewed = removed
            || wallet.authority.signature.is_empty()
            || wallet.authority.expires_at <= now + 86400;
        if renewed {
            wallet.authority.issued_at = now;
            wallet.authority.expires_at = now + MAX_LIFETIME;
            wallet.authority.signature =
                owner.sign_bytes(&signing_bytes("posting-authority", &wallet.authority)?);
        }
        wallet.members = members;
        wallet.revocation_version = revocation_version;
        let old_destinations = wallet.destinations.clone();
        let published_destinations: Vec<String> = state["published"]["destinations"]
            .as_array()
            .map(|destinations| {
                destinations
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        for destination in &published_destinations {
            if !wallet.destinations.contains(destination) {
                wallet.destinations.push(destination.clone());
            }
        }
        wallet.destinations.sort();
        if wallet.destinations.len() > 64 {
            return Err(invalid(
                "Use at most 64 publication hosts for one anonymous alias",
            ));
        }
        if let Some(root) = root {
            binding.root_key = root.public_key_b64();
            binding.signature =
                root.sign_bytes(&signing_bytes("private-posting-owner-binding", &binding)?);
        }
        let mut tx = self.social_write_tx().await?;
        self.social_require_local_write_on(&mut tx).await?;
        let revoked: bool = store::sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM identity_revocation WHERE organ_uid=? AND revoked_key=?)",
        )
        .bind(&organ.uid)
        .bind(&binding.root_key)
        .fetch_one(&mut *tx)
        .await?;
        let mut current = owner::extension_on(&mut tx, context, PUBLICATION_NAMESPACE).await?;
        let latest_wallet: Option<(String, i64)> =
            store::sqlx::query_as("SELECT body,version FROM social_device_state WHERE id=?")
                .bind(&id)
                .fetch_optional(&mut *tx)
                .await?;
        let latest_private = owner::extension_on(&mut tx, &organ.uid, PRIVATE_NAMESPACE).await?;
        if revoked
            || current != expected_state
            || latest_wallet != held
            || wallet.members != Self::social_editor_members_on(&mut tx, &organ.uid).await?
            || revocation_version
                != latest_private["posting_revocation_version"]
                    .as_i64()
                    .unwrap_or(0)
        {
            return Err(invalid(
                "The announcement changed during authorization; refresh",
            ));
        }
        current["anonymous_binding"] = json!(binding);
        current["anonymous_authority"] = json!(wallet.authority);
        current["secret"] = json!(wallet.editor_secret);
        if held.is_none() || previous != serde_json::to_value(&wallet)? {
            store::social::put_device_state_on(
                &mut tx,
                &id,
                "authority",
                &binding.context,
                &session::seal_local(&id, &wallet, &key)?,
                held.as_ref().map(|(_, version)| *version),
                now,
            )
            .await?;
        }
        if wallet.authority.generation != "1"
            && (renewed
                || old_destinations != wallet.destinations
                || current["anonymous_controls"][0]["authority"] != json!(wallet.authority))
        {
            let mut controls = Vec::new();
            for destinations in wallet.destinations.chunks(8) {
                let control = PostingAuthorityPublication {
                    authority: wallet.authority.clone(),
                    destinations: destinations.to_vec(),
                };
                store::social::enqueue_on(
                    &mut tx,
                    "posting-authority",
                    &document_hash("posting-control", &control)?,
                    &serde_json::to_string(&control)?,
                    destinations,
                    control.authority.expires_at,
                )
                .await?;
                controls.push(control);
            }
            current["anonymous_controls"] = json!(controls);
        }
        if current != owner::extension_on(&mut tx, context, PUBLICATION_NAMESPACE).await? {
            store::records::set_extension_on(&mut tx, context, PUBLICATION_NAMESPACE, &current)
                .await?;
        }
        store::social::anchor_posting_authority_on(&mut tx, &wallet.authority).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn social_refresh_posting_authorities(&self) -> Result<(), EngineError> {
        for (context, _) in self.social_publications().await? {
            if let Err(error) = self.social_prepare_posting_authority(&context, None).await {
                tracing::debug!(%error, %context, "Anonymous posting authority will retry");
            }
        }
        Ok(())
    }
}
