use axum::{Router, routing::get};

use crate::infrastructure::http::handlers::configuration::get_configuration_handler;

pub async fn configuration_router() -> Router {
    Router::new().route("/unhovered", get(get_configuration_handler))
}
