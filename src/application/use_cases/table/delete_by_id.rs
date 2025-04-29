use crate::application::providers::table::delete_by_id::provider_table_delete_by_id;

pub async fn use_case_table_delete_by_id(table: String, id: String) {
    provider_table_delete_by_id(table, id).await;
}
