//! Frequency definitions, activation metadata, and the next scheduled time.

use std::collections::{HashMap, HashSet};

use nucleus::karma::ScheduleCursorLifecycle;
use serde_json::{Value, json};
use store::{
    Store,
    frequency::all,
    karma::{
        frequencies::{get_activation, get_handle, get_revision},
        schedules::{StoredScheduleCursor, list_cursors},
    },
    records::quantity,
};

use super::karma::karma_query_error;
use crate::ProteinError;

pub(crate) async fn execute_frequency(
    store: &Store,
    visible: Option<&HashSet<String>>,
) -> Result<Vec<Value>, ProteinError> {
    let mut rows = Vec::new();
    let pool = &store.pool;
    let cursors = list_cursors(pool).await?;
    let cursors: HashMap<_, _> = cursors
        .iter()
        .map(|cursor| (&cursor.activation_hash, cursor))
        .collect();
    for frequency in all(pool).await? {
        if visible.is_some_and(|visible| !visible.contains(&frequency.uid)) {
            continue;
        }
        let amount = quantity(pool, &frequency.uid)
            .await?
            .ok_or_else(|| karma_query_error("protein_frequency_record_missing", &frequency.uid))?;
        let handle = get_handle(pool, &frequency.uid).await?;
        let definition = match &handle {
            Some(handle) => get_revision(pool, &handle.head_revision_hash).await?,
            None => None,
        };
        let last_run = match handle
            .as_ref()
            .and_then(|handle| handle.latest_activation_hash.as_ref())
        {
            Some(hash) => get_activation(pool, hash).await?,
            None => None,
        };
        let cursor = handle
            .as_ref()
            .and_then(|handle| cursors.get(handle.active_activation_hash.as_ref()?))
            .copied();
        let next = cursor
            .filter(|cursor| {
                matches!(
                    cursor.lifecycle,
                    ScheduleCursorLifecycle::Armed | ScheduleCursorLifecycle::Leased
                )
            })
            .and_then(|cursor| cursor.cursor.next_intended_at());
        let next_local = match cursor
            .filter(|_| next.is_some())
            .map(|cursor| &cursor.cursor)
        {
            Some(StoredScheduleCursor::Calendar { cursor }) => {
                Some(cursor.next().requested_local.to_string())
            }
            _ => None,
        };
        rows.push(json!({
            "kind": "frequency",
            "uid": frequency.uid,
            "slug": frequency.slug,
            "head": frequency.head,
            "quantity": amount,
            "every": frequency.every,
            "anchor_at": frequency.anchor_at,
            "created_at": frequency.created_at,
            "next_at_ms": next.map(|next| next.as_millis()),
            "next_local": next_local,
            "handle_revision": handle.as_ref().map(|handle| handle.handle_revision),
            "head_revision_hash": handle.as_ref().map(|handle| &handle.head_revision_hash),
            "active_revision_hash": handle.as_ref().and_then(|handle| handle.active_revision_hash.as_ref()),
            "last_run_revision_hash": last_run.as_ref().map(|activation| activation.epoch.definition_revision_hash()),
            "last_run_parameters": last_run.as_ref().map(|activation| activation.epoch.effective_parameters()),
            "definition": definition.as_ref().map(|revision| &revision.frequency),
            "source": definition.as_ref().map(|revision| &revision.canonical_dsl),
            "status": handle.map(|handle| handle.status),
        }));
    }
    Ok(rows)
}
