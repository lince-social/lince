use std::collections::HashMap;

use nucleus::transfer::TransferRemainderPolicy;
use store::transfers::{
    OccurrenceDisputeEventRow, OccurrenceSettlementProgress, TransferOccurrenceRow,
};

use super::Occurrences;

pub(super) struct Actions<'a> {
    identity: Vec<&'static str>,
    pub acting: Option<&'a str>,
    pub correction_actor: Option<&'a str>,
    pub can_cancel: bool,
    pub report_as: Option<&'a str>,
    signer: Option<&'a str>,
    viewer_person: Option<&'a str>,
    creator: Option<&'a str>,
    local: bool,
}

pub(super) struct Rules {
    pub delivery: Vec<&'static str>,
    pub receipt: Vec<&'static str>,
    pub remainder: Vec<&'static str>,
    pub reverse: Vec<&'static str>,
    pub dispute: Vec<&'static str>,
    pub retract_dispute: Vec<&'static str>,
    pub participant_disputed: bool,
}

impl<'a> Actions<'a> {
    pub(super) fn new(input: &Occurrences<'a>) -> Self {
        let viewer = input.viewer;
        let signer = input.installed_signer_actor;
        let viewer_person = viewer.person.as_deref();
        let acting = signer.or(viewer_person);
        let identity = viewer.update_identity_blockers();
        let can_cancel = identity.is_empty()
            && acting
                .is_some_and(|person| input.parties.iter().any(|(_, actor, _)| actor == person));
        let correction_actor = if viewer.local || viewer_person == signer {
            signer
        } else {
            None
        };
        Self {
            report_as: input
                .create_person
                .filter(|person| identity.is_empty() && signer == Some(*person)),
            identity,
            acting,
            correction_actor,
            can_cancel,
            signer,
            viewer_person,
            creator: input.creator_person,
            local: viewer.local,
        }
    }

    pub(super) fn source_owner(&self, owner: Option<&str>) -> bool {
        let viewer = if self.local {
            self.signer
        } else {
            self.viewer_person
        };
        owner.is_some() && owner == viewer
    }

    pub(super) fn settlement_blockers(&self, owner: Option<&str>) -> Vec<&'static str> {
        self.blockers([
            (!self.source_owner(owner), "not_source_promise_owner"),
            (
                owner.is_none() || self.signer != owner,
                "missing_person_signer",
            ),
        ])
    }

    pub(super) fn for_occurrence(
        &self,
        occurrence: &TransferOccurrenceRow,
        progress: &OccurrenceSettlementProgress,
        owner: Option<&str>,
        remainder_policy: Option<TransferRemainderPolicy>,
        events: &[OccurrenceDisputeEventRow],
    ) -> Rules {
        let actor = self.correction_actor;
        let participant = actor.is_some_and(|person| participates(occurrence, person));
        let disputes: HashMap<_, _> = events
            .iter()
            .map(|event| (event.actor_person_uid.as_str(), event.disputed))
            .collect();
        let actor_disputes = actor
            .and_then(|person| disputes.get(person))
            .copied()
            .unwrap_or_default();

        let delivery = self.claim(&occurrence.giver_person_uid, "not_occurrence_giver");
        let receipt = self.claim(&occurrence.receiver_person_uid, "not_occurrence_receiver");
        let remainder = self.blockers([
            (!self.source_owner(owner), "not_source_promise_owner"),
            (actor.is_none() || actor != owner, "missing_person_signer"),
            (
                !progress.partially_settled || progress.remaining_quantity <= 0.0,
                "occurrence_has_no_partial_remainder",
            ),
            (
                remainder_policy != Some(TransferRemainderPolicy::LocalDraft),
                "remainder_policy_not_local_draft",
            ),
        ]);
        let reverse = self.blockers([
            (
                actor.is_none() || actor != self.creator || self.signer != self.creator,
                "not_transfer_creator_with_signer",
            ),
            (!participant, "creator_not_occurrence_participant"),
            (
                owner != actor && occurrence.concept_uid.is_none(),
                "canonical_concept_required_for_reversal",
            ),
            (
                !occurrence.quantity.is_finite() || occurrence.quantity <= 0.0,
                "occurrence_quantity_invalid",
            ),
        ]);
        let mut dispute = self.blockers([
            (!participant, "not_occurrence_participant"),
            (
                actor.is_none() || actor != self.signer,
                "missing_person_signer",
            ),
        ]);
        let mut retract_dispute = dispute.clone();
        if actor_disputes {
            dispute.push("viewer_already_disputes_occurrence");
        } else {
            retract_dispute.push("viewer_has_no_active_dispute");
        }
        Rules {
            delivery,
            receipt,
            remainder,
            reverse,
            dispute,
            retract_dispute,
            participant_disputed: disputes.into_values().any(|asserted| asserted),
        }
    }

    fn claim(&self, party: &str, wrong_party: &'static str) -> Vec<&'static str> {
        self.blockers([
            (
                !self.local && self.viewer_person != Some(party),
                wrong_party,
            ),
            (self.signer != Some(party), "missing_person_signer"),
        ])
    }

    fn blockers<const N: usize>(&self, checks: [(bool, &'static str); N]) -> Vec<&'static str> {
        self.identity
            .iter()
            .copied()
            .chain(
                checks
                    .into_iter()
                    .filter_map(|(blocked, reason)| blocked.then_some(reason)),
            )
            .collect()
    }
}

pub(super) fn participates(occurrence: &TransferOccurrenceRow, person: &str) -> bool {
    person == occurrence.giver_person_uid || person == occurrence.receiver_person_uid
}
