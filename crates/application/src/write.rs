use domain::clean::frequency::Frequency;
use injection::cross_cutting::InjectedServices;
use persistence::write_coordinator::{SqlParameter, WriteOutcome};
use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
};

use crate::karma::{deliver_record_karma, deliver_transfer_karma, refresh_karma_cache};
use crate::record_sync::{RecordSyncAction, SyncOrigin};

pub async fn execute_record_insert_returning_id(
    services: InjectedServices,
    sql: impl Into<String>,
    params: Vec<SqlParameter>,
) -> Result<WriteOutcome, Error> {
    let outcome = services
        .writer
        .execute_statement_returning_id(sql.into(), params)
        .await?;
    if outcome.rows_affected > 0
        && let Some(id) = outcome.last_insert_rowid
    {
        handle_record_change(services, [id as u32], RecordSyncAction::Insert, SyncOrigin::Local)
            .await?;
    }
    Ok(outcome)
}

pub async fn execute_record_update(
    services: InjectedServices,
    record_ids: impl IntoIterator<Item = u32>,
    sql: impl Into<String>,
    params: Vec<SqlParameter>,
) -> Result<WriteOutcome, Error> {
    let ids = record_ids.into_iter().collect::<Vec<_>>();
    let outcome = execute_statement(services.clone(), sql, params).await?;
    if outcome.rows_affected > 0 {
        handle_record_change(services, ids, RecordSyncAction::Update, SyncOrigin::Local).await?;
    }
    Ok(outcome)
}

pub async fn execute_record_delete(
    services: InjectedServices,
    id: u32,
    sql: impl Into<String>,
    params: Vec<SqlParameter>,
) -> Result<WriteOutcome, Error> {
    let identity = crate::record_sync::capture_record_identity(services.clone(), id).await?;
    let outcome = execute_statement(services.clone(), sql, params).await?;
    if outcome.rows_affected > 0 {
        cleanup_deleted_record_sidecars(services.clone(), id).await?;
        crate::record_sync::enqueue_captured_record_delete(
            services.clone(),
            identity,
            SyncOrigin::Local,
        )
        .await?;
        handle_record_business_change(services, [id]).await?;
    }
    Ok(outcome)
}

pub async fn execute_record_sidecar_insert_returning_id(
    services: InjectedServices,
    table_name: &'static str,
    root_record_id: Option<u32>,
    sql: impl Into<String>,
    params: Vec<SqlParameter>,
) -> Result<WriteOutcome, Error> {
    let outcome = services
        .writer
        .execute_statement_returning_id(sql.into(), params)
        .await?;
    if outcome.rows_affected > 0
        && let Some(row_id) = outcome.last_insert_rowid
        && let Some(root_record_id) = root_record_id
    {
        handle_record_business_change(services.clone(), [root_record_id]).await?;
        crate::record_sync::enqueue_record_sidecar_operation(
            services,
            table_name,
            row_id,
            root_record_id,
            RecordSyncAction::Insert,
            SyncOrigin::Local,
        )
        .await?;
    }
    Ok(outcome)
}

pub async fn execute_record_sidecar_update(
    services: InjectedServices,
    table_name: &'static str,
    row_id: i64,
    root_record_id: Option<u32>,
    sql: impl Into<String>,
    params: Vec<SqlParameter>,
) -> Result<WriteOutcome, Error> {
    let root_record_id = match root_record_id {
        Some(id) => Some(id),
        None => resolve_record_sidecar_root_record_id(services.clone(), table_name, row_id).await?,
    };
    let outcome = execute_statement(services.clone(), sql, params).await?;
    if outcome.rows_affected > 0
        && let Some(root_record_id) = root_record_id
    {
        handle_record_business_change(services.clone(), [root_record_id]).await?;
        crate::record_sync::enqueue_record_sidecar_operation(
            services,
            table_name,
            row_id,
            root_record_id,
            RecordSyncAction::Update,
            SyncOrigin::Local,
        )
        .await?;
    }
    Ok(outcome)
}

pub async fn execute_record_sidecar_delete(
    services: InjectedServices,
    table_name: &'static str,
    row_id: i64,
    sql: impl Into<String>,
    params: Vec<SqlParameter>,
) -> Result<WriteOutcome, Error> {
    let root_record_id =
        resolve_record_sidecar_root_record_id(services.clone(), table_name, row_id).await?;
    let outcome = execute_statement(services.clone(), sql, params).await?;
    if outcome.rows_affected > 0
        && let Some(root_record_id) = root_record_id
    {
        handle_record_business_change(services.clone(), [root_record_id]).await?;
        crate::record_sync::enqueue_record_sidecar_operation(
            services,
            table_name,
            row_id,
            root_record_id,
            RecordSyncAction::Delete,
            SyncOrigin::Local,
        )
        .await?;
    }
    Ok(outcome)
}

