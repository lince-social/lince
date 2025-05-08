use crate::{
    application::{
        providers::query::execute::provider_query_execute,
        use_cases::operation::{
            crud::use_case_operation_create_persist, execute::use_case_operation_execute,
        },
    },
    domain::entities::operation::{Operation, Query},
    presentation::web::{
        operation::get::presentation_web_operation_get_operation_input,
        section::main::presentation_web_section_main,
    },
};
use axum::{Form, extract::Path, response::Html};
use std::collections::HashMap;

pub async fn get_operation_handler() -> Html<String> {
    Html(presentation_web_operation_get_operation_input().to_string())
}

pub async fn post_operation_handler(Form(operation): Form<Operation>) -> Html<String> {
    Html(use_case_operation_execute(operation.operation).await)
}

pub async fn handler_operation_create(
    Path(table): Path<String>,
    Form(data): Form<HashMap<String, String>>,
) -> Html<String> {
    Html(use_case_operation_create_persist(table, data).await)
}

pub async fn handler_operation_execute_query(Form(data): Form<Query>) -> Html<String> {
    let _ = provider_query_execute(data.query).await;
    Html(presentation_web_section_main().await)
}
