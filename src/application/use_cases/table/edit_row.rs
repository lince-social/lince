use crate::presentation::web::table::editable_row::presentation_web_table_editable_row;

pub async fn use_case_table_edit_row(
    table: String,
    id: String,
    column: String,
    value: String,
) -> String {
    presentation_web_table_editable_row(table, id, column, value)
        .await
        .0
}
