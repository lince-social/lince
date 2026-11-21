//! Remote Transfer references and their read-only projection.

mod history;

use super::{application, hierarchy::TransferOutput, viewer::TransferViewer};
use crate::{Predicate, Protein, ProteinError};
use nucleus::execution::now;
use serde_json::{Map, Value, from_str, json};
use std::collections::HashSet;
use store::{
    Store, organs,
    sqlx::{Row, SqlitePool, query, query_scalar},
    transfer_outcomes::{MAX_REMOTE_AGE_SECONDS, fresh},
};

struct Reference {
    uid: String,
    origin_organ_uid: String,
    transfer_uid: String,
    recipient_person_uid: String,
    mode: String,
    state: String,
    policy_revision: i64,
    last_cursor: i64,
    last_transfer_revision: i64,
    projection: Option<String>,
    last_fetched_at: Option<String>,
    last_error: Option<String>,
}

pub(super) async fn append_remote_transfer_delivery_rows(
    store: &Store,
    protein: &Protein,
    viewer: &TransferViewer,
    signer: Option<&str>,
    rows: &mut Vec<TransferOutput>,
    visible: Option<&HashSet<String>>,
) -> Result<(), ProteinError> {
    let pool = &store.pool;
    let Some(local_organ) = organs::local(pool).await? else {
        return Ok(());
    };
    let references = query(
        "SELECT uid, origin_organ_uid, transfer_uid, recipient_person_uid, mode, state,
                policy_revision, last_cursor, last_transfer_revision, projection,
                last_fetched_at, last_error
         FROM transfer_remote_reference WHERE recipient_organ_uid = ?
         ORDER BY updated_at, uid",
    )
    .bind(&local_organ.uid)
    .try_map(|row| {
        Ok(Reference {
            uid: row.try_get("uid")?,
            origin_organ_uid: row.try_get("origin_organ_uid")?,
            transfer_uid: row.try_get("transfer_uid")?,
            recipient_person_uid: row.try_get("recipient_person_uid")?,
            mode: row.try_get("mode")?,
            state: row.try_get("state")?,
            policy_revision: row.try_get("policy_revision")?,
            last_cursor: row.try_get("last_cursor")?,
            last_transfer_revision: row.try_get("last_transfer_revision")?,
            projection: row.try_get("projection")?,
            last_fetched_at: row.try_get("last_fetched_at")?,
            last_error: row.try_get("last_error")?,
        })
    })
    .fetch_all(pool)
    .await?;
    for reference in references {
        if (!viewer.local && viewer.person.as_deref() != Some(&reference.recipient_person_uid))
            || rows.iter().any(|row| row.uid == reference.transfer_uid)
            || !protein
                .filter
                .iter()
                .all(|predicate| reference.matches(predicate))
        {
            continue;
        }
        rows.push(
            reference
                .project(store, &local_organ.uid, viewer, signer, visible)
                .await?,
        );
    }
    Ok(())
}

impl Reference {
    fn matches(&self, predicate: &Predicate) -> bool {
        // The database constrains last_transfer_revision to nonnegative integers.
        let revision = self.last_transfer_revision.cast_unsigned();
        match predicate {
            Predicate::UidEq(uid) => *uid == self.transfer_uid,
            Predicate::RevisionEq(bound) => revision == *bound,
            Predicate::RevisionLt(bound) => revision < *bound,
            Predicate::RevisionLte(bound) => revision <= *bound,
            Predicate::RevisionGt(bound) => revision > *bound,
            Predicate::RevisionGte(bound) => revision >= *bound,
            Predicate::PersonEq(person) => *person == self.recipient_person_uid,
            _ => true,
        }
    }

    fn refresh(&self) -> Value {
        json!({
            "action": "refresh-transfer-delivery",
            "transfer": self.transfer_uid,
            "delivery": self.uid,
            "person": self.recipient_person_uid,
            "request_id": null,
        })
    }

