use super::*;

impl Engine {
    pub(in crate::trust_analysis) async fn remote_trust_command(
        &self,
        person: &str,
        request_id: &str,
        command: &Command,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        if let Command::Preview { draft } = command {
            draft.questions().map_err(invalid)?;
            let evaluated = self
                .evaluate_trust_query(person, actor, &draft.query, draft.as_organ, now)
                .await?;
            let endpoints = match self.remote_endpoints(draft, actor).await {
                Ok(endpoints) if !endpoints.is_empty() => endpoints,
                result => {
                    return Ok(ActionOutcome {
                        data: Some(
                            json!({"trust":{"kind":"remote_unavailable","local":evaluated.analysis,"proof_records":evaluated.proof_records,"message":result.err().map(|e|e.to_string()).unwrap_or_else(||"No readable pinned contacts match this query's endpoint policy and Ask consent".into())}}),
                        ),
                        ..Default::default()
                    });
                }
            };
            let mut tx = self.store.pool.begin().await?;
            self.contact_reader_on(&mut tx, person, actor, draft.as_organ)
                .await?;
            let profiles = self
                .trust_profiles_on(
                    &mut tx,
                    std::iter::once(Identity::Person(person.into()))
                        .chain(std::iter::once(draft.query.subject.clone()))
                        .chain(
                            draft
                                .linked_claims
                                .iter()
                                .map(|series| series.issuer.clone()),
                        )
                        .chain(draft.contact_subjects.iter().cloned())
                        .chain(
                            draft
                                .receipt_sources
                                .iter()
                                .map(|source| Identity::Organ(source.origin.clone())),
                        )
                        .collect(),
                    actor,
                )
                .await?;
            let review = Review {
                nonce: nucleus::new_uid("ask"),
                person: person.into(),
                draft: draft.clone(),
                questions: draft.questions().map_err(invalid)?,
                endpoints,
                profiles,
                reviewed_at: nucleus::execution::now().timestamp(),
            };
            if *self.watch_query_changes().borrow() != evaluated.revision {
                return Err(invalid(
                    "Local evidence changed during review; refresh the local snapshot",
                ));
            }
            drop(tx);
            self.remote_access(&review, actor, None).await?;
            let hash = review.hash().map_err(invalid)?;
            let mut reviews = self.trust_remote.reviews.lock().await;
            reviews.retain(|_, issued| Instant::now() < issued.expires);
            if reviews.len() >= model::MAX_REVIEWS {
                return Err(invalid(
                    "Too many pending question reviews; let an older review expire",
                ));
            }
            reviews.insert(
                review.nonce.clone(),
                Issued {
                    review: review.clone(),
                    actor: actor.map(str::to_owned),
                    expires: Instant::now() + Duration::from_secs(300),
                },
            );
            return Ok(ActionOutcome {
                data: Some(
                    json!({"trust":{"kind":"remote_review","review":review,"hash":hash,"local":evaluated.analysis,"proof_records":evaluated.proof_records,"checked_at":review.reviewed_at}}),
                ),
                ..Default::default()
            });
        }
        if let Command::Start {
            review,
            expected_review,
        } = command
        {
            if review.person != person || review.hash().map_err(invalid)? != *expected_review {
                return Err(invalid(
                    "Start only the exact questions reviewed by this Person",
                ));
            }
            {
                let jobs = self.trust_remote.jobs.lock().await;
                if let Some((id, job)) = jobs.iter().find(|(_, job)| job.request_id == request_id) {
                    if job.review.as_ref() != review.as_ref() || job.actor.as_deref() != actor {
                        return Err(invalid("This request ID already binds another query"));
                    }
                    return Ok(ActionOutcome {
                        data: Some(
                            json!({"trust":{"kind":"remote_started","job":id,"state":job.state,"replayed":true}}),
                        ),
                        ..Default::default()
                    });
                }
            }
            {
                let reviews = self.trust_remote.reviews.lock().await;
                let issued = reviews.get(&review.nonce).ok_or_else(|| {
                    invalid("This local review expired or was consumed; preview again")
                })?;
                if issued.review != **review
                    || issued.actor.as_deref() != actor
                    || Instant::now() >= issued.expires
                {
                    return Err(invalid(
                        "The original review or reader changed; preview again",
                    ));
                }
            }
            self.remote_access(review, actor, None).await?;
            if review.reviewed_at > now.timestamp()
                || now.timestamp().saturating_sub(review.reviewed_at) > 300
                || self.remote_endpoints(&review.draft, actor).await? != review.endpoints
            {
                return Err(invalid(
                    "The reviewed endpoints or scope changed; preview again",
                ));
            }
            let mut tx = self.store.pool.begin().await?;
            let profiles = self
                .trust_profiles_on(
                    &mut tx,
                    review
                        .profiles
                        .iter()
                        .map(|profile| profile.identity.clone())
                        .collect(),
                    actor,
                )
                .await?;
            if profiles != review.profiles {
                return Err(invalid("A reviewed profile changed; preview again"));
            }
            drop(tx);
            let mut reviews = self.trust_remote.reviews.lock().await;
            let issued = reviews
                .get(&review.nonce)
                .ok_or_else(|| invalid("This local review was already consumed"))?;
            if Instant::now() >= issued.expires {
                return Err(invalid("This local review expired"));
            }
            let mut jobs = self.trust_remote.jobs.lock().await;
            for job in jobs.values_mut() {
                job.expire();
            }
            jobs.retain(|_, job| job.created.elapsed() < Duration::from_secs(900));
            if jobs.len() >= model::MAX_JOBS
                || jobs.values().filter(|job| job.active()).count() >= 4
            {
                return Err(invalid(
                    "Keep at most four active and eight retained queries; forget finished work to continue",
                ));
            }
            let id = review.nonce.clone();
            let instant = Instant::now();
            jobs.insert(
                id.clone(),
                Job {
                    review: Arc::new(review.as_ref().clone()),
                    actor: actor.map(str::to_owned),
                    request_id: request_id.into(),
                    state: State::Queued,
                    created: instant,
                    deadline: instant + Duration::from_secs(model::DEADLINE_SECONDS),
                    lease: instant + Duration::from_secs(model::LEASE_SECONDS),
                    progress: review
                        .endpoints
                        .iter()
                        .map(|peer| Progress {
                            organ: peer.organ.clone(),
                            endpoint: peer.endpoint.clone(),
                            state: "queued".into(),
                            questions: 0,
                            documents: 0,
                            error: None,
                        })
                        .collect(),
                    artifacts: BTreeMap::new(),
                    bytes: 0,
                    sent: 0,
                    spent: 0,
                    reserved: 0,
                    requests: 0,
                    events: 0,
                    identities: BTreeSet::new(),
                    partial: false,
                },
            );
            reviews.remove(&review.nonce);
            drop(jobs);
            drop(reviews);
            self.notify_query_changed();
            return Ok(ActionOutcome {
                data: Some(
                    json!({"trust":{"kind":"remote_started","job":id,"state":State::Queued,"replayed":false}}),
                ),
                ..Default::default()
            });
        }
        let id = match command {
            Command::Poll { job }
            | Command::Verify { job }
            | Command::Cancel { job }
            | Command::Inspect { job, .. }
            | Command::Forget { job } => job,
            _ => return Err(invalid("Choose a remote-query operation")),
        };
        if matches!(command, Command::Cancel { .. } | Command::Forget { .. }) {
            let mut reviews = self.trust_remote.reviews.lock().await;
            let reviewed = reviews.get(id).is_some_and(|issued| {
                issued.review.person == person && issued.actor.as_deref() == actor
            });
            let mut jobs = self.trust_remote.jobs.lock().await;
            if let Some(job) = jobs.get_mut(id) {
                if job.review.person != person || job.actor.as_deref() != actor {
                    return Err(invalid("Cancel only your own query"));
                }
                if matches!(command, Command::Forget { .. }) {
                    jobs.remove(id);
                } else {
                    job.stop();
                }
            } else if !reviewed {
                return Err(invalid("This transient query is no longer retained"));
            }
            if reviewed {
                reviews.remove(id);
            }
            drop(jobs);
            drop(reviews);
            self.notify_query_changed();
            return Ok(ActionOutcome {
                data: Some(
                    json!({"trust":{"kind":"remote_cancelled","job":id,"forgotten":matches!(command,Command::Forget {..})}}),
                ),
                ..Default::default()
            });
        }
        let (review, owner) = {
            let jobs = self.trust_remote.jobs.lock().await;
            let job = jobs
                .get(id)
                .ok_or_else(|| invalid("This transient query is no longer retained"))?;
            if job.review.person != person || job.actor.as_deref() != actor {
                return Err(invalid("Read only your own query"));
            }
            (job.review.clone(), job.actor.clone())
        };
        if let Err(error) = self.remote_access(&review, owner.as_deref(), None).await {
            if let Some(job) = self.trust_remote.jobs.lock().await.get_mut(id) {
                job.stop();
            }
            return Err(error);
        }
        if matches!(command, Command::Verify { .. }) {
            return self.verify_remote_evidence(id, &review, actor).await;
        }
        if let Command::Inspect { artifact, .. } = command {
            let found = {
                let jobs = self.trust_remote.jobs.lock().await;
                jobs.get(id)
                    .and_then(|job| job.artifacts.get(artifact))
                    .cloned()
                    .ok_or_else(|| invalid("This transient artifact is no longer available"))?
            };
            if found.expires_at <= nucleus::execution::now().timestamp() {
                return Err(invalid("This artifact's shared term expired"));
            }
            self.remote_packet_access(&review, actor, &found.artifact)
                .await?;
            let stamp = self.remote_stamp(&found.artifact).await?;
            let checked_at = nucleus::execution::now();
            let signature = self.remote_signature(&found.artifact, checked_at).await;
            self.remote_access(&review, actor, None).await?;
            self.remote_packet_access(&review, actor, &found.artifact)
                .await?;
            if self.remote_stamp(&found.artifact).await? != stamp {
                return Err(invalid(
                    "Identity or evidence controls changed; reopen the original artifact",
                ));
            }
            if !self
                .trust_remote
                .jobs
                .lock()
                .await
                .get(id)
                .is_some_and(|job| job.artifacts.contains_key(artifact))
            {
                return Err(invalid("The query was cancelled or cleared"));
            }
            return Ok(ActionOutcome {
                data: Some(
                    json!({"trust":{"kind":"remote_artifact","job":id,"artifact":found.artifact.as_ref(),"hash":artifact,"signature_state":signature,"checked_at":checked_at.timestamp(),"sources":found.sources,"notice":"Original evidence, without import or accounting effects. Signature recognition is separate from reliance, complete corrections and physical delivery."}}),
                ),
                ..Default::default()
            });
        }
        let entries = {
            let mut jobs = self.trust_remote.jobs.lock().await;
            let job = jobs
                .get_mut(id)
                .ok_or_else(|| invalid("This transient query was cleared"))?;
            job.expire();
            if job.active() {
                job.lease = Instant::now() + Duration::from_secs(model::LEASE_SECONDS);
            }
            job.artifacts
                .retain(|_, found| found.expires_at > nucleus::execution::now().timestamp());
            job.artifacts
                .iter()
                .map(|(hash, found)| (hash.clone(), found.clone()))
                .collect::<Vec<_>>()
        };
        let mut headers = Vec::new();
        for (hash, found) in entries {
            if self
                .remote_packet_access(&review, actor, &found.artifact)
                .await
                .is_ok()
            {
                headers.push(json!({"hash":hash,"kind":found.artifact.kind(),"sources":found.sources,"signature_snapshot":found.signature,"checked_at":found.checked_at}));
            }
        }
        let jobs = self.trust_remote.jobs.lock().await;
        let job = jobs
            .get(id)
            .ok_or_else(|| invalid("This transient query was cleared"))?;
        headers.retain(|header| {
            header["hash"]
                .as_str()
                .is_some_and(|hash| job.artifacts.keys().any(|key| key.as_str() == hash))
        });
        Ok(ActionOutcome {
            data: Some(
                json!({"trust":{"kind":"remote_progress","job":id,"state":job.state,"endpoints":job.progress,"bytes":job.bytes,"sent_bytes":job.sent,"requests":job.requests,"events":job.events,"identities":job.identities.len(),"elapsed_ms":job.created.elapsed().as_millis() as u64,"partial":job.partial || !matches!(job.state,State::Completed),"artifacts":headers,"notice":"Raw permitted documents, separate from issuer/counterparty/path analysis and contribution totals. Signature states are dated snapshots; open a document to recheck. Remote quantities are not yet combined with your local analysis."}}),
            ),
            ..Default::default()
        })
    }
}
