use super::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingProfile {
    organ: String,
    cell: String,
    device_key: String,
    signature: String,
    fields: ProfileFields,
    images: Vec<profile_media::DraftImage>,
    parents: Vec<String>,
    destinations: Vec<String>,
    state: PostState,
    saved_at: i64,
    error: Option<String>,
    #[serde(default)]
    retry_at: i64,
}

fn payload(draft: &PendingProfile) -> Value {
    json!({"organ":draft.organ,"cell":draft.cell,"device_key":draft.device_key,
        "fields":draft.fields,"images":draft.images,"parents":draft.parents,"destinations":draft.destinations,
        "state":draft.state,"saved_at":draft.saved_at})
}

fn checked_draft(key: &str, state: &Value) -> Result<PendingProfile, EngineError> {
    if serde_json::to_vec(state)?.len() > store::records::MAX_EXTENSION_BYTES {
        return Err(invalid(
            "Resolve the oversized retained profile draft state",
        ));
    }
    let draft: PendingProfile = serde_json::from_value(
        state
            .get(key)
            .ok_or_else(|| invalid("The pending profile draft changed"))?
            .clone(),
    )?;
    if key != format!("pending_profile_{}", draft.cell)
        || draft.saved_at <= 0
        || draft.saved_at > nucleus::execution::now().timestamp() + 300
        || !crate::roster::verify_with(
            &draft.device_key,
            &signing_bytes("private-profile-draft", &payload(&draft))?,
            &draft.signature,
        )
    {
        return Err(invalid(
            "The saved profile draft has an invalid device signature",
        ));
    }
    Ok(draft)
}

pub(super) async fn pending_images(
    state: &Value,
    key: &str,
    fields: &ProfileFields,
) -> Result<Vec<profile_media::PreparedImage>, EngineError> {
    let draft = checked_draft(key, state)?;
    if &draft.fields != fields {
        return Err(invalid(
            "The selected profile fields differ from the signed draft",
        ));
    }
    profile_media::validate_images(fields, &draft.images).await
}

pub(super) async fn require_members_on(
    tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
    organ: &str,
    local_cell: &str,
    local_key: &str,
    origin: Option<(&str, &str)>,
    actor: Option<&str>,
) -> Result<(), EngineError> {
    if let Some(actor) = actor {
        let principal = store::auth::principal_on(&mut **tx, actor).await?;
        if principal.is_none_or(|principal| !principal.permits("organ:update")) {
            return Err(EngineError::Forbidden(
                "Current profile editing permission is required".into(),
            ));
        }
    }
    let roster: Option<(String, String)> =
        store::sqlx::query_as("SELECT payload,signature FROM organ_roster WHERE organ_uid=?")
            .bind(organ)
            .fetch_optional(&mut **tx)
            .await?;
    if let Some((payload, signature)) = roster {
        let signed = crate::roster::SignedRoster {
            roster: serde_json::from_str(&payload)?,
            signature,
        };
        let allowed = |cell: &str, key: &str| {
            signed.roster.cells.iter().any(|entry| {
                entry.cell_uid == cell
                    && entry.operational_key == key
                    && entry.may(crate::roster::CAP_WRITE)
            })
        };
        if signed.roster.organ_uid != organ
            || signed.roster.cells.len() > 64
            || !crate::roster::roster_signature_is_valid(&signed)
            || !allowed(local_cell, local_key)
            || origin.is_some_and(|(cell, key)| !allowed(cell, key))
        {
            return Err(invalid(
                "This draft's device no longer has valid write authority. Review its fields on a current authorized device",
            ));
        }
    } else if origin.is_some_and(|(cell, key)| cell != local_cell || key != local_key) {
        return Err(invalid(
            "A profile draft needs current enrolled device authority",
        ));
    }
    Ok(())
}

pub(super) async fn authorize_consumption_on(
    tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
    state: &Value,
    key: &str,
    organ: &str,
    local_cell: &str,
    local_key: &str,
) -> Result<(), EngineError> {
    let draft = checked_draft(key, state)?;
    if draft.organ != organ {
        return Err(invalid("The pending profile belongs to another Organ"));
    }
    require_members_on(
        tx,
        organ,
        local_cell,
        local_key,
        Some((&draft.cell, &draft.device_key)),
        None,
    )
    .await
}

