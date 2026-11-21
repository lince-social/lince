//! Store projections of incoming sync operations.

mod assertions;
mod concepts;
mod extensions;
mod records;

pub use assertions::{retract_assertion, upsert_assertion};
pub use concepts::{delete_concept, upsert_concept};
pub use extensions::{set_extension_key, set_extension_whole, tombstone_extension_key};
pub use records::{
    ensure_record_stub, record_deleted, set_record_field, set_record_text_raw, tombstone_record,
    undelete_record,
};

use sqlx::{Sqlite, SqliteConnection, query, query::Query, sqlite::SqliteArguments};

use crate::StoreError;

/// The operation identity and clock used to reject stale replays.
#[derive(Debug, Clone, Copy)]
pub struct Stamp<'a> {
    pub tbl: &'a str,
    pub uid: &'a str,
    pub field: &'a str,
    pub hlc: i64,
}

macro_rules! stamped {
    ($($sql:literal),+ $(,)?) => {
        concat!(
            $($sql,)+
            " AND NOT EXISTS (SELECT 1 FROM sync_op WHERE tbl = ? AND uid = ? AND field = ? AND hlc > ?)",
        )
    };
}

use stamped;

type SqliteQuery<'q> = Query<'q, Sqlite, SqliteArguments>;

impl Stamp<'_> {
    fn bind<'q>(self, query: SqliteQuery<'q>) -> SqliteQuery<'q> {
        query
            .bind(self.tbl)
            .bind(self.uid)
            .bind(self.field)
            .bind(self.hlc)
    }

    async fn superseded(self, connection: &mut SqliteConnection) -> Result<bool, StoreError> {
        Ok(self
            .bind(query(
                "SELECT 1 FROM sync_op
              WHERE tbl = ? AND uid = ? AND field = ? AND hlc > ? LIMIT 1",
            ))
            .fetch_optional(connection)
            .await?
            .is_some())
    }
}
