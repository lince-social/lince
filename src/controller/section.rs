use axum::{Router, routing::get};

use crate::view::section::{body::body, main::main};

pub async fn section_router() -> Router {
    Router::new()
        .route("/body", get(body))
        .route("/main", get(main))
}
