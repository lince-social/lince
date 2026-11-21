//! Invitation history and actions exposed to the viewer of a Transfer.

use super::{
    projection::{RecordIndex, attach_action_access},
    viewer::TransferViewer,
};
use crate::ProteinError;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use store::{
    Store,
    transfers::{
        TransferInvitationRow,
        TransferInvitationStatus::{Expired, Pending, Rejected, Withdrawn},
        invitation_events,
    },
};

type History<'a> = HashMap<&'a str, Vec<Value>>;

pub(super) struct Viewer<'a> {
    pub viewer: &'a TransferViewer,
    pub viewer_person: Option<&'a str>,
    pub show_all: bool,
    pub creator_person: Option<&'a str>,
    pub is_creator: bool,
    pub signer: Option<&'a str>,
}

pub(super) async fn read_history<'a>(
    store: &Store,
    invitations: &'a [TransferInvitationRow],
) -> Result<History<'a>, ProteinError> {
    let mut invitation_history = HashMap::new();
    for invitation in invitations {
        let events = invitation_events(&store.pool, &invitation.uid).await?;
        let mut attempts: BTreeMap<u64, Vec<Value>> = Default::default();
        for event in events {
            attempts.entry(event.attempt).or_default().push(json!({
                "uid": event.uid,
                "kind": event.kind,
                "actor_person": event.actor_uid,
                "revision": event.revision,
                "fact": event.fact_uid,
                "request_id": event.idempotency_key,
                "at": event.created_at,
            }));
        }
        invitation_history.insert(
            invitation.uid.as_str(),
            attempts
                .into_iter()
                .map(|(attempt, events)| {
                    json!({
                        "attempt": attempt,
                        "events": events,
                    })
                })
                .collect::<Vec<_>>(),
        );
    }
    Ok(invitation_history)
}

pub(super) fn project(
    invitations: &[TransferInvitationRow],
    mut invitation_history: History<'_>,
    context: &Viewer<'_>,
    records_by_uid: &RecordIndex<'_>,
) -> Vec<Value> {
    let &Viewer {
        viewer,
        viewer_person,
        show_all,
        creator_person,
        is_creator,
        signer,
    } = context;
    invitations
        .iter()
        .filter(|invitation| {
            show_all || viewer_person == Some(invitation.addressed_person_uid.as_str())
        })
        .map(|invitation| {
            let addressed = records_by_uid.get(invitation.addressed_person_uid.as_str());
            let inviter = records_by_uid.get(invitation.invited_by_person_uid.as_str());
            let expired = invitation.expires_at.as_deref().is_some_and(|value| {
                DateTime::parse_from_rfc3339(value).is_ok_and(|instant| instant <= Utc::now())
            });
            let pending = invitation.status == Pending;
            let addressee = Some(invitation.addressed_person_uid.as_str());
            let viewer_is_addressed = viewer.person.as_deref() == addressee;
            let mut accept_blockers = viewer.update_identity_blockers();
            if !pending {
                accept_blockers.push("invitation_not_pending");
            }
            if expired {
                accept_blockers.push("invitation_expired");
            }
            if !viewer.local && !viewer_is_addressed {
                accept_blockers.push("not_invitation_addressee");
            }
            if signer != addressee {
                accept_blockers.push("missing_person_signer");
            }
            let manage_blockers = |state_blocker| {
                let mut blockers = viewer.update_identity_blockers();
                blockers.extend(state_blocker);
                blockers.extend((!is_creator).then_some("not_transfer_creator"));
                blockers.extend((signer != creator_person).then_some("missing_person_signer"));
                blockers
            };
            let withdraw_blockers = manage_blockers((!pending).then_some("invitation_not_pending"));
            let closed = matches!(invitation.status, Rejected | Withdrawn | Expired);
            let reopen_blockers = manage_blockers((!closed).then_some("invitation_not_closed"));
            let mut value = json!({
                "uid": invitation.uid,
                "status": invitation.status.as_str(),
                "addressed_person": invitation.addressed_person_uid,
                "addressed_person_head": addressed.map(|person| person.head.as_str()),
                "addressed_person_slug": addressed
                    .and_then(|person| person.slug.as_deref()),
                "invited_by_person": invitation.invited_by_person_uid,
                "invited_by_person_head": inviter.map(|person| person.head.as_str()),
                "invited_by_person_slug": inviter
                    .and_then(|person| person.slug.as_deref()),
                "viewer_is_addressed": viewer_is_addressed,
                "viewer_is_inviter": viewer.person.as_deref()
                    == Some(invitation.invited_by_person_uid.as_str()),
                "expires_at": invitation.expires_at,
                "party": invitation.party_uid,
                "created_at": invitation.created_at,
                "updated_at": invitation.updated_at,
                "attempt": invitation.attempt,
                "attempts": invitation_history.remove(invitation.uid.as_str()).unwrap_or_default(),
                "capabilities": null,
                "blocking_reasons": null,
            });
            attach_action_access(
                &mut value,
                &[
                    ("accept", &accept_blockers),
                    ("reject", &accept_blockers),
                    ("withdraw", &withdraw_blockers),
                    ("reopen", &reopen_blockers),
                ],
            );
            value
        })
        .collect::<Vec<_>>()
}
