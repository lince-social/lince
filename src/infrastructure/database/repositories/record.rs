use crate::{
    application::schema::record::{record_head::RecordHead, record_quantity::RecordQuantity},
    infrastructure::database::management::lib::connection,
};
use std::io::{Error, ErrorKind};

pub async fn repository_record_set_quantity(id: u32, quantity: f64) -> Result<(), Error> {
    let query = format!(
        "UPDATE record SET quantity = {} WHERE id = {}",
        quantity, id
    );

    let pool = connection().await.unwrap();

    match sqlx::query(&query).execute(&pool).await {
        Ok(_) => Ok(()),
        Err(e) => Err(Error::new(ErrorKind::InvalidData, e)),
    }
}

pub async fn repository_record_get_quantity_by_id(id: u32) -> Result<f64, Error> {
    let pool = connection().await.unwrap();

    let record: Result<RecordQuantity, sqlx::Error> =
        sqlx::query_as(&format!("SELECT quantity FROM record WHERE id = {}", id))
            .fetch_one(&pool)
            .await;
    if record.is_err() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("No record with id: {} found", id),
        ));
    }
    let record = record.unwrap();

    Ok(record.quantity)
}

pub async fn repository_record_get_head_by_id(id: u32) -> Result<String, Error> {
    let pool = connection().await.unwrap();

    let record: Result<RecordHead, sqlx::Error> =
        sqlx::query_as(&format!("SELECT head FROM record WHERE id = {}", id))
            .fetch_one(&pool)
            .await;
    if record.is_err() {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("No record with id: {} found", id),
        ));
    }
    let record = record.unwrap();

    Ok(record.head)
}
