use axum::{Router, routing::get};

use crate::infrastructure::http::handlers::karma::handler_get_karma_orchestra;

pub async fn router_karma() -> Router {
    Router::new().route("/", get(handler_get_karma_orchestra))
}
