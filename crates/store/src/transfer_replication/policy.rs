use serde_json::Value;
use sqlx::SqliteConnection;

use super::{Change, schema::Table};
use crate::StoreError;

fn bootstrap_sql(prefix: &str) -> String {
    format!(
        "{prefix}trust IN ('unknown','known') AND {prefix}share_protein IS NULL AND {prefix}scope_fields IS NULL AND {prefix}scope_version = 0 AND {prefix}accept_fields IS NULL AND {prefix}accept_version = 0 AND {prefix}closed_by_default = 0"
    )
}

fn bootstrap(row: &Value) -> bool {
    matches!(row["trust"].as_str(), Some("unknown" | "known"))
        && row["share_protein"].is_null()
        && row["scope_fields"].is_null()
        && row["scope_version"] == 0
        && row["accept_fields"].is_null()
        && row["accept_version"] == 0
        && row["closed_by_default"] == 0
}

pub(super) fn capture_condition(table: &str, operation: &str, key: &str) -> String {
    if table != "organ_contact" || operation == "DELETE" {
        return String::new();
    }
    let unowned = format!(
        "NOT EXISTS(SELECT 1 FROM transfer_sync_owner WHERE table_name = 'organ_contact' AND row_key = {key})"
    );
    let defaults = bootstrap_sql("new.");
    match operation {
        "INSERT" => format!(" AND NOT ({unowned} AND ({defaults}))"),
        "UPDATE" => format!(
            " AND NOT ({unowned} AND ({defaults}) AND ({}) AND old.trust = 'unknown' AND new.trust = 'known')",
            bootstrap_sql("old.")
        ),
        _ => String::new(),
    }
}

pub(super) async fn prepare(
    connection: &mut SqliteConnection,
    table: &Table,
    owner: &Option<String>,
    change: &Change,
) -> Result<(), StoreError> {
    if table.name != "organ_contact"
        || owner.is_some()
        || change.after.is_none()
        || !change.before.as_ref().is_some_and(bootstrap)
    {
        return Ok(());
    }
    let before = change.before.as_ref().expect("bootstrap policy has a row");
    let current = table.current(connection, before).await?;
    if current.as_ref().is_none_or(bootstrap) {
        table.change(connection, &current, &change.before).await?;
    }
    Ok(())
}