    fn freshness(&self, source_at: Option<&str>) -> &'static str {
        if self.state != "active" {
            "revoked"
        } else if self.last_error.is_some() {
            "failed"
        } else if self.last_cursor == 0 {
            "never_fetched"
        } else if source_at.is_some_and(|at| fresh(at, now())) {
            "fresh"
        } else {
            "stale"
        }
    }

    async fn project(
        self,
        store: &Store,
        local_organ: &str,
        viewer: &TransferViewer,
        signer: Option<&str>,
        visible: Option<&HashSet<String>>,
    ) -> Result<TransferOutput, ProteinError> {
        let active = self.state == "active";
        let mut value = self
            .projection
            .as_deref()
            .and_then(|payload| from_str(payload).ok())
            .unwrap_or_else(|| {
                json!({
                    "kind": "transfer",
                    "uid": self.transfer_uid,
                    "head": "Remote Transfer",
                    "revision": self.last_transfer_revision,
                    "status": "remote",
                    "primary_status": "remote",
                })
            });
        let blocker = if !active {
            Some("delivery_revoked")
        } else if signer != Some(&self.recipient_person_uid) {
            Some("missing_person_signer")
        } else if !viewer.local && !viewer.has_permission("transfer:update") {
            Some("missing_transfer_update_permission")
        } else {
            None
        };
        if let Some(reason) = blocker {
            disable_remote_transfer_actions(&mut value, reason);
        }

        let pool = &store.pool;
        let replica_history = history::envelopes(pool, &self.uid).await?;
        let commands = history::outgoing_commands(pool, &self).await?;
        let refresh = self.refresh();
        let conflicts = history::conflicts(pool, &self, &refresh, active).await?;
        let handoff_uids: Vec<String> = query_scalar(
            "SELECT uid FROM transfer_remote_application_handoff
             WHERE reference_uid = ? ORDER BY origin_created_at, uid",
        )
        .bind(&self.uid)
        .fetch_all(pool)
        .await?;
        let mut handoffs = Vec::with_capacity(handoff_uids.len());
        for uid in handoff_uids {
            handoffs.push(application::preview(store, &uid, signer, visible).await?);
        }
        let source_at = self.evidence_time(pool).await?;
        if let Some(object) = value.as_object_mut() {
            object.insert("application_handoffs".into(), json!(handoffs));
            object.insert("delivery_read_only".into(), Value::Bool(true));
            object.insert("social_delivery".into(), json!({
                "authority": {
                    "role": if self.mode == "replicated" { "replica" } else { "hosted_reference" },
                    "origin_organ": self.origin_organ_uid,
                    "local_organ": local_organ,
                    "canonical_writes": "remote",
                },
                "local_view": {
                    "uid": self.uid,
                    "person": self.recipient_person_uid,
                    "mode": self.mode,
                    "state": self.state,
                    "freshness": {
                        "state": self.freshness(source_at.as_deref()),
                        "source_at": source_at,
                        "fresh_for_seconds": MAX_REMOTE_AGE_SECONDS,
                        "cursor": self.last_cursor,
                        "remote_revision": self.last_transfer_revision,
                        "last_pull_at": self.last_fetched_at,
                        "last_error": self.last_error,
                    },
                    "replica_history": replica_history,
                },
                "mode": self.mode,
                "state": self.state,
                "revision": self.policy_revision,
                "replica_history": replica_history,
                "outgoing_commands": commands,
                "conflicts": conflicts,
                "application_handoffs": handoffs,
                "capabilities": { "refresh": active },
                "blocking_reasons": { "refresh": (!active).then_some("delivery_revoked").as_slice() },
                "action_payloads": { "refresh": refresh },
            }));
        }
        let slug = value.get("slug").and_then(Value::as_str).map(str::to_owned);
        let head = value
            .get("head")
            .and_then(Value::as_str)
            .unwrap_or("Remote Transfer")
            .into();
        let status = value
            .get("primary_status")
            .or_else(|| value.get("status"))
            .and_then(Value::as_str)
            .unwrap_or("remote")
            .into();
        Ok(TransferOutput {
            uid: self.transfer_uid,
            slug,
            head,
            revision: self.last_transfer_revision.cast_unsigned(),
            status,
            value,
        })
    }

    async fn evidence_time(&self, pool: &SqlitePool) -> Result<Option<String>, ProteinError> {
        query_scalar(
            "SELECT e.origin_created_at FROM transfer_outcome_evidence e
             JOIN transfer_remote_reference r ON r.uid = e.reference_uid AND r.last_envelope_uid = e.envelope_uid
             WHERE r.uid = ?",
        )
        .bind(&self.uid)
        .fetch_optional(pool)
        .await
    }
}

pub(super) fn disable_remote_transfer_actions(value: &mut Value, reason: &str) {
    match value {
        Value::Object(object) => {
            if let Some(Value::Object(capabilities)) = object.get_mut("capabilities") {
                let mut blockers = Map::new();
                for (key, enabled) in capabilities {
                    *enabled = Value::Bool(false);
                    blockers.insert(key.clone(), json!([reason]));
                }
                object.insert("blocking_reasons".into(), Value::Object(blockers));
            }
            for child in object.values_mut() {
                disable_remote_transfer_actions(child, reason);
            }
        }
        Value::Array(rows) => {
            for row in rows {
                disable_remote_transfer_actions(row, reason);
            }
        }
        _ => {}
    }
}
