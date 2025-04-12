use crate::infrastructure::database::repositories::general::repository_general_execute_query;

pub async fn provider_table_edit_row(table: String, id: String, column: String, value: String) {
    let query = format!(
        "UPDATE {} SET {} = '{}' WHERE id = {}",
        table, column, value, id
    );
    repository_general_execute_query(query).await
}
