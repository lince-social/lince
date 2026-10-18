use super::serving::{PeerAuthority, authority_stamp};
use super::*;
use crate::actions::{ActionOutcome, VerifiedActionAuthorship};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use nucleus::trust_analysis::{
    custody::{self as model, Acknowledgment, Command, Offer},
    remote::Verification,
    serving::Artifact,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};
use store::sqlx::{Sqlite, Transaction};
use tokio::sync::Mutex;
mod discovery;
mod forwarding;
mod local;

#[derive(Default)]
pub(crate) struct Runtime {
    uploads: Arc<Mutex<BTreeMap<String, Upload>>>,
    forwarding: Mutex<()>,
    scans: Mutex<BTreeMap<(String, bool), i64>>,
}

struct Upload {
    endpoint: String,
    owner: String,
    version: u64,
    offer: Offer,
    authority: PeerAuthority,
    deadline: Instant,
    bytes: Vec<u8>,
}

impl Engine {
    pub(super) async fn evidence_custody_command(
        &self,
        person: &str,
        request: &Command,
        actor: Option<&str>,
        now: DateTime<Utc>,
        authorship: Option<&VerifiedActionAuthorship>,
    ) -> Result<ActionOutcome, EngineError> {
        if let Command::InspectOriginal { record } = request {
            let mut tx = self.store.pool.begin().await?;
            self.require_permission_on(&mut tx, actor, "record:read")
                .await?;
            self.validate_local_custody_on(&mut tx, record, actor, now.timestamp())
                .await?;
            let bytes:Vec<u8>=store::sqlx::query_scalar("SELECT p.body FROM evidence_custody_payload p JOIN evidence_custody_commitment c ON c.artifact_hash=p.artifact_hash WHERE c.record_uid=?").bind(record).fetch_one(&mut *tx).await?;
            let artifact = Artifact::parse(&bytes, now.timestamp()).map_err(invalid)?;
            let stamp = authority_stamp(&mut tx, &artifact).await?;
            drop(tx);
            let signature = self.remote_signature(&artifact, now).await;
            let mut tx = self.store.pool.begin().await?;
            self.validate_local_custody_on(
                &mut tx,
                record,
                actor,
                nucleus::execution::now().timestamp(),
            )
            .await?;
            if authority_stamp(&mut tx, &artifact).await? != stamp {
                return Err(invalid(
                    "Original signing authority changed during inspection",
                ));
            }
            return Ok(ActionOutcome {
                data: Some(
                    json!({"trust":{"kind":"custody_original","record":record,"hash":artifact.hash().map_err(invalid)?,"artifact":artifact,"signature":signature}}),
                ),
                ..Default::default()
            });
        }
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_permission_on(&mut tx, actor, request_permission(request))
            .await?;
        let result = match request {
            Command::InspectOriginal { .. } => unreachable!(),
            Command::Outbox { before } => {
                store::evidence_forwarding::cleanup_on(&mut tx, now.timestamp()).await?;
                let mut rows =
                    store::evidence_forwarding::page_on(&mut tx, person, *before).await?;
                let next = if rows.len() > 32 {
                    rows.truncate(32);
                    rows.last().and_then(|row| row["sequence"].as_i64())
                } else {
                    None
                };
                let ids = rows
                    .iter()
                    .filter_map(|row| row["record"].as_str())
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                let graph = crate::access::policy_graph_on(&mut tx, &ids)
                    .await
                    .map_err(|e| invalid(e.to_string()))?;
                let allowed = self.readable_on(&mut tx, actor, &graph).await?;
                rows.retain(|row| {
                    row["record"]
                        .as_str()
                        .is_some_and(|record| allowed.contains(record))
                });
                json!({"kind":"custody_outbox","rows":rows,"next":next})
            }
            Command::CancelDelivery {
                record,
                expected_offer,
            } => {
                self.contact_record_readable_on(&mut tx, record, actor)
                    .await?;
                store::evidence_forwarding::cancel_on(&mut tx, record, person, expected_offer)
                    .await?;
                json!({"kind":"custody_delivery","job":store::evidence_forwarding::get_on(&mut tx,record).await?})
            }
            Command::InspectPolicy => {
                let state = store::evidence_custody::policy_on(&mut tx, person).await?;
                if state.version > 0 {
                    let record: String = store::sqlx::query_scalar(
                        "SELECT record_uid FROM evidence_custody_policy WHERE owner_person_uid=?",
                    )
                    .bind(person)
                    .fetch_one(&mut *tx)
                    .await?;
                    self.contact_record_readable_on(&mut tx, &record, actor)
                        .await?;
                }
                json!({"kind":"custody_policy","state":state})
            }
            Command::SetPolicy {
                policy,
                expected_version,
            } => {
                self.require_permission_on(&mut tx, actor, "record:create")
                    .await?;
                let signer = self.signer.lock().await.clone();
                let intent = authorship.map(|authorship| authorship.intent_uid.as_str());
                let state = store::evidence_custody::set_policy_on(
                    &mut tx,
                    person,
                    policy,
                    *expected_version,
                    now.timestamp(),
                    |hash| signer.as_ref().map(|signer| signer.sign_hash(hash)),
                    intent,
                )
                .await?;
                json!({"kind":"custody_policy","state":state})
            }
            Command::Inventory { before } => {
                let mut rows =
                    store::evidence_custody::inventory_on(&mut tx, person, *before).await?;
                let next = if rows.len() > 32 {
                    rows.truncate(32);
                    rows.last().and_then(|row| row["sequence"].as_i64())
                } else {
                    None
                };
                let ids = rows
                    .iter()
                    .filter_map(|row| row["record"].as_str())
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                let graph = crate::access::policy_graph_on(&mut tx, &ids)
                    .await
                    .map_err(|e| invalid(e.to_string()))?;
                let allowed = self.readable_on(&mut tx, actor, &graph).await?;
                rows.retain(|row| {
                    row["record"]
                        .as_str()
                        .is_some_and(|record| allowed.contains(record))
                });
                json!({"kind":"custody_inventory","rows":rows,"next":next})
            }
            Command::Cleanup => {
                let (payloads, controls) =
                    store::evidence_custody::cleanup_on(&mut tx, now.timestamp()).await?;
                self.evidence_custody
                    .uploads
                    .lock()
                    .await
                    .retain(|_, upload| upload.deadline > Instant::now());
                json!({"kind":"custody_cleanup","expired_payloads":payloads,"expired_controls":controls})
            }
        };
        tx.commit().await?;
        Ok(ActionOutcome {
            data: Some(json!({"trust":result})),
            ..Default::default()
        })
    }

