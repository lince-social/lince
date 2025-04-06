use axum::response::Html;

use crate::{
    application::use_cases::section::body::use_case_section_get_body,
    presentation::web::section::main::presentation_web_main,
};

pub async fn main_handler() -> Html<String> {
    Html(presentation_web_main().await.0)
}

pub async fn handler_section_get_body() -> Html<String> {
    Html(use_case_section_get_body().to_string())
}
