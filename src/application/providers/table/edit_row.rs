use crate::infrastructure::database::repositories::query::repository_query_execute;
use std::io::Error;

pub async fn provider_table_edit_row(
    table: String,
    id: String,
    column: String,
    value: String,
) -> Result<(), Error> {
    let value = value.replace("'", "''");
    let query = format!(
        "UPDATE {} SET {} = '{}' WHERE id = {}",
        table, column, value, id
    );
    repository_query_execute(query).await
}
