use crate::infrastructure::database::management::lib::connection;

pub async fn repository_general_execute_query(query: String) {
    let pool = connection().await.unwrap();
    let _ = sqlx::query(&query).execute(&pool).await;
}
