use axum::{Router, routing::get};

use crate::{
    infrastructure::http::handlers::section::{handler_section_get_body, main_handler},
    presentation::web::section::{header::header, nav::nav},
};

pub async fn section_router() -> Router {
    Router::new()
        .route("/body", get(handler_section_get_body))
        .route("/header", get(header))
        .route("/nav", get(nav))
        .route("/main", get(main_handler))
}
