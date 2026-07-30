use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use nucleus::karma::{CompiledSchedule, FrequencyParameterValue, LocalId};
use store::karma::frequencies::{
    ActivateFrequencyInput, CreateFrequencyInput, FrequencyMutationCommit,
    FrequencyRuntimeAdmission, PauseFrequencyInput, ResetFrequencyParametersInput,
    ReviseFrequencyInput, SetFrequencyParametersInput,
};
use store::karma::programs::{
    ActivateProgramInput, CreateProgramInput, PauseProgramInput, ProgramMutationCommit,
    ReviseProgramInput,
};

use crate::karma_runtime::KarmaDeadlineDirectorConfig;
use crate::{Engine, EngineError};

impl Engine {
    pub async fn respond_karma_candidate(
        &self,
        input: store::karma::candidates::RespondCandidateInput,
        now: DateTime<Utc>,
    ) -> Result<store::karma::candidates::CandidateReviewCommit, EngineError> {
        let signer = self.signer.lock().await.clone();
        let commit = store::karma::candidates::respond(&self.store.pool, input, now, |hash| {
            signer.as_ref().map(|value| value.sign_hash(hash))
        })
        .await?;
        if let store::karma::candidates::CandidateReviewCommit::Committed { fact, .. } = &commit {
            let _ = self.observe_committed_fact(fact.clone(), now).await?;
        }
        Ok(commit)
    }

    pub async fn create_karma_program(
        &self,
        input: CreateProgramInput,
        now: DateTime<Utc>,
    ) -> Result<ProgramMutationCommit, EngineError> {
        let signer = self.signer.lock().await.clone();
        let commit = store::karma::programs::create(&self.store.pool, input, now, |hash| {
            signer.as_ref().map(|value| value.sign_hash(hash))
        })
        .await?;
        self.finish_program_mutation(commit, now).await
    }

    pub async fn revise_karma_program(
        &self,
        input: ReviseProgramInput,
        now: DateTime<Utc>,
    ) -> Result<ProgramMutationCommit, EngineError> {
        let signer = self.signer.lock().await.clone();
        let commit = store::karma::programs::revise(&self.store.pool, input, now, |hash| {
            signer.as_ref().map(|value| value.sign_hash(hash))
        })
        .await?;
        self.finish_program_mutation(commit, now).await
    }

    pub async fn activate_karma_program(
        &self,
        input: ActivateProgramInput,
        now: DateTime<Utc>,
    ) -> Result<ProgramMutationCommit, EngineError> {
        let signer = self.signer.lock().await.clone();
        let commit = store::karma::programs::activate(&self.store.pool, input, now, |hash| {
            signer.as_ref().map(|value| value.sign_hash(hash))
        })
        .await?;
        self.finish_program_mutation(commit, now).await
    }

    pub async fn pause_karma_program(
        &self,
        input: PauseProgramInput,
        now: DateTime<Utc>,
    ) -> Result<ProgramMutationCommit, EngineError> {
        let signer = self.signer.lock().await.clone();
        let commit = store::karma::programs::pause(&self.store.pool, input, now, |hash| {
            signer.as_ref().map(|value| value.sign_hash(hash))
        })
        .await?;
        self.finish_program_mutation(commit, now).await
    }

    pub async fn create_karma_frequency(
        &self,
        input: CreateFrequencyInput,
        now: DateTime<Utc>,
    ) -> Result<FrequencyMutationCommit, EngineError> {
        let signer = self.signer.lock().await.clone();
        let commit = store::karma::frequencies::create(&self.store.pool, input, now, |hash| {
            signer.as_ref().map(|value| value.sign_hash(hash))
        })
        .await?;
        self.finish_frequency_mutation(commit, now, false).await
    }

    pub async fn revise_karma_frequency(
        &self,
        input: ReviseFrequencyInput,
        now: DateTime<Utc>,
    ) -> Result<FrequencyMutationCommit, EngineError> {
        let signer = self.signer.lock().await.clone();
        let commit = store::karma::frequencies::revise(&self.store.pool, input, now, |hash| {
            signer.as_ref().map(|value| value.sign_hash(hash))
        })
        .await?;
        self.finish_frequency_mutation(commit, now, false).await
    }

    pub async fn activate_karma_frequency(
        &self,
        input: ActivateFrequencyInput,
        runtime: &KarmaDeadlineDirectorConfig,
        now: DateTime<Utc>,
    ) -> Result<FrequencyMutationCommit, EngineError> {
        let admission = self
            .frequency_admission(&input.revision_hash, &input.parameter_overrides, runtime)
            .await?;
        let signer = self.signer.lock().await.clone();
        let commit = store::karma::frequencies::activate_admitted(
            &self.store.pool,
            input,
            admission,
            now,
            |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
        )
        .await?;
        self.finish_frequency_mutation(commit, now, true).await
    }

