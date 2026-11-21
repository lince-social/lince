// non-lean-yet

use super::super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchOutcome, Engine, KarmaKind, Utc,
    apply_frequency_mutation, invalid_dispatch,
};

impl Engine {
    pub(in super::super) async fn dispatch_karma_frequencies(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
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
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
