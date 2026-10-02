use super::*;

pub fn validate_delegation(doc: &Delegation, now: i64) -> Result<(), EngineError> {
    validate_delegation_for(doc, now, false)
}

pub(super) fn validate_ending_delegation(doc: &Delegation, now: i64) -> Result<(), EngineError> {
    validate_delegation_for(doc, now, true)
}

fn validate_delegation_for(doc: &Delegation, now: i64, ending: bool) -> Result<(), EngineError> {
    if !nucleus::valid_uid(&doc.organ, "r")
        || doc.expires_at <= now
        || doc.issued_at > now + 300
        || doc.expires_at <= doc.issued_at
        || doc.expires_at.saturating_sub(doc.issued_at) > MAX_LIFETIME
        || doc
            .generation
            .parse::<i64>()
            .ok()
            .is_none_or(|n| n <= 0 || n.to_string() != doc.generation)
        || doc.successions.len() > 8
    {
        return Err(invalid("The public-profile authority expired"));
    }
    for (index, change) in doc.successions.iter().enumerate() {
        if change.old_key == change.new_key
            || change.created_at.len() > 64
            || index > 0 && doc.successions[index - 1].new_key != change.old_key
            || DateTime::parse_from_rfc3339(&change.created_at)
                .ok()
                .is_none_or(|at| at.timestamp() > now + 300)
            || !crate::roster::verify_with(
                &change.old_key,
                &crate::roster::succession_signing_payload(
                    &doc.organ,
                    &change.old_key,
                    &change.new_key,
                    &change.created_at,
                ),
                &change.signature,
            )
        {
            return Err(invalid("Invalid public-profile identity succession"));
        }
    }
    if doc
        .successions
        .last()
        .is_some_and(|change| change.new_key != doc.root_key)
    {
        return Err(invalid(
            "Public-profile succession does not reach its signing identity",
        ));
    }
    if !crate::roster::verify_with(
        &doc.root_key,
        &signing_bytes("profile-authority", doc)?,
        &doc.signature,
    ) && !(ending
        && crate::roster::verify_with(
            &doc.root_key,
            &signing_bytes("profile-ending-authority", doc)?,
            &doc.signature,
        ))
    {
        return Err(invalid("Invalid public-profile authority"));
    }
    Ok(())
}

pub fn validate_profile(doc: &Profile, now: i64) -> Result<(), EngineError> {
    validate_delegation(&doc.authority, now)?;
    if doc.protocol != "lince.profile.1"
        || doc
            .revision
            .parse::<i64>()
            .ok()
            .is_none_or(|n| n <= 0 || n.to_string() != doc.revision)
        || doc.issued_at > now + 300
        || doc.expires_at <= now
        || doc.expires_at <= doc.issued_at
        || doc.issued_at < doc.authority.issued_at
        || doc.expires_at > doc.authority.expires_at
        || doc.expires_at.saturating_sub(doc.issued_at) > MAX_LIFETIME
        || doc.destinations.len() > 8
        || !matches!(doc.state, PostState::Active | PostState::Withdrawn)
        || doc
            .destinations
            .iter()
            .any(|id| id.parse::<iroh::EndpointId>().is_err())
        || doc.parents.len() > 64
        || doc
            .parents
            .iter()
            .any(|p| p.len() != 64 || !p.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(invalid("Invalid public-profile revision or lifetime"));
    }
    validate_fields(&doc.fields)?;
    if serde_json::to_vec(doc)?.len() > MAX_PROFILE_BYTES
        || !crate::roster::verify_with(
            &doc.authority.editor_key,
            &signing_bytes("profile", doc)?,
            &doc.signature,
        )
    {
        return Err(invalid("Invalid or oversized signed profile"));
    }
    Ok(())
}

fn validate_fields(fields: &ProfileFields) -> Result<(), EngineError> {
    text(&fields.name, 160, false).map_err(invalid)?;
    text(&fields.description, 8000, true).map_err(invalid)?;
    text(&fields.area, 160, true).map_err(invalid)?;
    text(&fields.contact, 512, true).map_err(invalid)?;
    for hash in [&fields.avatar, &fields.banner].into_iter().flatten() {
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid("Select a public image content hash"));
        }
    }
    Ok(())
}

