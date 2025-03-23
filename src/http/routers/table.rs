use axum::{Router, routing::get};

use crate::infrastructure::http::handlers::table::get::get_handler;

pub async fn table_router() -> Router {
    Router::new().route("/{query}", get(get_handler))
}
