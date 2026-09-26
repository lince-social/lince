use std::collections::BTreeSet;

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::{Engine, EngineError, trust::Signer};

pub const NAMESPACE: &str = "lince.conversation.membership";
pub const ADMISSION_NAMESPACE: &str = "lince.call-admission";
const DOMAIN: &[u8] = b"lince/conversation-membership/1\0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub organ: String,
    pub name: String,
    pub node_id: String,
    pub keys: Vec<(String, String)>,
    pub accepted: bool,
    pub removed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Membership {
    pub root: String,
    pub thread: String,
    pub owner: String,
    pub creator: Option<String>,
    pub title: String,
    pub revision: i64,
    pub invitation_expires: i64,
    pub members: Vec<Member>,
    pub key_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedMembership {
    pub membership: Membership,
    pub signature: String,
}

fn refused(message: impl Into<String>) -> EngineError {
    EngineError::Consequence(message.into())
}

fn bytes(membership: &Membership) -> Result<Vec<u8>, EngineError> {
    let mut bytes = DOMAIN.to_vec();
    bytes.extend(serde_json::to_vec(membership).map_err(|error| refused(error.to_string()))?);
    Ok(bytes)
}

fn verify(signed: &SignedMembership, public_key: &str) -> bool {
    let Ok(key) = B64.decode(public_key) else {
        return false;
    };
    let Ok(key) = <[u8; 32]>::try_from(key.as_slice()) else {
        return false;
    };
    let Ok(key) = VerifyingKey::from_bytes(&key) else {
        return false;
    };
    let Ok(signature) = B64.decode(&signed.signature) else {
        return false;
    };
    let Ok(signature) = Signature::from_slice(&signature) else {
        return false;
    };
    bytes(&signed.membership).is_ok_and(|payload| key.verify(&payload, &signature).is_ok())
}

impl Engine {
    pub async fn group(&self, root: &str) -> Result<Option<SignedMembership>, EngineError> {
        let raw: Option<String> =
            store::sqlx::query_scalar("SELECT payload FROM conversation_group WHERE root = ?")
                .bind(root)
                .fetch_optional(&self.store.pool)
                .await?;
        let cached: Option<SignedMembership> = raw
            .map(|raw| serde_json::from_str(&raw).map_err(|error| refused(error.to_string())))
            .transpose()?;
        let Some(value) = store::records::get_extension(&self.store.pool, root, NAMESPACE).await?
        else {
            return Ok(cached);
        };
        let signed: SignedMembership =
            serde_json::from_value(value).map_err(|_| refused("Invalid group membership"))?;
        if cached
            .as_ref()
            .is_some_and(|cached| cached.membership.revision >= signed.membership.revision)
        {
            return Ok(cached);
        }
        let owner = store::records::get(&self.store.pool, root)
            .await?
            .and_then(|record| record.organ_uid);
        if signed.membership.root != root || owner.as_ref() != Some(&signed.membership.owner) {
            return Err(refused("Invalid group owner"));
        }
        let key = crate::trust::keys_of(&self.store, &signed.membership.owner)
            .await?
            .into_iter()
            .find(|(id, _)| id == &signed.membership.key_id)
            .ok_or_else(|| refused("Group signing key is unavailable"))?
            .1;
        if !verify(&signed, &key) {
            return Err(refused("Group signature is invalid"));
        }
        Ok(Some(signed))
    }

    pub async fn groups(&self) -> Result<Vec<SignedMembership>, EngineError> {
        let rows: Vec<String> = store::sqlx::query_scalar(
            "SELECT root FROM conversation_group UNION SELECT record_uid FROM record_extension WHERE namespace = 'lince.conversation.membership' LIMIT 256",
        )
        .fetch_all(&self.store.pool)
        .await?;
        let mut groups = Vec::new();
        for root in rows {
            if let Some(group) = self.group(&root).await? {
                groups.push(group);
            }
        }
        Ok(groups)
    }

    pub async fn group_accepted_locally(&self, root: &str) -> Result<bool, EngineError> {
        if let Some(local) = store::organs::local(&self.store.pool).await? {
            if self.group(root).await?.is_some_and(|group| {
                group
                    .membership
                    .members
                    .iter()
                    .any(|member| member.organ == local.uid && member.accepted && !member.removed)
            }) {
                return Ok(true);
            }
        }
        Ok(store::sqlx::query_scalar::<_, bool>(
            "SELECT accepted FROM conversation_group WHERE root = ?",
        )
        .bind(root)
        .fetch_optional(&self.store.pool)
        .await?
        .unwrap_or(false))
    }

    async fn group_signer(&self) -> Result<Signer, EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| refused("No local Organ"))?;
        let signer = self
            .organ_signer
            .lock()
            .await
            .clone()
            .or(self.signer.lock().await.clone())
            .ok_or_else(|| refused("This Cell cannot sign group invitations"))?;
        if signer.actor_uid != organ.uid {
            return Err(refused("Group invitations require an Organ signing key"));
        }
        Ok(signer)
    }

    pub async fn propose_group(
        &self,
        thread: &str,
        title: &str,
        organs: &[String],
        actor: Option<&str>,
    ) -> Result<SignedMembership, EngineError> {
        self.require_permission(actor, "record:create").await?;
        let thread = store::records::resolve(&self.store.pool, thread)
            .await?
            .ok_or_else(|| refused("Thread not found"))?;
        if thread.kind != nucleus::RecordKind::Thread.as_str()
            || !self.may_read_record(actor, &thread.uid).await?
        {
            return Err(EngineError::Forbidden("This thread is unavailable".into()));
        }
        let title = title.trim();
        if title.is_empty() || title.len() > 256 {
            return Err(refused("Give the group a title of at most 256 bytes"));
        }
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| refused("No local Organ"))?;
        let mut unique: BTreeSet<_> = organs.iter().cloned().collect();
        unique.remove(&local.uid);
        if unique.is_empty() || unique.len() > 5 || organs.len() > 6 {
            return Err(refused("Choose one to five other Organs"));
        }
        let signer = self.group_signer().await?;
        let node_id = self
            .enroller
            .lock()
            .map_err(|_| refused("Network unavailable"))?
            .as_ref()
            .and_then(|transport| transport.upgrade())
            .and_then(|transport| transport.local_node_id())
            .ok_or_else(|| refused("Start this Cell's network before inviting Organs"))?;
        let mut members = vec![Member {
            organ: local.uid.clone(),
            name: local.head,
            node_id,
            keys: crate::trust::keys_of(&self.store, &local.uid).await?,
            accepted: true,
            removed: false,
        }];
        for organ in unique {
            let contact = store::organs::contact(&self.store.pool, &organ)
                .await?
                .ok_or_else(|| refused("Only an existing contact can be introduced"))?;
            if contact.trust == "blocked" {
                return Err(refused("A blocked Organ cannot be invited"));
            }
            let keys = crate::trust::keys_of(&self.store, &organ).await?;
            if keys.is_empty() || keys.len() > 32 {
                return Err(refused("The contact has no usable identity keys"));
            }
            let node_id = contact
                .node_id
                .ok_or_else(|| refused("The contact has no network address"))?;
            members.push(Member {
                organ,
                name: contact.head,
                node_id,
                keys,
                accepted: false,
                removed: false,
            });
        }
        let root = store::records::create(
            &self.store.pool,
            store::records::NewRecord {
                slug: None,
                kind: nucleus::RecordKind::Conversation,
                head: title,
                body: "",
                quantity: store::exact::one(),
            },
        )
        .await?;
        store::replica::make_own_root(&self.store.pool, &root.uid).await?;
        let thread = self.open_thread(&root.uid, "Thread 1").await?;
        let membership = Membership {
            root: root.uid.clone(),
            thread,
            owner: local.uid,
            creator: actor.map(str::to_owned),
            title: title.into(),
            revision: 1,
            invitation_expires: (chrono::Utc::now() + chrono::Duration::hours(24)).timestamp(),
            members,
            key_id: signer.key_id.clone(),
        };
        let signed = SignedMembership {
            signature: signer.sign_bytes(&bytes(&membership)?),
            membership,
        };
        self.save_group(&signed, true).await?;
        if let Some(person) = actor {
            self.write_group_person(&root.uid, person, true).await?;
        }
        self.publish_group_record(&signed).await?;
        Ok(signed)
    }

    pub(crate) async fn save_group(
        &self,
        signed: &SignedMembership,
        accepted: bool,
    ) -> Result<(), EngineError> {
        let raw = serde_json::to_string(signed).map_err(|error| refused(error.to_string()))?;
        if raw.len() > 65536 {
            return Err(refused("Group membership is too large"));
        }
        store::sqlx::query("INSERT INTO conversation_group(root, owner, revision, accepted, payload) VALUES (?, ?, ?, ?, ?) ON CONFLICT(root) DO UPDATE SET revision = excluded.revision, payload = excluded.payload WHERE conversation_group.owner = excluded.owner AND conversation_group.revision < excluded.revision")
            .bind(&signed.membership.root).bind(&signed.membership.owner).bind(signed.membership.revision).bind(accepted).bind(raw).execute(&self.store.pool).await?;
        Ok(())
    }

    async fn publish_group_record(&self, signed: &SignedMembership) -> Result<(), EngineError> {
        store::records::set_extension(
            &self.store.pool,
            &signed.membership.root,
            NAMESPACE,
            &serde_json::to_value(signed).map_err(|error| refused(error.to_string()))?,
        )
        .await?;
        self.query_changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        Ok(())
    }

    pub async fn receive_group(
        &self,
        authenticated: &str,
        signed: &SignedMembership,
    ) -> Result<(), EngineError> {
        let _guard = self.thread_creation_lock.lock().await;
        let group = &signed.membership;
        if store::offers::standing_refusal(
            &self.store.pool,
            store::offers::OfferKind::ThreadInvite,
            &group.root,
            authenticated,
        )
        .await?
        {
            return Err(refused("This group invitation was declined"));
        }
        if authenticated != group.owner
            || group.revision <= 0
            || group.members.len() < 2
            || group.members.len() > 6
            || group.title.len() > 256
            || !nucleus::valid_uid(&group.root, "r")
            || !nucleus::valid_uid(&group.thread, "r")
            || group.root == group.thread
            || !group
                .members
                .iter()
                .any(|member| member.organ == group.owner && member.accepted && !member.removed)
        {
            return Err(refused("Invalid group invitation"));
        }
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| refused("No local Organ"))?;
        let own = group
            .members
            .iter()
            .find(|member| member.organ == local.uid)
            .ok_or_else(|| refused("This invitation is addressed to another Organ"))?;
        let mut identities = BTreeSet::new();
        for member in &group.members {
            let key_ids: BTreeSet<_> = member.keys.iter().map(|(id, _)| id).collect();
            if !nucleus::valid_uid(&member.organ, "r")
                || !identities.insert(&member.organ)
                || member.name.len() > 256
                || member.node_id.parse::<iroh::EndpointId>().is_err()
                || member.keys.is_empty()
                || member.keys.len() > 32
                || key_ids.len() != member.keys.len()
                || member.keys.iter().any(|(id, key)| {
                    id.len() > 256 || !B64.decode(key).is_ok_and(|key| key.len() == 32)
                })
            {
                return Err(refused("Invalid group member identity"));
            }
            let held = crate::trust::keys_of(&self.store, &member.organ).await?;
            if member.keys.iter().any(|(id, key)| {
                held.iter()
                    .any(|(known_id, known_key)| id == known_id && key != known_key)
            }) {
                return Err(refused(
                    "The introduction conflicts with a known identity key",
                ));
            }
            if store::organs::contact(&self.store.pool, &member.organ)
                .await?
                .is_some_and(|contact| contact.trust == "blocked")
                && !member.removed
            {
                return Err(refused("A group member is blocked"));
            }
        }
        let key = crate::trust::keys_of(&self.store, authenticated)
            .await?
            .into_iter()
            .find(|(id, _)| id == &group.key_id)
            .ok_or_else(|| refused("The inviter's signing key is unknown"))?
            .1;
        if !verify(signed, &key) {
            return Err(refused("Group invitation signature is invalid"));
        }
        let previous = self.group(&group.root).await?;
        if let Some(previous) = &previous {
            if previous.membership.owner != group.owner
                || previous.membership.thread != group.thread
                || previous.membership.revision > group.revision
                || (previous.membership.revision == group.revision && previous != signed)
                || previous.membership.members.iter().any(|before| {
                    group
                        .members
                        .iter()
                        .find(|after| before.organ == after.organ)
                        .is_none_or(|after| {
                            before.node_id != after.node_id
                                || before.keys != after.keys
                                || before.removed && !after.removed
                        })
                })
                || previous.membership.members.len() != group.members.len()
            {
                return Err(refused("Group membership was replayed or changed identity"));
            }
        } else {
            if store::records::get(&self.store.pool, &group.root)
                .await?
                .is_some()
                || store::records::get(&self.store.pool, &group.thread)
                    .await?
                    .is_some()
            {
                return Err(refused("A new group must have a fresh root and thread"));
            }
            if own.removed
                || own.accepted
                || group.invitation_expires <= chrono::Utc::now().timestamp()
            {
                return Err(refused(
                    "This group invitation has expired or is no longer offered",
                ));
            }
            let count: i64 = store::sqlx::query_scalar(
                "SELECT COUNT(*) FROM conversation_group WHERE owner = ? AND accepted = 0",
            )
            .bind(authenticated)
            .fetch_one(&self.store.pool)
            .await?;
            if count >= 16 {
                return Err(refused("Too many pending group invitations"));
            }
        }
        self.save_group(signed, false).await?;
        if previous.is_none() {
            store::invites::put(&self.store.pool, authenticated, &group.root, &group.title).await?;
            self.notify_notifications_changed();
        }
        if self.group_accepted_locally(&group.root).await? {
            self.apply_group_grants(group).await?;
        }
        Ok(())
    }

    pub async fn accept_group(&self, root: &str) -> Result<(), EngineError> {
        let _guard = self.thread_creation_lock.lock().await;
        let signed = self
            .group(root)
            .await?
            .ok_or_else(|| refused("Group invitation is unavailable"))?;
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| refused("No local Organ"))?;
        let member = signed
            .membership
            .members
            .iter()
            .find(|member| member.organ == local.uid)
            .ok_or_else(|| refused("This Organ was not invited"))?;
        if member.removed
            || (!member.accepted
                && signed.membership.invitation_expires <= chrono::Utc::now().timestamp())
        {
            return Err(refused("This group invitation is no longer valid"));
        }
        store::sqlx::query("UPDATE conversation_group SET accepted = 1 WHERE root = ?")
            .bind(root)
            .execute(&self.store.pool)
            .await?;
        self.apply_group_grants(&signed.membership).await?;
        Ok(())
    }

    pub async fn admit_group_organ(
        &self,
        authenticated: &str,
        root: &str,
    ) -> Result<SignedMembership, EngineError> {
        let _guard = self.thread_creation_lock.lock().await;
        let mut signed = self
            .group(root)
            .await?
            .ok_or_else(|| refused("Group not found"))?;
        let signer = self.group_signer().await?;
        if signer.actor_uid != signed.membership.owner {
            return Err(refused("Only the group's owning Organ can accept members"));
        }
        if store::organs::contact(&self.store.pool, authenticated)
            .await?
            .is_none_or(|contact| contact.trust == "blocked")
        {
            return Err(refused("The invited Organ is unavailable"));
        }
        let member = signed
            .membership
            .members
            .iter_mut()
            .find(|member| member.organ == authenticated)
            .ok_or_else(|| refused("This Organ was not invited"))?;
        if member.removed
            || (!member.accepted
                && signed.membership.invitation_expires <= chrono::Utc::now().timestamp())
        {
            return Err(refused("This group invitation is no longer valid"));
        }
        if !member.accepted {
            member.accepted = true;
            signed.membership.revision += 1;
            signed.signature = signer.sign_bytes(&bytes(&signed.membership)?);
            self.save_group(&signed, true).await?;
            self.publish_group_record(&signed).await?;
        }
        self.apply_group_grants(&signed.membership).await?;
        Ok(signed)
    }

    pub(crate) async fn apply_group_grants(&self, group: &Membership) -> Result<(), EngineError> {
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| refused("No local Organ"))?;
        let own = group
            .members
            .iter()
            .find(|member| member.organ == local.uid)
            .ok_or_else(|| refused("Local Organ is missing"))?;
        for member in group
            .members
            .iter()
            .filter(|member| member.organ != local.uid)
        {
            if own.removed || !own.accepted || member.removed || !member.accepted {
                store::replica::revoke(&self.store.pool, &group.root, &member.organ).await?;
                continue;
            }
            if store::organs::contact(&self.store.pool, &member.organ)
                .await?
                .is_none()
            {
                store::organs::add_contact(
                    &self.store.pool,
                    &member.organ,
                    None,
                    &member.name,
                    "",
                    1,
                )
                .await?;
                store::organs::set_node_id(&self.store.pool, &member.organ, Some(&member.node_id))
                    .await?;
            }
            if store::organs::contact(&self.store.pool, &member.organ)
                .await?
                .is_some_and(|contact| contact.trust == "blocked")
            {
                store::replica::revoke(&self.store.pool, &group.root, &member.organ).await?;
                continue;
            }
            for (id, key) in &member.keys {
                crate::trust::adopt_key(&self.store, &member.organ, id, key).await?;
            }
            store::replica::accept(&self.store.pool, &group.root, &member.organ).await?;
        }
        Ok(())
    }

    pub async fn set_group_person(
        &self,
        root: &str,
        person: &str,
        allowed: bool,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        self.require_permission(actor, "user:update").await?;
        let _guard = self.thread_creation_lock.lock().await;
        if !self.group_accepted_locally(root).await? {
            return Err(refused("Accept the group before admitting people"));
        }
        let person = store::records::resolve(&self.store.pool, person)
            .await?
            .ok_or_else(|| refused("Person not found"))?;
        if person.kind != nucleus::RecordKind::Person.as_str()
            || !store::people::is_active(&self.store.pool, &person.uid).await?
        {
            return Err(refused("Choose an active Person"));
        }
        self.write_group_person(root, &person.uid, allowed).await
    }

    async fn write_group_person(
        &self,
        root: &str,
        person: &str,
        allowed: bool,
    ) -> Result<(), EngineError> {
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| refused("No local Organ"))?;
        if store::records::get(&self.store.pool, person)
            .await?
            .and_then(|row| row.organ_uid)
            .as_ref()
            != Some(&local.uid)
        {
            return Err(refused("An Organ can admit only its own people"));
        }
        let mut value =
            store::records::get_extension(&self.store.pool, person, ADMISSION_NAMESPACE)
                .await?
                .unwrap_or_else(|| serde_json::json!({}));
        value[root] = serde_json::Value::Bool(allowed);
        store::records::set_extension(&self.store.pool, person, ADMISSION_NAMESPACE, &value)
            .await?;
        Ok(())
    }

    pub async fn group_person_admitted(
        &self,
        root: &str,
        person: &str,
    ) -> Result<bool, EngineError> {
        Ok(
            store::records::get_extension(&self.store.pool, person, ADMISSION_NAMESPACE)
                .await?
                .is_some_and(|value| value[root].as_bool() == Some(true)),
        )
    }

    pub async fn remove_group_organ(
        &self,
        root: &str,
        organ: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        self.require_permission(actor, "record:update").await?;
        let _guard = self.thread_creation_lock.lock().await;
        let mut signed = self
            .group(root)
            .await?
            .ok_or_else(|| refused("Group not found"))?;
        let signer = self.group_signer().await?;
        if signed.membership.owner != signer.actor_uid
            || actor.is_some() && signed.membership.creator.as_deref() != actor
            || organ == signer.actor_uid
        {
            return Err(EngineError::Forbidden(
                "Only the conversation creator can remove another Organ".into(),
            ));
        }
        let member = signed
            .membership
            .members
            .iter_mut()
            .find(|member| member.organ == organ)
            .ok_or_else(|| refused("Organ is not a group member"))?;
        member.removed = true;
        signed.membership.revision += 1;
        signed.signature = signer.sign_bytes(&bytes(&signed.membership)?);
        self.save_group(&signed, true).await?;
        self.apply_group_grants(&signed.membership).await?;
        self.publish_group_record(&signed).await?;
        Ok(())
    }
}
