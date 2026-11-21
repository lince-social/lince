use std::collections::HashSet;

use serde_json::{Value, json};
use store::{
    transfer_agreement::{AgreementReadinessProjection, with_promise_context},
    transfers::TransferAgreementReadinessInput,
};

use super::audience::{Audience, PartyLevel};
use crate::transfers::{
    projection::{RecordIndex, attach_action_access, party_blockers},
    viewer::AgreementTransition,
};

pub(super) struct Parties<'a> {
    pub input: &'a TransferAgreementReadinessInput,
    pub readiness: &'a AgreementReadinessProjection,
    pub coalition: Option<&'a HashSet<&'a str>>,
    pub audience: &'a Audience<'a>,
    pub signer: Option<&'a str>,
    pub revision_signed: bool,
    pub records: &'a RecordIndex<'a>,
}

pub(super) fn project(parties: &[PartyLevel], facts: &Parties<'_>) -> Vec<Value> {
    let Parties {
        input,
        readiness,
        coalition,
        audience,
        signer,
        revision_signed,
        records,
    } = *facts;
    let blockers_by_person = party_blockers(
        parties,
        input,
        readiness,
        audience.show_details(),
        audience.person,
    );
    parties.iter().filter(|(_, actor, _)| audience.sees_person(actor)).map(|(party, actor, level)| {
        let level = u8::try_from(*level).unwrap_or(0);
        let transition = AgreementTransition {
            person: actor,
            party,
            level,
            revision: input.revision,
            revision_signed,
            coalition,
            has_signer: signer == Some(actor.as_str()),
        };
        let blockers = |target| audience.viewer.agreement_transition_blockers(&transition, target);
        let review = if level == 0 { blockers(1) } else { vec!["party_is_not_at_unchecked_level"] };
        let commit = if level == 1 { blockers(2) } else { vec!["party_is_not_at_checked_level"] };
        let back = level.checked_sub(1).map_or_else(|| vec!["party_is_already_at_unchecked_level"], blockers);
        let person_blockers = blockers_by_person.get(actor.as_str()).into_iter().flatten()
            .map(|blocker| with_promise_context(blocker.promise, blocker.value)).collect::<Vec<_>>();
        let record = records.get(actor.as_str());
        let ready = readiness.people.get(actor).copied().unwrap_or_default();
        let mut value = json!({
            "uid": party,
            "actor": actor,
            "actor_head": record.map(|record| record.head.as_str()),
            "actor_slug": record.and_then(|record| record.slug.as_deref()),
            "level": level,
            "level_label": match level {
                0 => "Unchecked",
                1 => "Checked · ready to agree",
                2 => "Agreed",
                _ => "Unknown",
            },
            "ready": ready,
            "readiness": { "ready": ready, "blockers": person_blockers },
            "coalition_member": coalition.is_some_and(|members| members.contains(party.as_str())),
            "capabilities": null,
            "blocking_reasons": null,
        });
        attach_action_access(&mut value, &[
            ("review", &review),
            ("commit", &commit),
            ("agreement_back", &back),
        ]);
        value
    }).collect()
}
