use crate::presentation::web::section::main::presentation_web_main;

pub async fn use_case_table_patch_row(
    table: String,
    id: String,
    column: String,
    value: String,
) -> String {
    presentation_web_main().await.0
}