impl Engine {
    pub(super) async fn social_save_pending_profile(
        &self,
        fields: ProfileFields,
        parents: Vec<String>,
        destinations: Vec<String>,
        status: PostState,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        self.social_require_local_write().await?;
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        let key = format!("pending_profile_{}", cell.uid);
        let operational = self.operational_key_for(&organ.uid).await?;
        let images = self.social_profile_draft_images(&fields).await?;
        let mut draft = PendingProfile {
            organ: organ.uid.clone(),
            cell: cell.uid.clone(),
            device_key: operational.public_key_b64(),
            signature: String::new(),
            fields,
            images,
            parents,
            destinations,
            state: status,
            saved_at: now.timestamp(),
            error: None,
            retry_at: 0,
        };
        draft.signature =
            operational.sign_bytes(&signing_bytes("private-profile-draft", &payload(&draft))?);
        let factsigner = self.signer.lock().await.clone();
        let mut tx = self.social_write_tx().await?;
        require_members_on(
            &mut tx,
            &organ.uid,
            &cell.uid,
            &draft.device_key,
            None,
            actor,
        )
        .await?;
        let mut state = owner::extension_on(&mut tx, &organ.uid, PROFILE_NAMESPACE).await?;
        if state.get(&key).is_none()
            && state
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(key, _)| key.starts_with("pending_profile_"))
                .count()
                >= 32
        {
            return Err(invalid(
                "Resolve saved profile drafts before adding more than 32",
            ));
        }
        for parent in &draft.parents {
            let doc: Profile = serde_json::from_value(
                state
                    .get(format!("revision_{parent}"))
                    .ok_or_else(|| {
                        invalid("Refresh the profile before selecting parent revisions")
                    })?
                    .clone(),
            )?;
            profile::validate_profile(&doc, doc.issued_at)?;
            if doc.authority.organ != organ.uid || document_hash("profile", &doc)? != *parent {
                return Err(invalid("Invalid retained profile draft parent"));
            }
        }
        state[&key] = json!(draft);
        if serde_json::to_vec(&state)?.len() > store::records::MAX_EXTENSION_BYTES {
            return Err(invalid(
                "Resolve saved profile branches before adding more drafts",
            ));
        }
        store::records::set_extension_on(&mut tx, &organ.uid, PROFILE_NAMESPACE, &state).await?;
        let fact = crate::append::append_one_in_transaction(
            &mut tx,
            nucleus::NewFact {
                uid: None,
                record_uid: organ.uid,
                delta: store::exact::zero(),
                at: Some(now),
                actor_uid: actor.map(str::to_owned),
                cause: nucleus::Cause::user_edit(),
                payload: Some(json!({"social":"profile-draft-saved"}).to_string()),
            },
            now,
            factsigner.as_ref(),
        )
        .await?;
        tx.commit().await?;
        self.notify_query_changed();
        let mut view = json!(draft);
        profile_media::omit_encoded_images(&mut view);
        Ok(ActionOutcome {
            facts: fact.into_iter().collect(),
            created: None,
            warnings: Vec::new(),
            data: Some(
                json!({"profile_draft":view,"status":"Profile changes and selected prepared images are saved through Own sync, waiting for owner authorization before publication"}),
            ),
        })
    }

    pub(super) async fn social_publish_profile_drafts(&self) -> Result<usize, EngineError> {
        let is_owner = self.social_checked_root_signer().await?.is_some();
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let Some(state) =
            store::records::get_extension(&self.store.pool, &organ.uid, PROFILE_NAMESPACE).await?
        else {
            return Ok(0);
        };
        if !is_owner {
            let private =
                store::records::get_extension(&self.store.pool, &organ.uid, PRIVATE_NAMESPACE)
                    .await?
                    .unwrap_or_else(|| json!({}));
            if serde_json::from_value::<Delegation>(private["profile_signer"]["authority"].clone())
                .ok()
                .is_none_or(|authority| {
                    profile::validate_delegation(&authority, nucleus::execution::now().timestamp())
                        .is_err()
                })
            {
                return Ok(0);
            }
        }
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Cell"))?;
        let local_key = format!("pending_profile_{}", cell.uid);
        if is_owner
            && state
                .as_object()
                .into_iter()
                .flatten()
                .any(|(key, _)| key.starts_with("pending_profile_"))
        {
            self.social_profile_signer().await?;
        }
        let keys: Vec<_> = state
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(key, _)| key.starts_with("pending_profile_"))
            .map(|(key, _)| key.clone())
            .collect();
        if keys.len() > 32 {
            return Err(invalid("Resolve the oversized pending profile draft set"));
        }
        let mut signed = 0;
        for key in keys {
            if !is_owner && key != local_key {
                continue;
            }
            let state =
                store::records::get_extension(&self.store.pool, &organ.uid, PROFILE_NAMESPACE)
                    .await?
                    .unwrap_or_else(|| json!({}));
            let Some(saved) = state.get(&key) else {
                continue;
            };
            let draft: PendingProfile = serde_json::from_value(saved.clone())?;
            if draft.retry_at > nucleus::execution::now().timestamp() {
                continue;
            }
            let allowed = if let Some(roster) = self.roster_of(&organ.uid).await? {
                roster.roster.cells.iter().any(|member| {
                    member.cell_uid == draft.cell
                        && member.operational_key == draft.device_key
                        && member.may(crate::roster::CAP_WRITE)
                })
            } else {
                draft.cell == cell.uid
            };
            let valid = allowed
                && draft.organ == organ.uid
                && key == format!("pending_profile_{}", draft.cell)
                && crate::roster::verify_with(
                    &draft.device_key,
                    &signing_bytes("private-profile-draft", &payload(&draft))?,
                    &draft.signature,
                );
            let result = if valid {
                self.social_save_profile(
                    draft.fields,
                    draft.parents,
                    draft.destinations,
                    draft.state,
                    None,
                    nucleus::execution::now(),
                    Some(&state),
                    Some(&key),
                )
                .await
            } else {
                Err(invalid(
                    "This draft's device no longer has valid write authority. Review its fields on a current authorized device",
                ))
            };
            match result {
                Ok(_) => signed += 1,
                Err(error) => {
                    let mut tx = self.social_write_tx().await?;
                    let mut current =
                        owner::extension_on(&mut tx, &organ.uid, PROFILE_NAMESPACE).await?;
                    if current.get(&key) == Some(saved) {
                        current[&key]["error"] = json!(format!(
                            "Owner could not sign this draft: {error}. Review the current profile and save again."
                        ));
                        current[&key]["retry_at"] =
                            json!(nucleus::execution::now().timestamp() + 60);
                        store::records::set_extension_on(
                            &mut tx,
                            &organ.uid,
                            PROFILE_NAMESPACE,
                            &current,
                        )
                        .await?;
                    }
                    tx.commit().await?;
                }
            }
        }
        Ok(signed)
    }
}
