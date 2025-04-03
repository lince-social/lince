use axum::{Router, routing::get};

use crate::{
    infrastructure::http::handlers::section::main_handler,
    presentation::web::section::{body::body, header::header, nav::nav},
};

pub async fn section_router() -> Router {
    Router::new()
        .route("/body", get(body))
        .route("/header", get(header))
        .route("/nav", get(nav))
        .route("/main", get(main_handler))
}
