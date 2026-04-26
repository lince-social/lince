use crate::schema::types::{ColumnDef, IndexDef, TableSchema};

pub fn render_create_table(schema: &TableSchema, table_name: &str) -> String {
    let mut entries = schema
        .columns
        .iter()
        .map(|column| render_column(column, schema.composite_primary_key.is_none()))
        .collect::<Vec<_>>();

    if let Some(columns) = &schema.composite_primary_key {
        entries.push(format!("PRIMARY KEY ({})", columns.join(", ")));
    }

    for check in &schema.checks {
        entries.push(format!("CHECK ({check})"));
    }

    let strict_suffix = if schema.strict { " STRICT" } else { "" };
    format!(
        "CREATE TABLE IF NOT EXISTS {table_name} (\n    {}\n){strict_suffix}",
        entries.join(",\n    ")
    )
}

pub fn render_add_column(table_name: &str, column: &ColumnDef) -> String {
    format!(
        "ALTER TABLE {table_name} ADD COLUMN {}",
        render_column(column, false)
    )
}

pub fn render_create_index(table_name: &str, index: &IndexDef) -> String {
    let unique_prefix = if index.unique { "UNIQUE " } else { "" };
    let where_clause = index
        .where_sql
        .map(|value| format!(" WHERE {value}"))
        .unwrap_or_default();
    format!(
        "CREATE {unique_prefix}INDEX IF NOT EXISTS {} ON {table_name}({}){where_clause}",
        index.name,
        index.columns.join(", "),
    )
}

fn render_column(column: &ColumnDef, allow_inline_primary_key: bool) -> String {
    let mut parts = vec![column.name.to_string(), column.sql_type.to_string()];

    if column.primary_key && allow_inline_primary_key {
        parts.push("PRIMARY KEY".into());
    }
    if !column.nullable && !(column.primary_key && allow_inline_primary_key) {
        parts.push("NOT NULL".into());
    }
    if column.unique {
        parts.push("UNIQUE".into());
    }
    if let Some(default_sql) = column.default_sql {
        parts.push(format!("DEFAULT {default_sql}"));
    }
    if let Some(references_sql) = column.references_sql {
        parts.push(format!("REFERENCES {references_sql}"));
    }
    if let Some(check_sql) = column.check_sql {
        parts.push(format!("CHECK ({check_sql})"));
    }

    parts.join(" ")
}
