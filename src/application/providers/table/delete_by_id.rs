use crate::infrastructure::database::repositories::table::repository_table_delete_by_id;

pub async fn provider_table_delete_by_id(table: String, id: String) {
    repository_table_delete_by_id(table, id).await
}
