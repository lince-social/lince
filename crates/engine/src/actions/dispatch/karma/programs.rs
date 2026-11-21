// non-lean-yet

use super::super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError, KarmaKind,
    Utc, apply_candidate_review, apply_program_mutation, invalid_dispatch,
};

impl Engine {
    pub(in super::super) async fn dispatch_karma_programs(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
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
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
