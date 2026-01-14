use sqlx::{Pool, Sqlite};
use std::{io::Error, sync::Arc};

#[allow(dead_code)]
pub async fn execute_migration(db: Arc<Pool<Sqlite>>) -> Result<(), Error> {
    sqlx::query(
        "
        PRAGMA foreign_keys = OFF;
         -- 1. Rename old table
         ALTER TABLE frequency RENAME TO frequency_old;

         -- 2. Recreate with day_week as TEXT
         CREATE TABLE frequency (
             id INTEGER PRIMARY KEY,
             quantity REAL NOT NULL DEFAULT 1,
             name TEXT NOT NULL DEFAULT 'Frequency',
             day_week TEXT,
             months REAL DEFAULT 0 NOT NULL,
             days REAL DEFAULT 0 NOT NULL,
             seconds REAL DEFAULT 0 NOT NULL,
             next_date TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL,
             finish_date DATETIME,
             catch_up_sum INTEGER NOT NULL DEFAULT 0
         );

         -- 3. Copy old data (SQLite will auto-convert REAL → TEXT if possible)
         INSERT INTO frequency (
             id, quantity, name, day_week, months, days, seconds, next_date, finish_date, catch_up_sum
         )
         SELECT
             id, quantity, name, day_week, months, days, seconds, next_date, finish_date, catch_up_sum
         FROM frequency_old;

         -- 4. Drop old table
         DROP TABLE frequency_old;

        PRAGMA foreign_keys = ON;
        ",
    )
    .execute(&*db)
    .await
    .map_err(|e| Error::other(format!("Error in executing migration. Error: {}", e)))?;

    Ok(())
}