pub(super) fn profile_editor(state: &Value) -> Value {
    let revisions: std::collections::HashMap<_, _> = state
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| {
            key.strip_prefix("revision_")
                .map(|hash| (hash.to_owned(), value))
        })
        .collect();
    let parents: std::collections::HashSet<_> = revisions
        .values()
        .flat_map(|doc| {
            doc["parents"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
        })
        .collect();
    let mut heads: Vec<_> = revisions
        .iter()
        .filter(|(hash, _)| !parents.contains(hash.as_str()))
        .collect();
    heads.sort_by(|(a, _), (b, _)| a.cmp(b));
    let mut fields = heads
        .iter()
        .max_by_key(|(hash, doc)| {
            (
                doc["authority"]["generation"]
                    .as_str()
                    .and_then(|value| value.parse::<i64>().ok())
                    .unwrap_or(0),
                doc["revision"]
                    .as_str()
                    .and_then(|value| value.parse::<i64>().ok())
                    .unwrap_or(0),
                (*hash).clone(),
            )
        })
        .map(|(_, doc)| doc["fields"].clone())
        .unwrap_or_else(|| json!(ProfileFields::default()));
    let mut common: Option<std::collections::HashSet<String>> = None;
    for (hash, _) in &heads {
        let mut ancestors = std::collections::HashSet::new();
        let mut pending = vec![(*hash).clone()];
        while let Some(hash) = pending.pop() {
            if !ancestors.insert(hash.clone()) {
                continue;
            }
            if ancestors.len() > 256 {
                break;
            }
            if let Some(doc) = revisions.get(&hash) {
                pending.extend(
                    doc["parents"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(str::to_owned),
                );
            }
        }
        common = Some(match common {
            None => ancestors,
            Some(common) => common.intersection(&ancestors).cloned().collect(),
        });
    }
    let base = common
        .into_iter()
        .flatten()
        .filter_map(|hash| revisions.get(&hash).map(|doc| (hash, *doc)))
        .max_by_key(|(hash, doc)| {
            (
                doc["revision"]
                    .as_str()
                    .and_then(|value| value.parse::<i64>().ok())
                    .unwrap_or(0),
                hash.clone(),
            )
        })
        .map(|(_, doc)| &doc["fields"]);
    let mut conflicts = Vec::new();
    for key in ["name", "description", "area", "contact", "avatar", "banner"] {
        let mut changed = Vec::new();
        for (_, doc) in &heads {
            let value = &doc["fields"][key];
            if base.is_none_or(|base| value != &base[key]) && !changed.contains(&value) {
                changed.push(value);
            }
        }
        if changed.len() == 1 {
            fields[key] = changed[0].clone();
        }
        if changed.len() > 1 {
            conflicts.push(json!({"field":key,"values":changed}));
        }
    }
    json!({"fields":fields,"heads":heads.iter().take(8).map(|(hash, _)| hash.as_str()).collect::<Vec<_>>(),"remaining_heads":heads.len().saturating_sub(8),"conflicts":conflicts})
}

fn trim_profile_state(state: &mut Value) -> Result<(), EngineError> {
    let object = state
        .as_object_mut()
        .ok_or_else(|| invalid("Invalid retained profile state"))?;
    let parents: std::collections::HashSet<String> = object
        .iter()
        .filter(|(key, _)| key.starts_with("revision_"))
        .flat_map(|(_, doc)| {
            doc["parents"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
        })
        .collect();
    let mut revisions: Vec<_> = object
        .iter()
        .filter(|(key, _)| key.starts_with("revision_"))
        .map(|(key, doc)| {
            (
                key.clone(),
                doc["revision"]
                    .as_str()
                    .and_then(|value| value.parse::<i64>().ok())
                    .unwrap_or(0),
            )
        })
        .collect();
    revisions.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut ancestors = 0;
    for (key, _) in revisions {
        if parents.contains(key.strip_prefix("revision_").unwrap()) {
            ancestors += 1;
            if ancestors > 64 {
                object.remove(&key);
            }
        }
    }
    if serde_json::to_vec(state)?.len() > store::records::MAX_EXTENSION_BYTES {
        return Err(invalid(
            "Resolve the retained profile branches in groups before adding more edits",
        ));
    }
    Ok(())
}

impl Engine {
    pub async fn social_renew_profile(&self) -> Result<bool, EngineError> {
        self.social_require_local_write().await?;
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let Some(state) =
            store::records::get_extension(&self.store.pool, &organ.uid, PROFILE_NAMESPACE).await?
        else {
            return Ok(false);
        };
        let Some(value) = state.get("published") else {
            return Ok(false);
        };
        let document: Profile = serde_json::from_value(value.clone())?;
        let now = nucleus::execution::now();
        if document.state != PostState::Active || document.destinations.is_empty() {
            return Ok(false);
        }
        let Some(root) = self.social_checked_root_signer().await? else {
            return Ok(false);
        };
        let private =
            store::records::get_extension(&self.store.pool, &organ.uid, PRIVATE_NAMESPACE)
                .await?
                .unwrap_or_else(|| json!({}));
        if document.expires_at > now.timestamp() + 86400
            && document.authority.root_key == root.public_key_b64()
            && private["profile_signer"]["authority"]["generation"].as_str()
                == Some(document.authority.generation.as_str())
        {
            return Ok(false);
        }
        let editor = profile_editor(&state);
        let heads: Vec<String> = serde_json::from_value(editor["heads"].clone())?;
        if heads.len() != 1
            || editor["remaining_heads"].as_u64() != Some(0)
            || heads[0] != document_hash("profile", &document)?
        {
            return Ok(false);
        }
        self.social_save_profile(
            document.fields,
            heads,
            document.destinations,
            PostState::Active,
            None,
            now,
            Some(&state),
            None,
        )
        .await?;
        Ok(true)
    }

    pub(crate) async fn social_note_roster_change(
        &self,
        previous: Option<&crate::roster::Roster>,
        next: &crate::roster::Roster,
    ) -> Result<(), EngineError> {
        if store::organs::local(&self.store.pool)
            .await?
            .is_none_or(|organ| organ.uid != next.organ_uid)
        {
            return Ok(());
        }
        let cell = store::cells::local(&self.store.pool).await?;
        if cell.is_none_or(|cell| {
            !next
                .cells
                .iter()
                .any(|member| member.cell_uid == cell.uid && member.may(crate::roster::CAP_WRITE))
        }) {
            return Ok(());
        }
        self.social_note_reply_revocation(previous, next).await?;
        let posting_revoked = previous.is_some_and(|held| {
            held.cells
                .iter()
                .filter(|old| old.may(crate::roster::CAP_WRITE))
                .any(|old| {
                    !next.cells.iter().any(|new| {
                        new.cell_uid == old.cell_uid
                            && new.operational_key == old.operational_key
                            && new.may(crate::roster::CAP_WRITE)
                    })
                })
        });
        if posting_revoked {
            let mut state =
                store::records::get_extension(&self.store.pool, &next.organ_uid, PRIVATE_NAMESPACE)
                    .await?
                    .unwrap_or_else(|| json!({}));
            state["posting_revocation_version"] = json!(
                state["posting_revocation_version"]
                    .as_i64()
                    .unwrap_or(0)
                    .max(next.version)
            );
            store::records::set_extension(
                &self.store.pool,
                &next.organ_uid,
                PRIVATE_NAMESPACE,
                &state,
            )
            .await?;
        }
        let Some(mut private) =
            store::records::get_extension(&self.store.pool, &next.organ_uid, PRIVATE_NAMESPACE)
                .await?
        else {
            return Ok(());
        };
        if private["profile_signer"].is_null() {
            return Ok(());
        }
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
        if removed {
            private["editor_rotation_required"] = json!(true);
        }
        let mut members = serde_json::Map::new();
        for cell in next
            .cells
            .iter()
            .filter(|cell| cell.may(crate::roster::CAP_WRITE))
        {
            members.insert(cell.cell_uid.clone(), json!(cell.operational_key));
        }
        private["profile_signer"]["authorized_cells"] = Value::Object(members);
        store::records::set_extension(
            &self.store.pool,
            &next.organ_uid,
            PRIVATE_NAMESPACE,
            &private,
        )
        .await?;
        Ok(())
    }

    pub(super) async fn social_editor_members(&self) -> Result<Value, EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let mut tx = self.store.pool.begin().await?;
        self.social_require_local_write_on(&mut tx).await?;
        let members = Self::social_editor_members_on(&mut tx, &organ.uid).await?;
        tx.commit().await?;
        Ok(members)
    }

    pub(super) async fn social_editor_members_on(
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        organ: &str,
    ) -> Result<Value, EngineError> {
        let mut members = serde_json::Map::new();
        let payload: Option<String> =
            store::sqlx::query_scalar("SELECT payload FROM organ_roster WHERE organ_uid=?")
                .bind(organ)
                .fetch_optional(&mut **tx)
                .await?;
        if let Some(payload) = payload {
            let roster: crate::roster::Roster = serde_json::from_str(&payload)?;
            for member in roster
                .cells
                .into_iter()
                .filter(|cell| cell.may(crate::roster::CAP_WRITE))
            {
                members.insert(member.cell_uid, json!(member.operational_key));
            }
        }
        Ok(Value::Object(members))
    }

    pub(super) async fn social_refresh_editor_members(&self) -> Result<(), EngineError> {
        let Some(root) = self.social_checked_root_signer().await? else {
            return Ok(());
        };
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let private =
            store::records::get_extension(&self.store.pool, &organ.uid, PRIVATE_NAMESPACE)
                .await?
                .unwrap_or_else(|| json!({}));
        if private["profile_signer"].is_null() {
            return Ok(());
        }
        let current = self.social_editor_members().await?;
        let removed = private["profile_signer"]["authorized_cells"]
            .as_object()
            .is_some_and(|held| {
                held.iter()
                    .any(|(cell, key)| current.get(cell) != Some(key))
            });
        if removed
            || private["editor_rotation_required"] == true
            || private["profile_signer"]["authority"]["root_key"].as_str()
                != Some(root.public_key_b64().as_str())
        {
            self.social_rotate_profile_authority(None, nucleus::execution::now())
                .await?;
        }
        Ok(())
    }

    pub(super) async fn social_rotate_profile_authority(
        &self,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        self.require_permission(actor, "organ:update").await?;
        self.social_require_local_write().await?;
        let root = self
            .social_checked_root_signer()
            .await?
            .ok_or_else(|| invalid("Change public editing authority on the owner device"))?;
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let secret = new_social_signer(&organ.uid)?;
        let members = self.social_editor_members().await?;
        let factsigner = self.signer.lock().await.clone();
        let mut tx = self.social_write_tx().await?;
        self.social_require_local_write_on(&mut tx).await?;
        let revoked: bool = store::sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM identity_revocation WHERE organ_uid=? AND revoked_key=?)",
        )
        .bind(&organ.uid)
        .bind(root.public_key_b64())
        .fetch_one(&mut *tx)
        .await?;
        if revoked || members != Self::social_editor_members_on(&mut tx, &organ.uid).await? {
            return Err(invalid(
                "The public-profile owner key was revoked during renewal",
            ));
        }
        let private: Option<String> = store::sqlx::query_scalar(
            "SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?",
        )
        .bind(&organ.uid)
        .bind(PRIVATE_NAMESPACE)
        .fetch_optional(&mut *tx)
        .await?;
        let mut private: Value = private
            .map(|value| serde_json::from_str(&value))
            .transpose()?
            .unwrap_or_else(|| json!({}));
        let generation = private["profile_signer"]["authority"]["generation"]
            .as_str()
            .unwrap_or("0")
            .parse::<i64>()
            .map_err(|_| invalid("Invalid saved public editing authority"))?
            .checked_add(1)
            .ok_or_else(|| invalid("Public editing authority exhausted"))?;
        let changes = store::sqlx::query_as::<_,(String,String,String,String)>("SELECT old_key,new_key,created_at,signature FROM identity_succession WHERE organ_uid=? ORDER BY created_at DESC LIMIT 8").bind(&organ.uid).fetch_all(&mut *tx).await?;
        let mut authority = Delegation {
            organ: organ.uid.clone(),
            root_key: root.public_key_b64(),
            editor_key: secret.public_key_b64(),
            generation: generation.to_string(),
            issued_at: now.timestamp(),
            successions: changes
                .into_iter()
                .rev()
                .map(|(old_key, new_key, created_at, signature)| RootSuccession {
                    old_key,
                    new_key,
                    created_at,
                    signature,
                })
                .collect(),
            expires_at: now.timestamp() + MAX_LIFETIME,
            signature: String::new(),
        };
        authority.signature = root.sign_bytes(&signing_bytes("profile-authority", &authority)?);
        validate_delegation(&authority, now.timestamp())?;
        let profile: Option<String> = store::sqlx::query_scalar(
            "SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?",
        )
        .bind(&organ.uid)
        .bind(PROFILE_NAMESPACE)
        .fetch_optional(&mut *tx)
        .await?;
        let mut profile: Value = profile
            .map(|value| serde_json::from_str(&value))
            .transpose()?
            .unwrap_or_else(|| json!({}));
        let mut destinations: Vec<String> = profile["destinations"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        let post_hosts: Vec<String> = store::sqlx::query_scalar("SELECT DISTINCT j.value FROM record_extension e JOIN record r ON r.uid=e.record_uid JOIN json_each(e.fds,'$.published.destinations') j WHERE e.namespace=? AND r.organ_uid=? AND json_extract(e.fds,'$.published.mode')='identified'").bind(PUBLICATION_NAMESPACE).bind(&organ.uid).fetch_all(&mut *tx).await?;
        destinations.extend(post_hosts);
        destinations.sort();
        destinations.dedup();
        if destinations.len() > 64 {
            return Err(invalid(
                "Resolve the selected identity hosts before changing editing authority",
            ));
        }
        let mut controls = Vec::new();
        for hosts in destinations.chunks(8) {
            let control = AuthorityPublication {
                authority: authority.clone(),
                destinations: hosts.to_vec(),
            };
            let hash = document_hash("authority", &control)?;
            store::social::enqueue_on(
                &mut tx,
                "authority",
                &hash,
                &serde_json::to_string(&control)?,
                &control.destinations,
                authority.expires_at,
            )
            .await?;
            controls.push(control);
        }
        private["profile_signer"] = json!({"secret":B64.encode(secret.secret_bytes()),"authority":authority,"authorized_cells":members});
        private["editor_rotation_required"] = json!(false);
        profile["authority_controls"] = json!(controls);
        store::records::set_extension_on(&mut tx, &organ.uid, PRIVATE_NAMESPACE, &private).await?;
        store::records::set_extension_on(&mut tx, &organ.uid, PROFILE_NAMESPACE, &profile).await?;
        store::social::anchor_profile_authority_on(&mut tx, &authority, false).await?;
        let fact = crate::append::append_one_in_transaction(
            &mut tx,
            nucleus::NewFact {
                uid: None,
                record_uid: organ.uid,
                delta: store::exact::zero(),
                at: Some(now),
                actor_uid: actor.map(str::to_owned),
                cause: nucleus::Cause::user_edit(),
                payload: Some(
                    json!({"social":"public-authority-changed","generation":generation})
                        .to_string(),
                ),
            },
            now,
            factsigner.as_ref(),
        )
        .await?;
        tx.commit().await?;
        self.notify_query_changed();
        Ok(ActionOutcome {
            facts: fact.into_iter().collect(),
            created: None,
            warnings: Vec::new(),
            data: Some(
                json!({"generation":generation.to_string(),"status":"New public editing authority saved and queued to existing hosts. Save your profile with this authority; withdraw and replace identified posts that used the old key. Offline hosts learn this revocation when they receive it; old authority expires within seven days"}),
            ),
        })
    }

    pub(super) async fn social_profile_signer(&self) -> Result<(Signer, Delegation), EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let held = store::records::get_extension(&self.store.pool, &organ.uid, PRIVATE_NAMESPACE)
            .await?
            .unwrap_or_else(|| json!({}));
        let saved_signer = &held["profile_signer"];
        let mut existing_signer = None;
        if let (Some(secret), Some(authority)) = (
            saved_signer["secret"].as_str(),
            saved_signer.get("authority"),
        ) {
            let authority: Delegation = serde_json::from_value(authority.clone())?;
            let bytes: [u8; 32] = B64
                .decode(secret)
                .map_err(|_| invalid("Invalid profile signing authority"))?
                .try_into()
                .map_err(|_| invalid("Invalid profile signing authority"))?;
            let signer = Signer::from_bytes(&organ.uid, "social-profile", bytes);
            if signer.public_key_b64() != authority.editor_key || authority.organ != organ.uid {
                return Err(invalid("Profile authority identity mismatch"));
            }
            let root = self.social_checked_root_signer().await?;
            if validate_delegation(&authority, nucleus::execution::now().timestamp()).is_ok()
                && root
                    .as_ref()
                    .is_none_or(|root| root.public_key_b64() == authority.root_key)
                && (authority.expires_at > nucleus::execution::now().timestamp() + 86400
                    || root.is_none())
            {
                return Ok((signer, authority));
            }
            existing_signer = Some(signer);
        }
        let root = self.social_checked_root_signer().await?.ok_or_else(|| {
            invalid("The owner device must authorize public-profile publication first")
        })?;
        let signer = match existing_signer {
            Some(signer) => signer,
            None => new_social_signer(&organ.uid)?,
        };
        let held_generation = saved_signer["authority"]["generation"]
            .as_str()
            .unwrap_or("1")
            .parse::<i64>()
            .map_err(|_| invalid("Invalid saved profile authority generation"))?;
        let generation = if saved_signer["authority"]["root_key"]
            .as_str()
            .is_some_and(|key| key != root.public_key_b64())
        {
            held_generation
                .checked_add(1)
                .ok_or_else(|| invalid("Profile authority exhausted"))?
        } else {
            held_generation
        };
        let mut authority = Delegation {
            organ: organ.uid.clone(),
            root_key: root.public_key_b64(),
            editor_key: signer.public_key_b64(),
            generation: generation.to_string(),
            issued_at: nucleus::execution::now().timestamp(),
            successions: self
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
                .collect(),
            expires_at: nucleus::execution::now().timestamp() + MAX_LIFETIME,
            signature: String::new(),
        };
        authority.signature = root.sign_bytes(&signing_bytes("profile-authority", &authority)?);
        let mut state = held.clone();
        let members = self.social_editor_members().await?;
        state["profile_signer"] = json!({"secret":B64.encode(signer.secret_bytes()),"authority":authority,"authorized_cells":members});
        let mut tx = self.social_write_tx().await?;
        self.social_require_local_write_on(&mut tx).await?;
        if owner::extension_on(&mut tx, &organ.uid, PRIVATE_NAMESPACE).await? != held
            || Self::social_editor_members_on(&mut tx, &organ.uid).await? != members
        {
            return Err(invalid(
                "Profile authority changed while preparing its signing keys; refresh before retrying",
            ));
        }
        store::records::set_extension_on(&mut tx, &organ.uid, PRIVATE_NAMESPACE, &state).await?;
        tx.commit().await?;
        Ok((signer, authority))
    }

    pub(super) fn social_save_profile<'a>(
        &'a self,
        fields: ProfileFields,
        mut parents: Vec<String>,
        destinations: Vec<String>,
        status: PostState,
        actor: Option<&'a str>,
        now: DateTime<Utc>,
        expected: Option<&'a Value>,
        consume: Option<&'a str>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<ActionOutcome, EngineError>> + Send + 'a>,
    > {
        Box::pin(async move {
            self.require_permission(actor, "organ:update").await?;
            validate_fields(&fields)?;
            if destinations.len() > 8 || parents.len() > 8 {
                return Err(invalid("Use at most eight profile hosts/heads"));
            }
            for destination in &destinations {
                destination
                    .parse::<iroh::EndpointId>()
                    .map_err(|_| invalid("Use a valid profile host endpoint ID"))?;
            }
            parents.sort();
            parents.dedup();
            let organ = store::organs::local(&self.store.pool)
                .await?
                .ok_or_else(|| invalid("No local Organ"))?;
            let mut state =
                store::records::get_extension(&self.store.pool, &organ.uid, PROFILE_NAMESPACE)
                    .await?
                    .unwrap_or_else(|| json!({}));
            let cell = store::cells::local(&self.store.pool)
                .await?
                .ok_or_else(|| invalid("No local Cell"))?;
            let local_pending = format!("pending_profile_{}", cell.uid);
            let pending_to_replace = state.get(&local_pending).cloned();
            let draft_images = match (expected, consume) {
                (Some(expected), Some(key)) => {
                    profile_draft::pending_images(expected, key, &fields).await?
                }
                (None, Some(_)) => {
                    return Err(invalid("Consume only an exact retained profile draft"));
                }
                _ => Vec::new(),
            };
            for hash in [&fields.avatar, &fields.banner].into_iter().flatten() {
                let prepared: bool = store::sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM social_public_asset WHERE hash=?)",
                )
                .bind(hash)
                .fetch_one(&self.store.pool)
                .await?;
                let already_selected = [
                    &state["published"]["fields"]["avatar"],
                    &state["published"]["fields"]["banner"],
                ]
                .iter()
                .any(|value| value.as_str() == Some(hash));
                let old_hosts: Vec<_> = state["published"]["destinations"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                if !prepared
                    && !draft_images.iter().any(|image| image.hash == *hash)
                    && (!already_selected
                        || destinations
                            .iter()
                            .any(|host| !old_hosts.contains(&host.as_str())))
                {
                    return Err(invalid(
                        "Prepare or load the selected public image on this device before choosing it or adding another host",
                    ));
                }
            }
            if state["published"]["state"] == "active" {
                let old_hosts = state["published"]["destinations"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str);
                if status == PostState::Active
                    && old_hosts
                        .clone()
                        .any(|host| !destinations.iter().any(|id| id == host))
                {
                    return Err(invalid(
                        "Withdraw your profile from its existing hosts before removing a host",
                    ));
                }
            }
            if self.social_checked_root_signer().await?.is_none() {
                let private =
                    store::records::get_extension(&self.store.pool, &organ.uid, PRIVATE_NAMESPACE)
                        .await?
                        .unwrap_or_else(|| json!({}));
                let ready = serde_json::from_value::<Delegation>(
                    private["profile_signer"]["authority"].clone(),
                )
                .ok()
                .is_some_and(|authority| validate_delegation(&authority, now.timestamp()).is_ok());
                if !ready {
                    return self
                        .social_save_pending_profile(
                            fields,
                            parents,
                            destinations,
                            status,
                            actor,
                            now,
                        )
                        .await;
                }
            }
            let (signer, authority) = self.social_profile_signer().await?;
            let mut revision = 0;
            for parent in &parents {
                let doc: Profile = serde_json::from_value(
                    state
                        .get(format!("revision_{parent}"))
                        .ok_or_else(|| {
                            invalid("Refresh the profile before resolving its revisions")
                        })?
                        .clone(),
                )?;
                validate_profile(&doc, doc.issued_at)?;
                if doc.authority.organ != organ.uid || document_hash("profile", &doc)? != *parent {
                    return Err(invalid("Invalid retained profile parent"));
                }
                if doc.authority.generation == authority.generation {
                    revision = revision.max(
                        doc.revision
                            .parse::<i64>()
                            .map_err(|_| invalid("Invalid stored profile revision"))?,
                    );
                }
            }
            for (key, value) in state.as_object().into_iter().flatten() {
                if key.starts_with("revision_")
                    && value["authority"]["generation"].as_str()
                        == Some(authority.generation.as_str())
                {
                    revision = revision.max(
                        value["revision"]
                            .as_str()
                            .and_then(|value| value.parse::<i64>().ok())
                            .unwrap_or(0),
                    );
                }
            }
            if parents.is_empty() && state.get("published").is_some() {
                return Err(invalid(
                    "Refresh the current profile and select its parent revisions",
                ));
            }
            let mut pending = parents.clone();
            let mut ancestors = std::collections::HashSet::new();
            while let Some(hash) = pending.pop() {
                if !ancestors.insert(hash.clone()) || ancestors.len() > 256 {
                    continue;
                }
                if let Some(parent) = state.get(format!("revision_{hash}")) {
                    pending.extend(
                        parent["parents"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .map(str::to_owned),
                    );
                }
            }
            let direct: std::collections::HashSet<_> = parents.iter().cloned().collect();
            let mut ancestors: Vec<_> = ancestors
                .into_iter()
                .filter(|hash| !direct.contains(hash))
                .collect();
            ancestors.sort_by_key(|hash| {
                (
                    std::cmp::Reverse(
                        state[format!("revision_{hash}")]["revision"]
                            .as_str()
                            .and_then(|value| value.parse::<i64>().ok())
                            .unwrap_or(0),
                    ),
                    hash.clone(),
                )
            });
            parents.extend(ancestors.into_iter().take(64 - parents.len()));
            let issued_at = now.timestamp().max(authority.issued_at);
            let mut doc = Profile {
                protocol: "lince.profile.1".into(),
                authority,
                revision: revision
                    .checked_add(1)
                    .ok_or_else(|| invalid("Profile revision exhausted"))?
                    .to_string(),
                parents,
                issued_at,
                expires_at: issued_at + MAX_LIFETIME,
                fields,
                state: status,
                destinations: destinations.clone(),
                signature: String::new(),
            };
            doc.expires_at = doc.expires_at.min(doc.authority.expires_at);
            doc.signature = signer.sign_bytes(&signing_bytes("profile", &doc)?);
            validate_profile(&doc, now.timestamp())?;
            let hash = document_hash("profile", &doc)?;
            state[format!("revision_{hash}")] = serde_json::to_value(&doc)?;
            state["published"] = serde_json::to_value(&doc)?;
            state["destinations"] = json!(destinations);
            let factsigner = self.signer.lock().await.clone();
            let local_key = self.operational_key_for(&organ.uid).await?.public_key_b64();
            let mut tx = self.social_write_tx().await?;
            profile_draft::require_members_on(
                &mut tx, &organ.uid, &cell.uid, &local_key, None, actor,
            )
            .await?;
            if let Some(current) = store::sqlx::query_scalar::<_, String>(
                "SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?",
            )
            .bind(&organ.uid)
            .bind(PROFILE_NAMESPACE)
            .fetch_optional(&mut *tx)
            .await?
            {
                let mut current: Value = serde_json::from_str(&current)?;
                if expected.is_some_and(|expected| expected != &current) {
                    return Err(invalid(
                        "The profile changed during renewal; refresh before retrying",
                    ));
                }
                if let Some(key) = consume {
                    profile_draft::authorize_consumption_on(
                        &mut tx, &current, key, &organ.uid, &cell.uid, &local_key,
                    )
                    .await?;
                }
                current[format!("revision_{hash}")] = serde_json::to_value(&doc)?;
                current["published"] = serde_json::to_value(&doc)?;
                current["destinations"] = json!(destinations);
                state = current;
            } else if consume.is_some() {
                return Err(invalid(
                    "The pending profile state was removed before publication",
                ));
            }
            if let Some(key) = consume {
                state
                    .as_object_mut()
                    .ok_or_else(|| invalid("Invalid profile draft state"))?
                    .remove(key);
            }
            if expected.is_none() && state.get(&local_pending) == pending_to_replace.as_ref() {
                state
                    .as_object_mut()
                    .ok_or_else(|| invalid("Invalid profile draft state"))?
                    .remove(&local_pending);
            }
            trim_profile_state(&mut state)?;
            for image in draft_images {
                Self::social_store_asset(
                    &mut tx,
                    &image.hash,
                    &image.bytes,
                    image.dimensions,
                    64 * 1024 * 1024,
                )
                .await?;
            }
            store::records::set_extension_on(&mut tx, &organ.uid, PROFILE_NAMESPACE, &state)
                .await?;
            let body = serde_json::to_string(&doc)?;
            store::social::put_profile_on(&mut tx, &doc, &hash, "own profile").await?;
            store::social::enqueue_on(
                &mut tx,
                "profile",
                &hash,
                &body,
                &destinations,
                doc.expires_at,
            )
            .await?;
            Self::social_enqueue_images(&mut tx, &doc).await?;
            let fact = crate::append::append_one_in_transaction(
                &mut tx,
                nucleus::NewFact {
                    uid: None,
                    record_uid: organ.uid,
                    delta: store::exact::zero(),
                    at: Some(now),
                    actor_uid: actor.map(str::to_owned),
                    cause: nucleus::Cause::user_edit(),
                    payload: Some(
                        json!({"social":"profile-edited","revision":doc.revision}).to_string(),
                    ),
                },
                now,
                factsigner.as_ref(),
            )
            .await?;
            tx.commit().await?;
            self.notify_query_changed();
            Ok(ActionOutcome {
                facts: fact.into_iter().collect(),
                created: None,
                warnings: Vec::new(),
                data: Some(
                    json!({"profile":doc,"hash":hash,"status":if destinations.is_empty() { "Public profile saved on your devices; no hosts selected" } else { "Public profile saved and queued for selected hosts" }}),
                ),
            })
        })
    }
}
