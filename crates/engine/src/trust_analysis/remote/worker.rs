use super::*;
use futures::{StreamExt, stream};
use nucleus::{
    social::PublicRequest,
    trust_analysis::{discovery, serving},
};

pub(super) struct Counter {
    pub(super) bytes: usize,
    pub(super) limit: usize,
}
impl std::io::Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.bytes.saturating_add(bytes.len()) > self.limit {
            return Err(std::io::Error::other("Answer exceeds its frame budget"));
        }
        self.bytes += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Engine {
    pub async fn refresh_trust_requests_once(&self) -> Result<usize, EngineError> {
        let Ok(_worker) = self.trust_remote.worker.try_lock() else {
            return Ok(0);
        };
        let pending = {
            let mut jobs = self.trust_remote.jobs.lock().await;
            for job in jobs.values_mut() {
                job.expire();
            }
            jobs.retain(|_, job| job.created.elapsed() < Duration::from_secs(900));
            jobs.iter_mut()
                .filter_map(|(id, job)| {
                    if job.state == State::Queued && job.active() {
                        job.state = State::Running;
                        Some((
                            id.clone(),
                            job.review.clone(),
                            job.actor.clone(),
                            job.deadline,
                        ))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };
        let count = pending.len();
        stream::iter(pending)
            .map(|(id, review, actor, deadline)| async move {
                let work = stream::iter(review.endpoints.clone())
                    .map(|peer| {
                        let id = &id;
                        let review = &review;
                        let actor = actor.as_deref();
                        async move {
                            let result = self.remote_peer_work(id, review, actor, &peer).await;
                            if let Some(job) = self.trust_remote.jobs.lock().await.get_mut(id) {
                                let state = job.state;
                                if let Some(progress) = job
                                    .progress
                                    .iter_mut()
                                    .find(|progress| progress.endpoint == peer.endpoint)
                                {
                                    progress.state = match state {
                                        State::Cancelled => "cancelled",
                                        State::Expired => "expired",
                                        _ => {
                                            if result.is_ok() {
                                                "answered"
                                            } else {
                                                "failed"
                                            }
                                        }
                                    }
                                    .into();
                                    progress.error = result.as_ref().err().map(ToString::to_string);
                                }
                                job.partial |= result.is_err();
                            }
                        }
                    })
                    .buffer_unordered(4)
                    .collect::<Vec<_>>();
                let timed =
                    tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), work).await;
                if let Some(job) = self.trust_remote.jobs.lock().await.get_mut(&id) {
                    job.expire();
                    job.reserved = 0;
                    if job.state == State::Running {
                        job.state = if timed.is_ok() {
                            State::Completed
                        } else {
                            State::Expired
                        };
                        job.partial |= timed.is_err();
                    }
                }
                self.notify_query_changed();
            })
            .buffer_unordered(4)
            .collect::<Vec<_>>()
            .await;
        Ok(count)
    }

    async fn remote_io(
        &self,
        id: &str,
        review: &Review,
        actor: Option<&str>,
        peer: &Endpoint,
        message: PublicRequest,
        limit: usize,
    ) -> Result<Value, EngineError> {
        let _slot = self
            .trust_remote
            .slots
            .acquire()
            .await
            .map_err(|_| invalid("Query work stopped"))?;
        if !self.remote_active(id).await {
            return Err(invalid("Cancelled or expired query"));
        }
        self.remote_access(review, actor, Some(peer)).await?;
        let settings = self.social_settings().await?;
        let sent = serde_json::to_vec(&message)?.len();
        let reserve = nucleus::social::MAX_FRAME_BYTES;
        let deadline = {
            let mut jobs = self.trust_remote.jobs.lock().await;
            let job = jobs
                .get_mut(id)
                .ok_or_else(|| invalid("Query was cleared"))?;
            if !job.active()
                || job.requests >= 128
                || job
                    .spent
                    .saturating_add(job.reserved)
                    .saturating_add(reserve)
                    .saturating_add(sent)
                    > nucleus::trust_analysis::MAX_BYTES
            {
                job.partial = true;
                return Err(invalid("The query traffic or request budget is exhausted"));
            }
            job.requests += 1;
            job.reserved += reserve + sent;
            job.deadline
        };
        let mut sent_counted = false;
        let mut received_counted = false;
        let response=async {
            store::social::spend(&self.store.pool,&peer.endpoint,"out",sent,settings.outgoing_bytes_per_minute,nucleus::execution::now().timestamp()).await?;
            self.remote_access(review,actor,Some(peer)).await?;
            let network=self.social_network()?;
            {
                let mut jobs=self.trust_remote.jobs.lock().await;
                let job=jobs.get_mut(id).ok_or_else(||invalid("Query was cleared"))?;
                if !job.active() {return Err(invalid("Cancelled or expired query"));}
                job.sent+=sent;job.spent+=sent;job.reserved=job.reserved.saturating_sub(sent);sent_counted=true;
            }
            let request=network.request(&peer.endpoint,message);
            tokio::pin!(request);
            let mut heartbeat=tokio::time::interval(Duration::from_millis(100));
            let value=loop {
                tokio::select! {
                    result=&mut request=>break result?,
                    _=heartbeat.tick()=>if !self.remote_active(id).await {return Err(invalid("Cancelled, closed or expired query"));},
                    _=tokio::time::sleep_until(tokio::time::Instant::from_std(deadline))=>return Err(invalid("Remote query deadline ended")),
                }
            };
            let mut counter=Counter {bytes:0,limit:reserve};
            let encoded=serde_json::to_writer(&mut counter,&value);
            let received=if encoded.is_ok(){counter.bytes}else{reserve};
            if let Some(job)=self.trust_remote.jobs.lock().await.get_mut(id) {job.bytes+=received;job.spent+=received;job.reserved=job.reserved.saturating_sub(reserve);received_counted=true;}
            encoded.map_err(|_|invalid("The answer exceeds the social frame budget"))?;
            store::social::spend(&self.store.pool,&peer.endpoint,"in",received,settings.incoming_bytes_per_minute,nucleus::execution::now().timestamp()).await?;
            if received>limit {return Err(invalid("The answer exceeds its assigned frame budget"));}
            if value["service"].as_str()!=Some(peer.endpoint.as_str()) {return Err(invalid("The answer does not match its pinned endpoint"));}
            self.remote_access(review,actor,Some(peer)).await?;
            if !self.remote_active(id).await {return Err(invalid("Cancelled or expired query"));}
            Ok::<_,EngineError>(value)
        }.await;
        let mut jobs = self.trust_remote.jobs.lock().await;
        if let Some(job) = jobs.get_mut(id) {
            job.reserved = job.reserved.saturating_sub(
                if received_counted { 0 } else { reserve } + if sent_counted { 0 } else { sent },
            );
        }
        response
    }

    async fn remote_peer_work(
        &self,
        id: &str,
        review: &Review,
        actor: Option<&str>,
        peer: &Endpoint,
    ) -> Result<(), EngineError> {
        if let Some(job) = self.trust_remote.jobs.lock().await.get_mut(id) {
            if let Some(progress) = job
                .progress
                .iter_mut()
                .find(|progress| progress.endpoint == peer.endpoint)
            {
                progress.state = "contacting".into();
            }
        }
        for question in &review.questions {
            let now = nucleus::execution::now().timestamp();
            let remaining = {
                let jobs = self.trust_remote.jobs.lock().await;
                let job = jobs.get(id).ok_or_else(|| invalid("Query was cleared"))?;
                job.deadline
                    .saturating_duration_since(Instant::now())
                    .as_secs()
                    .clamp(1, 15)
            };
            let mut request = discovery::Request {
                id: nucleus::new_uid("ask"),
                question: question.clone(),
                issued_at: now,
                deadline: now + remaining as i64,
                after: None,
            };
            let mut cursors = BTreeSet::new();
            let mut finished = false;
            for _ in 0..32 {
                let response = self
                    .remote_io(
                        id,
                        review,
                        actor,
                        peer,
                        PublicRequest::DiscoverEvidence {
                            document: Box::new(request.clone()),
                        },
                        16384,
                    )
                    .await?;
                let page: discovery::Page = serde_json::from_value(response["page"].clone())?;
                page.validate(&request, nucleus::execution::now().timestamp())
                    .map_err(invalid)?;
                for header in page.headers {
                    let already = self
                        .trust_remote
                        .jobs
                        .lock()
                        .await
                        .get(id)
                        .is_some_and(|job| job.artifacts.contains_key(&header.hash));
                    if already {
                        continue;
                    }
                    let mut chunks = Vec::new();
                    let mut offset = 0;
                    while offset < header.bytes {
                        let response = self
                            .remote_io(
                                id,
                                review,
                                actor,
                                peer,
                                PublicRequest::FetchEvidence {
                                    hash: header.hash.clone(),
                                    offset,
                                },
                                serving::CHUNK_BYTES.div_ceil(3) * 4 + 4096,
                            )
                            .await?;
                        let chunk: serving::Chunk =
                            serde_json::from_value(response["chunk"].clone())?;
                        if chunk.total != header.bytes {
                            return Err(invalid("The artifact differs from its discovered size"));
                        }
                        let bytes = chunk.decode(&header.hash, offset).map_err(invalid)?;
                        offset += bytes.len() as u32;
                        chunks.push(chunk);
                    }
                    let artifact = serving::assemble(
                        &header.hash,
                        &chunks,
                        nucleus::execution::now().timestamp(),
                    )
                    .map_err(invalid)?;
                    if artifact.kind() != header.kind || !question.matches(&artifact) {
                        return Err(invalid(
                            "The returned artifact does not answer the reviewed question",
                        ));
                    }
                    self.remote_packet_access(review, actor, &artifact).await?;
                    let stamp = self.remote_stamp(&artifact).await?;
                    let checked_at = nucleus::execution::now();
                    let signature = self.remote_signature(&artifact, checked_at).await;
                    if signature == Verification::Invalid {
                        return Err(invalid("The original artifact signature is invalid"));
                    }
                    self.remote_access(review, actor, Some(peer)).await?;
                    self.remote_packet_access(review, actor, &artifact).await?;
                    if self.remote_stamp(&artifact).await? != stamp {
                        return Err(invalid(
                            "Identity or evidence controls changed while checking the answer",
                        ));
                    }
                    let events = match &artifact {
                        Artifact::Receipt(document) => document
                            .public_bundle()
                            .map(|bundle| bundle.publication.events.len())
                            .unwrap_or_default()
                            .max(1),
                        _ => 1,
                    };
                    let mut identities: BTreeSet<_> =
                        packet_profiles(&artifact).into_iter().collect();
                    let bytes = artifact.bytes().map_err(invalid)?.len();
                    let mut jobs = self.trust_remote.jobs.lock().await;
                    let held = jobs
                        .values()
                        .flat_map(|job| job.artifacts.values())
                        .map(|found| found.bytes)
                        .sum::<usize>();
                    let job = jobs
                        .get_mut(id)
                        .ok_or_else(|| invalid("Query was cleared"))?;
                    if !job.active() {
                        return Err(invalid("Cancelled or expired query"));
                    }
                    if let Some(found) = job.artifacts.get_mut(&header.hash) {
                        found.sources.insert(peer.organ.clone());
                        continue;
                    }
                    identities.extend(job.identities.iter().cloned());
                    if held.saturating_add(bytes) > nucleus::trust_analysis::MAX_BYTES
                        || identities.len() > nucleus::trust_analysis::MAX_IDENTITIES
                        || job.events.saturating_add(events)
                            > nucleus::trust_analysis::MAX_DISCLOSURES
                    {
                        job.partial = true;
                        return Err(invalid("The query evidence budget was reached"));
                    }
                    job.events += events;
                    job.identities = identities;
                    let expires_at = header
                        .expires_at
                        .min(artifact.expires_at().map_err(invalid)?);
                    job.artifacts.insert(
                        header.hash,
                        Found {
                            artifact: Arc::new(artifact),
                            sources: BTreeSet::from([peer.organ.clone()]),
                            signature,
                            checked_at: checked_at.timestamp(),
                            expires_at,
                            bytes,
                        },
                    );
                    if let Some(progress) = job
                        .progress
                        .iter_mut()
                        .find(|progress| progress.endpoint == peer.endpoint)
                    {
                        progress.documents += 1;
                    }
                }
                if let Some(cursor) = page.next {
                    if !cursors.insert(cursor.clone()) {
                        return Err(invalid("The source repeated its discovery cursor"));
                    }
                    request.after = Some(cursor);
                } else {
                    finished = true;
                    if page.partial {
                        if let Some(job) = self.trust_remote.jobs.lock().await.get_mut(id) {
                            job.partial = true;
                        }
                    }
                    break;
                }
            }
            if !finished {
                return Err(invalid("The source exceeded its bounded page work"));
            }
            if let Some(job) = self.trust_remote.jobs.lock().await.get_mut(id) {
                if let Some(progress) = job
                    .progress
                    .iter_mut()
                    .find(|progress| progress.endpoint == peer.endpoint)
                {
                    progress.questions += 1;
                }
            }
        }
        Ok(())
    }
}
