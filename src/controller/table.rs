use axum::{Router, routing::get};

pub async fn table_router() -> Router {
    Router::new().route("/{query}", get(table))
}