async fn cleanup_deleted_record_sidecars(
    services: InjectedServices,
    id: u32,
) -> Result<WriteOutcome, Error> {
    let id = i64::from(id);
    execute_statement(
        services,
        "DELETE FROM record_link WHERE record_id = ? OR (target_table = 'record' AND target_id = ?)",
        vec![SqlParameter::Integer(id), SqlParameter::Integer(id)],
    )
    .await
}

pub async fn insert_record_from_file_sync(
    services: InjectedServices,
    owner_organ_id: i64,
    head: String,
    body: String,
) -> Result<WriteOutcome, Error> {
    execute_record_insert_returning_id(
        services,
        "INSERT INTO record(head, body, owner_organ_id) VALUES (?, ?, ?) RETURNING id",
        vec![
            crate::file_sync::text_param(head),
            crate::file_sync::text_param(body),
            SqlParameter::Integer(owner_organ_id),
        ],
    )
    .await
}

pub async fn update_record_head_body_from_file_sync(
    services: InjectedServices,
    id: i64,
    head: String,
    body: String,
) -> Result<WriteOutcome, Error> {
    if id <= 0 {
        return Ok(crate::file_sync::empty_outcome());
    }

    execute_record_update(
        services,
        [id as u32],
        "UPDATE record SET head = ?, body = ? WHERE id = ?",
        vec![
            crate::file_sync::text_param(head),
            crate::file_sync::text_param(body),
            SqlParameter::Integer(id),
        ],
    )
    .await
}

pub async fn delete_record_from_file_sync(
    services: InjectedServices,
    id: i64,
) -> Result<WriteOutcome, Error> {
    if id <= 0 {
        return Ok(crate::file_sync::empty_outcome());
    }

    execute_record_delete(
        services,
        id as u32,
        "DELETE FROM record WHERE id = ?",
        vec![SqlParameter::Integer(id)],
    )
    .await
}

pub async fn execute_sql(
    services: InjectedServices,
    sql: impl Into<String>,
) -> Result<WriteOutcome, Error> {
    services.writer.execute_sql(sql.into()).await
}

pub async fn execute_statement(
    services: InjectedServices,
    sql: impl Into<String>,
    params: Vec<SqlParameter>,
) -> Result<WriteOutcome, Error> {
    services.writer.execute_statement(sql.into(), params).await
}

pub async fn table_patch_row(
    services: InjectedServices,
    table: String,
    id: String,
    column: String,
    value: String,
) -> Result<(), Error> {
    let table = validate_identifier(&table)?;
    let column = validate_identifier(&column)?;
    let id = parse_i64(&id)?;
    let needs_karma_refresh = matches!(
        table.as_str(),
        "karma" | "karma_condition" | "karma_consequence"
    );

    let _outcome = execute_statement(
        services.clone(),
        format!("UPDATE {table} SET {column} = ? WHERE id = ?"),
        vec![SqlParameter::Text(value), SqlParameter::Integer(id)],
    )
    .await?;

    if table == "record" && _outcome.rows_affected > 0 {
        handle_record_change(
            services.clone(),
            [id as u32],
            RecordSyncAction::Update,
            SyncOrigin::Local,
        )
        .await?;
    }
    if is_record_sidecar_table(&table) && _outcome.rows_affected > 0 {
        if let Some(root_record_id) =
            resolve_record_sidecar_root_record_id(services.clone(), &table, id).await?
        {
            handle_record_business_change(services.clone(), [root_record_id]).await?;
            crate::record_sync::enqueue_record_sidecar_operation(
                services.clone(),
                &table,
                id,
                root_record_id,
                RecordSyncAction::Update,
                SyncOrigin::Local,
            )
            .await?;
        }
    }
    if table == "transfer" && _outcome.rows_affected > 0 {
        handle_transfer_change(services.clone(), [id as u32]).await?;
    }
    if needs_karma_refresh && _outcome.rows_affected > 0 {
        refresh_karma_cache(services.clone()).await?;
    }

    Ok(())
}

