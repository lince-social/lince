use std::collections::{HashMap, HashSet};

use nucleus::PromiseState::{self, Active, Agreed, Broken, Kept, Open, Proposed, Withdrawn};
use serde_json::{Value, json};
use store::{
    misc::PromiseRow,
    transfer_agreement::AgreementReadinessProjection,
    transfers::{
        OccurrenceSettlementProgress, TransferInvitationRow,
        TransferInvitationStatus::{Expired, Pending, Rejected, Withdrawn as InvitationWithdrawn},
        TransferOccurrenceRow, TransferRow, TransferSourceGroupState,
    },
};

use super::audience::{Audience, PartyLevel, expired};
use crate::transfers::status::{
    PrimaryStatus, all_concrete_match, derive_transfer_primary_status, derive_transfer_status,
};

pub(super) struct Terms<'a> {
    pub transfer: &'a TransferRow,
    pub promises: &'a [PromiseRow],
    pub states: &'a [PromiseState],
    pub occurrences: &'a [TransferOccurrenceRow],
    pub progress: &'a HashMap<&'a str, OccurrenceSettlementProgress>,
    pub occurred: &'a HashSet<&'a str>,
    pub parties: &'a [PartyLevel],
    pub invitations: &'a [TransferInvitationRow],
    pub readiness: &'a AgreementReadinessProjection,
}

pub(super) struct Status {
    pub status: &'static str,
    pub primary: &'static str,
    pub derived_parent: bool,
    pub system_disputed: bool,
    pub disputed: bool,
    pub partially_settled: bool,
    mine: bool,
    invited: bool,
    awaiting_me: bool,
    awaiting_others: bool,
    discoverable_open: bool,
}

impl Status {
    pub(super) fn derive(
        terms: &Terms<'_>,
        levels: &[i64],
        group: Option<&TransferSourceGroupState>,
        audience: &Audience<'_>,
    ) -> Self {
        let Terms {
            transfer,
            promises,
            states,
            occurrences,
            progress,
            occurred,
            parties,
            invitations,
            readiness,
        } = *terms;
        let dependency = transfer.agreement_type == "dependency";
        let required_children = readiness.children.iter().any(|child| child.required);
        let derived_parent = promises.is_empty() && required_children;
        let all_agreed = all_concrete_match(states, required_children, |state| {
            matches!(state, Agreed | Kept)
        });
        let remainder_cancelled = !occurrences.is_empty()
            && progress
                .values()
                .any(|progress| progress.cancelled_exact.is_positive())
            && occurrences.iter().all(|occurrence| {
                progress
                    .get(occurrence.uid.as_str())
                    .is_some_and(|progress| progress.remaining_exact.is_zero())
            })
            && !promises.iter().any(|promise| {
                matches!(promise.state, Proposed | Agreed)
                    && !occurred.contains(promise.uid.as_str())
            });
        let system_disputed = occurrences
            .iter()
            .any(|occurrence| occurrence.system_disputed);
        let disputed = occurrences.iter().any(|occurrence| occurrence.disputed);
        let partially_settled = progress.values().any(|progress| progress.partially_settled);
        let satiated = group.is_some_and(|group| group.satiated);
        let observed_ready = readiness.observed && readiness.ready;
        let status = match derive_transfer_status(
            transfer.active,
            &transfer.agreement_type,
            transfer.agreement_pct,
            levels,
            states,
        ) {
            _ if satiated => "satiated",
            _ if system_disputed => "system_disputed",
            _ if disputed => "disputed",
            _ if remainder_cancelled => "cancelled",
            _ if partially_settled => "partially_settled",
            _ if derived_parent && readiness.settled || !derived_parent && observed_ready => {
                "settled"
            }
            _ if derived_parent && readiness.ready => "agreed",
            _ if derived_parent => "proposed",
            "agreed" if !readiness.ready => "proposed",
            "proposed" if readiness.ready && (all_agreed || dependency) => "agreed",
            status => status,
        };

        let (
            mut invited,
            mut other_invited,
            mut agreement,
            mut other_agreement,
            mut claim,
            mut other_claim,
        ) = (false, false, false, false, false, false);
        if let Some(person) = audience.person {
            for invitation in invitations
                .iter()
                .filter(|invitation| invitation.status == Pending)
            {
                invited |= invitation.addressed_person_uid == person;
                other_invited |= invitation.addressed_person_uid != person;
            }
            if !dependency {
                for (_, actor, _) in parties.iter().filter(|(_, _, level)| *level < 2) {
                    agreement |= actor == person;
                    other_agreement |= actor != person;
                }
            }
            for occurrence in occurrences {
                if !occurrence.delivery_claimed {
                    claim |= occurrence.giver_person_uid == person;
                    other_claim |= occurrence.giver_person_uid != person;
                }
                if !occurrence.receipt_claimed {
                    claim |= occurrence.receiver_person_uid == person;
                    other_claim |= occurrence.receiver_person_uid != person;
                }
            }
        }
        let expired = promises.iter().any(|promise| {
            matches!(promise.state, Open | Proposed | Agreed | Active)
                && promise.window_end.as_deref().is_some_and(expired)
        });
        let invitation_cancelled = !invitations.is_empty()
            && parties.len() <= 1
            && occurrences.is_empty()
            && invitations.iter().all(|invitation| {
                matches!(invitation.status, Rejected | InvitationWithdrawn | Expired)
            });
        let all_withdrawn = !states.is_empty() && states.iter().all(|state| *state == Withdrawn);
        let cancelled = invitation_cancelled || all_withdrawn || remainder_cancelled;
        let terminal = observed_ready
            || cancelled
            || expired
            || states.contains(&Broken)
            || satiated
            || !occurrences.is_empty() && progress.values().all(|progress| progress.settled);
        let awaiting_me =
            !derived_parent && !terminal && (invited || audience.mine && (agreement || claim));
        let awaiting_others = if derived_parent {
            !readiness.ready
        } else {
            !terminal && audience.mine && (other_invited || other_agreement || other_claim)
        };
        let primary = if observed_ready || derived_parent && readiness.settled {
            "completed"
        } else if derived_parent {
            if readiness.ready {
                "agreed"
            } else {
                "awaiting_others"
            }
        } else {
            derive_transfer_primary_status(&PrimaryStatus {
                revision: transfer.revision.cast_unsigned(),
                active: transfer.active,
                promise_states: states,
                occurrences,
                progress,
                source_group: group,
                system_disputed,
                disputed,
                agreement_ready: readiness.ready,
                expired,
                cancelled,
                awaiting_me,
                awaiting_others,
            })
        };

        Self {
            status,
            primary,
            derived_parent,
            system_disputed,
            disputed,
            partially_settled,
            mine: audience.mine,
            invited,
            awaiting_me,
            awaiting_others,
            discoverable_open: transfer.visibility == "public"
                && states.contains(&Open)
                && !audience.mine,
        }
    }

    pub(super) fn inbox(&self) -> Value {
        json!({
            "mine": self.mine,
            "invited": self.invited,
            "awaiting_me": self.awaiting_me,
            "awaiting_others": self.awaiting_others,
            "active": matches!(self.primary, "active" | "partially_settled" | "disputed" | "system_disputed"),
            "completed": matches!(self.primary, "completed" | "satiated"),
            "cancelled_or_broken": matches!(self.primary, "cancelled" | "broken" | "expired"),
            "discoverable_open": self.discoverable_open,
        })
    }
}
