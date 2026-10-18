use super::*;
use crate::actions::{ActionOutcome, VerifiedActionAuthorship};
use nucleus::transfer::receipt::{Approval, ApprovalRole, Command, Document, Review};
use serde_json::json;

pub(super) fn result(
    saved: &store::transfer_receipts::Saved,
) -> Result<ActionOutcome, EngineError> {
    Ok(ActionOutcome {
        created: Some(saved.record.clone()),
        data: Some(json!({"receipt": {
            "record":saved.record,"version":saved.version,"document":saved.document,
            "state":match &saved.document {
                Document::Authority(authority) if authority.participation.is_some() => "participation_approval",
                Document::Authority(_) => "verified_authority",
                Document::Review(review) if review.is_complete() => "approved_scope",
                Document::Review(_) => "private_review",
                Document::Publication(_) => "verified_receipt",
                Document::ProofFragment(_) => "authenticated_fragment",
            },
        }})),
        ..Default::default()
    })
}

impl Engine {
    pub(crate) async fn transfer_receipt_command(
        &self,
        explicit: Option<&str>,
        request_id: String,
        command: Command,
        actor: Option<&str>,
        now: DateTime<Utc>,
        authorship: Option<&VerifiedActionAuthorship>,
    ) -> Result<ActionOutcome, EngineError> {
        let person = self.transfer_action_person(actor, explicit, None).await?;
        if !store::people::is_active(&self.store.pool, &person).await? {
            return Err(EngineError::Forbidden(
                "Receipt controls require an active Person".into(),
            ));
        }
        self.require_permission(actor, command.permission()).await?;
        if let Command::History {
            origin,
            occurrence,
            before,
        } = &command
        {
            return self
                .receipt_history(origin, occurrence, before.as_deref(), &person, actor, now)
                .await;
        }
        match &command {
            Command::InspectVersion { record, version }
            | Command::ExportVersion { record, version } => {
                return self
                    .inspect_receipt_version(
                        record,
                        *version,
                        &person,
                        actor,
                        now,
                        matches!(command, Command::ExportVersion { .. }),
                    )
                    .await;
            }
            Command::Versions { record, before } => {
                return self
                    .receipt_versions(record, before.as_deref(), &person, actor, now)
                    .await;
            }
            Command::Proof {
                record,
                version,
                before,
            } => {
                return self
                    .receipt_proof_page(record, *version, before.as_deref(), &person, actor, now)
                    .await;
            }
            _ => {}
        }
        if let Command::Delivery { request } = command {
            return self
                .receipt_delivery_command(person, request_id, request, actor, now, authorship)
                .await;
        }
        if matches!(
            command,
            Command::RecordSource { .. }
                | Command::InspectSource { .. }
                | Command::SourceOptions { .. }
        ) {
            return self
                .receipt_source_command(person, request_id, command, actor, now, authorship)
                .await;
        }
        if let Command::PreviewEvent { request } = &command {
            let prepared = self
                .prepare_receipt_event(&person, request, now.timestamp(), actor, now)
                .await?;
            return Ok(ActionOutcome {
                data: Some(json!({"receipt":{
                    "record":request.record,"version":request.expected_version,"state":"unsigned_event_preview",
                    "document":{"kind":"event_preview","value":{
                        "event":prepared.event,"request":request,"reviewed_at":now.timestamp(),"expected_projection":prepared.hash()?,"artifact_kind":if prepared.fragment {"proof_fragment"} else {"publication"},
                    }},
                }})),
                ..Default::default()
            });
        }
        if let Command::PrepareParticipation {
            transfer,
            expected_revision,
            party,
        } = &command
        {
            let terms = self
                .prepare_transfer_participation(
                    transfer,
                    *expected_revision,
                    party.as_deref(),
                    actor,
                )
                .await?;
            return Ok(ActionOutcome {
                data: Some(json!({
                    "receipt":{"state":"unsigned_participation_request","document":{"kind":"participation_request","value":terms}},
                    "receipt_export":serde_json::to_string(&terms)?,
                })),
                ..Default::default()
            });
        }
        if let Command::List = command {
            let mut listing = store::transfer_receipts::list(&self.store.pool, &person).await?;
            if let Some(entries) = listing["entries"].as_array_mut() {
                let mut visible = Vec::new();
                for mut entry in entries.drain(..) {
                    if let Some(uid) = entry["record"].as_str()
                        && self.may_read_record(actor, uid).await?
                    {
                        if entry["kind"] == "authority" {
                            let saved =
                                store::transfer_receipts::get(&self.store.pool, uid).await?;
                            if let Document::Authority(authority) = saved.document {
                                entry["participation"] = json!(authority.participation);
                            }
                        }
                        visible.push(entry);
                    }
                }
                *entries = visible;
            }
            return Ok(ActionOutcome {
                data: Some(json!({"receipt_list":listing})),
                ..Default::default()
            });
        }
        if let Command::Inspect { record } | Command::Export { record } = &command {
            let uid = self.resolve(record).await?;
            self.refuse_unreadable(actor, std::slice::from_ref(&uid))
                .await?;
            let saved = store::transfer_receipts::get(&self.store.pool, &uid).await?;
            self.verify_transfer_receipt_document(&saved.document, now)
                .await?;
            let mut outcome = result(&saved)?;
            if matches!(command, Command::Export { .. }) {
                if saved.owner != person {
                    return Err(EngineError::Forbidden(
                        "Import a readable receipt into your own review before exporting it".into(),
                    ));
                }
                if saved.document.expires_at() <= now.timestamp() {
                    return Err(invalid(
                        "This receipt's sharing term has expired; review new terms before exporting",
                    ));
                }
                outcome.data.as_mut().unwrap()["receipt_export"] =
                    json!(serde_json::to_string(&saved.document)?);
            }
            return Ok(outcome);
        }
        if let Some(replay) =
            store::transfer_receipts::replay(&self.store.pool, &person, &request_id, &command)
                .await?
        {
            self.refuse_unreadable(actor, std::slice::from_ref(&replay.record))
                .await?;
            self.verify_transfer_receipt_document(&replay.document, now)
                .await?;
            return result(&replay);
        }
        self.require_permission(
            actor,
            if matches!(
                command,
                Command::Approve { .. } | Command::ApprovePerson { .. }
            ) {
                "record:update"
            } else {
                "record:create"
            },
        )
        .await?;
        let signer = self.transfer_person_signer(&person, authorship).await?;
        let mut local_source = None;
        let mut expected = None;
        let mut changes_record = true;
        let document = match &command {
            Command::IssueAuthority { cell } => {
                self.require_permission(actor, "organ:update").await?;
                let root = self.root_signer().await?.ok_or_else(|| {
                    invalid("The Organ owner must authorize this device's receipt signing key")
                })?;
                let cell = if cell.is_empty() {
                    store::cells::local(&self.store.pool)
                        .await?
                        .ok_or_else(|| invalid("No local device"))?
                        .uid
                } else {
                    self.resolve(cell).await?
                };
                Document::Authority(
                    self.create_transfer_receipt_authority(&root, &cell, now)
                        .await?,
                )
            }
            Command::AuthorizeParticipation { cell, request } => {
                if request.len() > 8192 {
                    return Err(invalid("Participation request exceeds its byte budget"));
                }
                let request: nucleus::transfer::receipt::ParticipationRequest =
                    serde_json::from_str(request)?;
                request.validate().map_err(invalid)?;
                if let Some(party) = request.party.as_ref() {
                    self.refuse_unreadable(actor, std::slice::from_ref(party))
                        .await?;
                    if !store::people::is_active(&self.store.pool, party).await? {
                        return Err(invalid(
                            "Review an identified active Person before approving their participation",
                        ));
                    }
                }
                let root = self.root_signer().await?.ok_or_else(|| {
                    invalid("The Organ owner must approve original participation")
                })?;
                let cell = if cell.is_empty() {
                    store::cells::local(&self.store.pool)
                        .await?
                        .ok_or_else(|| invalid("No local device"))?
                        .uid
                } else {
                    self.resolve(cell).await?
                };
                Document::Authority(
                    self.create_transfer_participation_authority(&root, &cell, request.terms, now)
                        .await?,
                )
            }
            Command::Prepare { scope } => {
                let prepared = self
                    .prepare_transfer_receipt_scope(scope.clone(), actor, now)
                    .await?;
                let occurrence = store::transfers::occurrence(&self.store.pool, &scope.occurrence)
                    .await?
                    .ok_or_else(|| invalid("Source occurrence is unavailable"))?;
                if person != occurrence.giver_person_uid && person != occurrence.receiver_person_uid
                {
                    self.require_permission(actor, "organ:update").await?;
                }
                local_source = Some(scope.clone());
                Document::Review(Review {
                    person_approvals: Vec::new(),
                    scope: prepared,
                    approvals: Vec::new(),
                    authorities: Vec::new(),
                })
            }
            Command::Import { document } => {
                let incoming = Document::parse(document.as_bytes(), now.timestamp())
                    .map_err(proof_shape_error)?;
                self.verify_transfer_receipt_document(&incoming, now)
                    .await?;
                incoming
            }
            Command::PublishEvent {
                request,
                reviewed_at,
                expected_projection,
            } => {
                let prepared = self
                    .prepare_receipt_event(&person, request, *reviewed_at, actor, now)
                    .await?;
                if prepared.hash()? != *expected_projection {
                    return Err(invalid(
                        "Event projection changed after review; preview it again",
                    ));
                }
                let mut signed = nucleus::transfer::receipt::SignedEvent {
                    event: prepared.event,
                    key_id: prepared.signer.key_id.clone(),
                    signature: String::new(),
                };
                signed.signature = prepared
                    .signer
                    .sign_bytes(&signed.signing_bytes().map_err(invalid)?);
                let mut bundle = prepared.bundle;
                bundle.publication.events.push(signed);
                let uid = self.resolve(&request.record).await?;
                local_source =
                    store::transfer_receipts::local_source(&self.store.pool, &uid).await?;
                if local_source.is_none() {
                    local_source = store::transfer_receipts::source_for_scope(
                        &self.store.pool,
                        &Document::Publication(bundle.clone()),
                    )
                    .await?;
                }
                if let (Some(source), Some(revision)) =
                    (&mut local_source, prepared.source_revision)
                {
                    source.expected_revision = revision;
                }
                if prepared.fragment {
                    Document::ProofFragment(bundle)
                } else {
                    Document::Publication(bundle)
                }
            }
            Command::ApprovePerson {
                record,
                expected_version,
                approval,
                private_transfer,
            } => {
                let (document, uid, version) = self
                    .approve_receipt_person(
                        &person,
                        record,
                        *expected_version,
                        approval,
                        private_transfer.as_deref(),
                        actor,
                        now,
                        signer.as_ref(),
                    )
                    .await?;
                expected = Some((uid, version));
                document
            }
            Command::Approve {
                record,
                expected_version,
                role,
                authority_record,
                private_transfer,
            } => {
                let uid = self.resolve(record).await?;
                self.refuse_unreadable(actor, std::slice::from_ref(&uid))
                    .await?;
                let saved = store::transfer_receipts::get(&self.store.pool, &uid).await?;
                if saved.owner != person || saved.version != *expected_version {
                    return Err(invalid(
                        "Review your own current receipt version before approving",
                    ));
                }
                self.verify_transfer_receipt_document(&saved.document, now)
                    .await?;
                let Document::Review(mut review) = saved.document.clone() else {
                    return Err(invalid("Choose a receipt publication review"));
                };
                if review.scope.expires_at <= now.timestamp() {
                    return Err(invalid("The publication review has expired"));
                }
                if review
                    .approvals
                    .iter()
                    .any(|approval| approval.role == *role)
                {
                    return Err(invalid("This role already approved these exact terms"));
                }
                let local = store::organs::local(&self.store.pool)
                    .await?
                    .ok_or_else(|| invalid("No local Organ"))?;
                if review.scope.organ(*role) != local.uid {
                    return Err(EngineError::Forbidden(
                        "This Organ cannot approve another Organ's receipt role".into(),
                    ));
                }
                let own_role = if let Some(transfer) = private_transfer {
                    if !review
                        .scope
                        .opens_original_party(*role, transfer, &person)
                        .map_err(invalid)?
                    {
                        return Err(EngineError::Forbidden("The private Transfer reference does not open this Person's original role".into()));
                    }
                    true
                } else {
                    false
                };
                local_source =
                    store::transfer_receipts::local_source(&self.store.pool, &uid).await?;
                if !own_role && review.scope.source.origin == local.uid {
                    if local_source.is_none() {
                        local_source = store::transfer_receipts::source_for_scope(
                            &self.store.pool,
                            &saved.document,
                        )
                        .await?;
                    }
                    let source = local_source.as_ref().ok_or_else(|| invalid("The origin must prepare this review from its original Transfer evidence"))?;
                    let mut source_check = source.clone();
                    if *role != ApprovalRole::Origin {
                        source_check.outcome_records.clear();
                        source_check.stages = vec![nucleus::transfer::receipt::Stage::Settled];
                    }
                    let prepared = self
                        .prepare_transfer_receipt_scope(
                            source_check,
                            actor,
                            DateTime::from_timestamp(review.scope.issued_at, 0)
                                .ok_or_else(|| invalid("Invalid scope review time"))?,
                        )
                        .await?;
                    if prepared.source != review.scope.source
                        || prepared.party_attribution != review.scope.party_attribution
                        || prepared.original_authorities != review.scope.original_authorities
                        || prepared.giver != review.scope.giver
                        || prepared.receiver != review.scope.receiver
                        || prepared.agreed_quantity != review.scope.agreed_quantity
                        || prepared.canonical_unit != review.scope.canonical_unit
                        || (*role == ApprovalRole::Origin && prepared != review.scope)
                    {
                        return Err(invalid(
                            "Source terms differ from the reviewed publication scope",
                        ));
                    }
                    let occurrence =
                        store::transfers::occurrence(&self.store.pool, &source.occurrence)
                            .await?
                            .ok_or_else(|| invalid("Source occurrence is unavailable"))?;
                    let required = match role {
                        ApprovalRole::Origin => {
                            self.require_permission(actor, "organ:update").await?;
                            None
                        }
                        ApprovalRole::Giver => Some(occurrence.giver_person_uid),
                        ApprovalRole::Receiver => Some(occurrence.receiver_person_uid),
                    };
                    if required.is_some_and(|required| required != person) {
                        return Err(EngineError::Forbidden(
                            "Only the actual Transfer party may approve this role".into(),
                        ));
                    }
                } else if !own_role {
                    self.require_permission(actor, "organ:update").await?;
                }
                let (authority, operational) = self
                    .receipt_event_signer(authority_record, actor, now)
                    .await?;
                let mut approval = Approval {
                    role: *role,
                    issued_at: now.timestamp(),
                    key_id: operational.key_id.clone(),
                    signature: String::new(),
                };
                approval.signature = operational
                    .sign_bytes(&approval.signing_bytes(&review.scope).map_err(invalid)?);
                review.approvals.push(approval);
                if !review.authorities.contains(&authority) {
                    review.authorities.push(authority);
                }
                expected = Some((uid, saved.version));
                saved
                    .document
                    .merge(&Document::Review(review), now.timestamp())
                    .map_err(invalid)?
            }
            Command::Inspect { .. }
            | Command::History { .. }
            | Command::InspectVersion { .. }
            | Command::ExportVersion { .. }
            | Command::Versions { .. }
            | Command::Proof { .. }
            | Command::Export { .. }
            | Command::List
            | Command::PrepareParticipation { .. }
            | Command::PreviewEvent { .. } => unreachable!(),
            Command::RecordSource { .. }
            | Command::InspectSource { .. }
            | Command::SourceOptions { .. } => unreachable!(),
            Command::Delivery { .. } => unreachable!(),
        };
        let document = if expected.is_none() {
            if let Some(saved) =
                store::transfer_receipts::find(&self.store.pool, &person, &document).await?
            {
                self.refuse_unreadable(actor, std::slice::from_ref(&saved.record))
                    .await?;
                self.verify_transfer_receipt_document(&saved.document, now)
                    .await?;
                expected = Some((saved.record, saved.version));
                let merged = saved
                    .document
                    .merge(&document, now.timestamp())
                    .map_err(invalid)?;
                changes_record = merged != saved.document;
                merged
            } else {
                document
            }
        } else {
            document
        };
        self.verify_transfer_receipt_document(&document, now)
            .await?;
        let record = expected
            .as_ref()
            .map(|(uid, _)| uid.clone())
            .unwrap_or_else(|| nucleus::new_uid("r"));
        let updating = expected.is_some();
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_permission_on(
            &mut tx,
            actor,
            if updating && changes_record {
                "record:update"
            } else {
                "record:create"
            },
        )
        .await?;
        let checkpoint = if changes_record {
            self.record_checkpoint_on(
                &mut tx,
                actor,
                vec![record.clone()],
                if updating {
                    protein::authority::Operation::Update
                } else {
                    protein::authority::Operation::Create
                },
            )
            .await?
        } else {
            None
        };
        let (committed, mut tx) = store::transfer_receipts::save(
            tx,
            store::transfer_receipts::Input {
                record,
                person,
                request_id,
                command,
                document,
                expected,
                local_source,
                authorization_intent: authorship.map(|proof| proof.intent_uid.clone()),
            },
            now,
            |hash| signer.as_ref().map(|signer| signer.sign_hash(hash)),
        )
        .await?;
        if committed.created
            && let Some(checkpoint) = checkpoint
        {
            let properties = if updating {
                std::collections::BTreeSet::from([protein::authority::Property::Extension(
                    protein::authority::ExtensionProperty {
                        namespace: nucleus::transfer::receipt::NAMESPACE.into(),
                        field: "value".into(),
                    },
                )])
            } else {
                Default::default()
            };
            checkpoint.bounded().finish(&mut tx, properties).await?;
        }
        tx.commit().await?;
        let mut outcome = result(&committed.saved)?;
        if committed.created {
            outcome.facts = self.publish_committed_fact(committed.saved.fact);
        }
        Ok(outcome)
    }
}
