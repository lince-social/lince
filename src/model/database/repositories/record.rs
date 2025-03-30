use crate::model::entities::record::Record;
use sqlx::SqlitePool;
use std::io::{Error, ErrorKind};

pub async fn create_record() -> Result<Option<Record>, Error> {
    let pool = SqlitePool::connect("/home/eduardo/.config/lince/lince.db").await;
    if pool.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error connecting to database",
        ));
    }
    let pool = pool.unwrap();

    sqlx::query("INSERT INTO record (head) VALUES $1")
        .bind("Record 1")
        .fetch_optional(&pool)
        .await;

    let record: Option<Record> = sqlx::query_as("SELECT * FROM record")
        .fetch_optional(&pool)
        .await
        .unwrap();

    Ok(record)
}

// pub async fn get_inactive() {
//     let conn = connection().await.unwrap();
//     let query = "SELECT * FROM configuration WHERE quantity <> 1";
// }

// pub async fn set_active(id: String) {
//     let conn = connection().await.unwrap();
//     let query =
//         format!("UPDATE configuration SET quantity = CASE WHEN id = {id} THEN 1 ELSE 0 END");
// }
