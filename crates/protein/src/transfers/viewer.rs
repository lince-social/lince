//! Viewer identity and capabilities used by Transfer projections.

use crate::ProteinError;
use std::collections::HashSet;
use store::{Store, auth::user_by_uid};

#[derive(Debug, Default)]
pub(super) struct TransferViewer {
    pub(super) local: bool,
    pub(super) recognized: bool,
    pub(super) subject: Option<String>,
    pub(super) person: Option<String>,
    permissions: HashSet<String>,
}

pub(super) struct AgreementTransition<'a> {
    pub person: &'a str,
    pub party: &'a str,
    pub level: u8,
    pub revision: u64,
    pub revision_signed: bool,
    pub coalition: Option<&'a HashSet<&'a str>>,
    pub has_signer: bool,
}

impl TransferViewer {
    pub(super) fn delivery(organ_uid: &str, person_uid: &str) -> Self {
        Self {
            local: false,
            recognized: true,
            subject: Some(organ_uid.to_string()),
            person: Some(person_uid.to_string()),
            permissions: HashSet::from(["transfer:update".into()]),
        }
    }

    pub(super) async fn resolve(
        store: &Store,
        subject: Option<&str>,
    ) -> Result<Self, ProteinError> {
        let Some(subject) = subject else {
            return Ok(Self {
                local: true,
                recognized: true,
                ..Self::default()
            });
        };
        let Some(user) = user_by_uid(&store.pool, subject).await? else {
            return Ok(Self {
                subject: Some(subject.to_string()),
                ..Self::default()
            });
        };
        Ok(Self {
            local: false,
            recognized: true,
            subject: Some(subject.to_string()),
            person: Some(user.uid),
            permissions: user.permissions.into_iter().collect(),
        })
    }

    pub(super) fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }

    fn identity_blockers(&self) -> Vec<&'static str> {
        let mut blockers = Vec::new();
        if !self.local {
            if !self.recognized {
                blockers.push("auth_subject_unrecognized");
            }
            if self.person.is_none() {
                blockers.push("missing_person_identity");
            }
        }
        blockers
    }

    pub(super) fn create_blockers(&self) -> Vec<&'static str> {
        let mut blockers = self.identity_blockers();
        if !self.local && !self.has_permission("transfer:create") {
            blockers.push("missing_transfer_create_permission");
        }
        blockers
    }

    pub(super) fn update_identity_blockers(&self) -> Vec<&'static str> {
        let mut blockers = self.identity_blockers();
        if !self.local && !self.has_permission("transfer:update") {
            blockers.push("missing_transfer_update_permission");
        }
        blockers
    }

    pub(super) fn negotiation_write_blockers(
        &self,
        is_creator: bool,
        is_participant: bool,
        is_pending_addressee: bool,
    ) -> Vec<&'static str> {
        let mut blockers = self.identity_blockers();
        if !self.local && !is_creator && !is_participant && !is_pending_addressee {
            blockers.push("not_transfer_negotiation_participant");
        }
        blockers
    }

    pub(super) fn agreement_transition_blockers(
        &self,
        transition: &AgreementTransition<'_>,
        target_level: u8,
    ) -> Vec<&'static str> {
        let mut blockers = self.update_identity_blockers();
        if !self.local && self.person.as_deref() != Some(transition.person) {
            blockers.push("cannot_author_another_party_agreement");
        }
        if transition.revision == 0 {
            blockers.push("legacy_transfer_requires_adoption");
        } else if !transition.revision_signed {
            blockers.push("transfer_revision_signature_required");
        }
        if !transition.has_signer {
            blockers.push("missing_person_signer");
        }
        if transition.level.abs_diff(target_level) != 1 {
            blockers.push("agreement_transition_must_be_adjacent");
        }
        if target_level > transition.level
            && transition
                .coalition
                .is_some_and(|members| !members.contains(transition.party))
        {
            blockers.push("percentage_coalition_frozen_without_party");
        }
        blockers
    }

    pub(super) fn draft_edit_blockers(
        &self,
        is_creator: bool,
        revision: u64,
        terms_editable: bool,
    ) -> Vec<&'static str> {
        let mut blockers = self.draft_creator_blockers(is_creator);
        if revision == 0 {
            blockers.push("legacy_transfer_requires_adoption");
        }
        if !terms_editable {
            blockers.push("transfer_terms_are_no_longer_draft_editable");
        }
        blockers
    }

    pub(super) fn draft_adopt_blockers(
        &self,
        is_creator: bool,
        revision: u64,
        terms_editable: bool,
    ) -> Vec<&'static str> {
        let mut blockers = self.draft_creator_blockers(is_creator);
        if revision != 0 {
            blockers.push("transfer_already_revisioned");
        }
        if !terms_editable {
            blockers.push("transfer_terms_are_no_longer_draft_editable");
        }
        blockers
    }

    fn draft_creator_blockers(&self, is_creator: bool) -> Vec<&'static str> {
        let mut blockers = self.update_identity_blockers();
        if !self.local && !is_creator {
            blockers.push("not_transfer_creator");
        }
        blockers
    }
}
