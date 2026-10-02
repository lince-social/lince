use super::*;

impl Engine {
    pub(super) fn dispatch_part_7(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
        _verified_authorship: Option<VerifiedActionAuthorship>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = DispatchOutcome> + Send + '_>> {
        Box::pin(async move {
            let mut outcome = ActionOutcome::default();
            match action {
                Action::AddParty {
                    transfer,
                    actor: person,
                } => {
                    if transfer_phase_locked() {
                        return Err(EngineError::Conflict {
                            code: "transfer_invitation_required",
                            message:
                                "a Person becomes a transfer party only by accepting an invitation"
                                    .into(),
                        });
                    }
                    let transfer = self.resolve(&transfer).await?;
                    self.require_transfer_editor(&transfer, actor.as_deref())
                        .await?;
                    let person = self.resolve(&person).await?;
                    let actor_record = store::records::get(&self.store.pool, &person)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(person.clone()))?;
                    if actor_record.kind != RecordKind::Person.as_str() {
                        return Err(EngineError::Consequence(
                            "transfer parties must be person records".into(),
                        ));
                    }
                    let party =
                        store::transfers::add_party(&self.store.pool, &transfer, &person).await?;
                    outcome.facts = self
                        .annotate(
                            transfer,
                            actor,
                            serde_json::json!({
                                "action": "add-party",
                                "party": party,
                                "person": person,
                            }),
                            now,
                        )
                        .await?;
                    outcome.created = Some(party);
                }
                Action::AddPromiseToTransfer {
                    transfer,
                    record,
                    delta,
                    party,
                    window_end,
                    condition,
                } => {
                    if transfer_phase_locked() {
                        return Err(EngineError::Conflict {
                            code: "transfer_phase_1_not_available",
                            message:
                                "adding a promise requires the revision-safe draft edit workflow"
                                    .into(),
                        });
                    }
                    if !delta.is_finite() || delta == 0.0 {
                        return Err(EngineError::Consequence(
                            "promise delta must be finite and non-zero".into(),
                        ));
                    }
                    let transfer = self.resolve(&transfer).await?;
                    self.require_transfer_editor(&transfer, actor.as_deref())
                        .await?;
                    let record = self.resolve(&record).await?;
                    let party = self.resolve(&party).await?;
                    let party_record = store::records::get(&self.store.pool, &party)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(party.clone()))?;
                    if party_record.kind != RecordKind::Person.as_str() {
                        return Err(EngineError::Consequence(
                            "promise parties must be person records".into(),
                        ));
                    }
                    let window_end = window_end
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty());
                    if let Some(value) = window_end.as_deref() {
                        let parsed = DateTime::parse_from_rfc3339(value).map_err(|_| {
                            EngineError::Consequence(format!(
                                "promise window `{value}` must be an RFC3339 date and time"
                            ))
                        })?;
                        if parsed.with_timezone(&Utc) <= now {
                            return Err(EngineError::Consequence(
                                "promise window must end in the future".into(),
                            ));
                        }
                    }
                    let condition = condition
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty());
                    if let Some(value) = condition.as_deref() {
                        nucleus::expr::Expr::parse(value).map_err(|error| {
                            EngineError::Consequence(format!("invalid promise condition: {error}"))
                        })?;
                    }
                    let promise = store::misc::insert_promise(
                        &self.store.pool,
                        store::misc::NewPromise {
                            record_uid: Some(record.clone()),
                            delta,
                            window_end,
                            party_uid: Some(party.clone()),
                            state: Some(PromiseState::Proposed),
                            condition,
                            transfer_uid: Some(transfer.clone()),
                            ..Default::default()
                        },
                    )
                    .await?;
                    outcome.facts = self
                        .annotate_many(
                            vec![transfer, record],
                            actor,
                            serde_json::json!({
                                "action": "add-promise-to-transfer",
                                "promise": promise,
                                "party": party,
                                "delta": delta,
                            }),
                            now,
                        )
                        .await?;
                    outcome.created = Some(promise);
                }
                Action::AgreeTransfer { .. } => {
                    return Err(EngineError::Conflict {
                    code: "signed_transfer_agreement_action_required",
                    message: "use set-transfer-agreement-level with expected_revision, request_id, and the acting Person"
                        .into(),
                });
                }
                Action::ActivateTransfer { transfer } => {
                    if transfer_phase_locked() {
                        return Err(EngineError::Conflict {
                            code: "transfer_phase_4_not_available",
                            message:
                                "transfer activation is not available before occurrence modeling"
                                    .into(),
                        });
                    }
                    let transfer = self.resolve(&transfer).await?;
                    self.require_transfer_editor(&transfer, actor.as_deref())
                        .await?;
                    let activated =
                        crate::transfer::activate_promises(&self.store, &transfer).await?;
                    outcome.facts = self
                        .annotate(
                            transfer,
                            actor,
                            serde_json::json!({
                                "action": "activate-transfer",
                                "activated_promises": activated,
                            }),
                            now,
                        )
                        .await?;
                }
                Action::SettleTransfer {
                    transfer,
                    actor: person,
                } => {
                    if transfer_phase_locked() {
                        return Err(EngineError::Conflict {
                        code: "transfer_phase_5_not_available",
                        message:
                            "transfer settlement is not available before occurrence confirmation"
                                .into(),
                    });
                    }
                    let transfer = self.resolve(&transfer).await?;
                    let person = self.resolve(&person).await?;
                    self.require_transfer_person(&transfer, &person, actor.as_deref())
                        .await?;
                    outcome.facts = self.settle_all_local(&transfer, &person, now).await?;
                }
                Action::SetPlace {
                    target,
                    lat,
                    lon,
                    address,
                } => {
                    let target = self.resolve(&target).await?;
                    let place =
                        store::places::create(&self.store.pool, lat, lon, address.as_deref())
                            .await?;
                    store::places::set_record_place(&self.store.pool, &target, &place).await?;
                    outcome.created = Some(place);
                }
                Action::GrantVisibility {
                    subject_kind,
                    subject,
                    target,
                } => {
                    let target = self.resolve(&target).await?;
                    outcome.created = Some(
                        store::visibility::grant(
                            &self.store.pool,
                            &subject_kind,
                            subject.as_deref(),
                            &target,
                        )
                        .await?,
                    );
                }
                Action::SaveProtein { slug, head, ast } => {
                    validate_saved_protein_shape(&ast)?;
                    let parsed: protein::Protein =
                        serde_json::from_value(ast.clone()).map_err(|error| {
                            EngineError::Consequence(format!("invalid Protein: {error}"))
                        })?;
                    protein::validate(&parsed).map_err(|error| {
                        EngineError::Consequence(format!("invalid Protein: {error}"))
                    })?;
                    let body = serde_json::to_string_pretty(&ast).unwrap_or_default();
                    let uid = match store::records::resolve(&self.store.pool, &slug).await? {
                        Some(existing) => {
                            if existing.kind != RecordKind::Protein.as_str() {
                                return Err(EngineError::Consequence(format!(
                                    "slug `{slug}` is a {} record, not a saved protein",
                                    existing.kind
                                )));
                            }
                            self.write_record_text(&existing.uid, Some(&head), Some(&body))
                                .await?;
                            if existing.quantity.is_zero() {
                                Box::pin(self.act(
                                    Action::SetQuantity {
                                        target: existing.uid.clone(),
                                        value: 1.0,
                                    },
                                    actor.clone(),
                                ))
                                .await?;
                            }
                            existing.uid
                        }
                        None => {
                            store::records::create(
                                &self.store.pool,
                                store::records::NewRecord {
                                    slug: Some(&slug),
                                    kind: RecordKind::Protein,
                                    head: &head,
                                    body: &body,
                                    quantity: store::exact::one(),
                                },
                            )
                            .await?
                            .uid
                        }
                    };
                    store::records::set_extension(&self.store.pool, &uid, "lince.protein", &ast)
                        .await?;
                    outcome.created = Some(uid);
                }
                Action::CreateKarmaProgram {
                    request_id,
                    program,
                    owner_person_uid,
                } => {
                    let commit = self
                        .create_karma_program(
                            store::karma::programs::CreateProgramInput {
                                request_id,
                                program,
                                owner_person_uid,
                                actor_person_uid: actor,
                            },
                            now,
                        )
                        .await?;
                    apply_program_mutation(commit, &mut outcome)?;
                    self.publish_karma_definition(&outcome, KarmaKind::Program)
                        .await?;
                }
                Action::ReviseKarmaProgram {
                    request_id,
                    program_uid,
                    expected_handle_revision,
                    program,
                } => {
                    let commit = self
                        .revise_karma_program(
                            store::karma::programs::ReviseProgramInput {
                                request_id,
                                program_uid,
                                expected_handle_revision,
                                program,
                                actor_person_uid: actor,
                            },
                            now,
                        )
                        .await?;
                    apply_program_mutation(commit, &mut outcome)?;
                    self.publish_karma_definition(&outcome, KarmaKind::Program)
                        .await?;
                }
                Action::ActivateKarmaProgram {
                    request_id,
                    program_uid,
                    expected_handle_revision,
                    revision_hash,
                } => {
                    let commit = self
                        .activate_karma_program(
                            store::karma::programs::ActivateProgramInput {
                                request_id,
                                program_uid,
                                expected_handle_revision,
                                revision_hash,
                                actor_person_uid: actor,
                            },
                            now,
                        )
                        .await?;
                    apply_program_mutation(commit, &mut outcome)?;
                    self.publish_karma_definition(&outcome, KarmaKind::Program)
                        .await?;
                }
                Action::PauseKarmaProgram {
                    request_id,
                    program_uid,
                    expected_handle_revision,
                } => {
                    let commit = self
                        .pause_karma_program(
                            store::karma::programs::PauseProgramInput {
                                request_id,
                                program_uid,
                                expected_handle_revision,
                                actor_person_uid: actor,
                            },
                            now,
                        )
                        .await?;
                    apply_program_mutation(commit, &mut outcome)?;
                    self.publish_karma_definition(&outcome, KarmaKind::Program)
                        .await?;
                }
                Action::SetKarmaExecution {
                    program_uid,
                    executes,
                    note,
                } => {
                    store::karma::execution::set_executes(
                        &self.store.pool,
                        &program_uid,
                        executes,
                        note.as_deref(),
                        now,
                    )
                    .await?;
                    outcome.warnings.push(if executes {
                    "This Cell now runs that rule.".into()
                } else {
                    "This Cell now holds that rule without running it. Other Cells are unchanged."
                        .into()
                });
                }
                Action::DesignateKarmaExecutor { target, cell_uid } => {
                    let target = match store::recurrence::get(&self.store.pool, &target).await? {
                        Some(rule) => rule.record_uid,
                        None => self.resolve(&target).await?,
                    };
                    if let Some(cell) = &cell_uid {
                        let organ = store::organs::local(&self.store.pool)
                            .await?
                            .ok_or_else(|| EngineError::Consequence("No local Organ".into()))?;
                        let roster = self.roster_of(&organ.uid).await?.ok_or_else(|| {
                            EngineError::Consequence(
                                "Executor selection needs a signed Organ roster".into(),
                            )
                        })?;
                        if !crate::roster::roster_signature_is_valid(&roster)
                            || !roster
                                .roster
                                .cells
                                .iter()
                                .any(|member| &member.cell_uid == cell)
                        {
                            return Err(EngineError::Consequence("Choose an enrolled Cell".into()));
                        }
                    }
                    Box::pin(self.act(
                        Action::SetExtension {
                            target,
                            namespace: store::executor::NAMESPACE.into(),
                            fds: serde_json::json!({"cell":cell_uid}),
                        },
                        actor.clone(),
                    ))
                    .await?;
                    outcome.warnings.push("Executor selection applies to this target Record's Rules. The Cell also needs signed Karma execution permission.".into());
                }
                Action::DesignateTransferExecutor {
                    transfer_uid,
                    cell_uid,
                } => {
                    let transfer = self.resolve(&transfer_uid).await?;
                    self.require_transfer_editor(&transfer, actor.as_deref())
                        .await?;
                    self.require_transfer_state_writer("transfer", &[&transfer])
                        .await?;
                    let local = store::cells::local(&self.store.pool)
                        .await?
                        .ok_or_else(|| {
                            EngineError::Consequence(
                                "Transfer delivery requires a writing Cell".into(),
                            )
                        })?;
                    if cell_uid.as_ref().is_some_and(|cell| cell != &local.uid) {
                        return Err(EngineError::Conflict { code: "transfer_origin_cell_required", message: "Transfer delivery and its queue stay on the Transfer's writing Cell".into() });
                    }
                    store::executor::designate(&self.store.pool, &transfer, Some(&local.uid))
                        .await?;
                    outcome.warnings.push("This Transfer's writing Cell delivers it. Deliveries wait while that Cell is offline.".into());
                }
                Action::RespondKarmaCandidate {
                    request_id,
                    candidate_hash,
                    expected_state_revision,
                    response,
                } => {
                    let commit = self
                        .respond_karma_candidate(
                            store::karma::candidates::RespondCandidateInput {
                                request_id,
                                candidate_hash,
                                expected_state_revision,
                                response,
                                actor_person_uid: actor,
                            },
                            now,
                        )
                        .await?;
                    apply_candidate_review(commit, &mut outcome)?;
                }
                Action::SaveKarmaFrequency {
                    request_id,
                    frequency_uid,
                    expected_handle_revision,
                    frequency,
                    restart,
                } => {
                    let commit = Box::pin(self.save_karma_frequency(
                        request_id,
                        frequency_uid,
                        expected_handle_revision,
                        frequency,
                        restart,
                        actor,
                        now,
                    ))
                    .await?;
                    apply_frequency_mutation(commit, &mut outcome)?;
                    self.publish_karma_definition(&outcome, KarmaKind::Frequency)
                        .await?;
                }
                Action::CreateKarmaFrequency {
                    request_id,
                    frequency,
                    owner_person_uid,
                } => {
                    let commit = self
                        .create_karma_frequency(
                            store::karma::frequencies::CreateFrequencyInput {
                                request_id,
                                frequency,
                                owner_person_uid,
                                actor_person_uid: actor,
                            },
                            now,
                        )
                        .await?;
                    apply_frequency_mutation(commit, &mut outcome)?;
                    self.publish_karma_definition(&outcome, KarmaKind::Frequency)
                        .await?;
                }
                Action::ReviseKarmaFrequency {
                    request_id,
                    frequency_uid,
                    expected_handle_revision,
                    frequency,
                } => {
                    let commit = self
                        .revise_karma_frequency(
                            store::karma::frequencies::ReviseFrequencyInput {
                                request_id,
                                frequency_uid,
                                expected_handle_revision,
                                frequency,
                                actor_person_uid: actor,
                            },
                            now,
                        )
                        .await?;
                    apply_frequency_mutation(commit, &mut outcome)?;
                    self.publish_karma_definition(&outcome, KarmaKind::Frequency)
                        .await?;
                }
                Action::ActivateKarmaFrequency {
                    request_id,
                    frequency_uid,
                    expected_handle_revision,
                    revision_hash,
                    parameter_overrides,
                } => {
                    let runtime = self.configured_karma_runtime()?;
                    let commit = self
                        .activate_karma_frequency(
                            store::karma::frequencies::ActivateFrequencyInput {
                                request_id,
                                frequency_uid,
                                expected_handle_revision,
                                revision_hash,
                                parameter_overrides,
                                actor_person_uid: actor,
                            },
                            &runtime,
                            now,
                        )
                        .await?;
                    apply_frequency_mutation(commit, &mut outcome)?;
                    self.publish_karma_definition(&outcome, KarmaKind::Frequency)
                        .await?;
                }
                Action::SetKarmaFrequencyParameters {
                    request_id,
                    frequency_uid,
                    expected_handle_revision,
                    expected_active_revision_hash,
                    parameter_overrides,
                } => {
                    let runtime = self.configured_karma_runtime()?;
                    let commit = self
                        .set_karma_frequency_parameters(
                            store::karma::frequencies::SetFrequencyParametersInput {
                                request_id,
                                frequency_uid,
                                expected_handle_revision,
                                expected_active_revision_hash,
                                parameter_overrides,
                                actor_person_uid: actor,
                            },
                            &runtime,
                            now,
                        )
                        .await?;
                    apply_frequency_mutation(commit, &mut outcome)?;
                    self.publish_karma_definition(&outcome, KarmaKind::Frequency)
                        .await?;
                }
                Action::ResetKarmaFrequencyParameters {
                    request_id,
                    frequency_uid,
                    expected_handle_revision,
                    expected_active_revision_hash,
                } => {
                    let runtime = self.configured_karma_runtime()?;
                    let commit = self
                        .reset_karma_frequency_parameters(
                            store::karma::frequencies::ResetFrequencyParametersInput {
                                request_id,
                                frequency_uid,
                                expected_handle_revision,
                                expected_active_revision_hash,
                                actor_person_uid: actor,
                            },
                            &runtime,
                            now,
                        )
                        .await?;
                    apply_frequency_mutation(commit, &mut outcome)?;
                    self.publish_karma_definition(&outcome, KarmaKind::Frequency)
                        .await?;
                }
                Action::PauseKarmaFrequency {
                    request_id,
                    frequency_uid,
                    expected_handle_revision,
                } => {
                    let commit = self
                        .pause_karma_frequency(
                            store::karma::frequencies::PauseFrequencyInput {
                                request_id,
                                frequency_uid,
                                expected_handle_revision,
                                actor_person_uid: actor,
                            },
                            now,
                        )
                        .await?;
                    apply_frequency_mutation(commit, &mut outcome)?;
                    self.publish_karma_definition(&outcome, KarmaKind::Frequency)
                        .await?;
                }
                Action::Decide { decision, answer } => {
                    store::misc::answer_decision(&self.store.pool, &decision, &answer).await?;
                    let chosen_action = store::misc::list_decisions(&self.store.pool)
                        .await?
                        .into_iter()
                        .find(|d| d.record_uid == decision)
                        .and_then(|d| {
                            d.options.as_array()?.iter().find_map(|option| {
                                (option.get("label")?.as_str()? == answer)
                                    .then(|| option.get("action").cloned())
                                    .flatten()
                            })
                        });
                    let current = store::records::quantity(&self.store.pool, &decision)
                        .await?
                        .unwrap_or_else(store::exact::zero);
                    if !current.is_zero() {
                        outcome.facts = self
                            .append(
                                NewFact {
                                    uid: None,
                                    record_uid: decision,
                                    delta: store::exact::negate(current)?,
                                    at: None,
                                    actor_uid: actor.clone(),
                                    cause: Cause {
                                        kind: CauseKind::Action,
                                        uid: None,
                                    },
                                    payload: Some(
                                        serde_json::json!({ "answer": answer }).to_string(),
                                    ),
                                },
                                now,
                            )
                            .await?;
                    }
                    if let Some(action_json) = chosen_action {
                        let action: Action = serde_json::from_value(action_json).map_err(|e| {
                            EngineError::Consequence(format!("bad option action: {e}"))
                        })?;
                        let inner = Box::pin(self.act_at(action, actor, now)).await?;
                        outcome.facts.extend(inner.facts);
                        outcome.warnings.extend(inner.warnings);
                        if outcome.created.is_none() {
                            outcome.created = inner.created;
                        }
                    }
                }
                Action::SaveKarmaCommand {
                    target,
                    expected_revision,
                    slug,
                    head,
                    configuration,
                    host,
                } => {
                    outcome.created = Some(
                        self.save_command(
                            target,
                            expected_revision,
                            slug,
                            head,
                            configuration,
                            host,
                            actor.as_deref(),
                        )
                        .await?,
                    );
                }
                Action::RunKarmaCommand {
                    command,
                    request_id,
                    numeric,
                } => {
                    self.run_saved_command(&command, &request_id, numeric, actor.as_deref())
                        .await?;
                    outcome.data =
                        Some(serde_json::json!({"request":request_id,"status":"queued"}));
                }
                Action::InspectKarmaCommands => {
                    outcome.data = Some(self.inspect_commands(actor.as_deref()).await?);
                }
                Action::CreateSignal {
                    slug,
                    head,
                    source_kind,
                    source,
                    schedule,
                } => {
                    self.require_permission(actor.as_deref(), "organ:update")
                        .await?;
                    if source_kind != "command"
                        || nucleus::parse_duration(&schedule).is_none_or(|seconds| seconds <= 0)
                    {
                        return Err(EngineError::Consequence(
                            "Signal needs a command and a positive sampling period".into(),
                        ));
                    }
                    outcome.created = Some(
                        store::misc::create_signal(
                            &self.store.pool,
                            store::misc::NewSignal {
                                slug: &slug,
                                head: &head,
                                source_kind: &source_kind,
                                source: &source,
                                schedule: &schedule,
                            },
                        )
                        .await?,
                    );
                    store::sqlx::query("UPDATE signal SET actor_uid = ? WHERE record_uid = ?")
                        .bind(actor.as_deref())
                        .bind(outcome.created.as_deref())
                        .execute(&self.store.pool)
                        .await?;
                }
                Action::CreateMatchRule {
                    slug,
                    head,
                    watch_concept,
                    max_proximity,
                    min_confidence,
                    auto,
                } => {
                    if !matches!(auto.as_str(), "draft_only" | "ask" | "auto_propose") {
                        return Err(EngineError::Consequence(format!(
                            "auto must be draft_only | ask | auto_propose, not `{auto}`"
                        )));
                    }
                    outcome.created = Some(
                        store::senses::create_sense_rule(
                            &self.store.pool,
                            store::senses::NewSenseRule {
                                slug: &slug,
                                head: &head,
                                watch_concept: watch_concept.as_deref(),
                                max_proximity,
                                min_confidence,
                                auto: &auto,
                            },
                        )
                        .await?,
                    );
                }
                Action::AdoptConcepts { concepts } => {
                    for seed in &concepts {
                        store::concepts::adopt(
                            &self.store.pool,
                            &seed.uid,
                            &seed.name,
                            seed.origin.as_deref(),
                            &seed.parents,
                        )
                        .await?;
                    }
                }
                _ => {
                    return Err(EngineError::Consequence(
                        "Invalid internal action dispatch".into(),
                    ));
                }
            }
            Ok(ControlFlow::Continue(outcome))
        })
    }
}