    pub async fn set_karma_frequency_parameters(
        &self,
        input: SetFrequencyParametersInput,
        runtime: &KarmaDeadlineDirectorConfig,
        now: DateTime<Utc>,
    ) -> Result<FrequencyMutationCommit, EngineError> {
        let admission = self
            .frequency_admission(
                &input.expected_active_revision_hash,
                &input.parameter_overrides,
                runtime,
            )
            .await?;
        let signer = self.signer.lock().await.clone();
        let commit = store::karma::frequencies::set_parameters_admitted(
            &self.store.pool,
            input,
            admission,
            now,
            |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
        )
        .await?;
        self.finish_frequency_mutation(commit, now, true).await
    }

    pub async fn reset_karma_frequency_parameters(
        &self,
        input: ResetFrequencyParametersInput,
        runtime: &KarmaDeadlineDirectorConfig,
        now: DateTime<Utc>,
    ) -> Result<FrequencyMutationCommit, EngineError> {
        let overrides = BTreeMap::new();
        let admission = self
            .frequency_admission(&input.expected_active_revision_hash, &overrides, runtime)
            .await?;
        let signer = self.signer.lock().await.clone();
        let commit = store::karma::frequencies::reset_parameters_admitted(
            &self.store.pool,
            input,
            admission,
            now,
            |hash| signer.as_ref().map(|value| value.sign_hash(hash)),
        )
        .await?;
        self.finish_frequency_mutation(commit, now, true).await
    }

    pub async fn pause_karma_frequency(
        &self,
        input: PauseFrequencyInput,
        now: DateTime<Utc>,
    ) -> Result<FrequencyMutationCommit, EngineError> {
        let signer = self.signer.lock().await.clone();
        let commit = store::karma::frequencies::pause(&self.store.pool, input, now, |hash| {
            signer.as_ref().map(|value| value.sign_hash(hash))
        })
        .await?;
        self.finish_frequency_mutation(commit, now, true).await
    }

    async fn frequency_admission<'a>(
        &self,
        revision_hash: &nucleus::karma::CanonicalHash,
        parameter_overrides: &BTreeMap<LocalId, FrequencyParameterValue>,
        runtime: &'a KarmaDeadlineDirectorConfig,
    ) -> Result<FrequencyRuntimeAdmission<'a>, EngineError> {
        let revision = store::karma::frequencies::get_revision(&self.store.pool, revision_hash)
            .await?
            .ok_or_else(|| EngineError::Conflict {
                code: "karma_frequency_revision_missing",
                message: format!(
                    "Karma Frequency revision {} does not exist",
                    revision_hash.as_str()
                ),
            })?;
        let compiled = revision
            .frequency
            .compile(parameter_overrides)
            .map_err(|error| EngineError::Conflict {
                code: "karma_frequency_compile",
                message: error.to_string(),
            })?;
        let calendar_provider = match &compiled.schedule {
            CompiledSchedule::Elapsed { .. } => None,
            CompiledSchedule::Calendar { schedule } => Some(
                runtime
                    .provider(&schedule.tzdb)
                    .ok_or_else(|| EngineError::Conflict {
                        code: "karma_calendar_provider_unavailable",
                        message: format!(
                            "no pinned provider is installed for tzdb {} ({})",
                            schedule.tzdb.version.as_str(),
                            schedule.tzdb.digest.as_str()
                        ),
                    })?,
            ),
        };
        Ok(FrequencyRuntimeAdmission {
            host: &runtime.host,
            grant: &runtime.grant,
            calendar_provider,
            demand_policy: store::karma::schedules::ScheduleDemandPolicy {
                workload: runtime.workload,
                calibration: runtime.calibration,
            },
            demand_capacity: runtime.demand_capacity,
        })
    }

    async fn finish_frequency_mutation(
        &self,
        commit: FrequencyMutationCommit,
        now: DateTime<Utc>,
        deadline_changed: bool,
    ) -> Result<FrequencyMutationCommit, EngineError> {
        let fact = match &commit {
            FrequencyMutationCommit::Committed { fact, .. } => Some(fact.clone()),
            FrequencyMutationCommit::Replayed { .. } | FrequencyMutationCommit::Stale { .. } => {
                None
            }
        };
        if fact.is_some() && deadline_changed {
            self.notify_karma_deadline_change();
        }
        if let Some(fact) = fact {
            let _ = self.observe_committed_fact(fact, now).await?;
        }
        Ok(commit)
    }

    async fn finish_program_mutation(
        &self,
        commit: ProgramMutationCommit,
        now: DateTime<Utc>,
    ) -> Result<ProgramMutationCommit, EngineError> {
        let fact = match &commit {
            ProgramMutationCommit::Committed { fact, .. } => Some(fact.clone()),
            ProgramMutationCommit::Replayed { .. } | ProgramMutationCommit::Stale { .. } => None,
        };
        if let Some(fact) = fact {
            let _ = self.observe_committed_fact(fact, now).await?;
        }
        Ok(commit)
    }
}