pub async fn table_delete_row(
    services: InjectedServices,
    table: String,
    id: String,
) -> Result<(), Error> {
    let table = validate_identifier(&table)?;
    let id = parse_i64(&id)?;
    let needs_karma_refresh = matches!(
        table.as_str(),
        "karma" | "karma_condition" | "karma_consequence"
    );
    let deleted_record_identity = if table == "record" {
        Some(crate::record_sync::capture_record_identity(
            services.clone(),
            id as u32,
        )
        .await?)
    } else {
        None
    };
    let deleted_sidecar_root_record_id = if is_record_sidecar_table(&table) {
        resolve_record_sidecar_root_record_id(services.clone(), &table, id).await?
    } else {
        None
    };

    let _outcome = execute_statement(
        services.clone(),
        format!("DELETE FROM {table} WHERE id = ?"),
        vec![SqlParameter::Integer(id)],
    )
    .await?;

    if table == "record" && _outcome.rows_affected > 0 {
        if let Some(identity) = deleted_record_identity {
            crate::record_sync::enqueue_captured_record_delete(
                services.clone(),
                identity,
                SyncOrigin::Local,
            )
            .await?;
        }
        handle_record_business_change(services.clone(), [id as u32]).await?;
    }
    if is_record_sidecar_table(&table)
        && _outcome.rows_affected > 0
        && let Some(root_record_id) = deleted_sidecar_root_record_id
    {
        handle_record_business_change(services.clone(), [root_record_id]).await?;
        crate::record_sync::enqueue_record_sidecar_operation(
            services.clone(),
            &table,
            id,
            root_record_id,
            RecordSyncAction::Delete,
            SyncOrigin::Local,
        )
        .await?;
    }
    if table == "transfer" && _outcome.rows_affected > 0 {
        handle_transfer_change(services.clone(), [id as u32]).await?;
    }
    if needs_karma_refresh && _outcome.rows_affected > 0 {
        refresh_karma_cache(services.clone()).await?;
    }

    Ok(())
}

pub async fn table_insert_row(
    services: InjectedServices,
    table: String,
    values: HashMap<String, String>,
) -> Result<(), Error> {
    let table = validate_identifier(&table)?;
    let needs_karma_refresh = matches!(
        table.as_str(),
        "karma" | "karma_condition" | "karma_consequence"
    );
    let insert_root_record_id = values
        .get("record_id")
        .and_then(|value| value.parse::<u32>().ok());
    if values.is_empty() {
        let outcome = execute_statement(
            services.clone(),
            format!("INSERT INTO {table} DEFAULT VALUES"),
            vec![],
        )
        .await?;
        if table == "record"
            && outcome.rows_affected > 0
            && let Some(id) = outcome.last_insert_rowid
        {
            handle_record_change(
                services.clone(),
                [id as u32],
                RecordSyncAction::Insert,
                SyncOrigin::Local,
            )
            .await?;
        }
        if is_record_sidecar_table(&table)
            && outcome.rows_affected > 0
            && let Some(id) = outcome.last_insert_rowid
        {
            crate::record_sync::ensure_table_row_identity(services.clone(), &table, id).await?;
        }
        if table == "transfer"
            && outcome.rows_affected > 0
            && let Some(id) = outcome.last_insert_rowid
        {
            handle_transfer_change(services.clone(), [id as u32]).await?;
        }
        if needs_karma_refresh && outcome.rows_affected > 0 {
            refresh_karma_cache(services.clone()).await?;
        }
        return Ok(());
    }

    let mut entries = values
        .into_iter()
        .map(|(column, value)| Ok((validate_identifier(&column)?, value)))
        .collect::<Result<Vec<_>, Error>>()?;
    entries.sort_by(|left, right| left.0.cmp(&right.0));

    let columns = entries
        .iter()
        .map(|(column, _)| column.clone())
        .collect::<Vec<_>>();
    let placeholders = vec!["?"; columns.len()].join(", ");
    let params = entries
        .into_iter()
        .map(|(_, value)| SqlParameter::Text(value))
        .collect::<Vec<_>>();

    let outcome = execute_statement(
        services.clone(),
        format!(
            "INSERT INTO {table} ({}) VALUES ({placeholders})",
            columns.join(", ")
        ),
        params,
    )
    .await?;

    if table == "record"
        && outcome.rows_affected > 0
        && let Some(id) = outcome.last_insert_rowid
    {
        handle_record_change(
            services.clone(),
            [id as u32],
            RecordSyncAction::Insert,
            SyncOrigin::Local,
        )
        .await?;
    }
    if is_record_sidecar_table(&table)
        && outcome.rows_affected > 0
        && let Some(id) = outcome.last_insert_rowid
    {
        if let Some(root_record_id) = insert_root_record_id {
            handle_record_business_change(services.clone(), [root_record_id]).await?;
            crate::record_sync::enqueue_record_sidecar_operation(
                services.clone(),
                &table,
                id,
                root_record_id,
                RecordSyncAction::Insert,
                SyncOrigin::Local,
            )
            .await?;
        } else {
            crate::record_sync::ensure_table_row_identity(services.clone(), &table, id).await?;
        }
    }
    if table == "transfer"
        && outcome.rows_affected > 0
        && let Some(id) = outcome.last_insert_rowid
    {
        handle_transfer_change(services.clone(), [id as u32]).await?;
    }
    if needs_karma_refresh && outcome.rows_affected > 0 {
        refresh_karma_cache(services.clone()).await?;
    }

    Ok(())
}