    async fn custody_policy_for_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        owner: &str,
        sender: &str,
        version: Option<u64>,
    ) -> Result<store::evidence_custody::PolicyState, EngineError> {
        let state = store::evidence_custody::policy_on(tx, owner).await?;
        state.policy.validate().map_err(invalid)?;
        if !state.policy.enabled
            || version.is_some_and(|version| version != state.version)
            || !store::people::is_active_on(tx, owner).await?
        {
            return Err(invalid(
                "Custody is unavailable under the current operator policy",
            ));
        }
        self.require_permission_on(tx, Some(owner), "organ:update")
            .await?;
        let record: String = store::sqlx::query_scalar(
            "SELECT record_uid FROM evidence_custody_policy WHERE owner_person_uid=?",
        )
        .bind(owner)
        .fetch_one(&mut **tx)
        .await?;
        self.contact_record_readable_on(tx, &record, Some(owner))
            .await?;
        let identity = Identity::Organ(sender.into());
        let context = self
            .contact_identity_context_on(tx, &identity, Some(owner))
            .await?;
        if !context.readable
            || context.blocked
            || state.policy.senders.include.is_empty()
            || !state.policy.senders.allows(&identity, &context)
        {
            return Err(invalid("Sender is outside the operator custody policy"));
        }
        Ok(state)
    }

    fn custody_signature(
        &self,
        authority: &PeerAuthority,
        endpoint: &str,
        key_id: &str,
        public_key: &str,
        signature: &str,
        bytes: &[u8],
    ) -> Result<(), EngineError> {
        let roster: crate::roster::Roster = serde_json::from_str(&authority.payload)?;
        let permitted = (crate::roster::is_root_key_id(key_id) && public_key == roster.root_key)
            || roster.cells.iter().any(|cell| {
                cell.node_id == endpoint
                    && cell.may(crate::roster::CAP_REPRESENT)
                    && cell.operational_key == public_key
                    && key_id == crate::roster::cell_key_id(&cell.cell_uid)
            });
        if !permitted || !crate::trust::verify_bytes_strict(public_key, bytes, signature) {
            return Err(invalid(
                "Custody signature lacks current endpoint representative authority",
            ));
        }
        Ok(())
    }

    pub(super) async fn custody_peer_unchanged_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        endpoint: &str,
        organ: &str,
        authority: &PeerAuthority,
    ) -> Result<(), EngineError> {
        self.social_ask_peer_on(tx, endpoint, organ, false).await?;
        self.custody_authority_unchanged_on(tx, organ, authority)
            .await
    }

    async fn custody_authority_unchanged_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        organ: &str,
        authority: &PeerAuthority,
    ) -> Result<(), EngineError> {
        let row: Option<(String, String)> =
            store::sqlx::query_as("SELECT payload,signature FROM organ_roster WHERE organ_uid=?")
                .bind(organ)
                .fetch_optional(&mut **tx)
                .await?;
        let revocations: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM identity_revocation WHERE organ_uid=?")
                .bind(organ)
                .fetch_one(&mut **tx)
                .await?;
        if row != Some((authority.payload.clone(), authority.signature.clone()))
            || revocations != authority.revocations
            || store::identity_roots::state_on(tx, organ).await? != authority.anchor
        {
            return Err(invalid(
                "Custody peer authority changed during the operation",
            ));
        }
        Ok(())
    }

    pub(crate) async fn begin_evidence_custody(
        &self,
        endpoint: &str,
        node: &str,
        offer: Offer,
        now: i64,
    ) -> Result<Value, EngineError> {
        offer.validate(now).map_err(invalid)?;
        let peer = self.social_ask_peer(endpoint, false).await?;
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("Custody requires a local Organ"))?;
        if offer.sender != peer.organ || offer.custodian != local.uid {
            return Err(invalid(
                "Custody offer belongs to another sender or destination",
            ));
        }
        let authority = self.serving_peer_authority(&peer.organ, endpoint).await?;
        self.custody_signature(
            &authority,
            endpoint,
            &offer.key_id,
            &offer.public_key,
            &offer.signature,
            &offer.signing_bytes().map_err(invalid)?,
        )?;
        let mut tx = self.store.pool.begin().await?;
        let owners:Vec<String>=store::sqlx::query_scalar("SELECT owner_person_uid FROM evidence_custody_policy WHERE json_extract(policy_json,'$.enabled')=1 ORDER BY owner_person_uid LIMIT 128").fetch_all(&mut *tx).await?;
        let mut selected = None;
        for owner in owners {
            if let Ok(state) = self
                .custody_policy_for_on(&mut tx, &owner, &peer.organ, None)
                .await
            {
                if offer.retained_until
                    <= now.saturating_add(i64::from(state.policy.retention_seconds))
                {
                    selected = Some((owner, state.version));
                    break;
                }
            }
        }
        let (owner, version) = selected
            .ok_or_else(|| invalid("No operator accepts this sender and retention term"))?;
        self.custody_peer_unchanged_on(&mut tx, endpoint, &peer.organ, &authority)
            .await?;
        if let Some(ack) = store::evidence_custody::acknowledgment_on(&mut tx, &offer).await? {
            return Ok(json!({"service":node,"acknowledgment":ack}));
        }
        drop(tx);
        let mut uploads = self.evidence_custody.uploads.lock().await;
        uploads.retain(|_, upload| upload.deadline > Instant::now());
        let hash = offer.hash().map_err(invalid)?;
        if let Some((id, upload)) = uploads.iter().find(|(_, upload)| {
            upload.endpoint == endpoint && upload.offer.hash().as_ref() == Ok(&hash)
        }) {
            return Ok(json!({"service":node,"upload":id,"offset":upload.bytes.len()}));
        }
        if uploads.len() >= model::MAX_UPLOADS {
            return Err(invalid("Custody upload capacity is full"));
        }
        let id = nucleus::new_uid("cust");
        let capacity = offer.bytes as usize;
        uploads.insert(
            id.clone(),
            Upload {
                endpoint: endpoint.into(),
                owner,
                version,
                offer,
                authority,
                deadline: Instant::now() + Duration::from_secs(model::UPLOAD_SECONDS),
                bytes: Vec::with_capacity(capacity),
            },
        );
        let pending = self.evidence_custody.uploads.clone();
        let expiring = id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(model::UPLOAD_SECONDS)).await;
            let mut uploads = pending.lock().await;
            if uploads
                .get(&expiring)
                .is_some_and(|upload| upload.deadline <= Instant::now())
            {
                uploads.remove(&expiring);
            }
        });
        Ok(json!({"service":node,"upload":id,"offset":0}))
    }

    pub(crate) async fn chunk_evidence_custody(
        &self,
        endpoint: &str,
        node: &str,
        id: &str,
        offset: u32,
        bytes: &str,
    ) -> Result<Value, EngineError> {
        if bytes.len() > ((nucleus::trust_analysis::serving::CHUNK_BYTES + 2) / 3) * 4 {
            return Err(invalid("Custody chunk exceeds its byte bound"));
        }
        let bytes = STANDARD
            .decode(bytes)
            .map_err(|_| invalid("Invalid custody chunk encoding"))?;
        if bytes.is_empty() || bytes.len() > nucleus::trust_analysis::serving::CHUNK_BYTES {
            return Err(invalid("Choose a nonempty bounded custody chunk"));
        }
        let mut uploads = self.evidence_custody.uploads.lock().await;
        uploads.retain(|_, upload| upload.deadline > Instant::now());
        let upload = uploads
            .get_mut(id)
            .filter(|upload| upload.endpoint == endpoint)
            .ok_or_else(|| invalid("Custody upload is unavailable or expired"))?;
        let offset = offset as usize;
        if offset == upload.bytes.len()
            && offset.saturating_add(bytes.len()) <= upload.offer.bytes as usize
        {
            upload.bytes.extend_from_slice(&bytes);
        } else if upload.bytes.get(offset..offset.saturating_add(bytes.len()))
            != Some(bytes.as_slice())
        {
            return Err(invalid(
                "Custody chunks must be contiguous and exact on retry",
            ));
        }
        Ok(json!({"service":node,"upload":id,"offset":upload.bytes.len()}))
    }

    pub(crate) async fn cancel_evidence_custody(
        &self,
        endpoint: &str,
        node: &str,
        id: &str,
    ) -> Result<Value, EngineError> {
        let mut uploads = self.evidence_custody.uploads.lock().await;
        if uploads
            .get(id)
            .is_some_and(|upload| upload.endpoint == endpoint)
        {
            uploads.remove(id);
        }
        Ok(json!({"service":node,"cancelled":true}))
    }

    pub(crate) async fn finish_evidence_custody(
        &self,
        endpoint: &str,
        node: &str,
        id: &str,
        now: i64,
    ) -> Result<Value, EngineError> {
        let _slot = self
            .evidence_serving_slots
            .try_acquire()
            .map_err(|_| invalid("Evidence work capacity is full"))?;
        let upload = {
            let mut uploads = self.evidence_custody.uploads.lock().await;
            uploads.retain(|_, upload| upload.deadline > Instant::now());
            if !uploads
                .get(id)
                .is_some_and(|upload| upload.endpoint == endpoint)
            {
                return Err(invalid("Custody upload is unavailable or expired"));
            }
            uploads.remove(id).unwrap()
        };
        if upload.bytes.len() != upload.offer.bytes as usize {
            return Err(invalid(
                "Complete the exact artifact before custody acknowledgment",
            ));
        }
        let artifact = Artifact::parse(&upload.bytes, now).map_err(invalid)?;
        if upload.bytes != artifact.bytes().map_err(invalid)? {
            return Err(invalid(
                "Custody requires the exact canonical original bytes",
            ));
        }
        upload.offer.accepts(&artifact, now).map_err(invalid)?;
        let at = DateTime::from_timestamp(now, 0).ok_or_else(|| invalid("Invalid custody time"))?;
        let stamp = authority_stamp(&mut *self.store.pool.acquire().await?, &artifact).await?;
        if self.remote_signature(&artifact, at).await != Verification::Accepted {
            return Err(invalid(
                "Independently recognize original signing identities before custody",
            ));
        }
        let signer = self
            .organ_signer
            .lock()
            .await
            .clone()
            .filter(|signer| signer.actor_uid == upload.offer.custodian)
            .or(self.root_signer().await?);
        let signer =
            signer.ok_or_else(|| invalid("Custodian representative signing key is unavailable"))?;
        let local_authority = self
            .serving_peer_authority(&upload.offer.custodian, node)
            .await?;
        let mut ack = Acknowledgment {
            offer: upload.offer.hash().map_err(invalid)?,
            artifact: upload.offer.artifact.clone(),
            sender: upload.offer.sender.clone(),
            custodian: upload.offer.custodian.clone(),
            bytes: upload.offer.bytes,
            retained_until: upload.offer.retained_until,
            stored_at: now,
            key_id: signer.key_id.clone(),
            public_key: signer.public_key_b64(),
            signature: String::new(),
        };
        ack.signature = signer.sign_bytes(&ack.signing_bytes().map_err(invalid)?);
        self.custody_signature(
            &local_authority,
            node,
            &ack.key_id,
            &ack.public_key,
            &ack.signature,
            &ack.signing_bytes().map_err(invalid)?,
        )?;
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.custody_policy_for_on(
            &mut tx,
            &upload.owner,
            &upload.offer.sender,
            Some(upload.version),
        )
        .await?;
        self.custody_peer_unchanged_on(&mut tx, endpoint, &upload.offer.sender, &upload.authority)
            .await?;
        self.custody_authority_unchanged_on(&mut tx, &upload.offer.custodian, &local_authority)
            .await?;
        if upload.deadline <= Instant::now() || authority_stamp(&mut tx, &artifact).await? != stamp
        {
            return Err(invalid(
                "Custody deadline or signing authority changed before commit",
            ));
        }
        let stored = store::evidence_custody::retain_on(
            &mut tx,
            store::evidence_custody::Retain {
                person: &upload.owner,
                expected_policy: upload.version,
                offer: &upload.offer,
                artifact: &artifact,
                acknowledgment: &ack,
                now,
            },
            |hash| Some(signer.sign_hash(hash)),
        )
        .await?;
        tx.commit().await?;
        self.notify_query_changed();
        Ok(json!({"service":node,"acknowledgment":stored}))
    }

    pub(super) async fn fetch_custody_evidence(
        &self,
        endpoint: &str,
        node: &str,
        hash: &nucleus::karma::CanonicalHash,
        offset: u32,
        now: i64,
    ) -> Result<Value, EngineError> {
        let peer = self.social_ask_peer(endpoint, false).await?;
        let peer_authority = self.serving_peer_authority(&peer.organ, endpoint).await?;
        let rows:Vec<(String,String,String)>=store::sqlx::query_as("SELECT c.owner_person_uid,c.record_uid,c.offer_json FROM evidence_custody_commitment c JOIN record r ON r.uid=c.record_uid WHERE c.artifact_hash=? AND c.retained_until>? AND r.deleted_at IS NULL ORDER BY c.sequence DESC LIMIT 256").bind(hash.as_str()).bind(now).fetch_all(&self.store.pool).await?;
        for (owner, record, offer) in rows {
            let offer: Offer = serde_json::from_str(&offer)?;
            let mut tx = self.store.pool.begin().await?;
            let state = match self
                .custody_policy_for_on(&mut tx, &owner, &offer.sender, None)
                .await
            {
                Ok(state) => state,
                Err(_) => continue,
            };
            let identity = Identity::Organ(peer.organ.clone());
            let context = self
                .contact_identity_context_on(&mut tx, &identity, Some(&owner))
                .await?;
            if !context.readable
                || context.blocked
                || !state.policy.readers.allows(&identity, &context)
                || !offer.readers.allows(&identity, &context)
                || self
                    .contact_record_readable_on(&mut tx, &record, Some(&owner))
                    .await
                    .is_err()
            {
                continue;
            }
            let bytes: Option<Vec<u8>> = store::sqlx::query_scalar(
                "SELECT body FROM evidence_custody_payload WHERE artifact_hash=?",
            )
            .bind(hash.as_str())
            .fetch_optional(&mut *tx)
            .await?;
            let Some(bytes) = bytes else {
                continue;
            };
            let artifact = Artifact::parse(&bytes, now).map_err(invalid)?;
            offer.accepts(&artifact, now).map_err(invalid)?;
            let stamp = authority_stamp(&mut tx, &artifact).await?;
            drop(tx);
            let at =
                DateTime::from_timestamp(now, 0).ok_or_else(|| invalid("Invalid custody time"))?;
            if self.remote_signature(&artifact, at).await != Verification::Accepted {
                continue;
            }
            let offset = offset as usize;
            if offset >= bytes.len() {
                return Err(invalid("Custody chunk is unavailable"));
            }
            let end = (offset + nucleus::trust_analysis::serving::CHUNK_BYTES).min(bytes.len());
            let response = json!({"service":node,"chunk":{"hash":hash,"offset":offset,"total":bytes.len(),"bytes":STANDARD.encode(&bytes[offset..end])}});
            let mut tx = self.store.pool.begin().await?;
            self.custody_policy_for_on(&mut tx, &owner, &offer.sender, Some(state.version))
                .await?;
            self.contact_record_readable_on(&mut tx, &record, Some(&owner))
                .await?;
            self.custody_peer_unchanged_on(&mut tx, endpoint, &peer.organ, &peer_authority)
                .await?;
            let current = self
                .contact_identity_context_on(&mut tx, &identity, Some(&owner))
                .await?;
            let still_retained:bool=store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM evidence_custody_commitment WHERE record_uid=? AND retained_until>?)").bind(&record).bind(nucleus::execution::now().timestamp()).fetch_one(&mut *tx).await?;
            if !still_retained
                || !current.readable
                || current.blocked
                || !state.policy.readers.allows(&identity, &current)
                || !offer.readers.allows(&identity, &current)
                || authority_stamp(&mut tx, &artifact).await? != stamp
            {
                return Err(invalid(
                    "Custody access or original authority changed during fetch",
                ));
            }
            return Ok(response);
        }
        Err(invalid(
            "Evidence is unavailable under current custody permissions",
        ))
    }
}

#[cfg(feature = "trust-test-hooks")]
impl Engine {
    pub async fn expire_custody_uploads_for_test(&self) {
        for upload in self.evidence_custody.uploads.lock().await.values_mut() {
            upload.deadline = Instant::now() - Duration::from_secs(1);
        }
    }
}

fn request_permission(request: &Command) -> &'static str {
    match request {
        Command::CancelDelivery { .. } => "record:update",
        Command::SetPolicy { .. } | Command::Cleanup => "organ:update",
        _ => "record:read",
    }
}
