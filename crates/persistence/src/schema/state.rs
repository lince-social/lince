use crate::schema::types::TableSchema;
use chrono::Utc;
use sqlx::{Pool, Sqlite};
use std::io::Error;

pub async fn ensure_schema_state_table(db: &Pool<Sqlite>) -> Result<(), Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS __schema_state (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            fingerprint TEXT NOT NULL,
            applied_at TEXT NOT NULL
        ) STRICT",
    )
    .execute(db)
    .await
    .map_err(Error::other)?;

    Ok(())
}

pub async fn has_schema_state(db: &Pool<Sqlite>) -> Result<bool, Error> {
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(1) FROM __schema_state WHERE id = 1")
        .fetch_one(db)
        .await
        .map_err(Error::other)?;

    Ok(count > 0)
}

pub async fn write_schema_state(
    db: &Pool<Sqlite>,
    tables: &[TableSchema],
) -> Result<String, Error> {
    let fingerprint = schema_fingerprint(tables);
    let applied_at = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO __schema_state (id, fingerprint, applied_at)
         VALUES (1, ?, ?)
         ON CONFLICT(id) DO UPDATE
         SET fingerprint = excluded.fingerprint,
             applied_at = excluded.applied_at",
    )
    .bind(&fingerprint)
    .bind(applied_at)
    .execute(db)
    .await
    .map_err(Error::other)?;

    Ok(fingerprint)
}

pub fn schema_fingerprint(tables: &[TableSchema]) -> String {
    let mut canonical = String::new();

    for table in tables {
        canonical.push_str(table.name);
        canonical.push('|');
        canonical.push_str(if table.strict { "strict" } else { "flex" });
        canonical.push('|');

        for column in &table.columns {
            canonical.push_str(column.name);
            canonical.push(':');
            canonical.push_str(column.sql_type);
            canonical.push(':');
            canonical.push_str(if column.nullable { "null" } else { "notnull" });
            canonical.push(':');
            canonical.push_str(if column.primary_key { "pk" } else { "col" });
            canonical.push(':');
            canonical.push_str(if column.unique { "unique" } else { "plain" });
            canonical.push(':');
            canonical.push_str(column.default_sql.unwrap_or(""));
            canonical.push(':');
            canonical.push_str(column.references_sql.unwrap_or(""));
            canonical.push(':');
            canonical.push_str(column.check_sql.unwrap_or(""));
            canonical.push('|');
        }

        canonical.push_str("pk:");
        if let Some(columns) = &table.composite_primary_key {
            canonical.push_str(&columns.join(","));
        }
        canonical.push('|');

        for check in &table.checks {
            canonical.push_str("check:");
            canonical.push_str(check);
            canonical.push('|');
        }

        for index in &table.indexes {
            canonical.push_str("index:");
            canonical.push_str(index.name);
            canonical.push(':');
            canonical.push_str(if index.unique { "unique" } else { "plain" });
            canonical.push(':');
            canonical.push_str(&index.columns.join(","));
            canonical.push(':');
            canonical.push_str(index.where_sql.unwrap_or(""));
            canonical.push('|');
        }

        canonical.push('\n');
    }

    format!("{:016x}", fnv1a64(canonical.as_bytes()))
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    let mut hash = OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }

    hash
}

#[cfg(test)]
mod tests {
    use super::schema_fingerprint;
    use crate::schema::types::{ColumnDef, TableSchema};

    #[test]
    fn fingerprint_is_stable_for_same_schema() {
        let tables = vec![TableSchema {
            name: "organ",
            strict: true,
            columns: vec![
                ColumnDef {
                    name: "id",
                    sql_type: "TEXT",
                    nullable: false,
                    primary_key: true,
                    unique: false,
                    default_sql: None,
                    references_sql: None,
                    check_sql: None,
                },
                ColumnDef {
                    name: "name",
                    sql_type: "TEXT",
                    nullable: false,
                    primary_key: false,
                    unique: false,
                    default_sql: None,
                    references_sql: None,
                    check_sql: None,
                },
            ],
            indexes: vec![],
            checks: vec![],
            composite_primary_key: None,
        }];

        assert_eq!(schema_fingerprint(&tables), schema_fingerprint(&tables));
    }
}
