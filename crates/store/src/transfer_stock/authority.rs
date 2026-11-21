//! Current and historical Cell authorization for hard stock limits.

use chrono::DateTime;
use nucleus::execution::now;
use serde_json::{Value, from_str};
use sqlx::SqliteConnection;

use super::invalid;
use crate::StoreError;

fn writers(roster: &Value) -> impl Iterator<Item = &Value> {
    roster["cells"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry["capabilities"]
                .as_array()
                .is_some_and(|caps| caps.iter().any(|cap| cap == "write"))
        })
        .map(|entry| &entry["cell_uid"])
}

pub(super) async fn local_cell(
    connection: &mut SqliteConnection,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar("SELECT uid FROM record WHERE slug = 'local-cell' AND kind = 'device'")
        .fetch_optional(connection)
        .await
}

pub(super) async fn require_only_writer(
    connection: &mut SqliteConnection,
    cell: &str,
) -> Result<(), StoreError> {
    let roster: Option<(String, String)> = sqlx::query_as(
        "SELECT payload, not_after FROM organ_roster
         WHERE organ_uid = (SELECT uid FROM record WHERE slug = 'local-organ' AND kind = 'organ')",
    )
    .fetch_optional(&mut *connection)
    .await?;
    if let Some((payload, expiry)) = roster {
        let valid = DateTime::parse_from_rfc3339(&expiry).is_ok_and(|expiry| expiry > now());
        let roster = from_str(&payload).map_err(invalid)?;
        let mut writers = writers(&roster);
        if !valid || writers.next().is_none_or(|writer| writer != cell) || writers.next().is_some()
        {
            return Err(invalid(
                "hard stock limits require a current Organ roster with this Cell as its only writer",
            ));
        }
    } else {
        let others: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM record WHERE kind = 'device' AND deleted_at IS NULL
               AND organ_uid = (SELECT uid FROM record WHERE slug = 'local-organ') AND uid != ?)",
        )
        .bind(cell)
        .fetch_one(&mut *connection)
        .await?;
        if others {
            return Err(invalid("hard stock limits require one writing Cell"));
        }
    }

    let history: Vec<(String, String)> = sqlx::query_as(
        "SELECT payload, not_after FROM transfer_stock_roster_history
         WHERE organ_uid = (SELECT uid FROM record WHERE slug = 'local-organ')",
    )
    .fetch_all(connection)
    .await?;
    for (payload, expiry) in history {
        // Keep considering a historical grant when its expiry cannot be parsed.
        if DateTime::parse_from_rfc3339(&expiry).is_ok_and(|expiry| expiry <= now()) {
            continue;
        }
        let roster = from_str(&payload).map_err(invalid)?;
        if writers(&roster).any(|writer| writer != cell) {
            return Err(invalid(
                "another Cell still has an unexpired write authorization; hard stock limits must wait for it to expire",
            ));
        }
    }
    Ok(())
}

/// Require the local Cell to hold current write access when an Organ roster exists.
///
/// # Errors
///
/// Returns an error if the roster is expired or denies write access, or if reading it fails.
pub async fn require_current_writer_on(
    connection: &mut SqliteConnection,
) -> Result<(), StoreError> {
    let roster: Option<(String, String, String)> = sqlx::query_as(
        "SELECT roster.payload, roster.not_after, cell.uid FROM organ_roster roster
         JOIN record organ ON organ.uid = roster.organ_uid AND organ.slug = 'local-organ'
         JOIN record cell ON cell.slug = 'local-cell' AND cell.kind = 'device'",
    )
    .fetch_optional(connection)
    .await?;
    if let Some((payload, expiry, cell)) = roster {
        let roster = from_str(&payload).map_err(invalid)?;
        if !writers(&roster).any(|writer| writer == &cell)
            || !DateTime::parse_from_rfc3339(&expiry).is_ok_and(|expiry| expiry > now())
        {
            return Err(invalid(
                "this Cell needs a current write authorization before changing resources or commitments",
            ));
        }
    }
    Ok(())
}
