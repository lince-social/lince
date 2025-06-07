use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::web::section::{
        body::presentation_web_section_body, main::presentation_web_section_main,
    },
};
use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse},
};
use std::path::Path;
use tokio::fs;

pub async fn main_handler(State(services): State<InjectedServices>) -> Html<String> {
    Html(presentation_web_section_main(services).await)
}

pub async fn handler_section_get_body(State(services): State<InjectedServices>) -> Html<String> {
    Html(presentation_web_section_body(services).await)
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
