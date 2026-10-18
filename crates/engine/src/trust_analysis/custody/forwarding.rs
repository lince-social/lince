use super::*;
use futures::{StreamExt, stream};
use nucleus::{
    social::PublicRequest,
    trust_analysis::{
        Filter,
        custody::forwarding::{self as contract, SourceMode},
    },
};
use store::evidence_forwarding::{self as outbox, Job};

struct Original {
    artifact: Artifact,
    readers: Filter,
    until: i64,
}

impl Engine {
    async fn forwarding_policy_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        owner: &str,
        peer: &str,
        version: u64,
    ) -> Result<store::evidence_custody::PolicyState, EngineError> {
        let state = store::evidence_custody::policy_on(tx, owner).await?;
        if state.version != version
            || !state.policy.forwarding.enabled
            || !store::people::is_active_on(tx, owner).await?
        {
            return Err(invalid("Evidence forwarding is not currently authorized"));
        }
        state.policy.validate().map_err(invalid)?;
        self.require_permission_on(tx, Some(owner), "organ:update")
            .await?;
        self.require_permission_on(tx, Some(owner), "record:read")
            .await?;
        let record: String = store::sqlx::query_scalar(
            "SELECT record_uid FROM evidence_custody_policy WHERE owner_person_uid=?",
        )
        .bind(owner)
        .fetch_one(&mut **tx)
        .await?;
        self.contact_record_readable_on(tx, &record, Some(owner))
            .await?;
        let identity = Identity::Organ(peer.into());
        let context = self
            .contact_identity_context_on(tx, &identity, Some(owner))
            .await?;
        if !context.readable
            || context.blocked
            || state.policy.forwarding.peers.include.is_empty()
            || !state.policy.forwarding.peers.allows(&identity, &context)
        {
            return Err(invalid("Peer is outside the current forwarding policy"));
        }
        Ok(state)
    }

    async fn forwarding_original(
        &self,
        owner: &str,
        source: &str,
        mode: SourceMode,
        peer: &str,
        now: DateTime<Utc>,
    ) -> Result<Original, EngineError> {
        let mut tx = self.store.pool.begin().await?;
        self.contact_record_readable_on(&mut tx, source, Some(owner))
            .await?;
        let identity = Identity::Organ(peer.into());
        let context = self
            .contact_identity_context_on(&mut tx, &identity, Some(owner))
            .await?;
        if !context.readable || context.blocked {
            return Err(invalid("Forwarding peer profile is unavailable"));
        }
        let original = match mode {
            SourceMode::Serving => {
                let saved = store::evidence_serving::get_on(&mut tx, source).await?;
                if saved.owner != owner
                    || !saved.active
                    || !saved.current
                    || saved.review.expires_at <= now.timestamp()
                    || saved.review.request.audience.include.is_empty()
                    || !saved.review.request.audience.allows(&identity, &context)
                {
                    return Err(invalid(
                        "Source lacks a current explicit serving approval for this peer",
                    ));
                }
                drop(tx);
                let snapshot = self
                    .serving_snapshot(owner, &saved.review.request, Some(owner), now)
                    .await?;
                if !saved.review.allows_current(
                    &snapshot.artifact,
                    snapshot.version,
                    snapshot.revision,
                ) {
                    return Err(invalid("Source changed beyond its reviewed serving scope"));
                }
                Original {
                    artifact: snapshot.artifact,
                    readers: saved.review.request.audience,
                    until: saved.review.expires_at,
                }
            }
            SourceMode::Custody => {
                let row:Option<(String,String,i64)>=store::sqlx::query_as("SELECT artifact_hash,offer_json,retained_until FROM evidence_custody_commitment WHERE record_uid=? AND owner_person_uid=? AND retained_until>?").bind(source).bind(owner).bind(now.timestamp()).fetch_optional(&mut *tx).await?;
                let (hash, offer, until) =
                    row.ok_or_else(|| invalid("Retained public original is unavailable"))?;
                let offer: Offer = serde_json::from_str(&offer)?;
                let policy = self
                    .custody_policy_for_on(&mut tx, owner, &offer.sender, None)
                    .await?;
                if !policy.policy.readers.allows(&identity, &context)
                    || !offer.readers.allows(&identity, &context)
                {
                    return Err(invalid(
                        "Retained original readers exclude this forwarding peer",
                    ));
                }
                let body: Vec<u8> = store::sqlx::query_scalar(
                    "SELECT body FROM evidence_custody_payload WHERE artifact_hash=?",
                )
                .bind(hash)
                .fetch_one(&mut *tx)
                .await?;
                drop(tx);
                Original {
                    artifact: Artifact::parse(&body, now.timestamp()).map_err(invalid)?,
                    readers: offer.readers,
                    until,
                }
            }
        };
        if self.remote_signature(&original.artifact, now).await != Verification::Accepted {
            return Err(invalid(
                "Original signing authority is unavailable for forwarding",
            ));
        }
        Ok(original)
    }

    async fn forwarding_signer(&self) -> Result<(Signer, String), EngineError> {
        let signer =
            self.organ_signer.lock().await.clone().ok_or_else(|| {
                invalid("Connect the authorized representative before forwarding")
            })?;
        let roster = self
            .roster_of(&signer.actor_uid)
            .await?
            .ok_or_else(|| invalid("Publish the current representative roster"))?;
        let node = roster
            .roster
            .cells
            .iter()
            .find(|member| {
                member.operational_key == signer.public_key_b64()
                    && member.may(crate::roster::CAP_REPRESENT)
                    && signer.key_id == crate::roster::cell_key_id(&member.cell_uid)
            })
            .map(|member| member.node_id.clone())
            .ok_or_else(|| invalid("The local forwarding key lacks representative authority"))?;
        self.serving_peer_authority(&signer.actor_uid, &node)
            .await?;
        Ok((signer, node))
    }

    pub(crate) async fn forward_evidence_once(
        &self,
        peers: &[(String, String)],
        now: i64,
    ) -> Result<usize, EngineError> {
        let Ok(_worker) = self.evidence_custody.forwarding.try_lock() else {
            return Ok(0);
        };
        if peers.is_empty() {
            return Ok(0);
        }
        let (signer, node) = match self.forwarding_signer().await {
            Ok(identity) => identity,
            Err(_) => return Ok(0),
        };
        let at =
            DateTime::from_timestamp(now, 0).ok_or_else(|| invalid("Invalid forwarding time"))?;
        let policies:Vec<(String,i64)>=store::sqlx::query_as("SELECT owner_person_uid,version FROM evidence_custody_policy WHERE json_extract(policy_json,'$.forwarding.enabled')=1 ORDER BY owner_person_uid LIMIT 128").fetch_all(&self.store.pool).await?;
        let active = policies
            .iter()
            .map(|(owner, _)| owner.clone())
            .collect::<BTreeSet<_>>();
        self.evidence_custody
            .scans
            .lock()
            .await
            .retain(|(owner, _), _| active.contains(owner));
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut scanned_bytes = 0usize;
        'owners: for (owner, version) in policies {
            for custody in [false, true] {
                let after = *self
                    .evidence_custody
                    .scans
                    .lock()
                    .await
                    .get(&(owner.clone(), custody))
                    .unwrap_or(&0);
                let rows: Vec<(i64, String, i64, bool)> = if custody {
                    store::sqlx::query_as("SELECT c.sequence,c.record_uid,c.byte_count,0 FROM evidence_custody_commitment c JOIN record r ON r.uid=c.record_uid WHERE c.owner_person_uid=? AND c.sequence>? AND c.retained_until>? AND r.deleted_at IS NULL ORDER BY c.sequence LIMIT 16").bind(&owner).bind(after).bind(now).fetch_all(&self.store.pool).await?
                } else {
                    store::sqlx::query_as("SELECT a.sequence,a.record_uid,length(CAST(a.review_json AS BLOB)),COALESCE(json_extract(a.review_json,'$.request.scope')='recipe',0) FROM record_evidence_serving a JOIN record r ON r.uid=a.record_uid WHERE a.owner_person_uid=? AND a.sequence>? AND a.expires_at>? AND r.deleted_at IS NULL AND NOT EXISTS(SELECT 1 FROM record_evidence_serving n WHERE n.owner_person_uid=a.owner_person_uid AND n.source_record_uid=a.source_record_uid AND n.source_kind=a.source_kind AND n.sequence>a.sequence) ORDER BY a.sequence LIMIT 16").bind(&owner).bind(after).bind(now).fetch_all(&self.store.pool).await?
                };
                let mut last = after;
                let mut complete = true;
                for (sequence, source, size, rolling) in &rows {
                    if Instant::now() >= deadline
                        || scanned_bytes.saturating_add(*size as usize)
                            > nucleus::trust_analysis::MAX_BYTES
                    {
                        complete = false;
                        break;
                    }
                    last = *sequence;
                    scanned_bytes += *size as usize;
                    for (peer_index, (peer, endpoint)) in peers.iter().take(32).enumerate() {
                        if Instant::now() >= deadline {
                            break 'owners;
                        }
                        let mut tx = self.store.pool.begin().await?;
                        let policy = match self
                            .forwarding_policy_on(&mut tx, &owner, peer, version as u64)
                            .await
                        {
                            Ok(policy) => policy,
                            Err(_) => continue,
                        };
                        if !rolling {
                            let seen:bool=store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record_evidence_forwarding WHERE owner_person_uid=? AND source_record_uid=? AND peer_organ_uid=? AND (policy_version=? OR (state='stored' AND retained_until>?)))").bind(&owner).bind(source).bind(peer).bind(version).bind(now).fetch_one(&mut *tx).await?;
                            if seen {
                                continue;
                            }
                        }
                        drop(tx);
                        if peer_index > 0 {
                            if scanned_bytes.saturating_add(*size as usize)
                                > nucleus::trust_analysis::MAX_BYTES
                            {
                                break 'owners;
                            }
                            scanned_bytes += *size as usize;
                        }
                        if self.social_ask_peer(endpoint, true).await.is_err()
                            || self.serving_peer_authority(peer, endpoint).await.is_err()
                        {
                            continue;
                        }
                        let mode = if custody {
                            SourceMode::Custody
                        } else {
                            SourceMode::Serving
                        };
                        let original = match self
                            .forwarding_original(&owner, source, mode, peer, at)
                            .await
                        {
                            Ok(original) => original,
                            Err(_) => continue,
                        };
                        let until = original
                            .until
                            .min(original.artifact.expires_at().map_err(invalid)?)
                            .min(now.saturating_add(i64::from(policy.policy.retention_seconds)));
                        if until <= now + 1 {
                            continue;
                        }
                        let mut offer = Offer {
                            nonce: nucleus::new_uid("cust"),
                            sender: signer.actor_uid.clone(),
                            custodian: peer.clone(),
                            artifact: original.artifact.hash().map_err(invalid)?,
                            source_kind: original.artifact.kind(),
                            bytes: original.artifact.bytes().map_err(invalid)?.len() as u32,
                            retained_until: until,
                            deliver_by: (now + 300).min(until - 1),
                            readers: original.readers,
                            redistribute: true,
                            key_id: signer.key_id.clone(),
                            public_key: signer.public_key_b64(),
                            signature: String::new(),
                        };
                        offer.signature =
                            signer.sign_bytes(&offer.signing_bytes().map_err(invalid)?);
                        if offer
                            .validate(now)
                            .and_then(|_| offer.accepts(&original.artifact, now))
                            .is_err()
                        {
                            continue;
                        }
                        let series = contract::series(&original.artifact).map_err(invalid)?;
                        let mut tx = store::write_tx(&self.store.pool).await?;
                        self.forwarding_policy_on(&mut tx, &owner, peer, version as u64)
                            .await?;
                        match outbox::queue_on(
                            &mut tx,
                            outbox::Queue {
                                owner: &owner,
                                source,
                                mode,
                                policy_version: version as u64,
                                offer: &offer,
                                endpoint,
                                series: &series,
                                control: contract::control(&original.artifact),
                                now,
                            },
                            |hash| Some(signer.sign_hash(hash)),
                        )
                        .await
                        {
                            Ok(_) => tx.commit().await?,
                            Err(_) => {
                                tx.rollback().await?;
                            }
                        }
                    }
                }
                let cursor = if complete && rows.len() < 16 { 0 } else { last };
                self.evidence_custody
                    .scans
                    .lock()
                    .await
                    .insert((owner.clone(), custody), cursor);
                if !complete {
                    break 'owners;
                }
            }
        }
        let mut tx = store::write_tx(&self.store.pool).await?;
        outbox::cleanup_on(&mut tx, now).await?;
        let jobs = outbox::pending_on(&mut tx, now).await?;
        tx.commit().await?;
        let results = stream::iter(jobs)
            .map(|record| self.forward_one_evidence(record, &node))
            .buffer_unordered(4)
            .collect::<Vec<_>>()
            .await;
        Ok(results
            .into_iter()
            .filter(|result| matches!(result, Ok(true)))
            .count())
    }

    async fn forward_one_evidence(&self, record: String, node: &str) -> Result<bool, EngineError> {
        let Ok(_slot) = self.evidence_serving_slots.try_acquire() else {
            return Ok(false);
        };
        let mut tx = store::write_tx(&self.store.pool).await?;
        let job = outbox::get_on(&mut tx, &record).await?;
        if !outbox::claim_on(
            &mut tx,
            &record,
            &job.offer.hash().map_err(invalid)?,
            nucleus::execution::now().timestamp(),
        )
        .await?
        {
            return Ok(false);
        }
        tx.commit().await?;
        let operation = async {
            let original = self.validate_forwarding_job(&job, node).await?;
            let authority = self
                .serving_peer_authority(&job.offer.custodian, &job.endpoint)
                .await?;
            job.offer
                .accepts(&original.artifact, nucleus::execution::now().timestamp())
                .map_err(invalid)?;
            let body = original.artifact.bytes().map_err(invalid)?;
            let network = self.social_network()?;
            let started = network
                .request(
                    &job.endpoint,
                    PublicRequest::CustodyBegin {
                        offer: Box::new(job.offer.clone()),
                    },
                )
                .await?;
            let completed = if started.get("acknowledgment").is_some() {
                started
            } else {
                let upload = started["upload"]
                    .as_str()
                    .filter(|id| nucleus::valid_uid(id, "cust"))
                    .ok_or_else(|| invalid("Custodian returned an invalid upload reference"))?
                    .to_owned();
                let mut offset = started["offset"]
                    .as_u64()
                    .filter(|offset| *offset <= body.len() as u64)
                    .ok_or_else(|| invalid("Custodian returned an invalid resume offset"))?
                    as usize;
                while offset < body.len() {
                    self.validate_forwarding_job(&job, node).await?;
                    let end =
                        (offset + nucleus::trust_analysis::serving::CHUNK_BYTES).min(body.len());
                    let reply = network
                        .request(
                            &job.endpoint,
                            PublicRequest::CustodyChunk {
                                upload: upload.clone(),
                                offset: offset as u32,
                                bytes: STANDARD.encode(&body[offset..end]),
                            },
                        )
                        .await?;
                    if reply["offset"].as_u64() != Some(end as u64) {
                        return Err(invalid(
                            "Custodian progress differs from the exact sent chunk",
                        ));
                    }
                    let mut tx = store::write_tx(&self.store.pool).await?;
                    if !outbox::progress_on(
                        &mut tx,
                        &record,
                        &job.offer.hash().map_err(invalid)?,
                        end as u32,
                    )
                    .await?
                    {
                        return Err(invalid("Evidence delivery was cancelled during upload"));
                    }
                    tx.commit().await?;
                    offset = end;
                }
                self.validate_forwarding_job(&job, node).await?;
                network
                    .request(&job.endpoint, PublicRequest::CustodyFinish { upload })
                    .await?
            };
            let ack: Acknowledgment = serde_json::from_value(completed["acknowledgment"].clone())?;
            if !ack.matches(&job.offer) {
                return Err(invalid(
                    "Custodian acknowledged a different original or retention term",
                ));
            }
            self.custody_signature(
                &authority,
                &job.endpoint,
                &ack.key_id,
                &ack.public_key,
                &ack.signature,
                &ack.signing_bytes().map_err(invalid)?,
            )?;
            let mut tx = self.store.pool.begin().await?;
            self.social_ask_peer_on(&mut tx, &job.endpoint, &job.offer.custodian, true)
                .await?;
            self.custody_authority_unchanged_on(&mut tx, &job.offer.custodian, &authority)
                .await?;
            Ok::<_, EngineError>(ack)
        };
        let completed = tokio::time::timeout(Duration::from_secs(15), operation).await;
        let mut tx = store::write_tx(&self.store.pool).await?;
        let result = match &completed {
            Ok(Ok(ack)) => {
                outbox::result_on(
                    &mut tx,
                    &record,
                    &job.offer.hash().map_err(invalid)?,
                    Ok(ack),
                    nucleus::execution::now().timestamp(),
                )
                .await?
            }
            Ok(Err(error)) => {
                outbox::result_on(
                    &mut tx,
                    &record,
                    &job.offer.hash().map_err(invalid)?,
                    Err(&error.to_string()),
                    nucleus::execution::now().timestamp(),
                )
                .await?
            }
            Err(_) => {
                outbox::result_on(
                    &mut tx,
                    &record,
                    &job.offer.hash().map_err(invalid)?,
                    Err("Evidence delivery deadline exceeded"),
                    nucleus::execution::now().timestamp(),
                )
                .await?
            }
        };
        tx.commit().await?;
        Ok(result && matches!(completed, Ok(Ok(_))))
    }

    async fn validate_forwarding_job(
        &self,
        job: &Job,
        node: &str,
    ) -> Result<Original, EngineError> {
        let checked = self.check_forwarding_job(job, node).await;
        if let Err(error) = &checked {
            let mut tx = store::write_tx(&self.store.pool).await?;
            outbox::hold_on(&mut tx, &job.record, &error.to_string()).await?;
            tx.commit().await?;
        }
        checked
    }

    async fn check_forwarding_job(&self, job: &Job, node: &str) -> Result<Original, EngineError> {
        let consent = self.social_gossip_settings().await?;
        if !consent.enabled
            || !consent
                .peers
                .iter()
                .any(|peer| peer.organ == job.offer.custodian && peer.send)
        {
            return Err(invalid(
                "Current public-gossip consent no longer authorizes this delivery",
            ));
        }
        job.offer
            .validate(nucleus::execution::now().timestamp())
            .map_err(invalid)?;
        self.social_ask_peer(&job.endpoint, true).await?;
        self.serving_peer_authority(&job.offer.custodian, &job.endpoint)
            .await?;
        let local = self.serving_peer_authority(&job.offer.sender, node).await?;
        self.custody_signature(
            &local,
            node,
            &job.offer.key_id,
            &job.offer.public_key,
            &job.offer.signature,
            &job.offer.signing_bytes().map_err(invalid)?,
        )?;
        let mut tx = self.store.pool.begin().await?;
        self.contact_record_readable_on(&mut tx, &job.record, Some(&job.owner))
            .await?;
        self.contact_record_readable_on(&mut tx, &job.source, Some(&job.owner))
            .await?;
        self.forwarding_policy_on(
            &mut tx,
            &job.owner,
            &job.offer.custodian,
            job.policy_version,
        )
        .await?;
        let current = outbox::get_on(&mut tx, &job.record).await?;
        if !matches!(current.state.as_str(), "pending" | "sending") || current.offer != job.offer {
            return Err(invalid(
                "Reviewed evidence delivery was cancelled or superseded",
            ));
        }
        drop(tx);
        let original = self
            .forwarding_original(
                &job.owner,
                &job.source,
                job.mode,
                &job.offer.custodian,
                nucleus::execution::now(),
            )
            .await?;
        job.offer
            .accepts(&original.artifact, nucleus::execution::now().timestamp())
            .map_err(invalid)?;
        Ok(original)
    }
}
