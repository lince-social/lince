use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
};

use axum::response::Html;

use crate::model::database::management::connection::connection;

pub async fn get_active() -> Result<Vec<(String, i32)>, Error> {
    let conn = connection().await.map_err(|e| {
        Error::new(
            ErrorKind::ConnectionAborted,
            format!("Error connecting to database: {}", e),
        )
    })?;

    let query = "SELECT name, quantity FROM configuration WHERE quantity = 1";
    let mut stmt = conn.prepare(query).map_err(|e| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("Failed to prepare query: {}", e),
        )
    })?;

    let active_config = stmt
        .query_map([], |row| {
            let name: String = row.get(0).unwrap_or_else(|_| "Unknown".to_string());
            let quantity: i32 = row.get(1).unwrap_or(0);
            Ok((name, quantity))
        })
        .unwrap();

    let mut configs = Vec::new();
    for config in active_config {
        let (name, quantity) = config.map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to process row: {}", e),
            )
        })?;
        configs.push((name, quantity));
    }

    Ok(configs)
}

pub async fn get_inactive() {
    let conn = connection().await.unwrap();
    let query = "SELECT * FROM configuration WHERE quantity <> 1";
}

pub async fn set_active(id: String) {
    let conn = connection().await.unwrap();
    let query =
        format!("UPDATE configuration SET quantity = CASE WHEN id = {id} THEN 1 ELSE 0 END");
}
