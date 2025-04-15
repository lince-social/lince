use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

use crate::infrastructure::database::management::lib::connection;

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
