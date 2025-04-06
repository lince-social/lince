use crate::domain::entities::record::{Record, RecordSchemaCreate};
use sqlx::{Pool, Sqlite, sqlite};
use std::io::{Error, ErrorKind};

pub async fn connection() -> Result<Pool<Sqlite>, Error> {
    let config_dir = dirs::config_dir().unwrap();
    let db_path = String::from(config_dir.to_str().unwrap()) + "/lince/lince.db";
    let opt = sqlite::SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true);
    let pool = sqlite::SqlitePool::connect_with(opt).await;
    if pool.is_err() {
        return Err(Error::new(ErrorKind::Other, "Pool error"));
    }
    let pool = pool.unwrap();
    Ok(pool)
}

pub async fn repository_record_create(record: RecordSchemaCreate) -> Result<(), Error> {
    let pool = connection().await;
    if pool.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error connecting to database",
        ));
    }
    let pool = pool.unwrap();

    // let result = sqlx::query("INSERT INTO record (quantity, head, body) VALUES (?1, ?2, ?3)")

    let result = sqlx::query("INSERT INTO record (head) VALUES (?1)")
        // .bind(record.quantity.unwrap_or_else(|| 1.0.into()))
        .bind(record.head)
        // .bind(record.head.unwrap_or_else(|| "record".into()))
        // .bind(record.body.unwrap_or_else(|| "".into()))
        .fetch_optional(&pool)
        .await;
    if result.is_err() {
        println!("error in inserting record")
    }

    Ok(())
}

pub async fn record_repository_fetch_all() -> Result<Vec<Record>, Error> {
    let pool = connection().await.unwrap();

    let record: Result<Vec<Record>, sqlx::Error> = sqlx::query_as("SELECT * FROM record")
        .fetch_all(&pool)
        .await;
    if record.is_err() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Error when querying record fetch all",
        ));
    }
    let record = record.unwrap();

    Ok(record)
}

pub async fn repository_record_delete_by_id(id: String) -> Result<(), Error> {
    let pool = connection().await.unwrap();

    let query = format!("DELETE FROM record WHERE id = {}", id);

    let record: Result<Vec<Record>, sqlx::Error> = sqlx::query_as(&query).fetch_all(&pool).await;
    if record.is_err() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Error when deleting record",
        ));
    }

    Ok(())
}

pub async fn repository_record_zero_quantity(id: String) -> () {
    let query = format!("UPDATE record SET quantity = 0 WHERE id = {}", id);
    let pool = connection().await.unwrap();
    let _ = sqlx::query(&query).fetch_optional(&pool).await;
    ()
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
