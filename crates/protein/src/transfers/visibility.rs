//! Read access and redaction for related Transfers and their history.

use super::{projection::RecordIndex, viewer::TransferViewer};
use crate::ProteinError;
use nucleus::transfer::{TransferDependencyUpstreamKind, TransferRevisionDependency};
use serde_json::{Value, json};
use std::collections::HashSet;
use store::{
    Store,
    facts::creator_uid,
    sqlx::query_scalar,
    transfer_agreement::AgreementReadinessProjection,
    transfer_children::{history_children, history_parents},
    transfer_outcomes::history_upstreams,
    transfers::{TransferRow, creator_party_actor, invitations_for_transfer, party_for_actor},
};

pub(super) struct Scope<'a> {
    pub viewer: &'a TransferViewer,
    pub visible: Option<&'a HashSet<String>>,
    pub records: &'a RecordIndex<'a>,
}

pub(super) struct RelatedTransfers {
    pub children: Vec<Value>,
    pub disclosure: Disclosure,
}

pub(super) struct Disclosure {
    pub history_hidden: bool,
    parent_hidden: bool,
    current_children_hidden: bool,
}

impl Disclosure {
    pub(super) fn redact(&self, row: &mut Value) {
        let Some(fields) = row.as_object_mut() else {
            return;
        };
        if self.parent_hidden {
            for key in ["parent", "parent_head", "parent_slug"] {
                fields.remove(key);
            }
        }
        if self.history_hidden {
            for key in [
                "revision_evidence",
                "proof",
                "timeline",
                "draft_terms",
                "terms",
                "proposal",
                "counteroffer",
            ] {
                fields.remove(key);
            }
            fields.insert(
                "children_details_hidden".into(),
                Value::Bool(self.current_children_hidden),
            );
            fields.insert("hierarchy_details_hidden".into(), Value::Bool(true));
        }
    }
}

pub(super) async fn related_transfers(
    store: &Store,
    transfer: &TransferRow,
    dependencies: &[TransferRevisionDependency],
    readiness: &mut AgreementReadinessProjection,
    dependency_status: &mut [Value],
    scope: &Scope<'_>,
) -> Result<RelatedTransfers, ProteinError> {
    let &Scope {
        viewer,
        visible,
        records,
    } = scope;
    let uid = &transfer.record_uid;
    let mut children = Vec::new();
    let mut hidden_children = HashSet::new();
    for child in &readiness.children {
        if can_read(store, &child.uid, viewer, visible).await? {
            children.push(json!({
                "uid": child.uid, "required": child.required, "revision": child.revision,
                "ready": child.ready, "settled": child.settled,
                "head": records.get(child.uid.as_str()).map(|record| &record.head),
            }));
        } else {
            hidden_children.insert(child.uid.clone());
        }
    }
    let current_children_hidden = !hidden_children.is_empty();
    let mut hidden_parent_history = false;
    if !viewer.local {
        for child in history_children(&store.pool, uid).await? {
            if !can_read(store, &child, viewer, visible).await? {
                hidden_children.insert(child);
            }
        }
        for parent in history_parents(&store.pool, uid).await? {
            hidden_parent_history |= !can_read(store, &parent, viewer, visible).await?;
        }
    }
    let parent_hidden = match transfer.parent_uid.as_deref() {
        Some(parent) => !can_read(store, parent, viewer, visible).await?,
        None => false,
    };
    let mut hidden_upstreams = HashSet::new();
    if !viewer.local {
        for dependency in dependencies {
            if dependency.upstream_kind != TransferDependencyUpstreamKind::Transfer
                || !can_read(store, &dependency.upstream_uid, viewer, visible).await?
            {
                hidden_upstreams.insert(dependency.upstream_uid.clone());
            }
        }
        for (kind, upstream) in history_upstreams(&store.pool, uid).await? {
            if kind != "transfer" || !can_read(store, &upstream, viewer, visible).await? {
                hidden_upstreams.insert(upstream);
            }
        }
        for dependency in dependency_status {
            if let Some(fields) = dependency.as_object_mut() {
                fields.remove("evidence");
            }
            if hidden(dependency, "upstream", &hidden_upstreams) {
                *dependency = json!({
                    "required_state": dependency["required_state"],
                    "satisfied": dependency["satisfied"],
                    "message": "Required outcome details are private",
                });
            }
        }
    }
    for blocker in readiness.blockers.iter_mut().chain(
        readiness
            .promises
            .values_mut()
            .flat_map(|promise| &mut promise.blockers),
    ) {
        if hidden(blocker, "child", &hidden_children) {
            *blocker =
                json!({"code": "required_part_not_ready", "message": "Waiting on a required part"});
        } else if hidden(blocker, "upstream", &hidden_upstreams) {
            *blocker = json!({"code": "required_outcome_unresolved", "message": "Waiting on a required outcome"});
        }
    }
    if readiness.blockers.len() > 1 {
        let mut seen = HashSet::new();
        readiness
            .blockers
            .retain(|blocker| seen.insert(blocker.to_string()));
    }
    Ok(RelatedTransfers {
        children,
        disclosure: Disclosure {
            parent_hidden,
            current_children_hidden,
            history_hidden: !hidden_children.is_empty()
                || hidden_parent_history
                || parent_hidden
                || !hidden_upstreams.is_empty(),
        },
    })
}

fn hidden(value: &Value, key: &str, ids: &HashSet<String>) -> bool {
    value
        .get(key)
        .and_then(Value::as_str)
        .is_some_and(|uid| ids.contains(uid))
}

pub(super) async fn can_read(
    store: &Store,
    transfer_uid: &str,
    viewer: &TransferViewer,
    granted: Option<&HashSet<String>>,
) -> Result<bool, ProteinError> {
    if viewer.local || granted.is_some_and(|targets| targets.contains(transfer_uid)) {
        return Ok(true);
    }
    if let Some(subject) = viewer.subject.as_deref()
        && creator_uid(&store.pool, transfer_uid).await?.as_deref() == Some(subject)
    {
        return Ok(true);
    }
    let Some(person) = viewer.person.as_deref() else {
        return Ok(false);
    };
    let remote: bool = query_scalar("SELECT EXISTS(SELECT 1 FROM transfer_remote_reference r JOIN record own ON own.uid = r.recipient_organ_uid AND own.slug = 'local-organ' AND own.deleted_at IS NULL WHERE r.transfer_uid = ? AND r.recipient_person_uid = ? AND r.state = 'active' AND r.projection IS NOT NULL)")
        .bind(transfer_uid).bind(person).fetch_one(&store.pool).await?;
    if remote {
        return Ok(true);
    }
    if creator_party_actor(&store.pool, transfer_uid)
        .await?
        .as_deref()
        == Some(person)
        || party_for_actor(&store.pool, transfer_uid, person)
            .await?
            .is_some()
    {
        return Ok(true);
    }
    Ok(invitations_for_transfer(&store.pool, transfer_uid)
        .await?
        .iter()
        .any(|invitation| invitation.addressed_person_uid == person))
}
