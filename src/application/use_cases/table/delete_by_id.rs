use crate::infrastructure::cross_cutting::InjectedServices;

pub async fn use_case_table_delete_by_id(services: InjectedServices, table: String, id: String) {
    services.providers.table.delete_by_id(table, id).await;
}
