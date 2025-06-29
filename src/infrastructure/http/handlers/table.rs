use crate::{
    application::use_cases::table::patch_row::use_case_table_patch_row,
    infrastructure::cross_cutting::InjectedServices,
    presentation::html::{
        section::main::presentation_html_section_main,
        table::editable_row::presentation_html_table_editable_row,
    },
};
use axum::{
    Form,
    extract::{Path, State},
    response::Html,
};
use serde::Deserialize;

pub async fn handler_table_delete_by_id(
    State(services): State<InjectedServices>,
    Path((table, id)): Path<(String, String)>,
) -> Html<String> {
    let _ = services.providers.table.delete_by_id(table, id).await;
    Html(presentation_html_section_main(services).await)
}

pub async fn handler_table_editable_row(
    Path((table, id, column)): Path<(String, String, String)>,
    Form(ValueForm { value }): Form<ValueForm>,
) -> Html<String> {
    Html(
        presentation_html_table_editable_row(table, id, column, value)
            .await
            .0,
    )
}

#[derive(Deserialize)]
pub struct ValueForm {
    pub value: String,
}

pub async fn handler_table_patch_row(
    State(services): State<InjectedServices>,
    Path((table, id, column)): Path<(String, String, String)>,
    Form(ValueForm { value }): Form<ValueForm>,
) -> Html<String> {
    let _ = use_case_table_patch_row(services.clone(), table, id, column, value).await;
    Html(presentation_html_section_main(services).await)
}
