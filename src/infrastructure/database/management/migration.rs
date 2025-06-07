use crate::infrastructure::database::management::lib::connection;
use std::io::Error;

pub async fn execute_migration() -> Result<(), Error> {
    let db = connection().await?;

    sqlx::query(
        "
        PRAGMA foreign_keys = OFF;



        PRAGMA foreign_keys = ON;
        ",
    )
    .execute(&db)
    .await
    .map_err(|e| Error::other(format!("Error in executing migration. Error: {}", e)))?;

    Ok(())
}
