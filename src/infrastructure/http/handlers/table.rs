
use crate::{
    application::table::table_patch_row,
    infrastructure::cross_cutting::InjectedServices,
    presentation::html::{
        section::main::presentation_html_section_main,
        table::editable_row::presentation_html_table_editable_row,
    },
};
use axum::{
    Form,
    extract::{Path, State},
    response::{Html, IntoResponse},
};
use serde::Deserialize;

pub async fn handler_table_delete_by_id(
    State(services): State<InjectedServices>,
    Path((table, id)): Path<(String, String)>,
) -> Html<String> {
    let _ = services.repository.table.delete_by_id(table, id).await;
    Html(presentation_html_section_main(services).await)
}

pub async fn handler_table_editable_row(
    Path((table, id, column)): Path<(String, String, String)>,
    Form(ValueForm { value, token_kind }): Form<ValueForm>,
) -> impl IntoResponse {
    Html(
        presentation_html_table_editable_row(table, id, column, value, token_kind)
            .await
            .0,
    )
}

#[derive(Deserialize)]
pub struct ValueForm {
    pub value: String,
    pub token_kind: Option<String>,
}

pub async fn handler_table_patch_row(
    State(services): State<InjectedServices>,
    Path((table, id, column)): Path<(String, String, String)>,
    Form(ValueForm {
        value,
        token_kind: _,
    }): Form<ValueForm>,
) -> Html<String> {
    let _ = table_patch_row(services.clone(), table, id, column, value).await;
    Html(presentation_html_section_main(services).await)
}
