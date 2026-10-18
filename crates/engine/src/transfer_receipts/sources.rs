use super::*;
use crate::actions::{ActionOutcome, VerifiedActionAuthorship};
use nucleus::transfer::{
    outcome::{Content as PrivateContent, Request, Role},
    receipt::{ApprovalRole, Command, Content, Document},
    receipt_source::RecordedClock,
};
use serde_json::{Value, json};

fn role(content: &PrivateContent) -> ApprovalRole {
    match content {
        PrivateContent::NeedBinding { .. }
        | PrivateContent::Statement {
            role: Role::Receiver,
            ..
        } => ApprovalRole::Receiver,
        _ => ApprovalRole::Giver,
    }
}

fn source_result(
    saved: &store::receipt_sources::Saved,
    receipt: Option<&str>,
) -> Result<ActionOutcome, EngineError> {
    Ok(ActionOutcome {
        created: Some(saved.snapshot.record.clone()),
        data: Some(json!({
            "receipt":{"record":saved.snapshot.record,"source_receipt":receipt,"state":"private_source_snapshot","document":{"kind":"source_snapshot","value":saved}},
            "receipt_export":serde_json::to_string(&vec![&saved.snapshot.measurement])?,
        })),
        ..Default::default()
    })
}

impl Engine {
    async fn source_scope(
        &self,
        person: &str,
        record: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<store::transfer_receipts::Saved, EngineError> {
        self.refuse_unreadable(actor, &[record.to_owned()]).await?;
        let saved = store::transfer_receipts::get(&self.store.pool, record).await?;
        if saved.owner != person || saved.document.scope().is_none() {
            return Err(invalid(
                "Import a receipt review into your own Records before recording its source",
            ));
        }
        self.verify_transfer_receipt_document(&saved.document, now)
            .await?;
        Ok(saved)
    }

    async fn source_author(
        &self,
        scope: &Scope,
        person: &str,
        private_transfer: Option<&str>,
        role: ApprovalRole,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        if scope.organ(role) != local.uid {
            return Err(EngineError::Forbidden(
                "This Organ cannot author another Organ's receipt source".into(),
            ));
        }
        if scope.party_attribution == PartyAttribution::OriginalOrganAuthority {
            if !private_transfer
                .map(|reference| scope.opens_original_party(role, reference, person))
                .transpose()
                .map_err(invalid)?
                .unwrap_or(false)
            {
                return Err(EngineError::Forbidden(
                    "Use the private Transfer reference for your own original role".into(),
                ));
            }
        } else {
            self.require_permission(actor, "organ:update").await?;
        }
        Ok(())
    }

    pub(super) async fn read_receipt_source(
        &self,
        record: &str,
        actor: Option<&str>,
    ) -> Result<store::receipt_sources::Saved, EngineError> {
        self.refuse_unreadable(actor, &[record.to_owned()]).await?;
        let saved = store::receipt_sources::get(&self.store.pool, record).await?;
        if !crate::trust::verify_fact(&self.store, &saved.fact).await? {
            return Err(withheld(
                "Source snapshot no longer has accepted author authority",
            ));
        }
        let mut dependencies = Vec::new();
        if let PrivateContent::NeedBinding { terms, .. } = &saved.snapshot.content {
            dependencies.push(terms.selection.need.clone());
        }
        if let PrivateContent::Statement {
            need_binding: Some(binding),
            ..
        } = &saved.snapshot.content
        {
            dependencies.push(binding.clone());
            let binding = store::receipt_sources::get(&self.store.pool, binding).await?;
            if let PrivateContent::NeedBinding { terms, .. } = binding.snapshot.content {
                dependencies.push(terms.selection.need);
            }
        }
        dependencies.extend(saved.snapshot.content.measure().object.iter().cloned());
        dependencies.extend(saved.snapshot.content.supersedes().map(str::to_owned));
        self.refuse_unreadable(actor, &dependencies).await?;
        Ok(saved)
    }

    fn check_source_scope(
        &self,
        scope: &Scope,
        saved: &store::receipt_sources::Saved,
    ) -> Result<(), EngineError> {
        let snapshot = &saved.snapshot;
        if snapshot.source != scope.source
            || snapshot.clock.organ != scope.organ(role(&snapshot.content))
            || !nucleus::valid_uid(&snapshot.clock.cell, "r")
        {
            return Err(invalid(
                "Snapshot source or reporting Organ differs from the receipt",
            ));
        }
        if scope.party_attribution == PartyAttribution::OriginalOrganAuthority
            && !snapshot
                .private_transfer
                .as_deref()
                .map(|reference| {
                    scope.opens_original_party(role(&snapshot.content), reference, &snapshot.author)
                })
                .transpose()
                .map_err(invalid)?
                .unwrap_or(false)
        {
            return Err(invalid("Snapshot lacks its original Person role opening"));
        }
        Ok(())
    }

    pub(super) async fn receipt_source_command(
        &self,
        person: String,
        id: String,
        command: Command,
        actor: Option<&str>,
        now: DateTime<Utc>,
        authorship: Option<&VerifiedActionAuthorship>,
    ) -> Result<ActionOutcome, EngineError> {
        match command {
            Command::InspectSource { record } => {
                let record = self.resolve(&record).await?;
                source_result(&self.read_receipt_source(&record, actor).await?, None)
            }
            Command::SourceOptions { receipt } => {
                let receipt = self.resolve(&receipt).await?;
                let saved = self.source_scope(&person, &receipt, actor, now).await?;
                let scope = saved.document.scope().unwrap();
                let mut needs =
                    store::transfer_evidence::need_options(&self.store.pool, &person, None).await?;
                let mut options = Vec::new();
                for option in needs["options"].as_array_mut().into_iter().flatten() {
                    let terms: nucleus::transfer::outcome::NeedTerms =
                        serde_json::from_value(option["terms"].clone())?;
                    let mut allowed = self.may_read_record(actor, &terms.selection.need).await?;
                    if let Some(object) = &terms.measure.object {
                        allowed &= self.may_read_record(actor, object).await?;
                    }
                    if allowed {
                        options.push(option.clone());
                    }
                }
                needs["options"] = json!(options);
                let listing =
                    store::receipt_sources::list(&self.store.pool, &person, &scope.source).await?;
                let mut sources = Vec::new();
                for record in listing["records"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    match self.read_receipt_source(record, actor).await {
                        Ok(source) => sources.push(json!(source)),
                        Err(EngineError::Forbidden(_)) => {}
                        Err(error) => return Err(error),
                    }
                }
                Ok(ActionOutcome {
                    data: Some(
                        json!({"receipt":{"record":receipt,"version":saved.version,"document":saved.document,"source_options":{"needs":needs,"sources":sources,"partial":listing["partial"]}}}),
                    ),
                    ..Default::default()
                })
            }
            Command::RecordSource { mut request } => {
                request.receipt = self.resolve(&request.receipt).await?;
                if let Some(replay) =
                    store::receipt_sources::replay(&self.store.pool, &person, &id, &request).await?
                {
                    return source_result(
                        &self
                            .read_receipt_source(&replay.snapshot.record, actor)
                            .await?,
                        Some(&request.receipt),
                    );
                }
                self.require_permission(actor, "record:create").await?;
                let saved = self
                    .source_scope(&person, &request.receipt, actor, now)
                    .await?;
                let scope = saved.document.scope().unwrap();
                if saved.version != request.expected_version || scope.expires_at <= now.timestamp()
                {
                    return Err(invalid(
                        "Review the current unexpired receipt before freezing a source",
                    ));
                }
                let anchored = match &saved.document {
                    Document::Review(review) => review
                        .approvals
                        .iter()
                        .any(|approval| approval.role == ApprovalRole::Origin),
                    Document::Publication(_) | Document::ProofFragment(_) => true,
                    _ => false,
                };
                if !anchored {
                    return Err(invalid(
                        "The origin must approve this source reference before a snapshot is frozen",
                    ));
                }
                let (source_role, measure) = match &request.outcome {
                    Request::BindNeed {
                        selection,
                        expected_measure,
                        ..
                    } => {
                        self.refuse_unreadable(actor, std::slice::from_ref(&selection.need))
                            .await?;
                        let terms = store::transfer_evidence::freeze_need_on(
                            &mut *self.store.pool.acquire().await?,
                            &person,
                            selection,
                        )
                        .await?;
                        if terms.measure != *expected_measure {
                            return Err(invalid("Need meaning or unit changed after review"));
                        }
                        (ApprovalRole::Receiver, terms.measure)
                    }
                    Request::State {
                        role,
                        measure,
                        need_binding,
                        ..
                    } => {
                        if let Some(binding) = need_binding {
                            let binding = self.read_receipt_source(binding, actor).await?;
                            self.check_source_scope(scope, &binding)?;
                        }
                        (
                            if *role == Role::Giver {
                                ApprovalRole::Giver
                            } else {
                                ApprovalRole::Receiver
                            },
                            measure.clone(),
                        )
                    }
                };
                self.source_author(
                    scope,
                    &person,
                    request.private_transfer.as_deref(),
                    source_role,
                    actor,
                )
                .await?;
                if let Some(object) = &measure.object {
                    self.refuse_unreadable(actor, std::slice::from_ref(object))
                        .await?;
                }
                let previous = match &request.outcome {
                    Request::BindNeed { supersedes, .. } | Request::State { supersedes, .. } => {
                        supersedes.as_deref()
                    }
                };
                if let Some(previous) = previous {
                    self.read_receipt_source(previous, actor).await?;
                }
                let mut measurement = super::projection::measurement(scope, &measure)?;
                if let Some(need) = &request.public_need {
                    let Some(bundle) = saved.document.public_bundle() else {
                        return Err(invalid("Choose an already signed public Need event"));
                    };
                    let binding = bundle
                        .publication
                        .events
                        .iter()
                        .find(|signed| signed.event.id().ok().as_ref() == Some(need))
                        .ok_or_else(|| invalid("Public Need proof is missing"))?;
                    let Content::Need {
                        measurement: bound, ..
                    } = &binding.event.content
                    else {
                        return Err(invalid("Choose a receiver-authored public Need"));
                    };
                    if measurement.meaning != bound.meaning
                        || measurement.unit != bound.unit
                        || (measure.object.is_some()
                            && measurement.object_commitment != bound.object_commitment)
                    {
                        return Err(invalid(
                            "Statement must retain the public Need's exact measurement",
                        ));
                    }
                    measurement = bound.clone();
                }
                let signer = self.transfer_person_signer(&person, authorship).await?;
                let clock = RecordedClock {
                    organ: store::organs::local(&self.store.pool)
                        .await?
                        .ok_or_else(|| invalid("No local Organ"))?
                        .uid,
                    cell: store::cells::local(&self.store.pool)
                        .await?
                        .ok_or_else(|| invalid("No local Cell"))?
                        .uid,
                };
                let record = nucleus::new_uid("r");
                let mut tx = store::write_tx(&self.store.pool).await?;
                let checkpoint = self
                    .record_checkpoint_on(
                        &mut tx,
                        actor,
                        vec![record.clone()],
                        protein::authority::Operation::Create,
                    )
                    .await?;
                let receipt = request.receipt.clone();
                let (committed, mut tx) = store::receipt_sources::save(
                    tx,
                    store::receipt_sources::Input {
                        record,
                        person,
                        request_id: id,
                        request,
                        source: scope.source.clone(),
                        measurement,
                        clock,
                        authorization_intent: authorship.map(|proof| proof.intent_uid.clone()),
                    },
                    now,
                    |hash| signer.as_ref().map(|signer| signer.sign_hash(hash)),
                )
                .await?;
                if committed.created
                    && let Some(checkpoint) = checkpoint
                {
                    checkpoint
                        .bounded()
                        .finish(&mut tx, Default::default())
                        .await?;
                }
                tx.commit().await?;
                let mut result = source_result(&committed.saved, Some(&receipt))?;
                if committed.created {
                    result.facts = self.publish_committed_fact(committed.saved.fact);
                }
                Ok(result)
            }
            _ => unreachable!(),
        }
    }

    async fn snapshot_content(
        &self,
        saved: &store::receipt_sources::Saved,
        bundle: &Bundle,
        actor: Option<&str>,
    ) -> Result<Content, EngineError> {
        self.check_source_scope(&bundle.publication.scope, saved)?;
        let frame = &saved.snapshot;
        match &frame.content {
            PrivateContent::NeedBinding { terms, .. } => {
                let content = super::projection::need_content(&bundle.publication.scope, terms)?;
                if content.measurement() != Some(&frame.measurement) {
                    return Err(invalid(
                        "Need snapshot measurement differs from its original terms",
                    ));
                }
                Ok(content)
            }
            PrivateContent::Statement {
                role,
                quantity,
                withdrawn,
                need_binding,
                ..
            } => {
                let need = if let Some(binding) = need_binding {
                    let binding = self.read_receipt_source(binding, actor).await?;
                    self.check_source_scope(&bundle.publication.scope, &binding)?;
                    let PrivateContent::NeedBinding { terms, .. } = &binding.snapshot.content
                    else {
                        return Err(invalid("Choose a receiver-authored Need snapshot"));
                    };
                    Some(super::projection::public_predecessor(
                        bundle,
                        &binding.fact.hash,
                        &super::projection::need_content(&bundle.publication.scope, terms)?,
                        "Receipt statement is missing its Need proof",
                    )?)
                } else {
                    frame.public_need.clone()
                };
                Ok(Content::Statement {
                    role: *role,
                    measurement: frame.measurement.clone(),
                    quantity: *quantity,
                    withdrawn: *withdrawn,
                    need,
                })
            }
        }
    }

    pub(super) async fn project_receipt_snapshot(
        &self,
        person: &str,
        record: &str,
        bundle: &Bundle,
        actor: Option<&str>,
    ) -> Result<(Content, CanonicalHash, i64, Option<CanonicalHash>), EngineError> {
        let record = self.resolve(record).await?;
        let saved = self.read_receipt_source(&record, actor).await?;
        if saved.snapshot.author != person {
            return Err(EngineError::Forbidden(
                "Only the snapshot's actual author may publish it".into(),
            ));
        }
        self.check_source_scope(&bundle.publication.scope, &saved)?;
        self.source_author(
            &bundle.publication.scope,
            person,
            saved.snapshot.private_transfer.as_deref(),
            role(&saved.snapshot.content),
            actor,
        )
        .await?;
        let content = self.snapshot_content(&saved, bundle, actor).await?;
        let parent = if let Some(previous) = saved.snapshot.content.supersedes() {
            let previous = self.read_receipt_source(previous, actor).await?;
            Some(super::projection::public_predecessor(
                bundle,
                &previous.fact.hash,
                &self.snapshot_content(&previous, bundle, actor).await?,
                "Receipt correction history is incomplete",
            )?)
        } else {
            None
        };
        Ok((
            content,
            Source::fact_commitment(&saved.fact.hash).map_err(invalid)?,
            saved.fact.at.timestamp(),
            parent,
        ))
    }
}
