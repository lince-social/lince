use crate::{
    application::providers::table::edit_row::provider_table_edit_row,
    presentation::web::section::main::presentation_web_section_main,
};

pub async fn use_case_table_patch_row(
    table: String,
    id: String,
    column: String,
    value: String,
) -> String {
    provider_table_edit_row(table, id, column, value).await;
    presentation_web_section_main().await.0
}
