use crate::{
    application::providers::table::edit_row::provider_table_edit_row,
    infrastructure::{
        cross_cutting::InjectedServices,
        utils::log::{LogEntry, log},
    },
    presentation::web::section::main::presentation_web_section_main,
};

pub async fn use_case_table_patch_row(
    services: InjectedServices,
    table: String,
    id: String,
    column: String,
    value: String,
) -> String {
    let _ = provider_table_edit_row(table, id, column, value)
        .await
        .map_err(|e| log(LogEntry::Error(e.kind(), e.to_string())));

    presentation_web_section_main(services).await
}
