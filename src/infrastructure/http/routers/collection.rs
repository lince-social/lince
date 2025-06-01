use crate::infrastructure::http::handlers::collection::{
    handler_collection_hovered, handler_collection_set_active, handler_collection_unhovered,
};
use axum::{
    Router,
    routing::{get, patch},
};

pub async fn collection_router() -> Router {
    Router::new()
        .route("/unhovered", get(handler_collection_unhovered))
        .route("/hovered", get(handler_collection_hovered))
        .route("/active/{id}", patch(handler_collection_set_active))
}
