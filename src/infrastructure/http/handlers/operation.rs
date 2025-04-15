use std::collections::HashMap;

use axum::{Form, extract::Path, response::Html};

use crate::{
    application::use_cases::operation::{
        crud::use_case_operation_create_persist, execute_operation::execute_operation,
    },
    domain::entities::operation::Operation,
    presentation::web::operation::get::presentation_web_operation_get_operation_input,
};

pub async fn get_operation_handler() -> Html<String> {
    Html(presentation_web_operation_get_operation_input().to_string())
}

pub async fn post_operation_handler(Form(operation): Form<Operation>) -> Html<String> {
    Html(execute_operation(operation.operation).await)
}

pub async fn handler_operation_create(
    Path(table): Path<String>,
    Form(data): Form<HashMap<String, String>>,
) {
    use_case_operation_create_persist(table, data).await
}
