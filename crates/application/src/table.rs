use injection::cross_cutting::InjectedServices;
use std::io::Error;

pub async fn table_patch_row(
    services: InjectedServices,
    table: String,
    id: String,
    column: String,
    value: String,
) -> Result<(), Error> {
    crate::write::table_patch_row(services, table, id, column, value).await
}
