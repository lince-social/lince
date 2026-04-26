use crate::schema::types::{LiveColumn, LiveIndex, LiveTable, TableSchema};
use sqlx::{Pool, Sqlite};
use std::io::Error;

#[derive(sqlx::FromRow)]
struct TableInfoRow {
    name: String,
    #[sqlx(rename = "type")]
    type_name: String,
    notnull: i64,
    dflt_value: Option<String>,
    pk: i64,
}

#[derive(sqlx::FromRow)]
struct IndexListRow {
    name: String,
    #[sqlx(rename = "unique")]
    is_unique: i64,
    origin: String,
}

pub async fn table_exists(db: &Pool<Sqlite>, table_name: &str) -> Result<bool, Error> {
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = ?",
    )
    .bind(table_name)
    .fetch_one(db)
    .await
    .map_err(Error::other)?;

    Ok(exists > 0)
}

pub async fn has_any_user_tables(db: &Pool<Sqlite>) -> Result<bool, Error> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(1)
         FROM sqlite_master
         WHERE type = 'table'
           AND name NOT LIKE 'sqlite_%'
           AND name NOT IN ('__schema_state', '_sqlx_migrations')",
    )
    .fetch_one(db)
    .await
    .map_err(Error::other)?;

    Ok(count > 0)
}

pub async fn inspect_declared_tables(
    db: &Pool<Sqlite>,
    tables: &[TableSchema],
) -> Result<Vec<LiveTable>, Error> {
    let mut live_tables = Vec::with_capacity(tables.len());
    for table in tables {
        live_tables.push(inspect_table(db, table.name).await?);
    }
    Ok(live_tables)
}

pub async fn inspect_table(db: &Pool<Sqlite>, table_name: &str) -> Result<LiveTable, Error> {
    let create_sql = sqlx::query_scalar::<_, String>(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ? LIMIT 1",
    )
    .bind(table_name)
    .fetch_optional(db)
    .await
    .map_err(Error::other)?
    .ok_or_else(|| Error::other(format!("table `{table_name}` not found")))?;

    let columns = sqlx::query_as::<_, TableInfoRow>(
        "SELECT name, type, \"notnull\" AS \"notnull\", dflt_value, pk
         FROM pragma_table_info(?)
         ORDER BY cid ASC",
    )
    .bind(table_name)
    .fetch_all(db)
    .await
    .map_err(Error::other)?
    .into_iter()
    .map(|row| LiveColumn {
        name: row.name,
        sql_type: row.type_name,
        nullable: row.notnull == 0,
        primary_key_position: row.pk,
        default_sql: row.dflt_value,
    })
    .collect::<Vec<_>>();

    let indexes = sqlx::query_as::<_, IndexListRow>(
        "SELECT name, \"unique\", origin
         FROM pragma_index_list(?)
         ORDER BY name ASC",
    )
    .bind(table_name)
    .fetch_all(db)
    .await
    .map_err(Error::other)?
    .into_iter()
    .filter(|row| row.origin != "pk")
    .map(|row| LiveIndex {
        name: row.name,
        unique: row.is_unique > 0,
    })
    .collect::<Vec<_>>();

    Ok(LiveTable {
        name: table_name.to_string(),
        strict: create_sql.to_ascii_uppercase().contains("STRICT"),
        columns,
        indexes,
    })
}
