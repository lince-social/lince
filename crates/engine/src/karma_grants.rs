use chrono::{DateTime, Utc};
use nucleus::karma::{
    CanonicalHash, DelegationGrantSpec, DelegationSignature, GrantAuthorityRequest, ReferenceKind,
    Slug, TypedUid,
};
use store::karma::grants::{
    ActivateGrantInput, CreateGrantInput, GrantAuthorityEvaluation, GrantHandleRow,
    GrantMutationCommit, GrantRevisionRow, NarrowGrantInput, RevokeGrantInput,
};

use crate::trust::Signer;
use crate::{Engine, EngineError};

pub(crate) struct GrantPrincipal {
    person: TypedUid,
    signer: Signer,
}

impl GrantPrincipal {
    fn uid(&self) -> String {
        self.person.as_str().to_string()
    }

    fn sign(&self) -> impl Fn(&str) -> Option<DelegationSignature> + Send + Sync + '_ {
        move |hash: &str| {
            Some(DelegationSignature {
                signer_person_uid: self.person.clone(),
                key_id: self.signer.key_id.clone(),
                signature: self.signer.sign_hash(hash),
            })
        }
    }
}

impl Engine {
    pub(crate) async fn apply_karma_grant_action(
        &self,
        action: crate::actions::Action,
        actor: Option<&str>,
        now: DateTime<Utc>,
        outcome: &mut crate::actions::ActionOutcome,
    ) -> Result<(), EngineError> {
        use crate::actions::Action;
        let commit = match action {
            Action::CreateKarmaGrant {
                request_id,
                slug,
                grant,
            } => {
                self.require_permission(actor, "karma:create").await?;
                self.create_karma_grant(actor, request_id, slug, grant, now)
                    .await?
            }
            Action::NarrowKarmaGrant {
                request_id,
                grant_uid,
                expected_handle_revision,
                grant,
            } => {
                self.require_permission(actor, "karma:update").await?;
                self.narrow_karma_grant(
                    actor,
                    request_id,
                    grant_uid,
                    expected_handle_revision,
                    grant,
                    now,
                )
                .await?
            }
            Action::ActivateKarmaGrant {
                request_id,
                grant_uid,
                expected_handle_revision,
                revision_hash,
            } => {
                self.require_permission(actor, "karma:update").await?;
                self.activate_karma_grant(
                    actor,
                    request_id,
                    grant_uid,
                    expected_handle_revision,
                    revision_hash,
                    now,
                )
                .await?
            }
            Action::RevokeKarmaGrant {
                request_id,
                grant_uid,
                expected_handle_revision,
            } => {
                self.require_permission(actor, "karma:update").await?;
                self.revoke_karma_grant(actor, request_id, grant_uid, expected_handle_revision, now)
                    .await?
            }
            _ => {
                return Err(EngineError::Conflict {
                    code: "karma_grant_action_unsupported",
                    message: "only Karma grant Actions reach the grant boundary".to_string(),
                });
            }
        };
        crate::actions::apply_grant_mutation(commit, outcome)
    }

    pub async fn create_karma_grant(
        &self,
        actor: Option<&str>,
        request_id: String,
        slug: Slug,
        grant: DelegationGrantSpec,
        now: DateTime<Utc>,
    ) -> Result<GrantMutationCommit, EngineError> {
        let principal = self.karma_grant_principal(actor).await?;
        let commit = store::karma::grants::create(
            &self.store.pool,
            CreateGrantInput {
                request_id,
                slug,
                grant,
                actor_person_uid: principal.uid(),
            },
            now,
            principal.sign(),
        )
        .await?;
        self.finish_grant_mutation(commit, now).await
    }

