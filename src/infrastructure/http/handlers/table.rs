use crate::{
    application::use_cases::table::{
        delete_by_id::use_case_table_delete_by_id, patch_row::use_case_table_patch_row,
    },
    infrastructure::cross_cutting::InjectedServices,
    presentation::web::{
        section::main::presentation_web_section_main,
        table::editable_row::presentation_web_table_editable_row,
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
    services.providers.table.delete_by_id(table, id).await;
    Html(presentation_web_section_main(services).await)
}

pub async fn handler_table_editable_row(
    State(services): State<InjectedServices>,
    Path((table, id, column)): Path<(String, String, String)>,
    Form(ValueForm { value }): Form<ValueForm>,
) -> Html<String> {
    Html(
        presentation_web_table_editable_row(services, table, id, column, value)
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
    Html(use_case_table_patch_row(services, table, id, column, value).await)
}
