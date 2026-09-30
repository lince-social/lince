use nucleus::karma::CanonicalHash;
use sqlx::{Connection, SqliteConnection};
use tokio::sync::Mutex;

use crate::{Store, StoreError};

pub struct StateHasher {
    store: Store,
    tracker: Option<Mutex<Tracker>>,
}

struct Tracker {
    connection: SqliteConnection,
    cached: Option<(i64, CanonicalHash)>,
}

impl StateHasher {
    pub async fn new(store: Store) -> Result<Self, StoreError> {
        let filename: String =
            sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name = 'main'")
                .fetch_one(&store.pool)
                .await?;
        let tracker = if filename.is_empty() {
            None
        } else {
            let options = sqlx::sqlite::SqliteConnectOptions::new()
                .filename(filename)
                .read_only(true);
            Some(Mutex::new(Tracker {
                connection: SqliteConnection::connect_with(&options).await?,
                cached: None,
            }))
        };
        Ok(Self { store, tracker })
    }

    pub async fn hash(&self) -> Result<CanonicalHash, StoreError> {
        let Some(tracker) = &self.tracker else {
            return self.store.state_hash().await;
        };
        let mut tracker = tracker.lock().await;
        let before: i64 = sqlx::query_scalar("PRAGMA data_version")
            .fetch_one(&mut tracker.connection)
            .await?;
        if let Some((version, hash)) = &tracker.cached
            && *version == before
        {
            return Ok(hash.clone());
        }
        let hash = self.store.state_hash().await?;
        let after: i64 = sqlx::query_scalar("PRAGMA data_version")
            .fetch_one(&mut tracker.connection)
            .await?;
        tracker.cached = (before == after).then(|| (after, hash.clone()));
        Ok(hash)
    }
}