    pub async fn narrow_karma_grant(
        &self,
        actor: Option<&str>,
        request_id: String,
        grant_uid: String,
        expected_handle_revision: u64,
        grant: DelegationGrantSpec,
        now: DateTime<Utc>,
    ) -> Result<GrantMutationCommit, EngineError> {
        let principal = self.karma_grant_principal(actor).await?;
        let commit = store::karma::grants::narrow(
            &self.store.pool,
            NarrowGrantInput {
                request_id,
                grant_uid,
                expected_handle_revision,
                grant,
                actor_person_uid: principal.uid(),
            },
            now,
            principal.sign(),
        )
        .await?;
        self.finish_grant_mutation(commit, now).await
    }

    pub async fn activate_karma_grant(
        &self,
        actor: Option<&str>,
        request_id: String,
        grant_uid: String,
        expected_handle_revision: u64,
        revision_hash: CanonicalHash,
        now: DateTime<Utc>,
    ) -> Result<GrantMutationCommit, EngineError> {
        let principal = self.karma_grant_principal(actor).await?;
        let commit = store::karma::grants::activate(
            &self.store.pool,
            ActivateGrantInput {
                request_id,
                grant_uid,
                expected_handle_revision,
                revision_hash,
                actor_person_uid: principal.uid(),
            },
            now,
            principal.sign(),
        )
        .await?;
        self.finish_grant_mutation(commit, now).await
    }

    pub async fn revoke_karma_grant(
        &self,
        actor: Option<&str>,
        request_id: String,
        grant_uid: String,
        expected_handle_revision: u64,
        now: DateTime<Utc>,
    ) -> Result<GrantMutationCommit, EngineError> {
        let principal = self.karma_grant_principal(actor).await?;
        let commit = store::karma::grants::revoke(
            &self.store.pool,
            RevokeGrantInput {
                request_id,
                grant_uid,
                expected_handle_revision,
                actor_person_uid: principal.uid(),
            },
            now,
            principal.sign(),
        )
        .await?;
        self.finish_grant_mutation(commit, now).await
    }

    pub async fn evaluate_karma_grant(
        &self,
        grant_uid: &str,
        request: &GrantAuthorityRequest,
    ) -> Result<GrantAuthorityEvaluation, EngineError> {
        Ok(store::karma::grants::evaluate_active(&self.store.pool, grant_uid, request).await?)
    }

    pub async fn get_karma_grant(
        &self,
        grant_uid: &str,
    ) -> Result<Option<GrantHandleRow>, EngineError> {
        Ok(store::karma::grants::get_handle(&self.store.pool, grant_uid).await?)
    }

    pub async fn get_karma_grant_revision(
        &self,
        grant_uid: &str,
        revision_hash: &CanonicalHash,
    ) -> Result<Option<GrantRevisionRow>, EngineError> {
        Ok(store::karma::grants::get_revision(&self.store.pool, grant_uid, revision_hash).await?)
    }

    async fn karma_grant_principal(
        &self,
        actor: Option<&str>,
    ) -> Result<GrantPrincipal, EngineError> {
        let signer = self.signer.lock().await.clone().ok_or_else(|| {
            EngineError::Forbidden(
                "Karma grants require an installed signing key; none is available".into(),
            )
        })?;
        let person =
            TypedUid::new(ReferenceKind::Person, signer.actor_uid.clone()).map_err(|_| {
                EngineError::Forbidden(
                    "the installed signing key does not belong to a Person and cannot hold a Karma grant"
                        .into(),
                )
            })?;
        if let Some(session_person) = self.actor_person(actor).await?
            && session_person != signer.actor_uid
        {
            return Err(EngineError::Forbidden(
                "a Karma grant is signed by its principal; this session cannot sign for another Person"
                    .into(),
            ));
        }
        Ok(GrantPrincipal { person, signer })
    }

    async fn finish_grant_mutation(
        &self,
        commit: GrantMutationCommit,
        now: DateTime<Utc>,
    ) -> Result<GrantMutationCommit, EngineError> {
        if let GrantMutationCommit::Committed { fact, .. } = &commit {
            let _ = self.observe_committed_fact(fact.clone(), now).await?;
        }
        Ok(commit)
    }
}
