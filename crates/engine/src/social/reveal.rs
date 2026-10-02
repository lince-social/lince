use super::profile::validate_profile;
use super::*;
use nucleus::social::requests::*;
use store::sqlx::{Sqlite, Transaction};

pub fn binding_bytes(
    token: &str,
    author: &str,
    recipient: &str,
    profile: &Profile,
) -> Result<Vec<u8>, EngineError> {
    signing_bytes(
        "private-profile-binding",
        &json!({"conversation":token,"author":author,"recipient":recipient,"profile":document_hash("profile",profile)?}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn expired_or_revoked_retained_reveal_needs_fresh_proof_without_erasing_consent() {
        let engine = Engine::open_memory().await.unwrap();
        let peer = Engine::open_memory().await.unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("root.key");
        std::fs::write(&path, [154; 32]).unwrap();
        peer.set_root_key_path(path);
        let saved = peer
            .social_command(
                Command::SaveProfile {
                    fields: ProfileFields {
                        name: "Friend".into(),
                        ..Default::default()
                    },
                    parents: vec![],
                    destinations: vec![],
                },
                None,
                nucleus::execution::now(),
            )
            .await
            .unwrap()
            .data
            .unwrap();
        let doc: Profile = serde_json::from_value(saved["profile"].clone()).unwrap();
        let p = ConversationParticipant {
            token: nucleus::new_uid("talk"),
            context: nucleus::new_uid("r"),
            local_owner: Signer::from_bytes("", "social", [155; 32]).public_key_b64(),
            peer_owner: Signer::from_bytes("", "social", [156; 32]).public_key_b64(),
            alias: String::new(),
            routes: vec![],
            state: ConversationState::Accepted,
            incoming: true,
            started_at: doc.issued_at,
            local_accepted: true,
            peer_accepted: true,
            provisional_sent: 0,
        };
        let (signer, _) = peer.social_profile_signer().await.unwrap();
        let signature = signer
            .sign_bytes(&binding_bytes(&p.token, &p.peer_owner, &p.local_owner, &doc).unwrap());
        let state = json!({"peer":{"profile":doc,"binding_signature":signature},"local_connect":true,"peer_connect":true,"connected":true});
        let now = nucleus::execution::now().timestamp();
        let fresh = engine.social_reveal_status(&p, &state, now).await.unwrap();
        assert_eq!(fresh["peer_current"], true);
        assert_eq!(fresh["contact_present"], false);
        let expired = engine
            .social_reveal_status(&p, &state, doc.expires_at + 1)
            .await
            .unwrap();
        assert_eq!(expired["peer_current"], false);
        assert_eq!(expired["needs_fresh_proof"], true);
        assert!(
            expired["recovery"]
                .as_str()
                .unwrap()
                .contains("Consent and history remain")
        );
        let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
        contact_on(&mut tx, &doc, now).await.unwrap();
        store::social::anchor_profile_authority_on(&mut tx, &doc.authority, false)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        store::sqlx::query(
            "UPDATE social_profile_authority SET generation=generation+1 WHERE organ=?",
        )
        .bind(&doc.authority.organ)
        .execute(&engine.store.pool)
        .await
        .unwrap();
        let revoked = engine.social_reveal_status(&p, &state, now).await.unwrap();
        assert_eq!(revoked["peer_current"], false);
        assert_eq!(revoked["contact_present"], true);
        assert_eq!(revoked["needs_fresh_proof"], true);
        let contacts = store::organs::contacts(&engine.store.pool).await.unwrap();
        assert_eq!(contacts.len(), 1);
        assert!(!contacts[0].sync_in && !contacts[0].sync_out);
        assert_eq!(state["connected"], true);
    }
}

pub fn validate_binding(
    token: &str,
    author: &str,
    recipient: &str,
    profile: &Profile,
    signature: &str,
    now: i64,
) -> Result<(), EngineError> {
    validate_profile(profile, now)?;
    request_auth::ed_key(author)?;
    request_auth::ed_key(recipient)?;
    if !nucleus::valid_uid(token, "talk")
        || profile.state != PostState::Active
        || !crate::roster::verify_with(
            &profile.authority.editor_key,
            &binding_bytes(token, author, recipient, profile)?,
            signature,
        )
    {
        return Err(invalid(
            "This profile is not bound to this private identity and conversation",
        ));
    }
    Ok(())
}

pub(super) async fn control_on(
    tx: &mut Transaction<'_, Sqlite>,
    root: &str,
    p: &ConversationParticipant,
    content: &PrivateContent,
    outgoing: bool,
    now: i64,
) -> Result<(), EngineError> {
    if !matches!(
        content.kind,
        ContentKind::Reveal { .. } | ContentKind::ContactRequest | ContentKind::ContactAccept
    ) {
        return Ok(());
    }
    if p.state != ConversationState::Accepted {
        return Err(invalid(
            "Accept this conversation before revealing a profile or connecting",
        ));
    }
    let mut state = owner::extension_on(tx, root, REVEAL_NAMESPACE).await?;
    if !state.is_object() {
        state = json!({});
    }
    match &content.kind {
        ContentKind::Reveal {
            profile,
            binding_signature,
        } => {
            let recipient = if outgoing {
                &p.peer_owner
            } else {
                &p.local_owner
            };
            validate_binding(
                &p.token,
                &content.author_owner,
                recipient,
                profile,
                binding_signature,
                now,
            )?;
            store::social::anchor_profile_authority_on(tx, &profile.authority, false).await?;
            let field = if outgoing { "local" } else { "peer" };
            if let Some(prior) = state[field].get("profile") {
                let old: Profile = serde_json::from_value(prior.clone())?;
                if old.authority.organ != profile.authority.organ {
                    return Err(invalid(
                        "A revealed Organ identity cannot be silently replaced",
                    ));
                }
                let older = profile.authority.generation.parse::<i64>().unwrap_or(0)
                    < old.authority.generation.parse::<i64>().unwrap_or(0)
                    || profile.authority.generation == old.authority.generation
                        && profile.revision.parse::<i64>().unwrap_or(0)
                            < old.revision.parse::<i64>().unwrap_or(0);
                if older {
                    return Err(invalid(
                        "An older revealed profile cannot replace the retained profile",
                    ));
                }
                if old.authority.generation == profile.authority.generation
                    && old.revision == profile.revision
                    && old != **profile
                {
                    return Err(invalid(
                        "A conflicting revealed profile needs a resolved newer revision",
                    ));
                }
            }
            state[field] =
                json!({"profile":profile,"binding_signature":binding_signature,"verified_at":now});
        }
        ContentKind::ContactRequest | ContentKind::ContactAccept => {
            if state["local"].get("profile").is_none() || state["peer"].get("profile").is_none() {
                return Err(invalid(
                    "Both people must reveal their profiles before connecting",
                ));
            }
            for field in ["local", "peer"] {
                let profile: Profile = serde_json::from_value(state[field]["profile"].clone())?;
                validate_profile(&profile, now)?;
                store::social::anchor_profile_authority_on(tx, &profile.authority, false).await?;
            }
            if !outgoing
                && matches!(content.kind, ContentKind::ContactAccept)
                && state["local_connect"] != true
            {
                return Err(invalid(
                    "Contact acceptance requires your own earlier consent",
                ));
            }
            state[if outgoing {
                "local_connect"
            } else {
                "peer_connect"
            }] = json!(true);
        }
        _ => {}
    }
    state["connected"] = json!(state["local_connect"] == true && state["peer_connect"] == true);
    if state["connected"] == true {
        let profile: Profile = serde_json::from_value(state["peer"]["profile"].clone())?;
        contact_on(tx, &profile, now).await?;
    }
    if serde_json::to_vec(&state)?.len() > 64 * 1024 {
        return Err(invalid("The retained private reveal exceeds its bound"));
    }
    store::records::set_extension_on(tx, root, REVEAL_NAMESPACE, &state).await?;
    Ok(())
}

async fn contact_on(
    tx: &mut Transaction<'_, Sqlite>,
    profile: &Profile,
    now: i64,
) -> Result<(), EngineError> {
    store::social::anchor_profile_authority_on(tx, &profile.authority, false).await?;
    let uid = &profile.authority.organ;
    let trust: Option<String> =
        store::sqlx::query_scalar("SELECT trust FROM organ_contact WHERE record_uid=?")
            .bind(uid)
            .fetch_optional(&mut **tx)
            .await?;
    if trust.as_deref() == Some("blocked") {
        return Err(invalid(
            "This revealed Organ is already blocked in your contacts",
        ));
    }
    let kind: Option<String> = store::sqlx::query_scalar("SELECT kind FROM record WHERE uid=?")
        .bind(uid)
        .fetch_optional(&mut **tx)
        .await?;
    if kind.as_ref().is_some_and(|k| k != "organ") {
        return Err(invalid(
            "The revealed Organ identifier belongs to another Record kind",
        ));
    }
    if kind.is_none() {
        let at = DateTime::from_timestamp(now, 0)
            .ok_or_else(|| invalid("Invalid contact time"))?
            .to_rfc3339();
        store::sqlx::query("INSERT INTO record(uid,kind,head,body,quantity_mantissa,quantity_scale,organ_uid,created_at,updated_at,replica_root) VALUES(?,'organ',?,'','1',0,?,?,?,?)").bind(uid).bind(&profile.fields.name).bind(uid).bind(&at).bind(&at).bind(uid).execute(&mut **tx).await?;
    }
    store::sqlx::query("INSERT INTO organ_contact(record_uid,trust,proximity,sync_out,sync_in,scope_fields,accept_fields) VALUES(?,'known',1,0,0,'[]','[]') ON CONFLICT(record_uid) DO UPDATE SET trust='known',sync_out=0,sync_in=0,scope_fields='[]',accept_fields='[]' WHERE organ_contact.trust<>'known'").bind(uid).execute(&mut **tx).await?;
    Ok(())
}

impl Engine {
    pub(super) async fn social_reveal_status(
        &self,
        p: &ConversationParticipant,
        state: &Value,
        now: i64,
    ) -> Result<Value, EngineError> {
        let mut status = json!({"local_current":false,"peer_current":false,"contact_present":false,"needs_fresh_proof":false});
        for field in ["local", "peer"] {
            if state[field].get("profile").is_none() {
                continue;
            }
            let doc = serde_json::from_value::<Profile>(state[field]["profile"].clone());
            let (author, recipient) = if field == "local" {
                (&p.local_owner, &p.peer_owner)
            } else {
                (&p.peer_owner, &p.local_owner)
            };
            let current = if let Ok(doc) = &doc {
                let held: Option<(String, String, i64)> = store::sqlx::query_as(
                    "SELECT root_key,editor_key,generation FROM social_profile_authority WHERE organ=?",
                ).bind(&doc.authority.organ).fetch_optional(&self.store.pool).await?;
                validate_binding(
                    &p.token,
                    author,
                    recipient,
                    doc,
                    state[field]["binding_signature"]
                        .as_str()
                        .unwrap_or_default(),
                    now,
                )
                .is_ok()
                    && held.is_none_or(|(root, editor, floor)| {
                        doc.authority
                            .generation
                            .parse::<i64>()
                            .is_ok_and(|generation| {
                                generation >= floor
                                    && (generation != floor || editor == doc.authority.editor_key)
                            })
                            && (root == doc.authority.root_key
                                || doc
                                    .authority
                                    .successions
                                    .iter()
                                    .any(|edge| edge.old_key == root))
                    })
            } else {
                false
            };
            status[format!("{field}_current")] = json!(current);
            if !current {
                status["needs_fresh_proof"] = json!(true);
            }
            if field == "peer"
                && let Ok(doc) = doc
            {
                let present: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organ_contact WHERE record_uid=? AND trust='known')")
                    .bind(&doc.authority.organ).fetch_one(&self.store.pool).await?;
                status["contact_present"] = json!(present);
            }
        }
        status["can_connect"] =
            json!(status["local_current"] == true && status["peer_current"] == true);
        if status["needs_fresh_proof"] == true {
            status["recovery"] = json!(
                "A retained profile proof is expired, invalid or revoked. Consent and history remain. Reveal your current reviewed profile again, and ask this person to do the same in this private thread. Existing contact permissions remain unchanged"
            );
        } else if state["connected"] == true && status["contact_present"] != true {
            status["recovery"] = json!(
                "Mutual consent is retained; this device is waiting to reconstruct the verified contact"
            );
        }
        Ok(status)
    }

    pub async fn social_reconcile_revealed_contacts(&self) -> Result<usize, EngineError> {
        self.social_require_local_write().await?;
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?
            .uid;
        let rows:Vec<(String,String,String)>=store::sqlx::query_as("SELECT r.uid,e.fds,p.fds FROM record r JOIN record_extension e ON e.record_uid=r.uid AND e.namespace=? JOIN record_extension p ON p.record_uid=r.uid AND p.namespace=? WHERE r.organ_uid=? AND r.deleted_at IS NULL AND json_extract(e.fds,'$.connected')=1 ORDER BY r.uid LIMIT 257").bind(REVEAL_NAMESPACE).bind(PARTICIPANTS_NAMESPACE).bind(organ).fetch_all(&self.store.pool).await?;
        if rows.len() > 256 {
            return Err(invalid(
                "Archive older social contacts before retaining more than 256 mappings",
            ));
        }
        let now = nucleus::execution::now().timestamp();
        let mut count = 0;
        for (root, body, p) in rows {
            let state: Value = serde_json::from_str(&body)?;
            let p: ConversationParticipant = serde_json::from_str(&p)?;
            let doc: Profile = serde_json::from_value(state["peer"]["profile"].clone())?;
            let result = async {
                validate_binding(
                    &p.token,
                    &p.peer_owner,
                    &p.local_owner,
                    &doc,
                    state["peer"]["binding_signature"]
                        .as_str()
                        .unwrap_or_default(),
                    now,
                )?;
                if state["local_connect"] != true
                    || state["peer_connect"] != true
                    || p.state != ConversationState::Accepted
                {
                    return Err(invalid(
                        "This retained social contact lost its mutual verified binding",
                    ));
                }
                let mut tx = self.social_write_tx().await?;
                self.social_require_local_write_on(&mut tx).await?;
                let available: bool = store::sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM record WHERE uid=? AND deleted_at IS NULL)",
                )
                .bind(&root)
                .fetch_one(&mut *tx)
                .await?;
                if !available
                    || owner::extension_on(&mut tx, &root, REVEAL_NAMESPACE).await? != state
                    || owner::extension_on(&mut tx, &root, PARTICIPANTS_NAMESPACE).await?
                        != serde_json::to_value(&p)?
                    || admission::map_on(&mut tx, &p.context).await?[&p.peer_owner]["blocked"]
                        == true
                {
                    return Err(invalid(
                        "This social contact mapping changed during reconciliation",
                    ));
                }
                store::social::anchor_profile_authority_on(&mut tx, &doc.authority, false).await?;
                contact_on(&mut tx, &doc, now).await?;
                tx.commit().await?;
                Ok::<(), EngineError>(())
            }
            .await;
            match result {
                Ok(()) => count += 1,
                Err(error) => {
                    tracing::debug!(%error,"Retained social contact awaits fresh identity proof")
                }
            }
        }
        Ok(count)
    }
    pub(super) async fn social_reveal_profile(
        &self,
        root: &str,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.social_own_record(root, actor).await?;
        let p = self.social_participant(root).await?;
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?
            .uid;
        self.social_own_record(&organ, actor).await?;
        let saved = store::records::get_extension(&self.store.pool, &organ, PROFILE_NAMESPACE)
            .await?
            .ok_or_else(|| invalid("Save your shared profile before revealing it"))?;
        let editor = profile::profile_editor(&saved);
        if editor["heads"].as_array().is_some_and(|h| h.len() != 1) {
            return Err(invalid(
                "Resolve the shared profile's concurrent edits before revealing it",
            ));
        }
        let doc: Profile = serde_json::from_value(saved["published"].clone())?;
        validate_profile(&doc, nucleus::execution::now().timestamp())?;
        let (signer, authority) = self.social_profile_signer().await?;
        if authority.editor_key != doc.authority.editor_key
            || authority.generation != doc.authority.generation
            || authority.root_key != doc.authority.root_key
        {
            return Err(invalid(
                "Save the shared profile under its current authority before revealing it",
            ));
        }
        let signature = signer.sign_bytes(&binding_bytes(
            &p.token,
            &p.local_owner,
            &p.peer_owner,
            &doc,
        )?);
        let content = PrivateContent {
            protocol: "lince.private-content.1".into(),
            conversation: p.token.clone(),
            message: nucleus::new_uid("msg"),
            author_owner: p.local_owner.clone(),
            issued_at: nucleus::execution::now().timestamp(),
            kind: ContentKind::Reveal {
                profile: Box::new(doc),
                binding_signature: signature,
            },
        };
        self.social_save_content(root, p, content, None, false)
            .await
    }

    pub(super) async fn social_connect_participant(
        &self,
        root: &str,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.social_own_record(root, actor).await?;
        let p = self.social_participant(root).await?;
        let state = store::records::get_extension(&self.store.pool, root, REVEAL_NAMESPACE)
            .await?
            .unwrap_or(Value::Null);
        if state["local_connect"] == true {
            return Err(invalid("You already consented to connect"));
        }
        let content = PrivateContent {
            protocol: "lince.private-content.1".into(),
            conversation: p.token.clone(),
            message: nucleus::new_uid("msg"),
            author_owner: p.local_owner.clone(),
            issued_at: nucleus::execution::now().timestamp(),
            kind: if state["peer_connect"] == true {
                ContentKind::ContactAccept
            } else {
                ContentKind::ContactRequest
            },
        };
        self.social_save_content(root, p, content, None, false)
            .await
    }
}
