use std::collections::HashSet;

use serde_json::{Value, json};
use store::{
    Store,
    transfers::{TransferInvitationRow, TransferRow},
    visibility::rules_for_target,
};

use super::audience::{Audience, PartyLevel};
use crate::ProteinError;

pub(super) async fn project(
    store: &Store,
    transfer: &TransferRow,
    creator: Option<&str>,
    parties: &[PartyLevel],
    invitations: &[TransferInvitationRow],
    audience: &Audience<'_>,
) -> Result<Value, ProteinError> {
    let rules = rules_for_target(&store.pool, &transfer.record_uid).await?;
    let mut keys = HashSet::new();
    let mut recipients = Vec::new();
    let mut push = |kind: &str, uid: Option<&str>, reason: &str, state: &str| {
        let key = format!("{kind}:{}:{reason}", uid.unwrap_or("*"));
        if keys.insert(key) {
            recipients.push(json!({ "kind": kind, "uid": uid, "reason": reason, "state": state }));
        }
    };
    if let Some(creator) = creator.filter(|creator| audience.sees_person(creator)) {
        push("person", Some(creator), "creator", "accepted");
    }
    for (_, person, _) in parties
        .iter()
        .filter(|(_, person, _)| audience.sees_person(person))
    {
        push("person", Some(person), "party", "accepted");
    }
    for invitation in invitations
        .iter()
        .filter(|invitation| audience.sees_person(&invitation.addressed_person_uid))
    {
        push(
            "person",
            Some(&invitation.addressed_person_uid),
            "invitation",
            invitation.status.as_str(),
        );
    }
    let explicit_rules = rules
        .iter()
        .filter(|rule| {
            audience.show_details()
                || rule.subject_kind == "public"
                || rule.subject_uid == audience.viewer.subject
        })
        .map(|rule| {
            push(
                &rule.subject_kind,
                rule.subject_uid.as_deref(),
                "explicit_rule",
                &rule.grant_level,
            );
            json!({
                "uid": rule.uid,
                "subject_kind": rule.subject_kind,
                "subject_uid": rule.subject_uid,
                "field": rule.field,
                "grant_level": rule.grant_level,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "policy": transfer.visibility,
        "max_proximity": transfer.max_proximity,
        "scope": "whole_transfer",
        "recipients": recipients,
        "explicit_rules": explicit_rules,
        "disclosure": {
            "included": [
                "signed_terms", "parties", "invitations", "agreements",
                "occurrences", "claims", "public_settlements", "disputes",
                "source_group_evidence", "threads"
            ],
            "excluded": [
                "private_record_quantity", "private_application_formula",
                "private_application_delta", "unrelated_records"
            ],
            "field_overrides_supported": true,
        },
    }))
}
