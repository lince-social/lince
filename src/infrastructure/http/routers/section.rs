use axum::{Router, routing::get};

use crate::{
    infrastructure::http::handlers::section::{handler_section_get_body, main_handler},
    presentation::web::section::{header::header, nav::presentation_web_section_nav},
};

pub async fn section_router() -> Router {
    Router::new()
        .route("/body", get(handler_section_get_body))
        .route("/header", get(header))
        .route("/nav", get(presentation_web_section_nav))
        .route("/main", get(main_handler))
}
