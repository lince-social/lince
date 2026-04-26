pub mod apply;
pub mod inspect;
pub mod registry;
pub mod sql;
pub mod state;
pub mod types;

use {
    sqlx::{Pool, Sqlite},
    std::{collections::HashMap, io::Error},
    types::{ColumnDef, LiveColumn, LiveTable, TableSchema},
};

pub async fn ensure_schema(db: &Pool<Sqlite>) -> Result<(), Error> {
    let declared_tables = registry::declared_tables();
    state::ensure_schema_state_table(db).await?;
    let has_schema_state = state::has_schema_state(db).await?;

    if !inspect::has_any_user_tables(db).await? {
        apply::create_all_declared_tables(db, &declared_tables).await?;
    } else if !has_schema_state && inspect::table_exists(db, "_sqlx_migrations").await? {
        run_legacy_compatibility_migrations(db).await?;
    } else {
        apply::reconcile_additive(db, &declared_tables).await?;
    }

    let live_tables = inspect::inspect_declared_tables(db, &declared_tables).await?;
    validate_declared_tables(&declared_tables, &live_tables)?;
    let _fingerprint = state::write_schema_state(db, &declared_tables).await?;

    Ok(())
}

async fn run_legacy_compatibility_migrations(db: &Pool<Sqlite>) -> Result<(), Error> {
    sqlx::migrate!("../../migrations")
        .run(db)
        .await
        .map_err(Error::other)
}

fn validate_declared_tables(
    declared_tables: &[TableSchema],
    live_tables: &[LiveTable],
) -> Result<(), Error> {
    let live_by_name = live_tables
        .iter()
        .map(|table| (table.name.as_str(), table))
        .collect::<HashMap<_, _>>();

    for declared in declared_tables {
        let live = live_by_name.get(declared.name).ok_or_else(|| {
            Error::other(format!("Declared table `{}` is missing", declared.name))
        })?;

        if live.strict != declared.strict {
            let expected = if declared.strict {
                "STRICT"
            } else {
                "non-STRICT"
            };
            let actual = if live.strict { "STRICT" } else { "non-STRICT" };
            return Err(Error::other(format!(
                "Table `{}` strictness drifted: expected {expected}, found {actual}",
                declared.name
            )));
        }

        if live.columns.len() != declared.columns.len() {
            return Err(Error::other(format!(
                "Table `{}` column count drifted: expected {}, found {}",
                declared.name,
                declared.columns.len(),
                live.columns.len()
            )));
        }

        for declared_column in &declared.columns {
            let live_column = live
                .columns
                .iter()
                .find(|column| column.name == declared_column.name)
                .ok_or_else(|| {
                    Error::other(format!(
                        "Table `{}` is missing declared column `{}`",
                        declared.name, declared_column.name
                    ))
                })?;

            validate_column(declared.name, declared_column, live_column, declared)?;
        }
    }

    Ok(())
}

fn validate_column(
    table_name: &str,
    declared: &ColumnDef,
    live: &LiveColumn,
    table: &TableSchema,
) -> Result<(), Error> {
    if !same_sql_type(declared.sql_type, &live.sql_type) {
        return Err(Error::other(format!(
            "Table `{table_name}` column `{}` type drifted: expected {}, found {}",
            declared.name, declared.sql_type, live.sql_type
        )));
    }

    let part_of_composite_pk = table
        .composite_primary_key
        .as_ref()
        .is_some_and(|columns| columns.iter().any(|column| *column == declared.name));
    let declared_is_pk = declared.primary_key || part_of_composite_pk;
    if (live.primary_key_position > 0) != declared_is_pk {
        return Err(Error::other(format!(
            "Table `{table_name}` column `{}` primary-key drifted",
            declared.name
        )));
    }

    if !declared_is_pk && live.nullable != declared.nullable {
        let expected = if declared.nullable {
            "nullable"
        } else {
            "NOT NULL"
        };
        let actual = if live.nullable {
            "nullable"
        } else {
            "NOT NULL"
        };
        return Err(Error::other(format!(
            "Table `{table_name}` column `{}` nullability drifted: expected {expected}, found {actual}",
            declared.name
        )));
    }

    let normalized_live_default = live.default_sql.as_deref().map(normalize_sql_fragment);
    let normalized_declared_default = declared.default_sql.map(normalize_sql_fragment);
    if normalized_live_default != normalized_declared_default {
        return Err(Error::other(format!(
            "Table `{table_name}` column `{}` default drifted: expected {:?}, found {:?}",
            declared.name, normalized_declared_default, normalized_live_default
        )));
    }

    Ok(())
}

fn same_sql_type(expected: &str, actual: &str) -> bool {
    normalize_sql_fragment(expected) == normalize_sql_fragment(actual)
}

fn normalize_sql_fragment(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase()
}
