use sqlx::{Row, SqliteConnection, query, query_scalar, sqlite::SqliteRow};

use super::protocol;
use crate::StoreError;

pub(super) struct BoundedSql {
    pub measure: &'static str,
    pub read: &'static str,
}

// Keep each row byte expression shared between preflight and payload queries.
macro_rules! bounded_sql {
    ($from:literal, $bytes:expr, $columns:literal, $order:literal $(,)?) => {
        $crate::access_snapshot::sql::bounded_sql!($from, $bytes, $columns => $columns, $order)
    };
    ($from:literal, $bytes:expr, $inner:literal => $output:literal, $order:literal $(,)?) => {
        $crate::access_snapshot::sql::BoundedSql {
            measure: concat!(
                "SELECT COUNT(*) AS row_count, COALESCE(SUM(row_bytes), 0) AS total_bytes, ",
                "COALESCE(MAX(row_bytes), 0) AS largest_bytes FROM (SELECT ",
                $bytes,
                " AS row_bytes FROM ",
                $from,
                ")",
            ),
            read: concat!(
                "WITH bounded AS (SELECT ",
                $inner,
                ", ",
                $bytes,
                " AS row_bytes FROM ",
                $from,
                ") SELECT ",
                $output,
                " FROM bounded WHERE row_bytes <= ?",
                $order,
            ),
        }
    };
}

// SQLite measures text in bytes after the BLOB cast. NULL accounts for zero bytes.
macro_rules! row_bytes {
    ($first:literal $(, $field:literal)* $(; $fixed:literal)? $(,)?) => {
        concat!(
            "COALESCE(length(CAST(", $first, " AS BLOB)), 0)",
            $(" + COALESCE(length(CAST(", $field, " AS BLOB)), 0)",)*
            $(" + ", stringify!($fixed),)?
        )
    };
}

pub(super) use {bounded_sql, row_bytes};

#[derive(Debug, Clone, Copy)]
pub(super) struct TableStats {
    pub rows: usize,
    pub bytes: usize,
    pub largest: usize,
}

pub(super) struct ByteBudget {
    used: usize,
    limit: usize,
}

impl ByteBudget {
    pub fn new(limit: usize) -> Self {
        Self { used: 0, limit }
    }

    pub fn add(&mut self, bytes: usize, label: &str) -> Result<(), StoreError> {
        self.used = self
            .used
            .checked_add(bytes)
            .ok_or_else(|| protocol(format!("{label} byte count overflow")))?;
        if self.used > self.limit {
            return Err(protocol(format!("{label} exceeds its total byte limit")));
        }
        Ok(())
    }
}

pub(super) fn sqlite_limit(value: usize, label: &str) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| protocol(format!("{label} does not fit SQLite")))
}

pub(super) fn checked_stat(value: i64, label: &str) -> Result<usize, StoreError> {
    usize::try_from(value).map_err(|_| protocol(format!("{label} is invalid")))
}

pub(super) async fn table_stats(
    connection: &mut SqliteConnection,
    sql: &'static str,
    row_limit: usize,
    row_bytes: usize,
    label: &str,
) -> Result<TableStats, StoreError> {
    let row = query(sql).fetch_one(&mut *connection).await?;
    let stats = TableStats::decode(&row, label)?;
    if stats.rows > row_limit {
        return Err(protocol(format!("{label} exceeds its row limit")));
    }
    if stats.largest > row_bytes {
        return Err(protocol(format!("{label} contains an oversized row")));
    }
    Ok(stats)
}

pub(super) async fn require_storage_types(
    connection: &mut SqliteConnection,
    sql: &'static str,
    label: &str,
) -> Result<(), StoreError> {
    let invalid: i64 = query_scalar(sql).fetch_one(&mut *connection).await?;
    if invalid != 0 {
        return Err(protocol(format!("{label} has an invalid storage type")));
    }
    Ok(())
}

pub(super) fn require_complete(
    actual: usize,
    expected: usize,
    label: &str,
) -> Result<(), StoreError> {
    if actual != expected {
        return Err(protocol(format!("{label} bounded read is incomplete")));
    }
    Ok(())
}

impl TableStats {
    pub fn decode(row: &SqliteRow, label: &str) -> Result<Self, StoreError> {
        let decode = |column, count| {
            let value = row.try_get::<i64, _>(column)?;
            usize::try_from(value).map_err(|_| protocol(format!("{label} {count} is invalid")))
        };
        Ok(Self {
            rows: decode("row_count", "row count")?,
            bytes: decode("total_bytes", "byte count")?,
            largest: decode("largest_bytes", "row byte count")?,
        })
    }
}

/// The count comes from preflight on the same connection and snapshot.
pub(super) async fn checked_rows(
    connection: &mut SqliteConnection,
    sql: &'static str,
    max_bytes: i64,
    expected: usize,
    label: &str,
) -> Result<Vec<SqliteRow>, StoreError> {
    let rows = query(sql)
        .bind(max_bytes)
        .fetch_all(&mut *connection)
        .await?;
    require_complete(rows.len(), expected, label)?;
    Ok(rows)
}