pub async fn set_active_collection(services: InjectedServices, id: &str) -> Result<(), Error> {
    let id = parse_i64(id)?;
    execute_statement(
        services,
        "UPDATE collection SET quantity = CASE WHEN id = ? THEN 1 ELSE 0 END",
        vec![SqlParameter::Integer(id)],
    )
    .await
    .map(|_| ())
}

pub async fn set_active_configuration(services: InjectedServices, id: &str) -> Result<(), Error> {
    let id = parse_i64(id)?;
    execute_statement(
        services,
        "UPDATE configuration SET quantity = CASE WHEN id = ? THEN 1 ELSE 0 END",
        vec![SqlParameter::Integer(id)],
    )
    .await
    .map(|_| ())
}

pub async fn toggle_collection_views(
    services: InjectedServices,
    collection_id: u32,
) -> Result<(), Error> {
    execute_statement(
        services,
        "
        UPDATE collection_view
        SET quantity = CASE
            WHEN EXISTS (
                SELECT 1
                FROM collection_view
                WHERE collection_id = ?
                  AND quantity = 1
            )
            THEN 0
            ELSE 1
        END
        WHERE collection_id = ?
        ",
        vec![
            SqlParameter::Integer(collection_id as i64),
            SqlParameter::Integer(collection_id as i64),
        ],
    )
    .await
    .map(|_| ())
}

pub async fn toggle_view(
    services: InjectedServices,
    collection_id: u32,
    view_id: u32,
) -> Result<(), Error> {
    execute_statement(
        services,
        "
        UPDATE collection_view
        SET quantity = CASE WHEN quantity = 1 THEN 0 ELSE 1 END
        WHERE view_id = ? AND collection_id = ?
        ",
        vec![
            SqlParameter::Integer(view_id as i64),
            SqlParameter::Integer(collection_id as i64),
        ],
    )
    .await
    .map(|_| ())
}

pub async fn set_record_quantity(
    services: InjectedServices,
    id: u32,
    quantity: f64,
) -> Result<(), Error> {
    if let Ok(record) = services.repository.record.get_by_id(id).await
        && (record.quantity - quantity).abs() < f64::EPSILON
    {
        return Ok(());
    }

    let outcome = execute_statement(
        services.clone(),
        "UPDATE record SET quantity = ? WHERE id = ?",
        vec![
            SqlParameter::Real(quantity),
            SqlParameter::Integer(id as i64),
        ],
    )
    .await?;

    if outcome.rows_affected > 0 {
        handle_record_change(
            services.clone(),
            [id],
            RecordSyncAction::Update,
            SyncOrigin::Local,
        )
        .await?;
    }

    Ok(())
}

pub async fn update_frequency(
    services: InjectedServices,
    frequency: Frequency,
) -> Result<(), Error> {
    execute_statement(
        services,
        "UPDATE frequency SET quantity = ?, next_date = ? WHERE id = ?",
        vec![
            SqlParameter::Real(frequency.quantity),
            SqlParameter::Text(frequency.next_date),
            SqlParameter::Integer(frequency.id as i64),
        ],
    )
    .await
    .map(|_| ())
}

pub async fn set_delete_confirmation_for_active(
    services: InjectedServices,
    enabled: bool,
) -> Result<(), Error> {
    execute_statement(
        services,
        "UPDATE configuration SET delete_confirmation = ? WHERE quantity = 1",
        vec![SqlParameter::Integer(i64::from(enabled))],
    )
    .await
    .map(|_| ())
}

pub async fn set_desktop_startup_for_active(
    services: InjectedServices,
    start_on_login: Option<bool>,
    start_silent: Option<bool>,
) -> Result<(), Error> {
    execute_statement(
        services,
        "
        UPDATE configuration
        SET desktop_start_on_login = ?,
            desktop_start_silent = ?
        WHERE quantity = 1
        ",
        vec![
            optional_bool_parameter(start_on_login),
            optional_bool_parameter(start_silent),
        ],
    )
    .await
    .map(|_| ())
}

