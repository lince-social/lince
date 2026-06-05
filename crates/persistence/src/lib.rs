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

pub async fn bootstrap_database(db: &Pool<Sqlite>, local_base_url: &str) -> Result<(), Error> {
    sqlx::migrate!("../../migrations")
        .run(db)
        .await
        .map_err(Error::other)?;
    seeder::seed(db, local_base_url).await?;
    Ok(())
}
