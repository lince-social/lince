use crate::presentation::web::section::{
    body::presentation_web_section_body, main::presentation_web_section_main,
};
use axum::{
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse},
};
use std::path::Path;
use tokio::fs;

pub async fn main_handler() -> Html<String> {
    Html(presentation_web_section_main().await)
}

pub async fn handler_section_get_body() -> Html<String> {
    Html(presentation_web_section_body().await)
}

pub async fn handler_section_favicon() -> impl IntoResponse {
    let path = Path::new("../../../assets/preto_no_branco.ico");

    match fs::read(path).await {
        Ok(bytes) => {
            let mut headers = HeaderMap::new();
            headers.insert("Content-Type", HeaderValue::from_static("image/x-icon"));
            (StatusCode::OK, headers, bytes).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
