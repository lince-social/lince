use axum::response::Html;

use crate::presentation::web::section::{
    body::presentation_web_section_body, main::presentation_web_main,
};

pub async fn main_handler() -> Html<String> {
    Html(presentation_web_main().await.0)
}

pub async fn handler_section_get_body() -> Html<String> {
    Html(presentation_web_section_body().to_string())
}
