use crate::{
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
    let _ = services.providers.record.edit_row(table, id, column, value).await;

    presentation_web_section_main(services).await
}
