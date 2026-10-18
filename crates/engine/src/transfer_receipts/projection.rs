use super::*;
use nucleus::transfer::receipt::{
    Content, Document, Event, EventRequest, EventSource, SignedEvent, Stage,
};
use store::transfer_evidence::SavedOutcome;

pub(super) struct PreparedEvent {
    pub bundle: Bundle,
    pub event: Event,
    pub signer: crate::trust::Signer,
    pub fragment: bool,
    pub source_revision: Option<u64>,
}

impl PreparedEvent {
    pub fn hash(&self) -> Result<CanonicalHash, EngineError> {
        canonical_hash(
            "lince.transfer.receipt.projection.v1",
            &(
                &self.bundle,
                &self.event,
                self.fragment,
                self.source_revision,
            ),
        )
        .map_err(|error| invalid(error.to_string()))
    }
}

pub(super) fn measurement(
    scope: &Scope,
    measure: &nucleus::transfer::outcome::Measure,
) -> Result<Measurement, EngineError> {
    measure.validate().map_err(invalid)?;
    Ok(Measurement {
        meaning: measure.meaning.clone(),
        unit: measure.unit.clone(),
        object_commitment: measure
            .object
            .as_ref()
            .map(|object| {
                canonical_hash(
                    "lince.transfer.receipt.object.v1",
                    &(&scope.source.origin, object),
                )
                .map_err(|error| invalid(error.to_string()))
            })
            .transpose()?,
    })
}

pub(super) fn need_content(
    scope: &Scope,
    terms: &nucleus::transfer::outcome::NeedTerms,
) -> Result<Content, EngineError> {
    Ok(Content::Need {
        measurement: measurement(scope, &terms.measure)?,
        requested: terms.requested,
        need_commitment: canonical_hash(
            "lince.transfer.receipt.need.v1",
            &(
                &scope.source.origin,
                &terms.selection.need,
                terms.selection.expected_need_revision,
            ),
        )
        .map_err(|error| invalid(error.to_string()))?,
    })
}

pub(super) fn public_predecessor(
    bundle: &Bundle,
    fact_hash: &str,
    content: &Content,
    missing: &str,
) -> Result<CanonicalHash, EngineError> {
    let source = Source::fact_commitment(fact_hash).map_err(invalid)?;
    let candidates: Vec<_> = bundle
        .publication
        .events
        .iter()
        .filter(|signed| {
            signed.event.source_commitment == source && &signed.event.content == content
        })
        .collect();
    if candidates.len() != 1 {
        return Err(proof_shape_error(missing.into()));
    }
    candidates[0].event.id().map_err(invalid)
}

