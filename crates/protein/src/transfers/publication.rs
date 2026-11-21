//! Adds local controls and loan status after Transfer sorting and truncation.

use super::{
    application, hierarchy::TransferOutput, remote_delivery::disable_remote_transfer_actions,
    viewer::TransferViewer,
};
use crate::ProteinError;
use nucleus::execution::now;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use store::{
    Store, cells,
    sqlx::query_scalar,
    transfer_loans::{status, terms},
};

pub(super) async fn finish(
    store: &Store,
    rows: &mut [TransferOutput],
    viewer: &TransferViewer,
    signer: Option<&str>,
    visible: Option<&HashSet<String>>,
) -> Result<(), ProteinError> {
    if viewer.local || viewer.person.as_deref() == signer {
        for row in &mut *rows {
            application::policies(store, &mut row.value, signer, visible).await?;
            attach_writing_cell(store, row, viewer, signer).await?;
        }
    }
    attach_loan_status(store, rows).await
}

async fn attach_writing_cell(
    store: &Store,
    row: &mut TransferOutput,
    viewer: &TransferViewer,
    signer: Option<&str>,
) -> Result<(), ProteinError> {
    let own_viewer = viewer.local
        || query_scalar("SELECT EXISTS(SELECT 1 FROM record p JOIN record own ON own.uid = p.organ_uid AND own.slug = 'local-organ' WHERE p.uid = ?)")
            .bind(viewer.person.as_deref()).fetch_one(&store.pool).await?;
    if !own_viewer {
        return Ok(());
    }
    // Resolve the local Cell in the same read phase as the Transfer's current writer.
    let Some(cell) = cells::local(&store.pool).await? else {
        return Ok(());
    };
    let owner = query_scalar::<_, String>("SELECT cell_uid FROM transfer_sync_owner WHERE (table_name = 'transfer' AND row_key = json_array(?1)) OR (table_name = 'transfer_remote_reference' AND row_key IN (SELECT json_array(uid) FROM transfer_remote_reference WHERE transfer_uid = ?1 AND recipient_person_uid = ?2)) ORDER BY table_name LIMIT 1")
        .bind(&row.uid).bind(signer).fetch_optional(&store.pool).await?;
    if let Some(owner) = owner.filter(|owner| *owner != cell.uid) {
        disable_remote_transfer_actions(&mut row.value, "transfer_origin_cell_required");
        let label =
            query_scalar::<_, String>("SELECT head FROM record WHERE uid = ? AND head != ''")
                .bind(&owner)
                .fetch_optional(&store.pool)
                .await?;
        row.value["writing_cell_name"] = Value::String(label.unwrap_or_else(|| owner.clone()));
        row.value["writing_cell"] = Value::String(owner);
    }
    Ok(())
}

async fn attach_loan_status(
    store: &Store,
    rows: &mut [TransferOutput],
) -> Result<(), ProteinError> {
    let mut promises = rows
        .iter_mut()
        .flat_map(|row| {
            let uid = row.uid.as_str();
            row.value
                .get_mut("promises")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
                .filter(|promise| promise["accepted_loan"].is_object())
                .map(move |promise| (uid, promise))
        })
        .peekable();
    if promises.peek().is_none() {
        return Ok(());
    }
    let loans = terms(&store.pool, true).await?;
    let mut by_exchange = HashMap::new();
    for loan in &loans {
        if let Some(route) = &loan.item.exchange {
            by_exchange
                .entry((loan.transfer.as_str(), route.uid.as_str()))
                .or_insert(loan);
        }
    }
    for (uid, promise) in promises {
        if let Some(exchange) = promise["exchange"].as_str()
            && let Some(loan) = by_exchange.get(&(uid, exchange))
        {
            promise["loan_status"] = status(loan, &loans, now().timestamp_millis())?;
        }
    }
    Ok(())
}
