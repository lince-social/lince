use domain::clean::frequency::Frequency;
use injection::cross_cutting::InjectedServices;
use persistence::write_coordinator::{SqlParameter, WriteOutcome};
use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
};

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

    execute_statement(
        services,
        format!("UPDATE {table} SET {column} = ? WHERE id = ?"),
        vec![SqlParameter::Text(value), SqlParameter::Integer(id)],
    )
    .await
    .map(|_| ())
}

pub async fn table_delete_row(
    services: InjectedServices,
    table: String,
    id: String,
) -> Result<(), Error> {
    let table = validate_identifier(&table)?;
    let id = parse_i64(&id)?;

    execute_statement(
        services,
        format!("DELETE FROM {table} WHERE id = ?"),
        vec![SqlParameter::Integer(id)],
    )
    .await
    .map(|_| ())
}

pub async fn table_insert_row(
    services: InjectedServices,
    table: String,
    values: HashMap<String, String>,
) -> Result<(), Error> {
    let table = validate_identifier(&table)?;
    if values.is_empty() {
        return execute_statement(
            services,
            format!("INSERT INTO {table} DEFAULT VALUES"),
            vec![],
        )
        .await
        .map(|_| ());
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

    execute_statement(
        services,
        format!(
            "INSERT INTO {table} ({}) VALUES ({placeholders})",
            columns.join(", ")
        ),
        params,
    )
    .await
    .map(|_| ())
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
    execute_statement(
        services,
        "UPDATE record SET quantity = ? WHERE id = ?",
        vec![
            SqlParameter::Real(quantity),
            SqlParameter::Integer(id as i64),
        ],
    )
    .await
    .map(|_| ())
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

pub async fn pin_view(
    services: InjectedServices,
    view_id: u32,
    position_x: f64,
    position_y: f64,
) -> Result<(), Error> {
    let pinned_views = services.repository.collection.get_pinned_views().await?;
    let new_z_index = pinned_views
        .iter()
        .map(|pinned| pinned.z_index)
        .max()
        .unwrap_or(0)
        + 1;

    let sql = if pinned_views.iter().any(|pinned| pinned.view_id == view_id) {
        "UPDATE pinned_view SET position_x = ?, position_y = ?, z_index = ? WHERE view_id = ?"
    } else {
        "INSERT INTO pinned_view (view_id, position_x, position_y, z_index) VALUES (?, ?, ?, ?)"
    };
    let params = if sql.starts_with("UPDATE") {
        vec![
            SqlParameter::Real(position_x),
            SqlParameter::Real(position_y),
            SqlParameter::Integer(new_z_index as i64),
            SqlParameter::Integer(view_id as i64),
        ]
    } else {
        vec![
            SqlParameter::Integer(view_id as i64),
            SqlParameter::Real(position_x),
            SqlParameter::Real(position_y),
            SqlParameter::Integer(new_z_index as i64),
        ]
    };

    execute_statement(services, sql, params).await.map(|_| ())
}

pub async fn unpin_view(services: InjectedServices, view_id: u32) -> Result<(), Error> {
    execute_statement(
        services,
        "DELETE FROM pinned_view WHERE view_id = ?",
        vec![SqlParameter::Integer(view_id as i64)],
    )
    .await
    .map(|_| ())
}

pub async fn update_view_position(
    services: InjectedServices,
    view_id: u32,
    position_x: f64,
    position_y: f64,
) -> Result<(), Error> {
    execute_statement(
        services,
        "UPDATE pinned_view SET position_x = ?, position_y = ? WHERE view_id = ?",
        vec![
            SqlParameter::Real(position_x),
            SqlParameter::Real(position_y),
            SqlParameter::Integer(view_id as i64),
        ],
    )
    .await
    .map(|_| ())
}

pub async fn update_view_size(
    services: InjectedServices,
    view_id: u32,
    width: f64,
    height: f64,
) -> Result<(), Error> {
    execute_statement(
        services,
        "UPDATE pinned_view SET width = ?, height = ? WHERE view_id = ?",
        vec![
            SqlParameter::Real(width),
            SqlParameter::Real(height),
            SqlParameter::Integer(view_id as i64),
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