impl Engine {
    pub(super) async fn receipt_event_signer(
        &self,
        record: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<(Authority, crate::trust::Signer), EngineError> {
        let record = self.resolve(record).await?;
        self.refuse_unreadable(actor, std::slice::from_ref(&record))
            .await?;
        let saved = store::transfer_receipts::get(&self.store.pool, &record).await?;
        self.verify_transfer_receipt_document(&saved.document, now)
            .await?;
        let Document::Authority(authority) = saved.document else {
            return Err(invalid("Choose receipt signing authority"));
        };
        if authority.participation.is_some() {
            return Err(invalid(
                "Choose receipt signing authority, not original participation approval",
            ));
        }
        let signer = self
            .organ_signer
            .lock()
            .await
            .clone()
            .ok_or_else(|| invalid("This device has no Organ signing key"))?;
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        if authority.organ != local.uid
            || signer.actor_uid != local.uid
            || authority.key_id != signer.key_id
            || authority.signing_key != signer.public_key_b64()
            || now.timestamp() < authority.issued_at
            || now.timestamp() >= authority.expires_at
        {
            return Err(invalid(
                "Refresh authority for this device's current receipt signing key",
            ));
        }
        Ok((authority, signer))
    }

    async fn publication_outcome(
        &self,
        record: &str,
        occurrence: &store::transfers::TransferOccurrenceRow,
        actor: Option<&str>,
    ) -> Result<SavedOutcome, EngineError> {
        self.refuse_unreadable(actor, &[record.to_owned()]).await?;
        let outcome = store::transfer_evidence::get(&self.store.pool, record).await?;
        let author = match &outcome.evidence.content {
            OutcomeContent::NeedBinding { .. }
            | OutcomeContent::Statement {
                role: nucleus::transfer::outcome::Role::Receiver,
                ..
            } => &occurrence.receiver_person_uid,
            _ => &occurrence.giver_person_uid,
        };
        if outcome.evidence.occurrence != occurrence.uid
            || outcome.evidence.transfer != occurrence.transfer_uid
            || outcome.evidence.agreement_revision != occurrence.revision
            || &outcome.evidence.author != author
            || !outcome.fact.delta.is_zero()
            || !crate::trust::verify_fact(&self.store, &outcome.fact).await?
        {
            return Err(invalid(
                "Outcome publication requires verified original party evidence",
            ));
        }
        let mut dependencies = Vec::new();
        if let Some(object) = &outcome.evidence.content.measure().object {
            dependencies.push(object.clone());
        }
        if let OutcomeContent::NeedBinding { terms, .. } = &outcome.evidence.content {
            dependencies.push(terms.selection.need.clone());
        }
        self.refuse_unreadable(actor, &dependencies).await?;
        Ok(outcome)
    }

    async fn outcome_public_content(
        &self,
        outcome: &SavedOutcome,
        bundle: &Bundle,
        occurrence: &store::transfers::TransferOccurrenceRow,
        actor: Option<&str>,
    ) -> Result<Content, EngineError> {
        match &outcome.evidence.content {
            OutcomeContent::NeedBinding { terms, .. } => {
                need_content(&bundle.publication.scope, terms)
            }
            OutcomeContent::Statement {
                role,
                measure,
                quantity,
                withdrawn,
                need_binding,
                ..
            } => {
                let need = if let Some(binding) = need_binding {
                    let binding = self.publication_outcome(binding, occurrence, actor).await?;
                    let OutcomeContent::NeedBinding { terms, .. } = &binding.evidence.content
                    else {
                        return Err(invalid(
                            "Statement requires a receiver-authored Need binding",
                        ));
                    };
                    if terms.measure != *measure {
                        return Err(invalid(
                            "Statement measurement differs from its Need binding",
                        ));
                    }
                    Some(public_predecessor(
                        bundle,
                        &binding.fact.hash,
                        &need_content(&bundle.publication.scope, terms)?,
                        "Receipt statement is missing its Need proof",
                    )?)
                } else {
                    None
                };
                Ok(Content::Statement {
                    role: *role,
                    measurement: measurement(&bundle.publication.scope, measure)?,
                    quantity: *quantity,
                    need,
                    withdrawn: *withdrawn,
                })
            }
        }
    }

    pub(super) async fn prepare_receipt_event(
        &self,
        person: &str,
        request: &EventRequest,
        reviewed_at: i64,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<PreparedEvent, EngineError> {
        if reviewed_at > now.timestamp() || now.timestamp().saturating_sub(reviewed_at) > 300 {
            return Err(invalid(
                "Review the current event projection before signing",
            ));
        }
        let uid = self.resolve(&request.record).await?;
        self.refuse_unreadable(actor, std::slice::from_ref(&uid))
            .await?;
        let saved = store::transfer_receipts::get(&self.store.pool, &uid).await?;
        if saved.owner != person || saved.version != request.expected_version {
            return Err(invalid(
                "Review your own current receipt version before publishing",
            ));
        }
        self.verify_transfer_receipt_document(&saved.document, now)
            .await?;
        let mut bundle = match &saved.document {
            Document::Review(review) => review.as_bundle(),
            Document::Publication(bundle) | Document::ProofFragment(bundle) => bundle.clone(),
            _ => return Err(invalid("Choose an approved receipt scope")),
        };
        let fragment_base = matches!(saved.document, Document::ProofFragment(_));
        if fragment_base {
            bundle
                .validate_fragment(now.timestamp())
                .map_err(proof_shape_error)?;
        } else {
            bundle
                .validate(now.timestamp())
                .map_err(proof_shape_error)?;
        }
        if !fragment_base
            && let Some(existing) = store::transfer_receipts::find(
                &self.store.pool,
                person,
                &Document::Publication(bundle.clone()),
            )
            .await?
        {
            self.refuse_unreadable(actor, std::slice::from_ref(&existing.record))
                .await?;
            self.verify_transfer_receipt_document(&existing.document, now)
                .await?;
            let Document::Publication(merged) = existing
                .document
                .merge(&Document::Publication(bundle), now.timestamp())
                .map_err(invalid)?
            else {
                unreachable!()
            };
            bundle = merged;
        }
        let context = super::collection::Collection::scope_context(
            self,
            &bundle.publication.scope,
            actor,
            now,
        )
        .await?;
        let mut fragment = fragment_base || context.has_fragments() || context.partial;
        let mut header = bundle.clone();
        header.publication.events.clear();
        header.authorities.retain(|authority| {
            header.publication.approvals.iter().any(|approval| {
                header.publication.scope.organ(approval.role) == authority.organ
                    && approval.key_id == authority.key_id
            })
        });
        context.extend_bundle(&mut bundle)?;
        if bundle.publication.scope.expires_at <= now.timestamp() {
            return Err(invalid("The approved publication term has expired"));
        }
        let mut source_revision = None;
        let (content, source_commitment, recorded_at, supersedes) = if let EventSource::Snapshot {
            record,
        } = &request.source
        {
            self.project_receipt_snapshot(person, record, &bundle, actor)
                .await?
        } else {
            let mut source = store::transfer_receipts::local_source(&self.store.pool, &uid).await?;
            if source.is_none() {
                source =
                    store::transfer_receipts::source_for_scope(&self.store.pool, &saved.document)
                        .await?;
            }
            let mut source = source.ok_or_else(|| invalid("Local source evidence is unavailable; import the receiver's minimal proof rather than a foreign Transfer"))?;
            let occurrence = store::transfers::occurrence(&self.store.pool, &source.occurrence)
                .await?
                .ok_or_else(|| invalid("Original occurrence is unavailable"))?;
            self.require_permission(actor, "transfer:read").await?;
            self.refuse_unreadable(actor, std::slice::from_ref(&occurrence.transfer_uid))
                .await?;
            let current = store::transfers::get(&self.store.pool, &occurrence.transfer_uid)
                .await?
                .ok_or_else(|| invalid("Transfer is unavailable"))?;
            source.expected_revision = u64::try_from(current.revision)
                .map_err(|_| invalid("Invalid Transfer revision"))?;
            source_revision = Some(source.expected_revision);
            source.outcome_records.clear();
            source.stages = vec![Stage::Settled];
            let scope = &bundle.publication.scope;
            let original = self
                .prepare_transfer_receipt_scope(
                    source,
                    actor,
                    DateTime::from_timestamp(scope.issued_at, 0)
                        .ok_or_else(|| invalid("Invalid scope time"))?,
                )
                .await?;
            if original.source != scope.source
                || original.giver != scope.giver
                || original.receiver != scope.receiver
                || original.party_attribution != scope.party_attribution
                || original.original_authorities != scope.original_authorities
                || original.agreed_quantity != scope.agreed_quantity
                || original.canonical_unit != scope.canonical_unit
            {
                return Err(invalid("Original source differs from the approved scope"));
            }
            let (content, source_commitment, recorded_at, supersedes) = match &request.source {
                EventSource::Snapshot { .. } => unreachable!(),
                EventSource::Outcome { record } => {
                    let record = self.resolve(record).await?;
                    let outcome = self
                        .publication_outcome(&record, &occurrence, actor)
                        .await?;
                    if outcome.evidence.author != person {
                        return Err(EngineError::Forbidden(
                            "Only the actual outcome author may publish this role".into(),
                        ));
                    }
                    let content = self
                        .outcome_public_content(&outcome, &bundle, &occurrence, actor)
                        .await?;
                    let supersedes = if let Some(previous) = outcome.evidence.content.supersedes() {
                        let previous = self
                            .publication_outcome(previous, &occurrence, actor)
                            .await?;
                        let content = self
                            .outcome_public_content(&previous, &bundle, &occurrence, actor)
                            .await?;
                        Some(public_predecessor(
                            &bundle,
                            &previous.fact.hash,
                            &content,
                            "Receipt correction history is incomplete",
                        )?)
                    } else {
                        None
                    };
                    (
                        content,
                        Source::fact_commitment(&outcome.fact.hash).map_err(invalid)?,
                        outcome.fact.at.timestamp(),
                        supersedes,
                    )
                }
                EventSource::Dispute { fact, supersedes } => {
                    let fact = store::facts::get(&self.store.pool, fact)
                        .await?
                        .ok_or_else(|| invalid("Dispute Fact is unavailable"))?;
                    if fact.record_uid != occurrence.transfer_uid
                        || fact.actor_uid.as_deref() != Some(person)
                        || !fact.delta.is_zero()
                        || !nucleus::fact::verify_chain_step(&fact)
                        || !crate::trust::verify_fact(&self.store, &fact).await?
                    {
                        return Err(invalid(
                            "Choose your verified dispute evidence for this occurrence",
                        ));
                    }
                    let evidence: nucleus::transfer::TransferOccurrenceDisputeEvidence =
                        serde_json::from_str(
                            fact.payload
                                .as_deref()
                                .ok_or_else(|| invalid("Dispute payload is unavailable"))?,
                        )?;
                    let original = store::transfers::occurrence_dispute_for_request(
                        &self.store.pool,
                        &evidence.idempotency_key,
                    )
                    .await?
                    .ok_or_else(|| invalid("Original dispute is unavailable"))?;
                    if evidence.action != "set-transfer-occurrence-dispute"
                        || evidence.transfer_uid != occurrence.transfer_uid
                        || evidence.occurrence_uid != occurrence.uid
                        || evidence.actor_person_uid != person
                        || original.fact != fact
                        || original.event.disputed != evidence.disputed
                        || original.event.actor_person_uid != person
                        || original.event.occurrence_uid != occurrence.uid
                    {
                        return Err(invalid("Dispute differs from its original signed evidence"));
                    }
                    let role = if person == occurrence.giver_person_uid {
                        nucleus::transfer::outcome::Role::Giver
                    } else if person == occurrence.receiver_person_uid {
                        nucleus::transfer::outcome::Role::Receiver
                    } else {
                        return Err(EngineError::Forbidden(
                            "Only the original party may publish this dispute".into(),
                        ));
                    };
                    (
                        Content::Dispute {
                            role,
                            active: evidence.disputed,
                        },
                        Source::fact_commitment(&fact.hash).map_err(invalid)?,
                        fact.at.timestamp(),
                        supersedes.clone(),
                    )
                }
                EventSource::Cancellation { fact, supersedes } => {
                    self.require_permission(actor, "organ:update").await?;
                    let (fact, content) = self.receipt_cancellation(&occurrence, fact).await?;
                    (
                        content,
                        Source::fact_commitment(&fact.hash).map_err(invalid)?,
                        fact.at.timestamp(),
                        supersedes.clone(),
                    )
                }
                EventSource::Compensation { fact, supersedes } => {
                    let (fact, content) = self
                        .receipt_compensation(person, actor, &occurrence, fact)
                        .await?;
                    (
                        content,
                        Source::fact_commitment(&fact.hash).map_err(invalid)?,
                        fact.at.timestamp(),
                        supersedes.clone(),
                    )
                }
                EventSource::Settlement { fact, supersedes }
                | EventSource::SettlementStatus { fact, supersedes } => {
                    self.require_permission(actor, "organ:update").await?;
                    let (fact, slice) = self.receipt_settlement(&occurrence, fact).await?;
                    let quantity = RecordedFloat::new(slice.cumulative_after).map_err(invalid)?;
                    let content = if matches!(request.source, EventSource::SettlementStatus { .. })
                    {
                        Content::SettlementStatus {
                            quantity,
                            remaining_after: RecordedFloat::new(slice.remaining_after)
                                .map_err(invalid)?,
                        }
                    } else {
                        Content::Settled { quantity }
                    };
                    (
                        content,
                        Source::fact_commitment(&fact.hash).map_err(invalid)?,
                        fact.at.timestamp(),
                        supersedes.clone(),
                    )
                }
            };
            (content, source_commitment, recorded_at, supersedes)
        };
        let scope = &bundle.publication.scope;
        let (authority, signer) = self
            .receipt_event_signer(&request.authority_record, actor, now)
            .await?;
        if scope.organ(content.role()) != signer.actor_uid {
            return Err(EngineError::Forbidden(
                "This Organ cannot publish another Organ's receipt role".into(),
            ));
        }
        let event = Event {
            scope: scope.id().map_err(invalid)?,
            source_commitment,
            recorded_at,
            issued_at: reviewed_at,
            supersedes,
            content,
        };
        if event.supersedes.is_none() {
            let slot = event.slot(scope).map_err(invalid)?;
            if bundle
                .publication
                .events
                .iter()
                .any(|signed| signed.event.slot(scope).ok().as_ref() == Some(&slot))
            {
                return Err(invalid(
                    "Select the previous public event instead of publishing another root for this role and measurement",
                ));
            }
        }
        if bundle.publication.events.iter().any(|signed| {
            signed.event.source_commitment == event.source_commitment
                && signed.event.content.stage() == event.content.stage()
        }) {
            return Err(invalid(
                "This source event is already published in this receipt",
            ));
        }
        if event.issued_at < authority.issued_at {
            return Err(invalid("Review again with the current signing authority"));
        }
        if let Some(parent) = &event.supersedes
            && let Some(previous) = bundle
                .publication
                .events
                .iter()
                .find(|signed| signed.event.id().ok().as_ref() == Some(parent))
            && !matches!(request.source, EventSource::Snapshot { .. })
            && previous.event.recorded_at > event.recorded_at
        {
            return Err(invalid("A correction cannot precede its source history"));
        }
        if !bundle.authorities.contains(&authority) {
            bundle.authorities.push(authority.clone());
        }
        if !header.authorities.contains(&authority) {
            header.authorities.push(authority);
        }
        let mut direct = header.clone();
        let need = match &event.content {
            Content::Statement { need, .. } => need.as_ref(),
            _ => None,
        };
        direct.publication.events.extend(
            bundle
                .publication
                .events
                .iter()
                .filter(|signed| {
                    signed
                        .event
                        .id()
                        .ok()
                        .as_ref()
                        .is_some_and(|id| Some(id) == event.supersedes.as_ref() || Some(id) == need)
                })
                .cloned(),
        );
        direct.publication.events.push(SignedEvent {
            event: event.clone(),
            key_id: signer.key_id.clone(),
            signature: "A".repeat(88),
        });
        direct
            .validate_fragment(now.timestamp())
            .map_err(proof_shape_error)?;
        let mut candidate = bundle.clone();
        candidate.publication.events.push(SignedEvent {
            event: event.clone(),
            key_id: signer.key_id.clone(),
            signature: "A".repeat(88),
        });
        if candidate.validate(now.timestamp()).is_err() {
            fragment = true;
        }
        if fragment {
            let mut minimal = header.clone();
            minimal.publication.events.push(SignedEvent {
                event: event.clone(),
                key_id: signer.key_id.clone(),
                signature: "A".repeat(88),
            });
            minimal
                .validate_fragment(now.timestamp())
                .map_err(proof_shape_error)?;
            bundle = header;
        } else {
            candidate
                .validate(now.timestamp())
                .map_err(proof_shape_error)?;
        }
        Ok(PreparedEvent {
            bundle,
            event,
            signer,
            fragment,
            source_revision,
        })
    }
}
