use std::collections::HashSet;

use nucleus::PromiseState;
use sqlx::{Sqlite, Transaction};

use super::DraftPromiseRevisionInput;
use crate::{StoreError, transfers::item_terms::encode_item};

pub(super) async fn replace(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
    promises: &[DraftPromiseRevisionInput],
    author: &str,
    revision: i64,
    at: &str,
) -> Result<(), StoreError> {
    let editable_promise_uids: HashSet<String> = sqlx::query_scalar(
        "SELECT uid FROM promise
         WHERE transfer_uid = ? AND state IN ('open', 'proposed', 'agreed')",
    )
    .bind(transfer_uid)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .collect();
    let retained_promise_uids: HashSet<&str> = promises
        .iter()
        .filter_map(|promise| promise.uid.as_deref())
        .collect();
    for uid in retained_promise_uids
        .iter()
        .copied()
        .filter(|uid| !editable_promise_uids.contains(*uid))
    {
        let existing: Option<(String, String)> =
            sqlx::query_as("SELECT transfer_uid, state FROM promise WHERE uid = ?")
                .bind(uid)
                .fetch_optional(&mut **tx)
                .await?;
        if let Some((owner, state)) = existing {
            return Err(sqlx::Error::Protocol(format!(
                "promise `{uid}` already belongs to Transfer `{owner}` in state `{state}`"
            )));
        }
    }
    for uid in &editable_promise_uids {
        if !retained_promise_uids.contains(uid.as_str()) {
            sqlx::query(
                "UPDATE promise SET state = 'withdrawn', revision = ?, updated_at = ?
                 WHERE uid = ? AND transfer_uid = ?",
            )
            .bind(revision)
            .bind(at)
            .bind(uid)
            .bind(transfer_uid)
            .execute(&mut **tx)
            .await?;
        }
    }

    for promise in promises {
        let owner = promise.person_uid.as_deref().ok_or_else(|| {
            sqlx::Error::Protocol("every Transfer promise requires an owning Person".into())
        })?;
        let existing_open_owner: Option<String> = if let Some(uid) = promise.uid.as_deref() {
            sqlx::query_scalar(
                "SELECT party_uid FROM promise
                 WHERE uid = ? AND transfer_uid = ? AND state = 'open'",
            )
            .bind(uid)
            .bind(transfer_uid)
            .fetch_optional(&mut **tx)
            .await?
            .flatten()
        } else {
            None
        };
        if let Some(existing_owner) = existing_open_owner.as_deref() {
            if !promise.open || existing_owner != owner {
                return Err(sqlx::Error::Protocol(
                    "retained OPEN promise must keep its original proposer and OPEN state".into(),
                ));
            }
        } else if promise.open && owner != author {
            return Err(sqlx::Error::Protocol(
                "new OPEN promise must belong to the Person signing this revision".into(),
            ));
        }
        let state = if promise.open {
            PromiseState::Open
        } else {
            PromiseState::Proposed
        };
        let location = promise.location.as_ref();
        let item_json = encode_item(promise.item.as_ref())?;
        if let Some(uid) = promise
            .uid
            .as_deref()
            .filter(|uid| editable_promise_uids.contains(*uid))
        {
            sqlx::query(
                "UPDATE promise SET record_uid = ?, concept_uid = ?, unit_uid = ?,
                    party_uid = ?, delta = ?, window_start = ?, window_end = ?,
                    location_lat = ?, location_lon = ?, location_address = ?,
                    condition = ?, reserve_from = ?, open_reuse_policy = ?, state = ?,
                    revision = ?, updated_at = ?, item_json = COALESCE(?, item_json)
                 WHERE uid = ? AND transfer_uid = ?",
            )
            .bind(&promise.record_uid)
            .bind(&promise.concept_uid)
            .bind(&promise.unit_uid)
            .bind(&promise.person_uid)
            .bind(promise.delta)
            .bind(&promise.window_start)
            .bind(&promise.window_end)
            .bind(location.and_then(|value| value.lat))
            .bind(location.and_then(|value| value.lon))
            .bind(location.and_then(|value| value.address.as_deref()))
            .bind(&promise.condition)
            .bind(&promise.reserve_from)
            .bind(promise.open_reuse_policy.as_str())
            .bind(state.as_str())
            .bind(revision)
            .bind(at)
            .bind(&item_json)
            .bind(uid)
            .bind(transfer_uid)
            .execute(&mut **tx)
            .await?;
        } else {
            let uid = promise.uid.clone().unwrap_or_else(|| nucleus::new_uid("p"));
            sqlx::query(
                "INSERT INTO promise
                    (uid, source_promise_uid, record_uid, concept_uid, unit_uid, party_uid, delta,
                     window_start, window_end, location_lat, location_lon,
                     location_address, condition, transfer_uid, reserve_from,
                     open_reuse_policy, state, revision, created_at, updated_at, item_json)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(uid)
            .bind(&promise.source_promise_uid)
            .bind(&promise.record_uid)
            .bind(&promise.concept_uid)
            .bind(&promise.unit_uid)
            .bind(&promise.person_uid)
            .bind(promise.delta)
            .bind(&promise.window_start)
            .bind(&promise.window_end)
            .bind(location.and_then(|value| value.lat))
            .bind(location.and_then(|value| value.lon))
            .bind(location.and_then(|value| value.address.as_deref()))
            .bind(&promise.condition)
            .bind(transfer_uid)
            .bind(&promise.reserve_from)
            .bind(promise.open_reuse_policy.as_str())
            .bind(state.as_str())
            .bind(revision)
            .bind(at)
            .bind(at)
            .bind(&item_json)
            .execute(&mut **tx)
            .await?;
        }
    }

    Ok(())
}
