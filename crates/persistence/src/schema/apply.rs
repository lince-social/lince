use crate::schema::{
    inspect, sql,
    types::{ColumnDef, LiveTable, TableSchema},
};
use sqlx::{Pool, Sqlite};
use std::{collections::HashSet, io::Error};

pub async fn create_all_declared_tables(
    db: &Pool<Sqlite>,
    declared_tables: &[TableSchema],
) -> Result<(), Error> {
    for schema in declared_tables {
        create_table_and_indexes(db, schema).await?;
    }

    Ok(())
}

pub async fn reconcile_additive(
    db: &Pool<Sqlite>,
    declared_tables: &[TableSchema],
) -> Result<(), Error> {
    for schema in declared_tables {
        if !inspect::table_exists(db, schema.name).await? {
            create_table_and_indexes(db, schema).await?;
            continue;
        }

        let live = inspect::inspect_table(db, schema.name).await?;
        add_missing_columns(db, schema, &live).await?;
        create_missing_indexes(db, schema, &live).await?;
    }

    Ok(())
}

pub async fn drop_obsolete_tables(db: &Pool<Sqlite>, table_names: &[&str]) -> Result<(), Error> {
    for table_name in table_names {
        if !inspect::table_exists(db, table_name).await? {
            continue;
        }

        let drop_sql = format!("DROP TABLE IF EXISTS {table_name}");
        sqlx::query(&drop_sql).execute(db).await.map_err(|error| {
            Error::other(format!(
                "failed to drop obsolete table `{table_name}` with SQL `{drop_sql}`: {error}"
            ))
        })?;
    }

    Ok(())
}

async fn create_table_and_indexes(db: &Pool<Sqlite>, schema: &TableSchema) -> Result<(), Error> {
    let create_table_sql = sql::render_create_table(schema, schema.name);
    sqlx::query(&create_table_sql)
        .execute(db)
        .await
        .map_err(|error| {
            Error::other(format!(
                "failed to create table `{}` with SQL `{}`: {error}",
                schema.name, create_table_sql
            ))
        })?;

    for index in &schema.indexes {
        let create_index_sql = sql::render_create_index(schema.name, index);
        sqlx::query(&create_index_sql)
            .execute(db)
            .await
            .map_err(|error| {
                Error::other(format!(
                    "failed to create index `{}` on table `{}` with SQL `{}`: {error}",
                    index.name, schema.name, create_index_sql
                ))
            })?;
    }

    Ok(())
}

async fn add_missing_columns(
    db: &Pool<Sqlite>,
    schema: &TableSchema,
    live: &LiveTable,
) -> Result<(), Error> {
    let live_columns = live
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect::<HashSet<_>>();

    for column in &schema.columns {
        if live_columns.contains(column.name) {
            continue;
        }

        ensure_additive_column_is_safe(schema.name, column)?;
        let add_column_sql = sql::render_add_column(schema.name, column);
        sqlx::query(&add_column_sql)
            .execute(db)
            .await
            .map_err(|error| {
                Error::other(format!(
                    "failed to add column `{}` to table `{}` with SQL `{}`: {error}",
                    column.name, schema.name, add_column_sql
                ))
            })?;
    }

    Ok(())
}

async fn create_missing_indexes(
    db: &Pool<Sqlite>,
    schema: &TableSchema,
    live: &LiveTable,
) -> Result<(), Error> {
    let live_indexes = live
        .indexes
        .iter()
        .map(|index| index.name.as_str())
        .collect::<HashSet<_>>();

    for index in &schema.indexes {
        if live_indexes.contains(index.name) {
            continue;
        }

        let create_index_sql = sql::render_create_index(schema.name, index);
        sqlx::query(&create_index_sql)
            .execute(db)
            .await
            .map_err(|error| {
                Error::other(format!(
                    "failed to create index `{}` on table `{}` with SQL `{}`: {error}",
                    index.name, schema.name, create_index_sql
                ))
            })?;
    }

    Ok(())
}

fn ensure_additive_column_is_safe(table_name: &str, column: &ColumnDef) -> Result<(), Error> {
    if column.primary_key {
        return Err(Error::other(format!(
            "cannot add primary key column `{}` to existing table `{table_name}` automatically",
            column.name
        )));
    }
    if column.unique {
        return Err(Error::other(format!(
            "cannot add UNIQUE column `{}` to existing table `{table_name}` automatically",
            column.name
        )));
    }
    if !column.nullable && column.default_sql.is_none() {
        return Err(Error::other(format!(
            "cannot add NOT NULL column `{}` to existing table `{table_name}` without a default",
            column.name
        )));
    }

    Ok(())
}
