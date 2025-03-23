use axum::{Router, routing::get};

use crate::infrastructure::http::handlers::section::{body::body_handler, main::main_handler};

pub async fn section_router() -> Router {
    Router::new()
        .route("/body", get(body_handler))
        .route("/main", get(main_handler))
}
