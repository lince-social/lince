use axum::{Router, routing::patch};

use crate::infrastructure::http::handlers::view::handler_view_toggle;
pub async fn view_router() -> Router {
    Router::new().route("/{id}", patch(handler_view_toggle))
}
