use axum::{Router, routing::get};

use crate::view::section::{body::body, header::header, main::main, nav::nav};

pub async fn section_router() -> Router {
    Router::new()
        .route("/body", get(body))
        .route("/header", get(header))
        .route("/nav", get(nav))
        .route("/main", get(main))
}
