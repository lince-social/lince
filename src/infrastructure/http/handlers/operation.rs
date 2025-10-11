use crate::{
    application::operation::{operation_create_persist, operation_execute},
    domain::clean::operation::{Operation, Query},
    infrastructure::cross_cutting::InjectedServices,
    presentation::html::{
        operation::get::presentation_html_operation_get_operation_input,
        section::main::presentation_html_section_main,
    },
};
use axum::{
    Form,
    extract::{Path, State},
    response::Html,
};
use std::collections::HashMap;

pub async fn get_operation_handler() -> Html<String> {
    Html(presentation_html_operation_get_operation_input().to_string())
}

pub async fn post_operation_handler(
    State(services): State<InjectedServices>,
    Form(operation): Form<Operation>,
) -> Html<String> {
    Html(operation_execute(services, operation.operation).await)
}

pub async fn handler_operation_create(
    State(services): State<InjectedServices>,
    Path(table): Path<String>,
    Form(data): Form<HashMap<String, String>>,
) -> Html<String> {
    Html(operation_create_persist(services, table, data).await)
}

pub async fn handler_operation_execute_query(
    State(services): State<InjectedServices>,
    Form(data): Form<Query>,
) -> Html<String> {
    let _ = services.repository.query.execute(&data.query).await;
    Html(presentation_html_section_main(services).await)
}
