use crate::infrastructure::database::management::lib::connection;

pub async fn repository_table_delete_by_id(table: String, id: String) {
    let pool = connection().await.unwrap();
    let query = format!("DELETE FROM {} WHERE id = {}", table, id);

    let _ = sqlx::query(&query).execute(&pool).await;
}
