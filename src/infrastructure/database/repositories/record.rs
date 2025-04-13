use std::io::{Error, ErrorKind};

use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

use crate::{
    domain::entities::record::{Record, RecordSchemaCreate},
    infrastructure::database::management::lib::connection,
};

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

pub async fn repository_record_set_quantity(id: String, quantity: f64) {
    let query = format!(
        "UPDATE record SET quantity = {} WHERE id = {}",
        quantity, id
    );
    let pool = connection().await.unwrap();
    let _ = sqlx::query(&query).fetch_optional(&pool).await;
}

pub async fn repository_record_get_quantity_by_id(id: u32) -> f64 {
    #[derive(FromRow, Serialize, Deserialize)]
    pub struct Record {
        pub quantity: f64,
    }
    let pool = connection().await.unwrap();
    let record: Record = sqlx::query_as(&format!("SELECT quantity FROM record WHERE id = {}", id))
        .fetch_one(&pool)
        .await
        .unwrap();
    record.quantity
}
