use crate::infrastructure::http::handlers::view::{
    handler_view_toggle_collection_id, handler_view_toggle_view_id,
};
use axum::{Router, routing::patch};

pub async fn view_router() -> Router {
    Router::new()
        .route(
            "/toggle/{collection_id}/{view_id}",
            patch(handler_view_toggle_view_id),
        )
        .route(
            "/toggle/collection/{collection_id}",
            patch(handler_view_toggle_collection_id),
        )
}