pub async fn set_active_configuration_language_if_unset(
    services: InjectedServices,
    language: &str,
) -> Result<(), Error> {
    execute_statement(
        services,
        "
        UPDATE configuration
        SET language = ?
        WHERE quantity = 1
          AND (language IS NULL OR trim(language) = '')
        ",
        vec![SqlParameter::Text(language.to_string())],
    )
    .await
    .map(|_| ())
}

pub async fn update_collection_view_column_widths(
    services: InjectedServices,
    collection_view_id: u32,
    widths: HashMap<String, f32>,
) -> Result<(), Error> {
    let widths_json = serde_json::to_string(&widths).map_err(Error::other)?;
    execute_statement(
        services,
        "UPDATE collection_view SET column_sizes = ? WHERE id = ?",
        vec![
            SqlParameter::Text(widths_json),
            SqlParameter::Integer(collection_view_id as i64),
        ],
    )
    .await
    .map(|_| ())
}

fn validate_identifier(identifier: &str) -> Result<String, Error> {
    if identifier.is_empty()
        || !identifier
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("Invalid SQL identifier: {identifier}"),
        ));
    }

    Ok(identifier.to_string())
}

fn parse_i64(value: &str) -> Result<i64, Error> {
    value.parse::<i64>().map_err(|error| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("Expected integer identifier, got {value}: {error}"),
        )
    })
}

fn optional_bool_parameter(value: Option<bool>) -> SqlParameter {
    match value {
        Some(value) => SqlParameter::Integer(if value { 1 } else { 0 }),
        None => SqlParameter::Null,
    }
}

fn is_record_sidecar_table(table_name: &str) -> bool {
    matches!(
        table_name,
        "record_extension"
            | "record_link"
            | "record_comment"
            | "record_worklog"
            | "record_resource_ref"
            | "work_metadata"
            | "work_assignment"
            | "work_subject"
    )
}

async fn handle_record_change(
    services: InjectedServices,
    record_ids: impl IntoIterator<Item = u32>,
    action: RecordSyncAction,
    origin: SyncOrigin,
) -> Result<(), Error> {
    let ids = record_ids.into_iter().collect::<Vec<_>>();
    handle_record_business_change(services.clone(), ids.clone()).await?;
    crate::record_sync::enqueue_record_operations(services, ids, action, origin).await?;
    Ok(())
}

async fn handle_record_business_change(
    services: InjectedServices,
    record_ids: impl IntoIterator<Item = u32>,
) -> Result<(), Error> {
    let ids = record_ids.into_iter().collect::<Vec<_>>();
    if !ids.is_empty() {
        deliver_record_karma(services.clone(), ids).await?;
    }
    crate::file_sync::sync_after_record_change(services).await?;
    Ok(())
}

async fn resolve_record_sidecar_root_record_id(
    services: InjectedServices,
    table_name: &str,
    row_id: i64,
) -> Result<Option<u32>, Error> {
    if row_id <= 0 {
        return Ok(None);
    }
    let sql = match table_name {
        "record_extension"
        | "record_link"
        | "record_comment"
        | "record_worklog"
        | "record_resource_ref" => {
            format!("SELECT record_id FROM {table_name} WHERE id = ?")
        }
        "work_metadata" => {
            "SELECT owner_id FROM work_metadata WHERE id = ? AND owner_kind = 'record'".to_string()
        }
        "work_assignment" => {
            "SELECT metadata.owner_id
             FROM work_assignment assignment
             JOIN work_metadata metadata ON metadata.id = assignment.work_metadata_id
             WHERE assignment.id = ? AND metadata.owner_kind = 'record'"
                .to_string()
        }
        "work_subject" => return Ok(None),
        _ => {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("Table {table_name} is not part of record sync"),
            ));
        }
    };
    let id = sqlx::query_scalar::<_, i64>(&sql)
        .bind(row_id)
        .fetch_optional(&*services.db)
        .await
        .map_err(Error::other)?;
    Ok(id.and_then(|value| u32::try_from(value).ok()))
}

async fn handle_transfer_change(
    services: InjectedServices,
    transfer_ids: impl IntoIterator<Item = u32>,
) -> Result<(), Error> {
    let ids = transfer_ids.into_iter().collect::<Vec<_>>();
    if !ids.is_empty() {
        deliver_transfer_karma(services, ids).await?;
    }
    Ok(())
}
