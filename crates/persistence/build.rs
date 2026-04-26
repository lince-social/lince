use std::{
    collections::BTreeMap,
    env, fs,
    io::Error,
    path::{Path, PathBuf},
};

mod schema {
    pub mod types {
        include!("src/schema/types.rs");
    }

    pub mod sql {
        include!("src/schema/sql.rs");
    }
}

mod models {
    pub mod auth {
        include!("src/models/auth.rs");
    }

    pub mod core {
        include!("src/models/core.rs");
    }

    pub mod karma {
        include!("src/models/karma.rs");
    }

    pub mod sidecars {
        include!("src/models/sidecars.rs");
    }
}

mod registry {
    include!("src/schema/registry.rs");
}

use schema::types::{ColumnDef, IndexDef, TableSchema};

const SNAPSHOT_FILE: &str = "lince-schema-snapshot.json";

fn main() {
    if let Err(error) = run() {
        panic!("failed to generate migrations: {error}");
    }
}

fn run() -> Result<(), Error> {
    rerun_if_changed("build.rs");
    rerun_if_changed("src/schema/types.rs");
    rerun_if_changed("src/schema/sql.rs");
    rerun_if_changed("src/schema/registry.rs");
    rerun_if_changed("src/models/auth.rs");
    rerun_if_changed("src/models/core.rs");
    rerun_if_changed("src/models/karma.rs");
    rerun_if_changed("src/models/sidecars.rs");

    let migrations_dir = repo_root().join("migrations");
    fs::create_dir_all(&migrations_dir)?;

    let snapshot_path = repo_root().join("target").join(SNAPSHOT_FILE);
    if let Some(parent) = snapshot_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let declared_tables = registry::declared_tables();
    let current_snapshot = declared_tables.clone();

    if !has_sql_migration_files(&migrations_dir)? {
        let version = next_version(&migrations_dir)?;
        let filename = format!("{version}_auto_init.sql");
        fs::write(
            migrations_dir.join(filename),
            render_migration(&initial_migration_statements(&declared_tables)),
        )?;
        write_snapshot(&snapshot_path, &current_snapshot)?;
        return Ok(());
    }

    let previous_snapshot = read_snapshot(&snapshot_path)?;
    let Some(previous_snapshot) = previous_snapshot.as_ref() else {
        write_snapshot(&snapshot_path, &current_snapshot)?;
        return Ok(());
    };

    if previous_snapshot != &current_snapshot {
        let statements = plan_schema_diff(previous_snapshot, &current_snapshot)?;
        if !statements.is_empty() {
            let version = next_version(&migrations_dir)?;
            let filename = format!("{version}_auto_schema_update.sql");
            fs::write(migrations_dir.join(filename), render_migration(&statements))?;
        }
        write_snapshot(&snapshot_path, &current_snapshot)?;
    }

    Ok(())
}

fn rerun_if_changed(path: &str) {
    println!("cargo:rerun-if-changed={path}");
}

fn repo_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../..")
}

fn read_snapshot(path: &Path) -> Result<Option<Vec<TableSchema>>, Error> {
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(path)?;
    match serde_json::from_str(&raw) {
        Ok(snapshot) => Ok(Some(snapshot)),
        Err(_) => Ok(None),
    }
}

fn write_snapshot(path: &Path, snapshot: &[TableSchema]) -> Result<(), Error> {
    let raw = serde_json::to_string_pretty(snapshot).map_err(Error::other)?;
    fs::write(path, raw)?;
    Ok(())
}

fn initial_migration_statements(declared_tables: &[TableSchema]) -> Vec<String> {
    declared_tables
        .iter()
        .flat_map(create_table_and_indexes_statements)
        .collect()
}

fn create_table_and_indexes_statements(schema: &TableSchema) -> Vec<String> {
    let mut statements = vec![schema::sql::render_create_table(schema, schema.name.as_str())];
    for index in &schema.indexes {
        statements.push(schema::sql::render_create_index(schema.name.as_str(), index));
    }
    statements
}

