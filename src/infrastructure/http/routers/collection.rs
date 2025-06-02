use crate::infrastructure::http::handlers::collection::handler_collection_set_active;
use axum::{Router, routing::patch};

pub async fn collection_router() -> Router {
    Router::new().route("/active/{id}", patch(handler_collection_set_active))
}
