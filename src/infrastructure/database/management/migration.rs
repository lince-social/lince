use sqlx::{Pool, Sqlite};
use std::{io::Error, sync::Arc};

pub async fn execute_migration(db: Arc<Pool<Sqlite>>) -> Result<(), Error> {
    sqlx::query(
        "
        PRAGMA foreign_keys = OFF;



        PRAGMA foreign_keys = ON;
        ",
    )
    .execute(&*db)
    .await
    .map_err(|e| Error::other(format!("Error in executing migration. Error: {}", e)))?;

    Ok(())
}
