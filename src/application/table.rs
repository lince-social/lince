use crate::infrastructure::cross_cutting::InjectedServices;
use std::io::Error;

pub async fn table_patch_row(
    services: InjectedServices,
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

    services.repository.query.execute(&query).await
}
