//! Check projected stock and imported authorship on the caller's transaction.

use sqlx::{Row, SqliteConnection};

use super::{
    authority::{local_cell, require_only_writer},
    invalid, limit_on, require_current_writer_on,
};
use crate::{StoreError, exact::read_decimal, transfer_balances, transport};

/// Check a Record's active stock limit on the caller's connection.
///
/// Local writes check current authorization and projected surplus. Imported writes
/// check authorship against the limit's writing Cell.
///
/// # Errors
///
/// Returns an error when authorization, balance projection, or the minimum surplus check fails.
pub async fn validate_record_on(
    connection: &mut SqliteConnection,
    record: &str,
    imported_writer: Option<&str>,
) -> Result<(), StoreError> {
    if imported_writer.is_none() {
        require_current_writer_on(connection).await?;
    }
    let Some(limit) = limit_on(connection, record).await? else {
        return Ok(());
    };
    let authority: &str = limit.get("writer_cell_uid");
    if let Some(writer) = imported_writer {
        return if writer == authority {
            Ok(())
        } else {
            Err(invalid(
                "this stock change was authored by a Cell without stock authority",
            ))
        };
    }
    if local_cell(connection).await?.as_deref() != Some(authority) {
        return Err(invalid("change hard-reserved stock on its writing Cell"));
    }
    require_only_writer(connection, authority).await?;
    let balance = transfer_balances::read_on(connection, record).await?;
    if !balance.incomplete.is_empty() {
        return Err(invalid(
            "hard stock limit cannot verify every remaining commitment",
        ));
    }
    let minimum = read_decimal(&limit, "minimum")?;
    let surplus = balance.surplus;
    if surplus.exact_numeric_cmp(minimum).is_lt() {
        return Err(invalid(format_args!(
            "hard stock limit: remaining surplus {surplus} would be below {minimum}",
        )));
    }
    Ok(())
}

/// Validate the local writer, active stock limits, and Transport capacities.
///
/// # Errors
///
/// Returns the first failed authorization, stock, capacity, or storage check.
pub async fn validate_all_on(connection: &mut SqliteConnection) -> Result<(), StoreError> {
    require_current_writer_on(connection).await?;
    let records: Vec<String> = sqlx::query_scalar("SELECT record_uid FROM transfer_stock_limit")
        .fetch_all(&mut *connection)
        .await?;
    for record in records {
        validate_record_on(connection, &record, None).await?;
    }
    transport::validate_all_on(connection).await
}