fn plan_schema_diff(previous: &[TableSchema], current: &[TableSchema]) -> Result<Vec<String>, Error> {
    let previous_by_name = previous
        .iter()
        .map(|table| (table.name.as_str(), table))
        .collect::<BTreeMap<_, _>>();

    let mut statements = Vec::new();

    for current_table in current {
        match previous_by_name.get(current_table.name.as_str()) {
            None => statements.extend(create_table_and_indexes_statements(current_table)),
            Some(previous_table) => {
                statements.extend(plan_table_diff(previous_table, current_table)?);
            }
        }
    }

    for previous_table in previous {
        if current
            .iter()
            .all(|table| table.name != previous_table.name)
        {
            return Err(Error::other(format!(
                "table `{}` was removed from the Rust schema; write a manual migration for drops",
                previous_table.name
            )));
        }
    }

    Ok(statements)
}

fn plan_table_diff(
    previous: &TableSchema,
    current: &TableSchema,
) -> Result<Vec<String>, Error> {
    if previous.strict != current.strict {
        return Err(Error::other(format!(
            "table `{}` changed strictness; write a manual migration",
            current.name
        )));
    }
    if previous.checks != current.checks {
        return Err(Error::other(format!(
            "table `{}` changed CHECK constraints; write a manual migration",
            current.name
        )));
    }
    if previous.composite_primary_key != current.composite_primary_key {
        return Err(Error::other(format!(
            "table `{}` changed primary key definition; write a manual migration",
            current.name
        )));
    }

    let previous_columns = previous
        .columns
        .iter()
        .map(|column| (column.name.as_str(), column))
        .collect::<BTreeMap<_, _>>();
    let mut statements = Vec::new();

    for previous_column in &previous.columns {
        if current
            .columns
            .iter()
            .all(|column| column.name != previous_column.name)
        {
            return Err(Error::other(format!(
                "column `{}` was removed from table `{}`; write a manual migration",
                previous_column.name, current.name
            )));
        }
    }

    for current_column in &current.columns {
        match previous_columns.get(current_column.name.as_str()) {
            None => {
                ensure_additive_column_is_safe(current.name.as_str(), current_column)?;
                statements.push(render_add_column(current.name.as_str(), current_column));
            }
            Some(previous_column) if *previous_column != current_column => {
                return Err(Error::other(format!(
                    "column `{}` changed in table `{}`; write a manual migration",
                    current_column.name, current.name
                )));
            }
            Some(_) => {}
        }
    }

    let previous_indexes = previous
        .indexes
        .iter()
        .map(|index| (index.name.as_str(), index))
        .collect::<BTreeMap<_, _>>();

    for current_index in &current.indexes {
        match previous_indexes.get(current_index.name.as_str()) {
            None => statements.push(render_create_index(current.name.as_str(), current_index)),
            Some(previous_index) if *previous_index != current_index => {
                return Err(Error::other(format!(
                    "index `{}` changed on table `{}`; write a manual migration",
                    current_index.name, current.name
                )));
            }
            Some(_) => {}
        }
    }

    for previous_index in &previous.indexes {
        if current
            .indexes
            .iter()
            .all(|index| index.name != previous_index.name)
        {
            return Err(Error::other(format!(
                "index `{}` was removed from table `{}`; write a manual migration",
                previous_index.name, current.name
            )));
        }
    }

    Ok(statements)
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

fn render_add_column(table_name: &str, column: &ColumnDef) -> String {
    schema::sql::render_add_column(table_name, column)
}

fn render_create_index(table_name: &str, index: &IndexDef) -> String {
    schema::sql::render_create_index(table_name, index)
}

fn next_version(dir: &Path) -> Result<i64, Error> {
    let mut next = 1_i64;
    if dir.exists() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("sql") {
                continue;
            }
            if let Some(file_name) = path.file_name().and_then(|value| value.to_str())
                && let Some(version) = migration_version_from_filename(file_name)
            {
                next = next.max(version.saturating_add(1));
            }
        }
    }

    let now = chrono::Utc::now().timestamp_millis().max(1);
    Ok(next.max(now))
}

fn migration_version_from_filename(filename: &str) -> Option<i64> {
    let prefix = filename.split_once('_')?.0;
    prefix.parse::<i64>().ok()
}

fn has_sql_migration_files(dir: &Path) -> Result<bool, Error> {
    if !dir.exists() {
        return Ok(false);
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some("sql") {
            return Ok(true);
        }
    }

    Ok(false)
}

fn render_migration(statements: &[String]) -> String {
    let mut sql = String::new();
    for statement in statements {
        sql.push_str(statement.trim());
        if !statement.trim_end().ends_with(';') {
            sql.push(';');
        }
        sql.push('\n');
    }
    sql
}
