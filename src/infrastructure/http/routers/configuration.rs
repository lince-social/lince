use axum::{Router, routing::get};

use crate::infrastructure::http::handlers::configuration::get::get_handler;

pub async fn configuration_router() -> Router {
    Router::new().route("/", get(get_handler))
}
