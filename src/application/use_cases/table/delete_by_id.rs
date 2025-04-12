use crate::{
    application::providers::table::provider_table_delete_by_id,
    presentation::web::section::main::presentation_web_main,
};

pub async fn use_case_table_delete_by_id(table: String, id: String) -> String {
    provider_table_delete_by_id(table, id).await;
    presentation_web_main().await.0
}
