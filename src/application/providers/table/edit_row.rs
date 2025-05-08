use crate::infrastructure::database::repositories::query::repository_query_execute;

pub async fn provider_table_edit_row(table: String, id: String, column: String, value: String) {
    let query = format!(
        "UPDATE {} SET {} = '{}' WHERE id = {}",
        table, column, value, id
    );
    let _ = repository_query_execute(query).await;
}
