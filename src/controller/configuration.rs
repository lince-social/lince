use axum::{Router, routing::get};

use crate::view::configuration::configurations::unhovered;

pub async fn configuration_router() -> Router {
    Router::new().route("/unhovered", get(unhovered))
    // .route("/hovered", get(hovered))
}
