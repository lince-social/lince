use std::collections::HashMap;

use nucleus::PromiseState::{Agreed, Open, Proposed, Withdrawn};
use serde_json::{Value, json};
use store::transfers::{TransferAgreementReadinessInput, TransferRow};

use super::{audience::Audience, state::Terms};
use crate::transfers::{
    projection::{AgreementOptions, attach_action_access},
    status::PHASE_1_DRAFT_ACTIONS_BLOCKER,
};

pub(super) struct Actions<'a> {
    pub claim_person: Option<&'a str>,
    pub claim_access_blocker: Option<&'static str>,
    pub signer_is_party: bool,
    pub negotiation: Vec<&'static str>,
    edit: Vec<&'static str>,
    adopt: Vec<&'static str>,
    address: Vec<&'static str>,
    counteroffer: Vec<&'static str>,
    claim: Vec<&'static str>,
    activate: Option<&'static str>,
}

impl<'a> Actions<'a> {
    pub(super) fn derive(
        terms: &Terms<'_>,
        input: &TransferAgreementReadinessInput,
        roles: &HashMap<&str, Result<(), &'static str>>,
        audience: &Audience<'a>,
        signer: Option<&'a str>,
        creator: Option<&str>,
    ) -> Self {
        let viewer = audience.viewer;
        let transfer = terms.transfer;
        let revision = transfer.revision as u64;
        let editable = terms
            .states
            .iter()
            .all(|state| matches!(state, Open | Proposed | Agreed | Withdrawn));
        let missing_creator_signer = signer != creator;
        let participant = audience.party.is_some();
        let mut edit = viewer.draft_edit_blockers(audience.is_creator, revision, editable);
        let mut adopt = viewer.draft_adopt_blockers(audience.is_creator, revision, editable);
        let common = [
            (revision == 0).then_some("legacy_transfer_requires_adoption"),
            (!editable).then_some("transfer_terms_are_no_longer_draft_editable"),
        ];
        let mut address = viewer.update_identity_blockers();
        address.extend((!audience.is_creator).then_some("not_transfer_creator"));
        address.extend(common.into_iter().flatten());
        if missing_creator_signer {
            for blockers in [&mut edit, &mut adopt, &mut address] {
                blockers.push("missing_person_signer");
            }
        }
        let claim_person = if viewer.local {
            signer
        } else {
            viewer.person.as_deref()
        };
        let signer_is_party =
            signer.is_some_and(|signer| terms.parties.iter().any(|(_, actor, _)| actor == signer));
        let mut counteroffer = viewer.update_identity_blockers();
        counteroffer.extend((!viewer.local && !participant).then_some("not_transfer_participant"));
        counteroffer.extend(common.into_iter().flatten());
        if !signer_is_party || signer != claim_person {
            counteroffer.push("missing_person_signer");
        }
        let claim_access_blocker = if !viewer.local && audience.is_invitee {
            Some("invitation_acceptance_required")
        } else if !viewer.local && !participant && transfer.visibility != "public" {
            Some("open_claim_requires_public_visibility")
        } else {
            None
        };
        let mut claim = viewer.update_identity_blockers();
        let mut open = terms
            .promises
            .iter()
            .filter(|promise| promise.state == Open)
            .peekable();
        if open.peek().is_none() {
            claim.push("no_open_promise");
        } else if claim_person.is_some_and(|claimant| {
            !open.any(|promise| {
                promise
                    .party_uid
                    .as_deref()
                    .is_some_and(|proposer| proposer != claimant)
            })
        }) {
            claim.push("no_counterparty_open_promise");
        }
        claim.extend(claim_access_blocker);
        if claim_person.is_none() || signer != claim_person {
            claim.push("missing_person_signer");
        }
        let can_activate = input.promises.iter().any(|promise| {
            let owner = promise.person_uid.as_deref();
            (viewer.local || viewer.person.as_deref() == owner)
                && ("agreed" == promise.state
                    || transfer.agreement_type == "dependency" && promise.state == "proposed")
                && owner.is_some()
                && signer == owner
                && terms
                    .readiness
                    .promises
                    .get(&promise.uid)
                    .is_some_and(|readiness| readiness.ready)
                && roles.get(promise.uid.as_str()).is_some_and(Result::is_ok)
                && !terms.occurred.contains(promise.uid.as_str())
        });
        Self {
            claim_person,
            claim_access_blocker,
            signer_is_party,
            negotiation: viewer.negotiation_write_blockers(
                audience.is_creator,
                participant,
                audience.is_invitee,
            ),
            edit,
            adopt,
            address,
            counteroffer,
            claim,
            activate: (!can_activate).then_some("no_policy_ready_occurrence"),
        }
    }

    pub(super) fn attach(&self, row: &mut Value, agreement: &AgreementOptions) {
        attach_action_access(
            row,
            &[
                ("edit_terms", &self.edit),
                ("adopt_terms", &self.adopt),
                ("address_invitation", &self.address),
                ("counteroffer", &self.counteroffer),
                ("claim_open", &self.claim),
                ("create_thread", &self.negotiation),
                ("create_message", &self.negotiation),
                ("add_party", &self.address),
                ("add_promise", &[PHASE_1_DRAFT_ACTIONS_BLOCKER]),
                ("review", &agreement.review),
                ("commit", &agreement.commit),
                ("agreement_back", &agreement.back),
                ("activate", self.activate.as_slice()),
            ],
        );
        // The legacy add_party capability mirrors invitations and exposes an empty reason list.
        row["blocking_reasons"]["add_party"] = Value::Array(vec![]);
    }
}

pub(super) fn attach_children(
    mut children: Vec<Value>,
    transfer: &TransferRow,
    audience: &Audience<'_>,
    signer: Option<&str>,
) -> Value {
    let can_change = audience.party.is_some()
        && audience.person.is_some()
        && signer == audience.person
        && (audience.viewer.local || audience.viewer.has_permission("transfer:update"));
    for child in &mut children {
        child["capabilities"] = json!({"set_requirement": can_change});
        if can_change {
            child["action"] = json!({
                "action": "set-transfer-child-requirement", "transfer": transfer.record_uid,
                "child": child["uid"], "required": child["required"] != true,
                "expected_revision": transfer.revision, "person": audience.person, "request_id": null,
            });
        }
    }
    Value::Array(children)
}
