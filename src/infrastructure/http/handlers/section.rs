use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::html::section::{
        body::presentation_html_section_body, header::presentation_html_section_header,
        main::presentation_html_section_main,
    },
};
use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse},
};
use std::path::Path;
use tokio::fs;

pub async fn handler_section_main(State(services): State<InjectedServices>) -> Html<String> {
    Html(presentation_html_section_main(services).await)
}

pub async fn handler_section_body(State(services): State<InjectedServices>) -> Html<String> {
    Html(presentation_html_section_body(services).await)
}

pub async fn handler_section_header(State(services): State<InjectedServices>) -> Html<String> {
    Html(presentation_html_section_header(services).await)
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
