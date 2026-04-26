pub mod connection;
pub mod models;
pub mod repositories;
pub mod schema;
pub mod seeder;
pub mod storage;
pub mod write_coordinator;

use {
    sqlx::{Pool, Sqlite},
    std::io::Error,
};

pub async fn bootstrap_database(db: &Pool<Sqlite>) -> Result<(), Error> {
    sqlx::migrate!("../../migrations")
        .run(db)
        .await
        .map_err(Error::other)?;
    seeder::seed(db).await?;
    Ok(())
}
