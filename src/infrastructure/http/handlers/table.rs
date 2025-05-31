use crate::{
    application::use_cases::table::{
        delete_by_id::use_case_table_delete_by_id, patch_row::use_case_table_patch_row,
    },
    presentation::web::{
        section::main::presentation_web_section_main,
        table::editable_row::presentation_web_table_editable_row,
    },
};
use axum::{Form, extract::Path, response::Html};
use serde::Deserialize;

pub async fn handler_table_delete_by_id(Path((table, id)): Path<(String, String)>) -> Html<String> {
    use_case_table_delete_by_id(table, id).await;
    Html(presentation_web_section_main().await)
}

pub async fn handler_table_editable_row(
    Path((table, id, column)): Path<(String, String, String)>,
    Form(ValueForm { value }): Form<ValueForm>,
) -> Html<String> {
    Html(
        presentation_web_table_editable_row(table, id, column, value)
            .await
            .0,
    )
}

#[derive(Deserialize)]
pub struct ValueForm {
    pub value: String,
}

pub async fn handler_table_patch_row(
    Path((table, id, column)): Path<(String, String, String)>,
    Form(ValueForm { value }): Form<ValueForm>,
) -> Html<String> {
    Html(use_case_table_patch_row(table, id, column, value).await)
}
