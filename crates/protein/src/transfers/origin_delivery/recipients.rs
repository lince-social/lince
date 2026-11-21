//! Origin-side delivery recipients and package receipts.

use crate::ProteinError;
use serde_json::{Value, json};
use std::collections::HashSet;
use store::{
    records,
    sqlx::{SqlitePool, query_as},
    transfer_delivery::policies_for_transfer,
};

pub(super) struct Recipients {
    pub(super) configured: Vec<Value>,
    pub(super) eligible: Vec<Value>,
    pub(super) receipts: Vec<Value>,
}

pub(super) async fn read(
    pool: &SqlitePool,
    transfer_uid: &str,
    acting_person: Option<&str>,
    can_admin_delivery: bool,
) -> Result<Recipients, ProteinError> {
    let policies = policies_for_transfer(pool, transfer_uid).await?;
    let eligible = query_as::<
        _,
        (
            String,
            String,
            Option<String>,
            String,
            String,
            Option<String>,
            String,
        ),
    >(
        "SELECT DISTINCT person.uid, person.head, person.slug, person.organ_uid,
                organ.head, organ.slug, contact.trust
         FROM record person
         JOIN record organ ON organ.uid = person.organ_uid
         JOIN organ_contact contact ON contact.record_uid = organ.uid
         JOIN transfer_party party ON party.actor_uid = person.uid AND party.transfer_uid = ?
         WHERE contact.sync_out = 1 AND contact.trust != 'blocked'
         ORDER BY person.head, person.uid",
    )
    .bind(transfer_uid)
    .fetch_all(pool)
    .await?;
    let configured_people: HashSet<_> = policies
        .iter()
        .map(|policy| policy.recipient_person_uid.as_str())
        .collect();
    let mut recipients = Vec::with_capacity(policies.len());
    let mut receipts_output = Vec::new();
    for policy in &policies {
        let person = records::get(pool, &policy.recipient_person_uid).await?;
        let organ = records::get(pool, &policy.recipient_organ_uid).await?;
        let latest = query_as::<_, (
            String, i64, Option<String>, Option<String>, Option<String>, Option<i64>, String,
        )>(
            "SELECT status, attempts, last_error, last_attempt_at, sent_at, acknowledged_cursor, envelope_uid
             FROM transfer_delivery_outbox WHERE delivery_uid = ? ORDER BY cursor DESC LIMIT 1",
        )
        .bind(&policy.uid)
        .fetch_optional(pool)
        .await?;
        let receipts = query_as::<_, (String, String, i64, String, String)>(
            "SELECT uid, kind, cursor, actor_organ_uid, created_at
             FROM transfer_delivery_receipt WHERE delivery_uid = ? ORDER BY created_at, uid",
        )
        .bind(&policy.uid)
        .fetch_all(pool)
        .await?;
        receipts_output.extend(receipts.into_iter().map(|(uid, kind, cursor, organ, at)| {
            json!({ "uid": uid, "kind": kind, "cursor": cursor, "organ": organ, "at": at })
        }));
        let (status, attempts, last_error, last_attempt, acknowledged_at, cursor, envelope) =
            latest.as_ref().map_or(
                ("not_queued", 0, None, None, None, None, None),
                |(status, attempts, error, last_attempt, sent_at, cursor, envelope)| {
                    (
                        status.as_str(),
                        *attempts,
                        error.as_deref(),
                        last_attempt.as_deref(),
                        sent_at.as_deref(),
                        *cursor,
                        Some(envelope.as_str()),
                    )
                },
            );
        let active = policy.state == "active";
        let can_manage = can_admin_delivery && active;
        let retryable = active && status == "failed";
        let revoked = (!active).then_some("delivery_revoked");
        recipients.push(json!({
            "uid": policy.uid,
            "delivery_uid": policy.uid,
            "person": policy.recipient_person_uid,
            "person_head": person.as_ref().map(|record| record.head.as_str()),
            "person_slug": person.as_ref().and_then(|record| record.slug.as_deref()),
            "organ": policy.recipient_organ_uid,
            "organ_head": organ.as_ref().map(|record| record.head.as_str()),
            "organ_slug": organ.as_ref().and_then(|record| record.slug.as_deref()),
            "mode": policy.mode.as_str(),
            "state": policy.state,
            "revision": policy.revision,
            "delivery": {
                "status": status,
                "attempts": attempts,
                "last_error": last_error,
                "last_attempt_at": last_attempt,
                "acknowledged_at": acknowledged_at,
                "acknowledged_cursor": cursor,
                "envelope": envelope,
            },
            "available_modes": ["hosted", "replicated"],
            "capabilities": {
                "set_mode": can_manage,
                "enqueue": can_manage,
                "retry": can_admin_delivery && retryable,
                "revoke": can_manage,
            },
            "blocking_reasons": {
                "set_mode": revoked.as_slice(),
                "enqueue": revoked.as_slice(),
                "retry": (!retryable).then_some("delivery_not_failed").as_slice(),
                "revoke": revoked.as_slice(),
            },
            "action_payloads": if can_admin_delivery { json!({
                "set_mode": {
                    "action": "set-transfer-delivery-mode",
                    "transfer": transfer_uid,
                    "delivery": policy.uid,
                    "expected_revision": policy.revision,
                    "person": acting_person,
                    "request_id": Value::Null,
                    "mode": policy.mode.as_str(),
                },
                "enqueue": {
                    "action": "enqueue-transfer-delivery",
                    "transfer": transfer_uid,
                    "delivery": policy.uid,
                    "person": acting_person,
                    "request_id": Value::Null,
                },
                "retry": {
                    "action": "retry-transfer-delivery",
                    "transfer": transfer_uid,
                    "delivery": policy.uid,
                    "person": acting_person,
                    "request_id": Value::Null,
                },
                "revoke": {
                    "action": "revoke-transfer-delivery",
                    "transfer": transfer_uid,
                    "delivery": policy.uid,
                    "expected_revision": policy.revision,
                    "person": acting_person,
                    "request_id": Value::Null,
                },
            }) } else { json!({}) },
        }));
    }
    let eligible_recipients = eligible
        .into_iter()
        .filter(|(person, ..)| !configured_people.contains(person.as_str()))
        .map(
            |(person, person_head, person_slug, organ, organ_head, organ_slug, trust)| {
                json!({
                    "person": person,
                    "person_head": person_head,
                    "person_slug": person_slug,
                    "organ": organ,
                    "organ_head": organ_head,
                    "organ_slug": organ_slug,
                    "contact_state": trust,
                    "default_mode": "hosted",
                    "available_modes": ["hosted", "replicated"],
                })
            },
        )
        .collect::<Vec<_>>();
    Ok(Recipients {
        configured: recipients,
        eligible: eligible_recipients,
        receipts: receipts_output,
    })
}
