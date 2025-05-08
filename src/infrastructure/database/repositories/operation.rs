use crate::infrastructure::database::management::lib::connection;
use sqlx::Row;

pub async fn repository_operation_get_column_names(table: String) -> Vec<String> {
    let pool = connection().await.unwrap();
    let query = format!("PRAGMA table_info({})", table);
    let column_names: Vec<String> = sqlx::query(&query)
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.get("name"))
        .collect();
    column_names
}

pub async fn repository_operation_create(query: String) {
    let pool = connection().await.unwrap();
    let _ = sqlx::query(&query).execute(&pool).await.unwrap();
}
