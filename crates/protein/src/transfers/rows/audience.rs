use chrono::{DateTime, Utc};
use store::transfers::{TransferInvitationRow, TransferInvitationStatus::Pending};

use crate::transfers::viewer::TransferViewer;

pub(super) type PartyLevel = (String, String, i64);

pub(super) struct Audience<'a> {
    pub viewer: &'a TransferViewer,
    pub person: Option<&'a str>,
    pub party: Option<&'a str>,
    pub is_creator: bool,
    pub is_invitee: bool,
    pub mine: bool,
}

impl<'a> Audience<'a> {
    pub(super) fn new(
        viewer: &'a TransferViewer,
        signer: Option<&'a str>,
        creator: Option<&str>,
        creator_person: Option<&str>,
        parties: &'a [PartyLevel],
        invitations: &[TransferInvitationRow],
    ) -> Self {
        let viewer_person = viewer.person.as_deref();
        let party = viewer_person.and_then(|person| {
            parties
                .iter()
                .find(|(_, actor, _)| actor == person)
                .map(|(party, _, _)| party.as_str())
        });
        let is_creator = viewer.local
            || viewer_person == creator_person
            || viewer
                .subject
                .as_deref()
                .is_some_and(|subject| creator == Some(subject));
        let is_invitee = viewer_person.is_some_and(|person| {
            invitations.iter().any(|invitation| {
                invitation.addressed_person_uid == person
                    && invitation.status == Pending
                    && !invitation.expires_at.as_deref().is_some_and(expired)
            })
        });
        let person = viewer_person.or(signer);
        let mine = person.is_some_and(|person| {
            creator_person == Some(person) || parties.iter().any(|(_, actor, _)| actor == person)
        });
        Self {
            viewer,
            person,
            party,
            is_creator,
            is_invitee,
            mine,
        }
    }

    pub(super) fn show_details(&self) -> bool {
        self.viewer.local || self.mine
    }

    pub(super) fn sees_person(&self, person: &str) -> bool {
        self.show_details() || self.person == Some(person)
    }

    pub(super) fn roles(&self) -> Vec<&'static str> {
        let mut roles = [
            ("creator", self.is_creator),
            ("invitee", self.is_invitee),
            ("local", self.viewer.local),
            ("participant", self.party.is_some()),
        ]
        .into_iter()
        .filter_map(|(role, active)| active.then_some(role))
        .collect::<Vec<_>>();
        if roles.is_empty() {
            roles.push("observer");
        }
        roles
    }
}

pub(super) fn expired(value: &str) -> bool {
    DateTime::parse_from_rfc3339(value)
        .is_ok_and(|deadline| deadline.with_timezone(&Utc) <= Utc::now())
}
